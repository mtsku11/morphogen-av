import SwiftUI

/// Shared "Quick Preview" band, restored after the sidebar+detail redesign
/// deleted `WorkflowPanelView.swift` (which owned this generically via an
/// 8-case `WorkflowEffect` enum that no longer exists). Ported from that
/// file's `previewBand` + `captureTakeLabel` (last state before deletion:
/// `git show <parent of 4b5bd08>:.../WorkflowPanelView.swift`, lines
/// ~823-988) into a component every eligible effect's detail view
/// instantiates directly — see `docs/QUICK_PREVIEW_RESTORE_MILESTONE.md` for
/// the eligibility table (which effects get this vs. why some can't).
///
/// A looping, downscaled preview render (`PreviewPlayerModel`) with a
/// performance-capture strip layered on top (`GestureRecorder`) for
/// recording a [0,1] gesture onto an armed Rutt-Etra modulation slot
/// (`docs/PERFORMANCE_CAPTURE_MILESTONE.md`). Neither of those models nor
/// AppState's preview/capture plumbing changed — this is a pure
/// reassembly into a new shared view.
///
/// One deliberate behavior change from the original: resets
/// `state.previewFrames` `.onAppear`. The old band lived inside one
/// persistent `WorkflowPanelView`, so stale frames always belonged to the
/// current `selectedEffect`. Now each effect has its own detail view that's
/// destroyed/recreated on sidebar navigation, but `previewFrames` lives on
/// shared `AppState` — without the reset, switching effects would show a
/// stale render mislabeled as the new effect's preview.
struct QuickPreviewBand: View {
  @ObservedObject var state: AppState
  let requiresModulator: Bool
  let runEffect: () -> Void
  @StateObject private var previewPlayer = PreviewPlayerModel()

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(spacing: 8) {
        Label("Preview", systemImage: "eye")
          .font(.headline)
        if state.isRenderingPreview {
          ProgressView()
            .controlSize(.small)
        }
        Spacer()
        // The two preview knobs: downscale factor and seconds of motion.
        // Both feed the session at beginEffectPreview time, nothing else.
        Picker("Scale", selection: $state.previewScale) {
          Text("Full").tag(1)
          Text("1/2").tag(2)
          Text("1/4").tag(4)
          Text("1/8").tag(8)
        }
        .pickerStyle(.segmented)
        .fixedSize()
        .disabled(state.isRenderingPreview)
        Stepper("\(state.previewSeconds)s", value: $state.previewSeconds, in: 1...12)
          .fixedSize()
          .disabled(state.isRenderingPreview)
      }

      Button {
        runQuickPreview()
      } label: {
        Label("Quick Preview", systemImage: "eye")
      }
      .buttonStyle(.bordered)
      .disabled(state.isRenderingPreview || state.isExtractingProxies)

