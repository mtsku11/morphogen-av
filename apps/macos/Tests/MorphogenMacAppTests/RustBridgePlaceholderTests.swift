import Foundation
@testable import MorphogenMacApp
import XCTest

final class RustBridgePlaceholderTests: XCTestCase {
  func testQueuedFrameSequenceArgumentsIncludeSelectedInputsAndOptions() throws {
    let request = FrameSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/frame-sequence-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: 12.5,
      maxFrames: 48,
      frameRate: 23.976,
      writesFlowCache: false,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddFrameSequenceArguments(request: request)

    XCTAssertEqual(arguments.prefix(7), ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-frame-sequence"])
    XCTAssertTrue(arguments.contains("/tmp/frame-sequence-queue.json"))
    XCTAssertTrue(arguments.contains("/tmp/source-a-frames"))
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/output-root"))
    XCTAssertTrue(arguments.contains("--amount"))
    XCTAssertTrue(arguments.contains("12.5"))
    XCTAssertTrue(arguments.contains("--frame-rate"))
    XCTAssertTrue(arguments.contains("23.976"))
    XCTAssertTrue(arguments.contains("--no-flow-cache"))
    XCTAssertTrue(arguments.contains("--max-frames"))
    XCTAssertTrue(arguments.contains("48"))
    XCTAssertTrue(arguments.contains("--project-path"))
    XCTAssertTrue(arguments.contains("/tmp/project.morphogen.json"))
  }

  func testQueuedFrameSequenceArgumentsRejectInvalidValues() {
    let request = FrameSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/frame-sequence-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: .nan,
      maxFrames: 48,
      frameRate: 24.0,
      writesFlowCache: true,
      projectURL: nil
    )

    XCTAssertThrowsError(try RustBridgePlaceholder.queueAddFrameSequenceArguments(request: request))
  }

