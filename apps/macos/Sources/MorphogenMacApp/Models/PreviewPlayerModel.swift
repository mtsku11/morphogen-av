import Foundation

/// Frame index shown at `elapsed` seconds into looping playback of
/// `frameCount` frames at `fps`. Pure free function separated from the
/// Timer/UI for testability (the `enumModulationMapping` / `lfoSourceSpec`
/// precedent). Pinned semantics:
/// - `frameCount <= 0` → 0 (callers only subscript non-empty frame arrays);
/// - invalid `fps` (non-finite or <= 0) → hold frame 0 rather than guess a
///   rate — no divide-by-zero, no motion;
/// - negative/invalid `elapsed` → frame 0;
/// - otherwise `floor(elapsed * fps) mod frameCount`, wrapped in Double space
///   so arbitrarily large elapsed times never overflow `Int`.
func previewFrameIndex(elapsed: TimeInterval, frameCount: Int, fps: Double) -> Int {
  guard frameCount > 0 else { return 0 }
  guard fps.isFinite, fps > 0, elapsed.isFinite, elapsed > 0 else { return 0 }
  let wrapped = (elapsed * fps).truncatingRemainder(dividingBy: Double(frameCount))
  return Int(wrapped)
}

/// Looping playback state for the effect-preview band: drives
/// `currentIndex` through the loaded preview frames at the proxy-extraction
/// frame rate. The index is always recomputed from total elapsed play time
/// via `previewFrameIndex` (never incremented), so timer jitter cannot
/// accumulate drift. Pausing freezes elapsed time; resuming continues from
/// the paused position.
final class PreviewPlayerModel: ObservableObject {
  @Published private(set) var currentIndex = 0
  @Published private(set) var isPlaying = false

  private(set) var frameCount = 0
  private(set) var fps: Double = 12

  /// Play time accumulated over completed play stretches (frozen by pause).
  private var accumulatedElapsed: TimeInterval = 0
  /// Start of the in-flight play stretch; nil while paused/stopped.
  private var resumedAt: Date?
  private var timer: Timer?

  /// Begin looping playback from frame 0. `now` is injectable for tests.
  func start(frameCount: Int, fps: Double, now: Date = Date()) {
    stop()
    self.frameCount = frameCount
    self.fps = fps
    guard frameCount > 0 else { return }
    resumedAt = now
    isPlaying = true
    startTimer()
  }

  func togglePlayPause(now: Date = Date()) {
    if isPlaying {
      pause(now: now)
    } else {
      resume(now: now)
    }
  }

  func pause(now: Date = Date()) {
    guard isPlaying else { return }
    accumulatedElapsed = elapsed(now: now)
    resumedAt = nil
    isPlaying = false
    timer?.invalidate()
    timer = nil
  }

  func resume(now: Date = Date()) {
    guard !isPlaying, frameCount > 0 else { return }
    resumedAt = now
    isPlaying = true
    startTimer()
  }

  func stop() {
    timer?.invalidate()
    timer = nil
    isPlaying = false
    resumedAt = nil
    accumulatedElapsed = 0
    frameCount = 0
    currentIndex = 0
  }

  /// Total play time: completed stretches plus the in-flight one. Frozen
  /// while paused (pause semantics pinned in PreviewPlayerModelTests).
  func elapsed(now: Date = Date()) -> TimeInterval {
    accumulatedElapsed + (resumedAt.map { now.timeIntervalSince($0) } ?? 0)
  }

  /// The frame index playback shows at `now` — the pure stepping function
  /// applied to this model's elapsed time.
  func index(at now: Date = Date()) -> Int {
    previewFrameIndex(elapsed: elapsed(now: now), frameCount: frameCount, fps: fps)
  }

  private func startTimer() {
    let tickRate = (fps.isFinite && fps > 0) ? fps : 12
    let timer = Timer(timeInterval: 1.0 / tickRate, repeats: true) { [weak self] _ in
      guard let self else { return }
      let index = self.index(at: Date())
      if index != self.currentIndex {
        self.currentIndex = index
      }
    }
    // .common so playback keeps stepping while scroll/UI tracking is active.
    RunLoop.main.add(timer, forMode: .common)
    self.timer = timer
  }

  deinit {
    timer?.invalidate()
  }
}
