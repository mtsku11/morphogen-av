@testable import MorphogenMacApp
import XCTest

/// Pins the preview-player stepping semantics (preview-loop milestone,
/// Slice 2): the pure `previewFrameIndex` function (wraparound, empty
/// frames, fps sanity) and the model's pause/resume elapsed-time handling —
/// all without spinning a Timer or any UI.
final class PreviewPlayerModelTests: XCTestCase {
  // MARK: previewFrameIndex — the pure stepping function

  func testStepsFramesAtTheGivenFps() {
    XCTAssertEqual(previewFrameIndex(elapsed: 0, frameCount: 8, fps: 12), 0)
    // 0.25 s at 12 fps = frame 3.
    XCTAssertEqual(previewFrameIndex(elapsed: 0.25, frameCount: 8, fps: 12), 3)
    // Just past the last frame boundary within the first loop.
    XCTAssertEqual(previewFrameIndex(elapsed: 7.001 / 12.0, frameCount: 8, fps: 12), 7)
  }

  func testWrapsAroundPastOneLoop() {
    // 8 frames at 12 fps: one full loop plus two frames lands on frame 2.
    XCTAssertEqual(previewFrameIndex(elapsed: 10.001 / 12.0, frameCount: 8, fps: 12), 2)
    // Exactly one loop wraps back to frame 0.
    XCTAssertEqual(previewFrameIndex(elapsed: 8.0 / 12.0, frameCount: 8, fps: 12), 0)
    // An enormous elapsed time stays in range (wrapped in Double space, no
    // Int overflow).
    let farIndex = previewFrameIndex(elapsed: 1e12, frameCount: 8, fps: 12)
    XCTAssertTrue((0..<8).contains(farIndex))
  }

  func testEmptyFramesHoldIndexZero() {
    // Pinned choice: frameCount <= 0 returns 0 (not nil) — callers only
    // subscript non-empty frame arrays, so 0 is never used as an index.
    XCTAssertEqual(previewFrameIndex(elapsed: 3, frameCount: 0, fps: 12), 0)
    XCTAssertEqual(previewFrameIndex(elapsed: 3, frameCount: -1, fps: 12), 0)
  }

  func testInvalidFpsHoldsFrameZeroWithoutDividingByZero() {
    // Pinned clamp: non-finite or <= 0 fps holds frame 0 rather than
    // guessing a rate.
    XCTAssertEqual(previewFrameIndex(elapsed: 3, frameCount: 8, fps: 0), 0)
    XCTAssertEqual(previewFrameIndex(elapsed: 3, frameCount: 8, fps: -12), 0)
    XCTAssertEqual(previewFrameIndex(elapsed: 3, frameCount: 8, fps: .nan), 0)
    XCTAssertEqual(previewFrameIndex(elapsed: 3, frameCount: 8, fps: .infinity), 0)
  }

  func testInvalidElapsedHoldsFrameZero() {
    XCTAssertEqual(previewFrameIndex(elapsed: -1, frameCount: 8, fps: 12), 0)
    XCTAssertEqual(previewFrameIndex(elapsed: .nan, frameCount: 8, fps: 12), 0)
  }

  // MARK: PreviewPlayerModel — pause/resume elapsed-time semantics

  func testPauseFreezesElapsedAndResumeContinuesFromPausedPosition() {
    let model = PreviewPlayerModel()
    defer { model.stop() }
    let t0 = Date(timeIntervalSinceReferenceDate: 1_000)

    model.start(frameCount: 8, fps: 12, now: t0)
    XCTAssertTrue(model.isPlaying)
    XCTAssertEqual(model.elapsed(now: t0.addingTimeInterval(0.5)), 0.5, accuracy: 1e-9)

    // Pause: elapsed freezes no matter how much wall time passes.
    model.pause(now: t0.addingTimeInterval(0.5))
    XCTAssertFalse(model.isPlaying)
    XCTAssertEqual(model.elapsed(now: t0.addingTimeInterval(60)), 0.5, accuracy: 1e-9)

    // Resume: playback continues from the paused position, not from zero
    // and not jumped ahead by the paused wall time.
    model.resume(now: t0.addingTimeInterval(60))
    XCTAssertTrue(model.isPlaying)
    XCTAssertEqual(model.elapsed(now: t0.addingTimeInterval(60.25)), 0.75, accuracy: 1e-9)
    // 0.75 s at 12 fps = frame 9, wrapping 8 frames to frame 1.
    XCTAssertEqual(model.index(at: t0.addingTimeInterval(60.25)), 1)
  }

  func testStartWithNoFramesDoesNotPlay() {
    let model = PreviewPlayerModel()
    defer { model.stop() }
    model.start(frameCount: 0, fps: 12, now: Date())
    XCTAssertFalse(model.isPlaying)
    XCTAssertEqual(model.currentIndex, 0)
  }

  func testStopResetsPlaybackState() {
    let model = PreviewPlayerModel()
    let t0 = Date(timeIntervalSinceReferenceDate: 2_000)
    model.start(frameCount: 8, fps: 12, now: t0)
    model.stop()
    XCTAssertFalse(model.isPlaying)
    XCTAssertEqual(model.currentIndex, 0)
    XCTAssertEqual(model.elapsed(now: t0.addingTimeInterval(5)), 0)
  }

  // MARK: preview session pure helpers (preview-loop Slice 3)

  func testPreviewFrameCapIsSecondsTimesFpsRounded() {
    XCTAssertEqual(previewFrameCap(seconds: 4, fps: 12), 48)
    // Fractional fps rounds to nearest.
    XCTAssertEqual(previewFrameCap(seconds: 3, fps: 12.5), 38)
    // Degenerate inputs clamp to a single frame, never zero or negative.
    XCTAssertEqual(previewFrameCap(seconds: 0, fps: 12), 1)
    XCTAssertEqual(previewFrameCap(seconds: 4, fps: 0), 1)
    XCTAssertEqual(previewFrameCap(seconds: 4, fps: .nan), 1)
  }

  func testPreviewInputOverrideIsNilAtScaleOneElseFixedDestination() {
    let root = URL(fileURLWithPath: "/tmp/preview-root", isDirectory: true)
    // Scale 1 = the identity anchor: no override, render from the originals.
    XCTAssertNil(previewInputOverrideURL(previewRoot: root, scale: 1, label: "carrier"))
    XCTAssertEqual(
      previewInputOverrideURL(previewRoot: root, scale: 4, label: "carrier")?.lastPathComponent,
      "downscaled-carrier"
    )
    XCTAssertEqual(
      previewInputOverrideURL(previewRoot: root, scale: 2, label: "modulator")?.lastPathComponent,
      "downscaled-modulator"
    )
  }
}
