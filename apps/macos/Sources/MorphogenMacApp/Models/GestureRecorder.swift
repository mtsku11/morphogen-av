import Foundation

/// One recorded breakpoint knot: `t` = timeline seconds from the take's frame
/// 0; `v` = the capture control's value, already clamped to `[0, 1]`. Mirrors
/// the `t:v` pair the CLI's `breakpoints(...)` route grammar parses
/// (`morphogen_render::modulation::parse_breakpoints_source`).
struct GestureKnot: Equatable {
  let t: Double
  let v: Double
}

/// Records a performance-capture gesture during preview playback into
/// breakpoint knots consumed by the offline `breakpoints(...)` modulation
/// source (Tier 1.7) — bit-exact, forever. Pure model, no UI/timers, so the
/// recording rules are unit-testable without driving SwiftUI (the
/// `previewFrameIndex` testability precedent).
///
/// Rules (pinned by `GestureRecorderTests`):
/// - `t` is the preview player's elapsed play time since the take started
///   (Hitting Record restarts playback at frame 0, so `t == 0` is frame 0 by
///   construction — the same origin the offline render's `frame / fps` uses).
/// - The take ends at `min(user stop, one loop duration)`: samples with
///   `t < 0` or `t > loopDuration` are dropped, and so is any sample whose
///   `t` regresses before the last *accepted* sample's `t` — a wrapped second
///   pass would scramble knot ordering, so this makes "one pass, no wrap" a
///   property of the recorder itself, not just caller discipline.
/// - `v` is clamped to `[0, 1]` on ingest; non-finite `t` or `v` samples are
///   rejected outright (never recorded, never move the wrap/decimation
///   cursor).
/// - Decimation: a sample is appended to `knots` only when it is the take's
///   first accepted sample, or `|v - lastRecorded.v| >= decimationThreshold`
///   (a held-still knob yields 2 knots — first + final — not hundreds).
/// - `finish()` closes the take: the last *accepted* sample is always
///   recorded (even if decimation dropped it), so the take's end value holds
///   — breakpoints clamp after the last knot. No-op on an empty take.
final class GestureRecorder {
  static let decimationThreshold = 0.005

  let loopDuration: TimeInterval
  private(set) var knots: [GestureKnot] = []
  private var lastAccepted: GestureKnot?

  init(loopDuration: TimeInterval) {
    self.loopDuration = loopDuration
  }

  /// Offer one `(t, v)` sample. Silently dropped when non-finite, out of
  /// `[0, loopDuration]`, or earlier than the last accepted sample's `t`.
  func ingest(t: Double, v: Double) {
    guard t.isFinite, v.isFinite else { return }
    guard t >= 0, t <= loopDuration else { return }
    if let last = lastAccepted, t < last.t { return }

    let knot = GestureKnot(t: t, v: min(1, max(0, v)))
    lastAccepted = knot

    if let recorded = knots.last, abs(knot.v - recorded.v) < Self.decimationThreshold {
      return
    }
    knots.append(knot)
  }

  /// Close the take. Forces the last accepted sample into `knots` if
  /// decimation held it back, so the take's end value is never lost.
  func finish() {
    guard let last = lastAccepted, knots.last != last else { return }
    knots.append(last)
  }
}
