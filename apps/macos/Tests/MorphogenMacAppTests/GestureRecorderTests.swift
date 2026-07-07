@testable import MorphogenMacApp
import XCTest

/// Performance-capture recorder + spec-emission rules
/// (docs/PERFORMANCE_CAPTURE_MILESTONE.md), pinned as pure-model tests — the
/// `previewFrameIndex` testability precedent: no UI, no timers.
final class GestureRecorderTests: XCTestCase {
  func testDecimationDropsSubThresholdMoves() {
    let recorder = GestureRecorder(loopDuration: 4)
    recorder.ingest(t: 0.0, v: 0.5)
    recorder.ingest(t: 0.1, v: 0.501)  // < 0.005 from last recorded — dropped
    recorder.ingest(t: 0.2, v: 0.503)  // still < 0.005 from 0.5 — dropped
    recorder.ingest(t: 0.3, v: 0.51)  // >= 0.005 — kept
    recorder.finish()
    XCTAssertEqual(
      recorder.knots,
      [GestureKnot(t: 0.0, v: 0.5), GestureKnot(t: 0.3, v: 0.51)]
    )
  }

  func testHeldStillKnobYieldsFirstAndFinalKnots() {
    let recorder = GestureRecorder(loopDuration: 4)
    for step in 0..<40 {
      recorder.ingest(t: Double(step) * 0.1, v: 0.7 + Double(step % 2) * 0.001)
    }
    recorder.finish()
    // First sample + the forced final accepted sample — not 40 knots.
    XCTAssertEqual(recorder.knots.count, 2)
    XCTAssertEqual(recorder.knots[0].t, 0.0)
    XCTAssertEqual(recorder.knots[1].t, Double(39) * 0.1)
  }

  func testClampAndNonFiniteRules() {
    let recorder = GestureRecorder(loopDuration: 4)
    recorder.ingest(t: 0.0, v: -0.5)  // clamps to 0
    recorder.ingest(t: 1.0, v: 1.5)  // clamps to 1
    recorder.ingest(t: Double.nan, v: 0.5)  // rejected outright
    recorder.ingest(t: 2.0, v: Double.infinity)  // rejected outright
    recorder.finish()
    XCTAssertEqual(recorder.knots, [GestureKnot(t: 0.0, v: 0.0), GestureKnot(t: 1.0, v: 1.0)])
  }

  func testOnePassNoWrap() {
    let recorder = GestureRecorder(loopDuration: 2)
    recorder.ingest(t: 0.0, v: 0.0)
    recorder.ingest(t: 1.5, v: 1.0)
    recorder.ingest(t: 2.5, v: 0.2)  // past the loop — dropped
    recorder.ingest(t: 0.4, v: 0.4)  // a wrapped second pass — dropped (t regressed)
    recorder.finish()
    XCTAssertEqual(recorder.knots, [GestureKnot(t: 0.0, v: 0.0), GestureKnot(t: 1.5, v: 1.0)])
  }

  func testFinishForcesLastAcceptedSampleAndEmptyTakeIsEmpty() {
    let recorder = GestureRecorder(loopDuration: 4)
    recorder.ingest(t: 0.0, v: 0.5)
    recorder.ingest(t: 3.0, v: 0.502)  // decimated away…
    recorder.finish()  // …but forced back as the end value
    XCTAssertEqual(recorder.knots.last, GestureKnot(t: 3.0, v: 0.502))

    let empty = GestureRecorder(loopDuration: 4)
    empty.finish()
    XCTAssertTrue(empty.knots.isEmpty)
  }

  func testRecorderIsDeterministic() {
    let stream: [(Double, Double)] = (0..<200).map { step in
      (Double(step) * 0.02, 0.5 + 0.5 * sin(Double(step) * 0.3))
    }
    func run() -> [GestureKnot] {
      let recorder = GestureRecorder(loopDuration: 4)
      for (t, v) in stream { recorder.ingest(t: t, v: v) }
      recorder.finish()
      return recorder.knots
    }
    XCTAssertEqual(run(), run())
  }

  @MainActor
  func testCaptureLifecycleOnAppState() {
    let state = AppState()

    // Unarmed: recording refuses with a status message.
    state.beginCaptureTake(loopDuration: 2)
    XCTAssertFalse(state.isCapturing)
    XCTAssertTrue(state.statusMessage.contains("Arm a Rutt-Etra mod slot"))

    // Armed but no take: the run path's precondition is nil — the
    // modulationRoutes guard then refuses (the missing-media precedent).
    state.ruttEtraModDepthSource = .captured
    XCTAssertEqual(state.ruttEtraArmedCaptureTargets, ["displacement_depth"])
    XCTAssertNil(state.ruttEtraCapturedSpec("displacement_depth"))

    // Record a take: begin ingests the slider's value at t = 0; the stored
    // take spells the exact breakpoints clause.
    state.captureSlider = 0.0
    state.beginCaptureTake(loopDuration: 2)
    XCTAssertTrue(state.isCapturing)
    XCTAssertEqual(state.captureTargetSelection, "displacement_depth")
    state.ingestCaptureSample(t: 2.0, v: 1.0)
    state.endCaptureTake()
    XCTAssertFalse(state.isCapturing)
    XCTAssertEqual(
      state.ruttEtraCapturedSpec("displacement_depth"), "breakpoints(0:0;2:1)")

    // Re-record replaces (delete + re-record IS the MVP edit story).
    state.captureSlider = 0.25
    state.beginCaptureTake(loopDuration: 2)
    state.endCaptureTake()
    XCTAssertEqual(
      state.ruttEtraCapturedSpec("displacement_depth"), "breakpoints(0:0.25)")
  }

  func testCapturedSourceSpecFormatsSortedKnots() {
    // Exact spec text from fixed knots — the render half parses this verbatim
    // (breakpoints(t:v;...), knots ascending by t).
    let spec = capturedSourceSpec([
      GestureKnot(t: 2.0, v: 1.0),
      GestureKnot(t: 0.0, v: 0.0),
      GestureKnot(t: 0.5, v: 0.25),
    ])
    XCTAssertEqual(spec, "breakpoints(0:0;0.5:0.25;2:1)")
    XCTAssertNil(capturedSourceSpec([]))
  }
}