  func testQueuedFeedbackSequenceArgumentsIncludeFlowControls() throws {
    let request = FeedbackSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/feedback-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      carrierAmount: 1.5,
      feedbackAmount: 2.0,
      feedbackMix: 0.72,
      decay: 0.995,
      iterations: 1,
      structureMix: 0.6,
      outputBitDepth: .png16,
      temporalSupersampling: 2,
      maxFrames: 48,
      resetAtFrame: 24,
      frameRate: 24.0,
      writesFlowCache: true,
      backend: .metal,
      flowSource: .opticalFlow,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddFeedbackSequenceArguments(request: request)

    XCTAssertEqual(arguments.prefix(7), ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-feedback-sequence"])
    XCTAssertTrue(arguments.contains("--carrier-amount"))
    XCTAssertTrue(arguments.contains("1.5"))
    XCTAssertTrue(arguments.contains("--feedback-amount"))
    XCTAssertTrue(arguments.contains("2"))
    XCTAssertTrue(arguments.contains("--feedback-mix"))
    XCTAssertTrue(arguments.contains("0.72"))
    XCTAssertTrue(arguments.contains("--decay"))
    XCTAssertTrue(arguments.contains("0.995"))
    XCTAssertTrue(arguments.contains("--structure-mix"))
    XCTAssertTrue(arguments.contains("0.6"))
    XCTAssertTrue(arguments.contains("--iterations"))
    XCTAssertTrue(arguments.contains("--output-bit-depth"))
    XCTAssertTrue(arguments.contains("16"))
    XCTAssertTrue(arguments.contains("--temporal-supersampling"))
    XCTAssertTrue(arguments.contains("--reset-at-frame"))
    XCTAssertTrue(arguments.contains("24"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("metal"))
    XCTAssertTrue(arguments.contains("--flow-source"))
    XCTAssertTrue(arguments.contains("optical-flow"))
  }

  func testQueuedFeedbackSequenceArgumentsRejectUnsupportedIterations() {
    let request = FeedbackSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/feedback-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      carrierAmount: 1.5,
      feedbackAmount: 2.0,
      feedbackMix: 0.72,
      decay: 0.995,
      iterations: 2,
      structureMix: 0.0,
      outputBitDepth: .png8,
      temporalSupersampling: 1,
      maxFrames: nil,
      resetAtFrame: nil,
      frameRate: 24.0,
      writesFlowCache: true,
      backend: .cpu,
      flowSource: .opticalFlow,
      projectURL: nil
    )

    XCTAssertThrowsError(try RustBridgePlaceholder.queueAddFeedbackSequenceArguments(request: request))
  }

  func testQueuedGranularMosaicPoolSequenceArgumentsIncludeAudioControls() throws {
    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/granular-pool-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      grainSize: 24,
      rearrangement: 0.5,
      variation: 0.3,
      seed: 7,
      audioWeight: 1.5,
      modulatorRMSCacheURL: URL(fileURLWithPath: "/tmp/source-a/analysis/rms.json"),
      carrierRMSCacheURL: URL(fileURLWithPath: "/tmp/source-b/analysis/rms.json"),
      maxFrames: 48,
      frameRate: 24.0,
      backend: .cpu,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddGranularMosaicPoolSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-granular-mosaic-pool-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-a-frames"))
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/output-root"))
    XCTAssertTrue(arguments.contains("--grain-size"))
    XCTAssertTrue(arguments.contains("24"))
    XCTAssertTrue(arguments.contains("--rearrangement"))
    XCTAssertTrue(arguments.contains("0.5"))
    XCTAssertTrue(arguments.contains("--variation"))
    XCTAssertTrue(arguments.contains("0.3"))
    XCTAssertTrue(arguments.contains("--seed"))
    XCTAssertTrue(arguments.contains("7"))
    XCTAssertTrue(arguments.contains("--audio-weight"))
    XCTAssertTrue(arguments.contains("1.5"))
    XCTAssertTrue(arguments.contains("--modulator-rms-cache"))
    XCTAssertTrue(arguments.contains("/tmp/source-a/analysis/rms.json"))
    XCTAssertTrue(arguments.contains("--carrier-rms-cache"))
    XCTAssertTrue(arguments.contains("/tmp/source-b/analysis/rms.json"))
    XCTAssertTrue(arguments.contains("--max-frames"))
    XCTAssertTrue(arguments.contains("48"))
    XCTAssertTrue(arguments.contains("--project-path"))
    XCTAssertTrue(arguments.contains("/tmp/project.morphogen.json"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("cpu"))
  }

  func testQueuedGranularMosaicPoolSequenceArgumentsSelectMetalBackend() throws {
    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/granular-pool-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      grainSize: 32,
      rearrangement: 1.0,
      variation: 0.25,
      seed: 0,
      audioWeight: 1.0,
      modulatorRMSCacheURL: nil,
      carrierRMSCacheURL: nil,
      maxFrames: nil,
      frameRate: 24.0,
      backend: .metal,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddGranularMosaicPoolSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("metal"))
  }

  func testQueuedGranularMosaicPoolSequenceArgumentsOmitAudioCachesWhenColorOnly() throws {
    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/granular-pool-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      grainSize: 32,
      rearrangement: 1.0,
      variation: 0.25,
      seed: 0,
      audioWeight: 1.0,
      modulatorRMSCacheURL: nil,
      carrierRMSCacheURL: nil,
      maxFrames: nil,
      frameRate: 24.0,
      backend: .cpu,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddGranularMosaicPoolSequenceArguments(request: request)

    XCTAssertFalse(arguments.contains("--modulator-rms-cache"))
    XCTAssertFalse(arguments.contains("--carrier-rms-cache"))
    XCTAssertFalse(arguments.contains("--max-frames"))
    XCTAssertFalse(arguments.contains("--project-path"))
    XCTAssertTrue(arguments.contains("--audio-weight"))
  }

  func testQueuedGranularMosaicPoolSequenceArgumentsRejectMismatchedAudioCaches() {
    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/granular-pool-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      grainSize: 32,
      rearrangement: 1.0,
      variation: 0.25,
      seed: 0,
      audioWeight: 1.0,
      modulatorRMSCacheURL: URL(fileURLWithPath: "/tmp/source-a/analysis/rms.json"),
      carrierRMSCacheURL: nil,
      maxFrames: nil,
      frameRate: 24.0,
      backend: .cpu,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddGranularMosaicPoolSequenceArguments(request: request)
    )
  }