      if state.previewFrames.isEmpty {
        Text(state.isRenderingPreview
          ? state.previewSummary
          : "Quick Preview renders ~\(state.previewSeconds)s of this effect on your loaded sources at reduced resolution — a fast look before committing to the full clip.")
          .font(.caption)
          .foregroundStyle(.secondary)
      } else {
        let frames = state.previewFrames
        let shownIndex = min(previewPlayer.currentIndex, frames.count - 1)

        Image(nsImage: frames[shownIndex])
          .resizable()
          .scaledToFit()
          .frame(maxWidth: .infinity, maxHeight: 260)
          .clipShape(RoundedRectangle(cornerRadius: 8))

        HStack(spacing: 10) {
          Button {
            previewPlayer.togglePlayPause()
          } label: {
            Image(systemName: previewPlayer.isPlaying ? "pause.fill" : "play.fill")
          }
          .help(previewPlayer.isPlaying ? "Pause preview playback" : "Play preview loop")
          Text("frame \(shownIndex + 1)/\(frames.count)")
            .font(.caption)
            .monospacedDigit()
            .foregroundStyle(.secondary)
          Spacer()
        }

        // Performance capture (docs/PERFORMANCE_CAPTURE_MILESTONE.md):
        // record a [0,1] gesture against the looping preview; the take
        // becomes a breakpoints(...) route on the armed Rutt-Etra slot.
        // Intentionally NOT gated on this being the Rutt-Etra detail view —
        // it's about whichever Rutt-Etra modulation slot(s) are armed
        // elsewhere; you can record while quick-previewing any effect.
        if !state.ruttEtraArmedCaptureTargets.isEmpty {
          HStack(spacing: 10) {
            Button {
              if state.isCapturing {
                state.endCaptureTake()
              } else {
                let fps = state.previewPlaybackFps
                guard fps.isFinite, fps > 0 else { return }
                // Restart playback at frame 0 in the same action, so the
                // recorder's t == 0 is frame 0 by construction.
                previewPlayer.start(frameCount: frames.count, fps: fps)
                state.beginCaptureTake(loopDuration: Double(frames.count) / fps)
              }
            } label: {
              Image(systemName: state.isCapturing ? "stop.circle.fill" : "record.circle")
                .foregroundStyle(state.isCapturing ? .primary : Color.red)
            }
            .help(state.isCapturing
              ? "Stop the take (it also auto-stops after one loop)"
              : "Record a gesture on the armed slot from frame 0")

            Picker("Capture", selection: $state.captureTargetSelection) {
              ForEach(state.ruttEtraArmedCaptureTargets, id: \.self) { target in
                Text(target).tag(target)
              }
            }
            .frame(width: 260)
            .disabled(state.isCapturing)
            .help("Which armed Rutt-Etra slot this take records onto.")

            Slider(value: $state.captureSlider, in: 0...1)
              .frame(maxWidth: 260)
              .onChange(of: state.captureSlider) { _, newValue in
                state.ingestCaptureSample(t: previewPlayer.elapsed(), v: newValue)
              }
              .help("The capture control — scrub while the preview loops.")

            Text(captureTakeLabel)
              .font(.caption)
              .monospacedDigit()
              .foregroundStyle(.secondary)
            Spacer()
          }
          .onAppear {
            if !state.ruttEtraArmedCaptureTargets.contains(state.captureTargetSelection) {
              state.captureTargetSelection = state.ruttEtraArmedCaptureTargets.first ?? ""
            }
          }
        }

        ScrollView(.horizontal, showsIndicators: true) {
          HStack(spacing: 8) {
            ForEach(Array(frames.enumerated()), id: \.offset) { index, image in
              VStack(spacing: 4) {
                Image(nsImage: image)
                  .resizable()
                  .scaledToFit()
                  .frame(height: 64)
                  .clipShape(RoundedRectangle(cornerRadius: 6))
                  .overlay(
                    RoundedRectangle(cornerRadius: 6)
                      .stroke(
                        index == shownIndex ? Color.accentColor : .clear,
                        lineWidth: 2
                      )
                  )
                Text("frame \(index)")
                  .font(.caption2)
                  .foregroundStyle(.secondary)
              }
            }
          }
          .padding(.vertical, 2)
        }

        Text(state.previewSummary)
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.quaternary.opacity(0.28), in: RoundedRectangle(cornerRadius: 8))
    // New: switching the sidebar selection destroys and recreates a
    // different per-effect view, but previewFrames lives on shared
    // AppState — reset so a newly-viewed effect never shows a stale
    // render from whichever effect was previewed before it.
    .onAppear {
      state.previewFrames = []
      previewPlayer.stop()
    }
    // Play-the-instrument: loop the preview automatically as soon as its
    // frames finish loading (beginEffectPreview empties the array first, so
    // the count reliably transitions 0 → N even at a constant frame cap).
    .onChange(of: state.previewFrames.count) { _, newCount in
      if newCount > 0 {
        // The fps recorded when THIS preview began — a proxy-fps change made
        // after extraction must not shift an already-rendered preview's rate.
        previewPlayer.start(frameCount: newCount, fps: state.previewPlaybackFps)
      } else {
        previewPlayer.stop()
      }
    }
  }

  private func runQuickPreview() {
    guard state.beginEffectPreview(requiresModulator: requiresModulator) else {
      return
    }
    runEffect()
  }

  /// Take status for the capture strip: recording, a stored take's knot count,
  /// or the arm-and-record hint.
  private var captureTakeLabel: String {
    if state.isCapturing {
      return "recording…"
    }
    let target = state.captureTargetSelection
    if let take = state.ruttEtraCapturedTakes[target], let last = take.last {
      return "\(take.count) knot(s) / \(String(format: "%.1f", last.t))s"
    }
    return "no take yet"
  }
}
