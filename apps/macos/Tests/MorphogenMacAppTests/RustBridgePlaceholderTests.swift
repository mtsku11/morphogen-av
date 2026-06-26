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

  func testShowcaseArgumentsIncludeCuratedPreviewOptions() throws {
    let request = ShowcaseRenderCommandRequest(
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputDirectoryURL: URL(fileURLWithPath: "/tmp/showcase-preview", isDirectory: true),
      intensity: .destructive,
      framesPerEffect: 15,
      frameRate: 12.0,
      granularGrainSize: 48,
      seed: 20260625,
      backend: .cpu,
      encodeMP4: true
    )

    let arguments = try RustBridgePlaceholder.renderShowcaseArguments(request: request)

    XCTAssertEqual(arguments.prefix(7), ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "render-showcase"])
    XCTAssertTrue(arguments.contains("/tmp/source-a-frames"))
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/showcase-preview"))
    XCTAssertTrue(arguments.contains("--intensity"))
    XCTAssertTrue(arguments.contains("destructive"))
    XCTAssertTrue(arguments.contains("--frames-per-effect"))
    XCTAssertTrue(arguments.contains("15"))
    XCTAssertTrue(arguments.contains("--granular-grain-size"))
    XCTAssertTrue(arguments.contains("48"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("cpu"))
    XCTAssertFalse(arguments.contains("--no-mp4"))
  }

  func testShowcaseArgumentsCanSkipMP4() throws {
    let request = ShowcaseRenderCommandRequest(
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputDirectoryURL: URL(fileURLWithPath: "/tmp/showcase-preview", isDirectory: true),
      intensity: .balanced,
      framesPerEffect: 2,
      frameRate: 12.0,
      granularGrainSize: 8,
      seed: 1,
      backend: .cpu,
      encodeMP4: false
    )

    let arguments = try RustBridgePlaceholder.renderShowcaseArguments(request: request)

    XCTAssertTrue(arguments.contains("--no-mp4"))
    XCTAssertTrue(arguments.contains("balanced"))
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

  func testQueuedFluidAdvectSequenceArgumentsIncludeProceduralControls() throws {
    let request = FluidAdvectSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/fluid-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/fluid", isDirectory: true),
      frames: 36,
      frameRate: 23.976,
      advect: 12.0,
      turbulenceScale: 0.008,
      turbulenceSpeed: 0.06,
      detail: 0.1,
      reinject: 0.05,
      seed: 42,
      backend: .metal,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddFluidAdvectSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-fluid-advect-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/output-root/fluid"))
    XCTAssertTrue(arguments.contains("--frames"))
    XCTAssertTrue(arguments.contains("36"))
    XCTAssertTrue(arguments.contains("--frame-rate"))
    XCTAssertTrue(arguments.contains("23.976"))
    XCTAssertTrue(arguments.contains("--advect"))
    XCTAssertTrue(arguments.contains("12"))
    XCTAssertTrue(arguments.contains("--turbulence-scale"))
    XCTAssertTrue(arguments.contains("0.008"))
    XCTAssertTrue(arguments.contains("--turbulence-speed"))
    XCTAssertTrue(arguments.contains("0.06"))
    XCTAssertTrue(arguments.contains("--detail"))
    XCTAssertTrue(arguments.contains("0.1"))
    XCTAssertTrue(arguments.contains("--reinject"))
    XCTAssertTrue(arguments.contains("0.05"))
    XCTAssertTrue(arguments.contains("--seed"))
    XCTAssertTrue(arguments.contains("42"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("metal"))
    XCTAssertTrue(arguments.contains("--project-path"))
  }

  func testQueuedFluidAdvectTwoSourceSequenceArgumentsIncludeSourcesAndBackend() throws {
    let request = FluidAdvectTwoSourceSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/fluid-two-source-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/fluid-ab", isDirectory: true),
      frames: 24,
      frameRate: 24.0,
      advect: 1.5,
      reinject: 0.08,
      backend: .cpu,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddFluidAdvectTwoSourceSequenceArguments(
      request: request
    )

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-fluid-advect-two-source-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-a-frames"))
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/output-root/fluid-ab"))
    XCTAssertTrue(arguments.contains("--frames"))
    XCTAssertTrue(arguments.contains("24"))
    XCTAssertTrue(arguments.contains("--advect"))
    XCTAssertTrue(arguments.contains("1.5"))
    XCTAssertTrue(arguments.contains("--reinject"))
    XCTAssertTrue(arguments.contains("0.08"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("cpu"))
  }

  func testQueuedOpticalFlowAdvectSequenceArgumentsUseSingleCarrierSource() throws {
    let request = OpticalFlowAdvectSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/self-fluid-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/self-fluid", isDirectory: true),
      frames: 12,
      frameRate: 30.0,
      advect: 0.75,
      reinject: 0.12,
      backend: .metal,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddOpticalFlowAdvectSequenceArguments(
      request: request
    )

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-optical-flow-advect-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/output-root/self-fluid"))
    XCTAssertTrue(arguments.contains("--frame-rate"))
    XCTAssertTrue(arguments.contains("30"))
    XCTAssertTrue(arguments.contains("--advect"))
    XCTAssertTrue(arguments.contains("0.75"))
    XCTAssertTrue(arguments.contains("--reinject"))
    XCTAssertTrue(arguments.contains("0.12"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("metal"))
  }

  func testQueuedFieldParticlesSequenceArgumentsIncludeParticleControls() throws {
    let request = FieldParticlesSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/particles-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/particles", isDirectory: true),
      frames: 48,
      frameRate: 60.0,
      spacing: 8,
      particleSize: 10,
      advect: 6.0,
      turbulenceScale: 0.012,
      turbulenceSpeed: 0.04,
      detail: 0.2,
      liveColour: true,
      seed: 9,
      backend: .metal,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddFieldParticlesSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-field-particles-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/output-root/particles"))
    XCTAssertTrue(arguments.contains("--spacing"))
    XCTAssertTrue(arguments.contains("8"))
    XCTAssertTrue(arguments.contains("--particle-size"))
    XCTAssertTrue(arguments.contains("10"))
    XCTAssertTrue(arguments.contains("--advect"))
    XCTAssertTrue(arguments.contains("6"))
    XCTAssertTrue(arguments.contains("--turbulence-scale"))
    XCTAssertTrue(arguments.contains("0.012"))
    XCTAssertTrue(arguments.contains("--turbulence-speed"))
    XCTAssertTrue(arguments.contains("0.04"))
    XCTAssertTrue(arguments.contains("--detail"))
    XCTAssertTrue(arguments.contains("0.2"))
    XCTAssertTrue(arguments.contains("--live-colour"))
    XCTAssertTrue(arguments.contains("--seed"))
    XCTAssertTrue(arguments.contains("9"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("metal"))
    XCTAssertTrue(arguments.contains("--project-path"))
  }

  func testQueuedCascadeTrailsSequenceArgumentsIncludeCascadeControls() throws {
    let request = CascadeTrailsSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cascade-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/cascade", isDirectory: true),
      frames: 144,
      frameRate: 24.0,
      tileSize: 28,
      gridSpacing: 60,
      advect: 1.6,
      turbulenceScale: 0.008,
      detail: 0.1,
      liveRefresh: true,
      seed: 7,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddCascadeTrailsSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-cascade-trails-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("--tile-size"))
    XCTAssertTrue(arguments.contains("28"))
    XCTAssertTrue(arguments.contains("--grid-spacing"))
    XCTAssertTrue(arguments.contains("60"))
    XCTAssertTrue(arguments.contains("--advect"))
    XCTAssertTrue(arguments.contains("1.6"))
    XCTAssertTrue(arguments.contains("--seed"))
    XCTAssertTrue(arguments.contains("7"))
    // Live refresh is on by default, so the disable flag must NOT be present.
    XCTAssertFalse(arguments.contains("--no-live-refresh"))
    XCTAssertTrue(arguments.contains("--project-path"))

    // Disabling live refresh appends the disable flag.
    let frozen = CascadeTrailsSequenceRenderQueueCommandRequest(
      queueURL: request.queueURL,
      sourceDirectoryURL: request.sourceDirectoryURL,
      outputRootDirectoryURL: request.outputRootDirectoryURL,
      frames: 144,
      frameRate: 24.0,
      tileSize: 28,
      gridSpacing: 60,
      advect: 1.6,
      turbulenceScale: 0.008,
      detail: 0.1,
      liveRefresh: false,
      seed: 7,
      projectURL: nil
    )
    let frozenArguments =
      try RustBridgePlaceholder.queueAddCascadeTrailsSequenceArguments(request: frozen)
    XCTAssertTrue(frozenArguments.contains("--no-live-refresh"))
  }

  func testQueuedCascadeTrailsSequenceArgumentsRejectInvalidValues() {
    let invalid = CascadeTrailsSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cascade-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/cascade", isDirectory: true),
      frames: 12,
      frameRate: 24.0,
      tileSize: 0,
      gridSpacing: 60,
      advect: 1.6,
      turbulenceScale: 0.008,
      detail: 0.1,
      liveRefresh: true,
      seed: 0,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddCascadeTrailsSequenceArguments(request: invalid)
    )
  }

  func testQueuedFluidAdvectionArgumentsRejectInvalidValues() {
    let invalidFrames = FluidAdvectSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/fluid-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/fluid", isDirectory: true),
      frames: 0,
      frameRate: 24.0,
      advect: 12.0,
      turbulenceScale: 0.008,
      turbulenceSpeed: 0.06,
      detail: 0.1,
      reinject: 0.05,
      seed: 0,
      backend: .cpu,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddFluidAdvectSequenceArguments(request: invalidFrames)
    )

    let invalidParticles = FieldParticlesSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/particles-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/particles", isDirectory: true),
      frames: 12,
      frameRate: 24.0,
      spacing: 0,
      particleSize: 8,
      advect: 6.0,
      turbulenceScale: 0.008,
      turbulenceSpeed: 0.06,
      detail: 0.1,
      liveColour: false,
      seed: 0,
      backend: .cpu,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddFieldParticlesSequenceArguments(request: invalidParticles)
    )
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

  func testQueuedVideoVocoderSequenceArgumentsIncludeModeAndAmount() throws {
    let request = VideoVocoderSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/vocoder-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      bands: 8,
      amount: 0.5,
      mode: .match,
      maxFrames: 12,
      frameRate: 24.0,
      backend: .metal,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddVideoVocoderSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-video-vocoder-sequence"]
    )
    XCTAssertEqual(Self.value(after: "--mode", in: arguments), "match")
    XCTAssertEqual(Self.value(after: "--bands", in: arguments), "8")
    XCTAssertEqual(Self.value(after: "--amount", in: arguments), "0.5")
    XCTAssertEqual(Self.value(after: "--backend", in: arguments), "metal")
    XCTAssertEqual(Self.value(after: "--max-frames", in: arguments), "12")
  }

  func testQueuedVideoVocoderSequenceArgumentsRejectMetalInGainMode() {
    let request = VideoVocoderSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/vocoder-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      bands: 8,
      amount: 1.0,
      mode: .gain,
      maxFrames: nil,
      frameRate: 24.0,
      backend: .metal,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddVideoVocoderSequenceArguments(request: request)
    )
  }

  func testQueuedSpectralCrossSynthArgumentsIncludeModeAndAnalysisKnobs() throws {
    let request = SpectralCrossSynthRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cross-synth-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      mode: .filter,
      amount: 0.75,
      filterType: .highpass,
      rmsWindow: 2048,
      rmsHop: 512,
      fftSize: 1024,
      stftHop: 256,
      window: .hamming,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddSpectralCrossSynthArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-spectral-cross-synth"]
    )
    XCTAssertEqual(arguments[7], "/tmp/cross-synth-queue.json")
    XCTAssertEqual(arguments[8], "/tmp/source-a.wav")
    XCTAssertEqual(arguments[9], "/tmp/source-b.wav")
    XCTAssertEqual(Self.value(after: "--mode", in: arguments), "filter")
    XCTAssertEqual(Self.value(after: "--amount", in: arguments), "0.75")
    XCTAssertEqual(Self.value(after: "--filter-type", in: arguments), "highpass")
    XCTAssertEqual(Self.value(after: "--rms-window", in: arguments), "2048")
    XCTAssertEqual(Self.value(after: "--rms-hop", in: arguments), "512")
    XCTAssertEqual(Self.value(after: "--fft-size", in: arguments), "1024")
    XCTAssertEqual(Self.value(after: "--stft-hop", in: arguments), "256")
    XCTAssertEqual(Self.value(after: "--window", in: arguments), "hamming")
  }

  func testQueuedSpectralCrossSynthArgumentsRejectInvalidValues() {
    let base = SpectralCrossSynthRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cross-synth-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      mode: .gain,
      amount: 1.5, // out of [0, 1]
      filterType: .lowpass,
      rmsWindow: 2048,
      rmsHop: 512,
      fftSize: 1000, // not a power of two
      stftHop: 256,
      window: .hann,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddSpectralCrossSynthArguments(request: base)
    )
  }

  func testQueuedVideoAudioRouteArgumentsIncludeDescriptorModeAmountAndFPS() throws {
    let request = VideoAudioRouteRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/video-audio-route-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      descriptor: .flow,
      mode: .filter,
      filterType: .highpass,
      sampling: .smooth,
      amount: 0.5,
      fps: 24.0,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddVideoAudioRouteArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-video-audio-route"]
    )
    XCTAssertEqual(arguments[7], "/tmp/video-audio-route-queue.json")
    XCTAssertEqual(arguments[8], "/tmp/source-a-frames")
    XCTAssertEqual(arguments[9], "/tmp/source-b.wav")
    XCTAssertEqual(Self.value(after: "--descriptor", in: arguments), "flow")
    XCTAssertEqual(Self.value(after: "--mode", in: arguments), "filter")
    XCTAssertEqual(Self.value(after: "--filter-type", in: arguments), "highpass")
    XCTAssertEqual(Self.value(after: "--sampling", in: arguments), "smooth")
    XCTAssertEqual(Self.value(after: "--amount", in: arguments), "0.5")
    XCTAssertEqual(Self.value(after: "--fps", in: arguments), "24")
  }

  func testQueuedVideoAudioRouteArgumentsRejectInvalidValues() {
    let base = VideoAudioRouteRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/video-audio-route-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      descriptor: .luma,
      mode: .gain,
      filterType: .lowpass,
      sampling: .hold,
      amount: 1.5, // out of [0, 1]
      fps: 0.0, // not greater than zero
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddVideoAudioRouteArguments(request: base)
    )
  }

  func testQueuedAudioImpulseConvolutionArgumentsIncludeAmountAndMaxSamples() throws {
    let request = AudioImpulseConvolutionRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/impulse-conv-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: 0.5,
      maxImpulseSamples: 4096,
      useFFT: false,
      resampleImpulse: false,
      usePerChannelIR: false,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddAudioImpulseConvolutionArguments(
      request: request
    )

    XCTAssertEqual(
      arguments.prefix(7),
      ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-audio-impulse-convolution"]
    )
    XCTAssertEqual(arguments[7], "/tmp/impulse-conv-queue.json")
    XCTAssertEqual(arguments[8], "/tmp/source-a.wav")
    XCTAssertEqual(arguments[9], "/tmp/source-b.wav")
    XCTAssertEqual(Self.value(after: "--amount", in: arguments), "0.5")
    XCTAssertEqual(Self.value(after: "--max-impulse-samples", in: arguments), "4096")
    // Direct, non-resampling defaults omit the HQ-tier flags.
    XCTAssertFalse(arguments.contains("--method"))
    XCTAssertFalse(arguments.contains("--resample-impulse"))
  }

  func testQueuedAudioImpulseConvolutionArgumentsIncludeFFTAndResample() throws {
    let request = AudioImpulseConvolutionRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/impulse-conv-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: 1.0,
      maxImpulseSamples: nil,
      useFFT: true,
      resampleImpulse: true,
      usePerChannelIR: false,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddAudioImpulseConvolutionArguments(
      request: request
    )

    XCTAssertEqual(Self.value(after: "--method", in: arguments), "fft")
    XCTAssertTrue(arguments.contains("--resample-impulse"))
    // Mono is the default; per-channel must not be emitted unless requested.
    XCTAssertFalse(arguments.contains("--ir-mode"))
  }

  func testQueuedAudioImpulseConvolutionArgumentsIncludePerChannelIR() throws {
    let request = AudioImpulseConvolutionRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/impulse-conv-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: 1.0,
      maxImpulseSamples: nil,
      useFFT: false,
      resampleImpulse: false,
      usePerChannelIR: true,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddAudioImpulseConvolutionArguments(
      request: request
    )

    XCTAssertEqual(Self.value(after: "--ir-mode", in: arguments), "per-channel")
  }

  func testQueuedAudioImpulseConvolutionArgumentsOmitMaxSamplesWhenNil() throws {
    let request = AudioImpulseConvolutionRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/impulse-conv-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: 1.0,
      maxImpulseSamples: nil,
      useFFT: false,
      resampleImpulse: false,
      usePerChannelIR: false,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddAudioImpulseConvolutionArguments(
      request: request
    )

    XCTAssertFalse(arguments.contains("--max-impulse-samples"))
  }

  func testQueuedAudioImpulseConvolutionArgumentsRejectInvalidValues() {
    let base = AudioImpulseConvolutionRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/impulse-conv-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: 1.5, // out of [0, 1]
      maxImpulseSamples: nil,
      useFFT: false,
      resampleImpulse: false,
      usePerChannelIR: false,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddAudioImpulseConvolutionArguments(request: base)
    )
  }

  func testQueuedAudioVideoRouteArgumentsIncludeShiftAndBackend() throws {
    let request = AudioVideoRouteSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/audio-route-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: 0.75,
      shiftX: 8,
      shiftY: -2,
      rmsWindow: 2048,
      rmsHop: 512,
      frameRate: 30,
      maxFrames: 48,
      backend: .metal,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddAudioVideoRouteSequenceArguments(
      request: request
    )

    XCTAssertEqual(
      arguments.prefix(7),
      [
        "cargo", "run", "--quiet", "-p", "morphogen-cli", "--",
        "queue-add-audio-video-route-sequence"
      ]
    )
    XCTAssertEqual(arguments[7], "/tmp/audio-route-queue.json")
    XCTAssertEqual(arguments[8], "/tmp/source-a.wav")
    XCTAssertEqual(arguments[9], "/tmp/source-b-frames")
    XCTAssertEqual(Self.value(after: "--amount", in: arguments), "0.75")
    XCTAssertEqual(Self.value(after: "--shift-x", in: arguments), "8")
    XCTAssertEqual(Self.value(after: "--shift-y", in: arguments), "-2")
    XCTAssertEqual(Self.value(after: "--rms-window", in: arguments), "2048")
    XCTAssertEqual(Self.value(after: "--rms-hop", in: arguments), "512")
    XCTAssertEqual(Self.value(after: "--frame-rate", in: arguments), "30")
    XCTAssertEqual(Self.value(after: "--backend", in: arguments), "metal")
    XCTAssertEqual(Self.value(after: "--max-frames", in: arguments), "48")
  }

  func testQueuedAudioVideoRouteArgumentsRejectInvalidValues() {
    let base = AudioVideoRouteSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/audio-route-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: -1, // negative amount
      shiftX: 8,
      shiftY: 0,
      rmsWindow: 0, // must be > 0
      rmsHop: 512,
      frameRate: 30,
      maxFrames: nil,
      backend: .cpu,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddAudioVideoRouteSequenceArguments(request: base)
    )
  }

  func testQueuedDatamoshArgumentsIncludeKeyframeIntervalAndBackend() throws {
    let request = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/datamosh-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      keyframeInterval: 4,
      amount: 0.75,
      blockSize: 16,
      residualGain: 0.5,
      residualDecay: 0.8,
      blockRefreshThreshold: 1.5,
      vectorRemix: .shuffle,
      preset: .vectorShuffle,
      remixSeed: 42,
      maxFrames: 48,
      backend: .metal,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(7),
      [
        "cargo", "run", "--quiet", "-p", "morphogen-cli", "--",
        "queue-add-datamosh-sequence"
      ]
    )
    XCTAssertEqual(arguments[7], "/tmp/datamosh-queue.json")
    XCTAssertEqual(arguments[8], "/tmp/source-a-frames")
    XCTAssertEqual(arguments[9], "/tmp/source-b-frames")
    XCTAssertEqual(Self.value(after: "--keyframe-interval", in: arguments), "4")
    XCTAssertEqual(Self.value(after: "--amount", in: arguments), "0.75")
    XCTAssertEqual(Self.value(after: "--block-size", in: arguments), "16")
    XCTAssertEqual(Self.value(after: "--residual-gain", in: arguments), "0.5")
    XCTAssertEqual(Self.value(after: "--residual-decay", in: arguments), "0.8")
    XCTAssertEqual(Self.value(after: "--block-refresh-threshold", in: arguments), "1.5")
    XCTAssertEqual(Self.value(after: "--vector-remix", in: arguments), "shuffle")
    XCTAssertEqual(Self.value(after: "--preset", in: arguments), "vector-shuffle")
    XCTAssertEqual(Self.value(after: "--remix-seed", in: arguments), "42")
    XCTAssertEqual(Self.value(after: "--backend", in: arguments), "metal")
    XCTAssertEqual(Self.value(after: "--max-frames", in: arguments), "48")
  }

  func testQueuedDatamoshArgumentsRejectInvalidAmount() {
    let base = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/datamosh-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      keyframeInterval: 0,
      amount: -1, // negative amount
      blockSize: 1,
      residualGain: 0,
      residualDecay: 0.9,
      blockRefreshThreshold: 0,
      vectorRemix: .none,
      preset: .custom,
      remixSeed: 0,
      maxFrames: nil,
      backend: .cpu,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: base)
    )
  }

  func testQueuedDatamoshArgumentsRejectInvalidBlockSize() {
    let base = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/datamosh-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      keyframeInterval: 0,
      amount: 1,
      blockSize: 0, // sub-1 macroblock size
      residualGain: 0,
      residualDecay: 0.9,
      blockRefreshThreshold: 0,
      vectorRemix: .none,
      preset: .custom,
      remixSeed: 0,
      maxFrames: nil,
      backend: .cpu,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: base)
    )
  }

  func testQueuedDatamoshArgumentsRejectInvalidResidualGain() {
    let base = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/datamosh-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      keyframeInterval: 0,
      amount: 1,
      blockSize: 16,
      residualGain: -0.5, // negative residual gain
      residualDecay: 0.9,
      blockRefreshThreshold: 0,
      vectorRemix: .none,
      preset: .custom,
      remixSeed: 0,
      maxFrames: nil,
      backend: .cpu,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: base)
    )
  }

  func testQueuedDatamoshArgumentsRejectInvalidBlockRefreshThreshold() {
    let base = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/datamosh-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      keyframeInterval: 0,
      amount: 1,
      blockSize: 16,
      residualGain: 0,
      residualDecay: 0.9,
      blockRefreshThreshold: -0.5, // negative refresh threshold
      vectorRemix: .none,
      preset: .custom,
      remixSeed: 0,
      maxFrames: nil,
      backend: .cpu,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: base)
    )
  }

  func testQueuedConvolutionalBlendArgumentsIncludeKernelAndBackend() throws {
    let request = ConvolutionalBlendSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/conv-blend-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      kernelSize: 5,
      amount: 0.75,
      useColorKernels: false,
      maxFrames: 24,
      backend: .metal,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddConvolutionalBlendSequenceArguments(
      request: request
    )

    XCTAssertEqual(
      arguments.prefix(7),
      [
        "cargo", "run", "--quiet", "-p", "morphogen-cli", "--",
        "queue-add-convolutional-blend-sequence"
      ]
    )
    XCTAssertEqual(arguments[7], "/tmp/conv-blend-queue.json")
    XCTAssertEqual(arguments[8], "/tmp/source-a-frames")
    XCTAssertEqual(arguments[9], "/tmp/source-b-frames")
    XCTAssertEqual(Self.value(after: "--kernel-size", in: arguments), "5")
    XCTAssertEqual(Self.value(after: "--amount", in: arguments), "0.75")
    XCTAssertEqual(Self.value(after: "--backend", in: arguments), "metal")
    XCTAssertEqual(Self.value(after: "--max-frames", in: arguments), "24")
    // Luma is the default; no kernel-mode flag is emitted.
    XCTAssertFalse(arguments.contains("--kernel-mode"))
  }

  func testQueuedConvolutionalBlendArgumentsIncludeColourMode() throws {
    let request = ConvolutionalBlendSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/conv-blend-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      kernelSize: 3,
      amount: 1.0,
      useColorKernels: true,
      maxFrames: nil,
      backend: .cpu,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddConvolutionalBlendSequenceArguments(
      request: request
    )

    XCTAssertEqual(Self.value(after: "--kernel-mode", in: arguments), "color")
  }

  func testQueuedConvolutionalBlendArgumentsRejectEvenKernel() {
    let base = ConvolutionalBlendSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/conv-blend-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      kernelSize: 4, // even -> not centerable
      amount: 1.0,
      useColorKernels: false,
      maxFrames: nil,
      backend: .cpu,
      projectURL: nil
    )

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddConvolutionalBlendSequenceArguments(request: base)
    )
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
      textureWeight: 0.0625,
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

    XCTAssertEqual(Self.value(after: "--texture-weight", in: arguments), "0.0625")
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