  func testQueuedGranularMosaicPoolSequenceArgumentsIncludeSchedulingKnobs() throws {
    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/granular-pool-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      grainSize: 32,
      rearrangement: 1.0,
      variation: 0.25,
      seed: 0,
      audioWeight: 1.0,
      modulatorRMSCacheURL: nil,
      carrierRMSCacheURL: nil,
      poolWindow: 3,
      antiRepeatWeight: 0.5,
      antiRepeatCooldown: 4,
      coherenceWeight: 0.25,
      coherenceReach: 6,
      spatialCoherenceWeight: 0.125,
      maxFrames: nil,
      frameRate: 24.0,
      backend: .cpu,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddGranularMosaicPoolSequenceArguments(request: request)

    XCTAssertEqual(Self.value(after: "--pool-window", in: arguments), "3")
    XCTAssertEqual(Self.value(after: "--anti-repeat-weight", in: arguments), "0.5")
    XCTAssertEqual(Self.value(after: "--anti-repeat-cooldown", in: arguments), "4")
    XCTAssertEqual(Self.value(after: "--coherence-weight", in: arguments), "0.25")
    XCTAssertEqual(Self.value(after: "--coherence-reach", in: arguments), "6")
    XCTAssertEqual(Self.value(after: "--spatial-coherence-weight", in: arguments), "0.125")
  }

  func testQueuedGranularMosaicPoolSequenceArgumentsIncludeCentroidCaches() throws {
    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/granular-pool-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      grainSize: 32,
      rearrangement: 1.0,
      variation: 0.25,
      seed: 0,
      audioWeight: 1.0,
      modulatorRMSCacheURL: nil,
      carrierRMSCacheURL: nil,
      modulatorCentroidCacheURL: URL(fileURLWithPath: "/tmp/source-a/analysis/stft.json"),
      carrierCentroidCacheURL: URL(fileURLWithPath: "/tmp/source-b/analysis/stft.json"),
      maxFrames: nil,
      frameRate: 24.0,
      backend: .cpu,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddGranularMosaicPoolSequenceArguments(request: request)

    XCTAssertEqual(
      Self.value(after: "--modulator-centroid-cache", in: arguments),
      "/tmp/source-a/analysis/stft.json"
    )
    XCTAssertEqual(
      Self.value(after: "--carrier-centroid-cache", in: arguments),
      "/tmp/source-b/analysis/stft.json"
    )
  }

  func testQueuedGranularMosaicPoolSequenceArgumentsRejectMismatchedCentroidCaches() {
    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/granular-pool-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      grainSize: 32,
      rearrangement: 1.0,
      variation: 0.25,
      seed: 0,
      audioWeight: 1.0,
      modulatorRMSCacheURL: nil,
      carrierRMSCacheURL: nil,
      modulatorCentroidCacheURL: URL(fileURLWithPath: "/tmp/source-a/analysis/stft.json"),
      carrierCentroidCacheURL: nil,
      maxFrames: nil,
      frameRate: 24.0,
      backend: .cpu,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddGranularMosaicPoolSequenceArguments(request: request)
    )
  }

  /// Returns the argument immediately following `flag`, or nil if the flag is absent
  /// or trailing. Lets tests pin a flag to its value rather than just membership.
  private static func value(after flag: String, in arguments: [String]) -> String? {
    guard let index = arguments.firstIndex(of: flag), index + 1 < arguments.count else {
      return nil
    }
    return arguments[index + 1]
  }

  func testMediaProxyArgumentsIncludeFrameAndAudioExtraction() throws {
    let request = MediaProxyExtractionCommandRequest(
      sourceURL: URL(fileURLWithPath: "/tmp/source.mov"),
      proxyDirectoryURL: URL(fileURLWithPath: "/tmp/proxy/source-a", isDirectory: true),
      framesPerSecond: 12.0,
      maxFrames: 120,
      sampleRate: 48_000
    )

    let arguments = try RustBridgePlaceholder.mediaProxyExtractionArguments(request: request)

    XCTAssertEqual(arguments.frameExtraction.prefix(7), ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "extract-frames"])
    XCTAssertTrue(arguments.frameExtraction.contains("/tmp/proxy/source-a/frames"))
    XCTAssertTrue(arguments.frameExtraction.contains("--fps"))
    XCTAssertTrue(arguments.frameExtraction.contains("12"))
    XCTAssertTrue(arguments.audioExtraction.contains("extract-audio"))
    XCTAssertTrue(arguments.audioExtraction.contains("/tmp/proxy/source-a/audio.wav"))
    XCTAssertTrue(arguments.audioExtraction.contains("--sample-rate"))
    XCTAssertTrue(arguments.audioExtraction.contains("48000"))
    XCTAssertTrue(arguments.audioExtraction.contains("--max-duration-seconds"))
    XCTAssertTrue(arguments.audioExtraction.contains("10"))
  }

  func testMediaProxyArgumentsGenerateRMSAndSTFTAnalysisCaches() throws {
    let request = MediaProxyExtractionCommandRequest(
      sourceURL: URL(fileURLWithPath: "/tmp/source.mov"),
      proxyDirectoryURL: URL(fileURLWithPath: "/tmp/proxy/source-a", isDirectory: true),
      framesPerSecond: 12.0,
      maxFrames: nil,
      sampleRate: 48_000
    )

    let arguments = try RustBridgePlaceholder.mediaProxyExtractionArguments(request: request)

    XCTAssertTrue(arguments.rmsCacheGeneration.contains("cache-rms"))
    XCTAssertTrue(arguments.rmsCacheGeneration.contains("/tmp/proxy/source-a/audio.wav"))
    XCTAssertTrue(arguments.rmsCacheGeneration.contains("/tmp/proxy/source-a/analysis/rms.json"))
    XCTAssertTrue(arguments.rmsCacheGeneration.contains("--window-size"))

    XCTAssertTrue(arguments.stftCacheGeneration.contains("cache-stft"))
    XCTAssertTrue(arguments.stftCacheGeneration.contains("/tmp/proxy/source-a/audio.wav"))
    XCTAssertTrue(arguments.stftCacheGeneration.contains("/tmp/proxy/source-a/analysis/stft.json"))
    XCTAssertTrue(arguments.stftCacheGeneration.contains("--fft-size"))
  }

  func testProjectProxyRegistrationArgumentsIncludeProxyAndAnalysisPaths() {
    let proxy = MediaProxyExtractionCommandResult(
      sourceURL: URL(fileURLWithPath: "/tmp/source.mov"),
      proxyDirectoryURL: URL(fileURLWithPath: "/tmp/proxy/source-a", isDirectory: true),
      frameDirectoryURL: URL(fileURLWithPath: "/tmp/proxy/source-a/frames", isDirectory: true),
      audioWAVURL: URL(fileURLWithPath: "/tmp/proxy/source-a/audio.wav"),
      rmsCacheURL: URL(fileURLWithPath: "/tmp/proxy/source-a/analysis/rms.json"),
      stftCacheURL: URL(fileURLWithPath: "/tmp/proxy/source-a/analysis/stft.json")
    )

    let arguments = RustBridgePlaceholder.projectSourceProxyRegistrationArguments(
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json"),
      sourceRole: .modulator,
      proxy: proxy
    )

    XCTAssertTrue(arguments.contains("project-register-proxy"))
    XCTAssertTrue(arguments.contains("/tmp/project.morphogen.json"))
    XCTAssertTrue(arguments.contains("--source-role"))
    XCTAssertTrue(arguments.contains("modulator"))
    XCTAssertTrue(arguments.contains("/tmp/proxy/source-a/frames"))
    XCTAssertTrue(arguments.contains("audio_rms=/tmp/proxy/source-a/analysis/rms.json"))
    XCTAssertTrue(arguments.contains("stft=/tmp/proxy/source-a/analysis/stft.json"))
  }
}
