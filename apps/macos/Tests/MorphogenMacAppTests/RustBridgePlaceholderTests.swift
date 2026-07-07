import Foundation
@testable import MorphogenMacApp
import XCTest

final class RustBridgePlaceholderTests: XCTestCase {
  func testQueuedCompositionArgumentsIncludeSpecOutputAndProject() throws {
    let request = CompositionRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/composition-queue.json"),
      specURL: URL(fileURLWithPath: "/tmp/piece.json"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/comp-output", isDirectory: true),
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = RustBridgePlaceholder.queueAddCompositionArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(9),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-composition", "/tmp/composition-queue.json"]
    )
    // Order matters: spec then output root are positional after the queue path.
    XCTAssertEqual(arguments[9], "/tmp/piece.json")
    XCTAssertEqual(arguments[10], "/tmp/comp-output")
    XCTAssertTrue(arguments.contains("--project-path"))
    XCTAssertTrue(arguments.contains("/tmp/project.morphogen.json"))
    // No top-level input directory: sources are per-scene inside the spec.
  }

  func testQueuedCoagulatedBlendArgumentsIncludeKnobsAndModulation() throws {
    let request = CoagulatedBlendSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/coag-queue.json"),
      sourceADirectoryURL: URL(fileURLWithPath: "/tmp/A", isDirectory: true),
      sourceBDirectoryURL: URL(fileURLWithPath: "/tmp/B", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/coag-out", isDirectory: true),
      frameRate: 24.0,
      patchSize: 16,
      colorWeight: 1.0,
      textureWeight: 0.0,
      coherencePasses: 2,
      coherenceStrength: 0.5,
      randomness: 0.0,
      coagulationStrength: 0.0,
      edgeHardness: 0.6,
      edgeDither: 0.0,
      blockJitter: 0.0,
      bias: 1.0,
      seed: 7,
      advectSource: .mixed,
      advectAmount: 0.0,
      refresh: 1.0,
      turbulence: 1.0,
      smear: 0.0,
      smearDecay: 0.9,
      backend: .cpu,
      maxFrames: 30,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json"),
      modulationRoutes: [
        ModulationRouteSpec(target: "coagulation_strength", source: "audio-rms", scale: 30, offset: 0, sampling: nil, modulator: nil)
      ],
      modulatorAudioURL: URL(fileURLWithPath: "/tmp/score.wav")
    )

    let arguments = try RustBridgePlaceholder.queueAddCoagulatedBlendSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(9),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-coagulated-blend-sequence", "/tmp/coag-queue.json"]
    )
    XCTAssertEqual(arguments[9], "/tmp/A")
    XCTAssertEqual(arguments[10], "/tmp/B")
    XCTAssertEqual(arguments[11], "/tmp/coag-out")
    XCTAssertTrue(arguments.contains("--advect-source"))
    XCTAssertTrue(arguments.contains("mixed"))
    XCTAssertTrue(arguments.contains("--bias"))
    XCTAssertTrue(arguments.contains("--max-frames"))
    XCTAssertTrue(arguments.contains("30"))
    XCTAssertTrue(arguments.contains("--project-path"))
    // Modulation route + its media flag.
    XCTAssertTrue(arguments.contains("--modulate"))
    XCTAssertTrue(arguments.contains("coagulation_strength=audio-rms:30,0"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertTrue(arguments.contains("/tmp/score.wav"))
  }

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

    XCTAssertEqual(arguments.prefix(8), ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-frame-sequence"])
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

    XCTAssertEqual(arguments.prefix(8), ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "render-showcase"])
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

    XCTAssertEqual(arguments.prefix(8), ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-feedback-sequence"])
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-fluid-advect-sequence"]
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-fluid-advect-two-source-sequence"]
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-optical-flow-advect-sequence"]
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-field-particles-sequence"]
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

  func testQueuedFieldParticlesSequenceArgumentsIncludeModulationRoute() throws {
    var request = FieldParticlesSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/particles-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/particles", isDirectory: true),
      frames: 24,
      frameRate: 24.0,
      spacing: 8,
      particleSize: 8,
      advect: 6.0,
      turbulenceScale: 0.01,
      turbulenceSpeed: 0.1,
      detail: 0.5,
      liveColour: false,
      seed: 0,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "advect", source: "audio-rms", scale: 48, offset: 0, sampling: nil, modulator: nil)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/score.wav")

    let arguments = try RustBridgePlaceholder.queueAddFieldParticlesSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("--modulate"))
    XCTAssertTrue(arguments.contains("advect=audio-rms:48,0"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertTrue(arguments.contains("/tmp/score.wav"))
    XCTAssertFalse(arguments.contains("--project-path"))
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
      field: "vortex",
      riverDirection: 0,
      riverSpeed: 0,
      riverTurbulence: 0,
      temporalTiles: false,
      decay: 0,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddCascadeTrailsSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-cascade-trails-sequence"]
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
      field: "vortex",
      riverDirection: 0,
      riverSpeed: 0,
      riverTurbulence: 0,
      temporalTiles: false,
      decay: 0,
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
      field: "vortex",
      riverDirection: 0,
      riverSpeed: 0,
      riverTurbulence: 0,
      temporalTiles: false,
      decay: 0,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddCascadeTrailsSequenceArguments(request: invalid)
    )
  }

  func testQueuedCascadeCollageSequenceArgumentsIncludeCascadeControls() throws {
    let request = CascadeCollageSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cascade-collage-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/cascade-collage", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      scribAmpScale: 1.0,
      edgeStrength: 0.85,
      faceStrength: 0.55,
      edgeDetect: 1.2,
      tileScale: 1.0,
      detailTiles: 4,
      hueRotate: 0.0,
      blockBlend: .screen,
      blockOpacity: 0.8,
      seed: 71,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddCascadeCollageSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-cascade-collage-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("--edge-detect"))
    XCTAssertTrue(arguments.contains("1.2"))
    XCTAssertTrue(arguments.contains("--block-blend"))
    XCTAssertTrue(arguments.contains("screen"))
    XCTAssertTrue(arguments.contains("--block-opacity"))
    XCTAssertTrue(arguments.contains("0.8"))
    XCTAssertTrue(arguments.contains("--detail-tiles"))
    XCTAssertTrue(arguments.contains("4"))
    XCTAssertTrue(arguments.contains("--seed"))
    XCTAssertTrue(arguments.contains("71"))
    XCTAssertTrue(arguments.contains("--project-path"))
  }

  func testQueuedCascadeCollageSequenceArgumentsRejectInvalidValues() {
    let invalid = CascadeCollageSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cascade-collage-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/cascade-collage", isDirectory: true),
      frames: 0,
      frameRate: 24.0,
      scribAmpScale: 1.0,
      edgeStrength: 0.85,
      faceStrength: 0.55,
      edgeDetect: 0.0,
      tileScale: 1.0,
      detailTiles: 4,
      hueRotate: 0.0,
      blockBlend: .normal,
      blockOpacity: 1.0,
      seed: 0,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddCascadeCollageSequenceArguments(request: invalid)
    )
  }

  func testQueuedCascadeTrailsSequenceArgumentsIncludeModulationRoute() throws {
    var request = CascadeTrailsSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cascade-trails-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/trail-cascade", isDirectory: true),
      frames: 48,
      frameRate: 24.0,
      tileSize: 28,
      gridSpacing: 60,
      advect: 1.6,
      turbulenceScale: 0.008,
      detail: 0.1,
      liveRefresh: true,
      seed: 0,
      field: "vortex",
      riverDirection: 0,
      riverSpeed: 3,
      riverTurbulence: 0.8,
      temporalTiles: false,
      decay: 0,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "advect", source: "audio-rms", scale: 48, offset: 0)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/score.wav")

    let arguments = try RustBridgePlaceholder.queueAddCascadeTrailsSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("--modulate"))
    XCTAssertTrue(arguments.contains("advect=audio-rms:48,0"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertTrue(arguments.contains("/tmp/score.wav"))
    XCTAssertFalse(arguments.contains("--project-path"))
  }

  func testQueuedCascadeCollageSequenceArgumentsIncludeModulationRoute() throws {
    var request = CascadeCollageSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cascade-collage-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/cascade-collage", isDirectory: true),
      frames: 48,
      frameRate: 24.0,
      scribAmpScale: 1.0,
      edgeStrength: 0.85,
      faceStrength: 0.55,
      edgeDetect: 0.0,
      tileScale: 1.0,
      detailTiles: 4,
      hueRotate: 0.0,
      blockBlend: .normal,
      blockOpacity: 1.0,
      seed: 71,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "edge_strength", source: "audio-rms", scale: 1, offset: 0)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/score.wav")

    let arguments = try RustBridgePlaceholder.queueAddCascadeCollageSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("--modulate"))
    XCTAssertTrue(arguments.contains("edge_strength=audio-rms:1,0"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertTrue(arguments.contains("/tmp/score.wav"))
    XCTAssertFalse(arguments.contains("--project-path"))
  }

  func testQueuedFluidMosaicSequenceArgumentsIncludeModulationRoute() throws {
    var request = FluidMosaicSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/fluid-mosaic-queue.json"),
      sourceADirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      sourceBDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputDirectoryURL: URL(fileURLWithPath: "/tmp/output/fluid-mosaic", isDirectory: true),
      tileSize: 8,
      colorBins: 5,
      cohesion: 0.035,
      repulsion: 1.4,
      fluidStrength: 0.5,
      damping: 0.88,
      settleIterations: 60,
      jitter: 0.03,
      turbulence: 0.0,
      frames: 120
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "cohesion", source: "audio-rms", scale: 1, offset: 0)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/score.wav")

    let arguments = try RustBridgePlaceholder.queueAddFluidMosaicSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("queue-add-fluid-mosaic-sequence"))
    XCTAssertTrue(arguments.contains("--modulate"))
    XCTAssertTrue(arguments.contains("cohesion=audio-rms:1,0"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertTrue(arguments.contains("/tmp/score.wav"))
    XCTAssertTrue(arguments.contains("--frames"))
    XCTAssertTrue(arguments.contains("120"))
  }

  func testQueuedRetroStaticSequenceArgumentsIncludeGlitchControls() throws {
    let request = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/retro-static-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/retro-static", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      realBpp: 4,
      assumedBpp: 3,
      filter: .paeth,
      strength: 1.0,
      backend: .metal,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddRetroStaticSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-retro-static-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("--real-bpp"))
    XCTAssertTrue(arguments.contains("4"))
    XCTAssertTrue(arguments.contains("--assumed-bpp"))
    XCTAssertTrue(arguments.contains("3"))
    XCTAssertTrue(arguments.contains("--filter"))
    XCTAssertTrue(arguments.contains("paeth"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("metal"))
    XCTAssertTrue(arguments.contains("--project-path"))
  }

  func testQueuedRetroStaticSequenceArgumentsCarryModulationRoutes() throws {
    var request = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/retro-static-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/retro-static", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      realBpp: 4,
      assumedBpp: 3,
      filter: .paeth,
      strength: 1.0,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "strength", source: "audio-rms", scale: 0.9, offset: 0.05)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    request.modulationSampling = .smooth

    let arguments = try RustBridgePlaceholder.queueAddRetroStaticSequenceArguments(request: request)

    guard let modulateIndex = arguments.firstIndex(of: "--modulate") else {
      return XCTFail("expected a --modulate flag")
    }
    XCTAssertEqual(arguments[modulateIndex + 1], "strength=audio-rms:0.9,0.05")
    guard let audioIndex = arguments.firstIndex(of: "--modulator-audio") else {
      return XCTFail("expected a --modulator-audio flag")
    }
    XCTAssertEqual(arguments[audioIndex + 1], "/tmp/modulator.wav")
    guard let samplingIndex = arguments.firstIndex(of: "--modulation-sampling") else {
      return XCTFail("expected a --modulation-sampling flag")
    }
    XCTAssertEqual(arguments[samplingIndex + 1], "smooth")
    XCTAssertFalse(arguments.contains("--modulator-frames"))
  }

  func testQueuedRetroStaticSequenceArgumentsOmitModulationFlagsWithoutRoutes() throws {
    let request = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/retro-static-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/retro-static", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      realBpp: 4,
      assumedBpp: 3,
      filter: .paeth,
      strength: 1.0,
      backend: .cpu,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddRetroStaticSequenceArguments(request: request)

    XCTAssertFalse(arguments.contains("--modulate"))
    XCTAssertFalse(arguments.contains("--modulator-audio"))
    XCTAssertFalse(arguments.contains("--modulator-frames"))
    XCTAssertFalse(arguments.contains("--modulation-sampling"))
  }

  func testQueuedRetroStaticSequenceArgumentsCarryPerRouteSmoothOverride() throws {
    var request = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/retro-static-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/retro-static", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      realBpp: 4,
      assumedBpp: 3,
      filter: .paeth,
      strength: 1.0,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(
        target: "strength", source: "audio-rms", scale: 0.9, offset: 0.05, sampling: .smooth
      )
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    // Panel-level default stays Hold; only the route overrides to Smooth.
    request.modulationSampling = .hold

    let arguments = try RustBridgePlaceholder.queueAddRetroStaticSequenceArguments(request: request)

    guard let modulateIndex = arguments.firstIndex(of: "--modulate") else {
      return XCTFail("expected a --modulate flag")
    }
    XCTAssertEqual(arguments[modulateIndex + 1], "strength=audio-rms:0.9,0.05@smooth")
    guard let samplingIndex = arguments.firstIndex(of: "--modulation-sampling") else {
      return XCTFail("expected the shared --modulation-sampling flag to remain present")
    }
    XCTAssertEqual(arguments[samplingIndex + 1], "hold")
  }

  func testQueuedRetroStaticSequenceArgumentsCarryPerRouteHoldOverride() throws {
    var request = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/retro-static-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/retro-static", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      realBpp: 4,
      assumedBpp: 3,
      filter: .paeth,
      strength: 1.0,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(
        target: "strength", source: "audio-rms", scale: 0.9, offset: 0.05, sampling: .hold
      )
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    // Panel-level default stays Smooth; only the route overrides to Hold.
    request.modulationSampling = .smooth

    let arguments = try RustBridgePlaceholder.queueAddRetroStaticSequenceArguments(request: request)

    guard let modulateIndex = arguments.firstIndex(of: "--modulate") else {
      return XCTFail("expected a --modulate flag")
    }
    XCTAssertEqual(arguments[modulateIndex + 1], "strength=audio-rms:0.9,0.05@hold")
    guard let samplingIndex = arguments.firstIndex(of: "--modulation-sampling") else {
      return XCTFail("expected the shared --modulation-sampling flag to remain present")
    }
    XCTAssertEqual(arguments[samplingIndex + 1], "smooth")
  }

  func testQueuedRetroStaticSequenceArgumentsOmitPerRouteSuffixWithoutOverride() throws {
    // Regression guard for the byte-identical invariant: a route with no
    // per-route override (`sampling == nil`) must emit the spec exactly as
    // before this slice — no `@hold`/`@smooth` suffix.
    var request = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/retro-static-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/retro-static", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      realBpp: 4,
      assumedBpp: 3,
      filter: .paeth,
      strength: 1.0,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "strength", source: "audio-rms", scale: 0.9, offset: 0.05)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    request.modulationSampling = .smooth

    let arguments = try RustBridgePlaceholder.queueAddRetroStaticSequenceArguments(request: request)

    guard let modulateIndex = arguments.firstIndex(of: "--modulate") else {
      return XCTFail("expected a --modulate flag")
    }
    XCTAssertEqual(arguments[modulateIndex + 1], "strength=audio-rms:0.9,0.05")
  }

  func testQueuedFeedbackSequenceArgumentsCarryModulationRoutes() throws {
    var request = FeedbackSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/feedback-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/feedback", isDirectory: true),
      carrierAmount: 1.0,
      feedbackAmount: 1.5,
      feedbackMix: 0.68,
      decay: 0.99,
      iterations: 1,
      structureMix: 0.0,
      outputBitDepth: .png16,
      temporalSupersampling: 1,
      maxFrames: 48,
      resetAtFrame: nil,
      frameRate: 24.0,
      writesFlowCache: false,
      backend: .cpu,
      flowSource: .opticalFlow,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "feedback_mix", source: "audio-rms", scale: 0.5, offset: 0.25)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")

    let arguments = try RustBridgePlaceholder.queueAddFeedbackSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("feedback_mix=audio-rms:0.5,0.25"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertFalse(arguments.contains("--modulator-frames"))

    // No routes ⇒ no modulation flags (the exact pre-slice command shape).
    request.modulationRoutes = []
    let unmodulated = try RustBridgePlaceholder.queueAddFeedbackSequenceArguments(request: request)
    XCTAssertFalse(unmodulated.contains("--modulate"))
    XCTAssertFalse(unmodulated.contains("--modulation-sampling"))
  }

  func testQueuedDatamoshSequenceArgumentsCarryModulationRoutes() throws {
    var request = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/datamosh-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/datamosh", isDirectory: true),
      keyframeInterval: 0,
      amount: 1.0,
      blockSize: 16,
      residualGain: 0.5,
      residualDecay: 0.8,
      blockRefreshThreshold: 0.0,
      vectorRemix: .none,
      preset: .custom,
      remixSeed: 0,
      maxFrames: 48,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "amount", source: "audio-onset", scale: 2, offset: 0.5),
      ModulationRouteSpec(target: "residual_gain", source: "luma", scale: 1, offset: 0),
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    request.modulatorFramesURL = URL(fileURLWithPath: "/tmp/modulator-frames", isDirectory: true)

    let arguments = try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("amount=audio-onset:2,0.5"))
    XCTAssertTrue(arguments.contains("residual_gain=luma:1,0"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertTrue(arguments.contains("--modulator-frames"))

    // A luma route without the modulator frame directory is rejected app-side.
    request.modulatorFramesURL = nil
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: request)
    )
  }

  func testQueuedFluidAdvectSequenceArgumentsCarryModulationRoutes() throws {
    var request = FluidAdvectSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/fluid-advect-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/fluid", isDirectory: true),
      frames: 96,
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
    request.modulationRoutes = [
      ModulationRouteSpec(target: "reinject", source: "audio-rms", scale: 0.5, offset: 0.25)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")

    let arguments = try RustBridgePlaceholder.queueAddFluidAdvectSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("reinject=audio-rms:0.5,0.25"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))

    // The two-source / self-flow builders share the same emission path.
    var motionRequest = OpticalFlowAdvectSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/optical-flow-advect-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/self-flow", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      advect: 1.0,
      reinject: 0.08,
      backend: .cpu,
      projectURL: nil
    )
    motionRequest.modulationRoutes = [
      ModulationRouteSpec(target: "advect", source: "flow", scale: 4, offset: 0)
    ]
    motionRequest.modulatorFramesURL = URL(fileURLWithPath: "/tmp/modulator-frames", isDirectory: true)
    let motionArguments =
      try RustBridgePlaceholder.queueAddOpticalFlowAdvectSequenceArguments(request: motionRequest)
    XCTAssertTrue(motionArguments.contains("advect=flow:4,0"))
    XCTAssertTrue(motionArguments.contains("--modulator-frames"))
  }

  func testQueuedPixelSortSequenceArgumentsCarryModulationRoutesAndRequireMedia() throws {
    var request = PixelSortSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/pixel-sort-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/pixel-sort", isDirectory: true),
      axis: .row,
      key: .luma,
      direction: .asc,
      thresholdLow: 0.25,
      thresholdHigh: 0.8,
      maxSpan: 0,
      maskSource: .selfMask,
      flowRadius: 4,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "threshold_low", source: "audio-onset", scale: 0.5, offset: 0.2),
      ModulationRouteSpec(target: "threshold_high", source: "flow", scale: 1, offset: 0),
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    request.modulatorFramesURL = URL(fileURLWithPath: "/tmp/modulator-frames", isDirectory: true)

    let arguments = try RustBridgePlaceholder.queueAddPixelSortSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("threshold_low=audio-onset:0.5,0.2"))
    XCTAssertTrue(arguments.contains("threshold_high=flow:1,0"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertTrue(arguments.contains("--modulator-frames"))

    // A flow route without the modulator frame directory is rejected app-side.
    request.modulatorFramesURL = nil
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddPixelSortSequenceArguments(request: request)
    )
  }

  func testQueuedChannelShiftSequenceArgumentsIncludeShiftControls() throws {
    let request = ChannelShiftSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/channel-shift-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/channel-shift", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      shiftRX: 8,
      shiftRY: 0,
      shiftGX: 0,
      shiftGY: 0,
      shiftBX: -6,
      shiftBY: 0,
      sourceADirectoryURL: nil,
      flowGain: 0,
      flowRadius: 4,
      backend: .metal,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddChannelShiftSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-channel-shift-sequence"]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("--shift-r-x=8"))
    // Negative shifts ride in `--flag=value` form so clap does not read them as flags.
    XCTAssertTrue(arguments.contains("--shift-b-x=-6"))
    XCTAssertTrue(arguments.contains("--shift-g-y=0"))
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("metal"))
    XCTAssertTrue(arguments.contains("--project-path"))
    // Constant mode carries no flow-driven flags.
    XCTAssertFalse(arguments.contains("--source-a-dir"))
    XCTAssertFalse(arguments.contains { $0.hasPrefix("--flow-gain") })
    XCTAssertFalse(arguments.contains("--radius"))
    XCTAssertFalse(arguments.contains("--modulate"))
    XCTAssertFalse(arguments.contains("--modulation-sampling"))
  }

  func testQueuedChannelShiftSequenceArgumentsCarryFlowModeAndModulationRoutes() throws {
    var request = ChannelShiftSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/channel-shift-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/channel-shift", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      shiftRX: 0,
      shiftRY: 0,
      shiftGX: 0,
      shiftGY: 0,
      shiftBX: 0,
      shiftBY: 0,
      sourceADirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      flowGain: 3,
      flowRadius: 5,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "shift_r_x", source: "luma", scale: 12, offset: 0),
      ModulationRouteSpec(target: "shift_b_y", source: "audio-rms", scale: -8, offset: 2),
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    request.modulatorFramesURL = URL(fileURLWithPath: "/tmp/modulator-frames", isDirectory: true)
    request.modulationSampling = .smooth

    let arguments = try RustBridgePlaceholder.queueAddChannelShiftSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("--source-a-dir"))
    XCTAssertTrue(arguments.contains("/tmp/source-a-frames"))
    XCTAssertTrue(arguments.contains("--flow-gain=3"))
    guard let radiusIndex = arguments.firstIndex(of: "--radius") else {
      return XCTFail("expected a --radius flag")
    }
    XCTAssertEqual(arguments[radiusIndex + 1], "5")
    XCTAssertTrue(arguments.contains("shift_r_x=luma:12,0"))
    XCTAssertTrue(arguments.contains("shift_b_y=audio-rms:-8,2"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    XCTAssertTrue(arguments.contains("--modulator-frames"))
    guard let samplingIndex = arguments.firstIndex(of: "--modulation-sampling") else {
      return XCTFail("expected a --modulation-sampling flag")
    }
    XCTAssertEqual(arguments[samplingIndex + 1], "smooth")
  }

  func testQueuedChannelShiftSequenceArgumentsRejectInvalidFlowModes() {
    let base = ChannelShiftSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/channel-shift-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/channel-shift", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      shiftRX: 0,
      shiftRY: 0,
      shiftGX: 0,
      shiftGY: 0,
      shiftBX: 0,
      shiftBY: 0,
      sourceADirectoryURL: nil,
      flowGain: 3,
      flowRadius: 4,
      backend: .cpu,
      projectURL: nil
    )
    // Flow-driven mode without Source A frames is rejected app-side.
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddChannelShiftSequenceArguments(request: base)
    )

    // Flow-driven mode on the Metal backend is rejected app-side (CPU-only path).
    let metalFlow = ChannelShiftSequenceRenderQueueCommandRequest(
      queueURL: base.queueURL,
      carrierDirectoryURL: base.carrierDirectoryURL,
      outputRootDirectoryURL: base.outputRootDirectoryURL,
      frames: 96,
      frameRate: 24.0,
      shiftRX: 0,
      shiftRY: 0,
      shiftGX: 0,
      shiftGY: 0,
      shiftBX: 0,
      shiftBY: 0,
      sourceADirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      flowGain: 3,
      flowRadius: 4,
      backend: .metal,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddChannelShiftSequenceArguments(request: metalFlow)
    )
  }

  func testQueuedChannelShiftSequenceArgumentsCarryNamedModulators() throws {
    var request = ChannelShiftSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/channel-shift-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/channel-shift", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      shiftRX: 0,
      shiftRY: 0,
      shiftGX: 0,
      shiftGY: 0,
      shiftBX: 0,
      shiftBY: 0,
      sourceADirectoryURL: nil,
      flowGain: 0,
      flowRadius: 4,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      // A named-audio route, a named-frames route, and a default-media route.
      ModulationRouteSpec(target: "shift_r_x", source: "audio-rms", scale: 8, offset: 0, modulator: "bass"),
      ModulationRouteSpec(target: "shift_g_y", source: "luma", scale: 12, offset: 0, modulator: "cam"),
      ModulationRouteSpec(target: "shift_b_y", source: "audio-onset", scale: -8, offset: 2),
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/default-modulator.wav")
    request.namedModulators = [
      NamedModulatorMediaSpec(
        name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass.wav"), framesURL: nil),
      NamedModulatorMediaSpec(
        name: "cam", audioURL: nil,
        framesURL: URL(fileURLWithPath: "/tmp/cam-frames", isDirectory: true)),
      // Declared but unreferenced — must NOT emit a flag (and must not error on nil media).
      NamedModulatorMediaSpec(name: "unused", audioURL: nil, framesURL: nil),
    ]
    request.modulationSampling = .hold

    let arguments = try RustBridgePlaceholder.queueAddChannelShiftSequenceArguments(request: request)

    // Named routes gain the `name.` prefix; the default route stays bare.
    XCTAssertTrue(arguments.contains("shift_r_x=bass.audio-rms:8,0"))
    XCTAssertTrue(arguments.contains("shift_g_y=cam.luma:12,0"))
    XCTAssertTrue(arguments.contains("shift_b_y=audio-onset:-8,2"))

    // The default audio route still emits the default `--modulator-audio`.
    guard let audioIndex = arguments.firstIndex(of: "--modulator-audio") else {
      return XCTFail("expected the default --modulator-audio flag")
    }
    XCTAssertEqual(arguments[audioIndex + 1], "/tmp/default-modulator.wav")
    // No default frames flag — no unnamed luma/flow route uses it.
    XCTAssertFalse(arguments.contains("--modulator-frames"))

    // Referenced named modulators emit `name=path` value tokens.
    guard let namedAudioIndex = arguments.firstIndex(of: "--named-modulator-audio") else {
      return XCTFail("expected a --named-modulator-audio flag")
    }
    XCTAssertEqual(arguments[namedAudioIndex + 1], "bass=/tmp/bass.wav")
    guard let namedFramesIndex = arguments.firstIndex(of: "--named-modulator-frames") else {
      return XCTFail("expected a --named-modulator-frames flag")
    }
    XCTAssertEqual(arguments[namedFramesIndex + 1], "cam=/tmp/cam-frames")

    // The unreferenced modulator contributes nothing.
    XCTAssertFalse(arguments.contains(where: { $0.hasPrefix("unused=") }))
  }

  func testQueuedChannelShiftSequenceArgumentsRejectNamedModulatorMissingMedia() {
    var request = ChannelShiftSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/channel-shift-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/channel-shift", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      shiftRX: 0,
      shiftRY: 0,
      shiftGX: 0,
      shiftGY: 0,
      shiftBX: 0,
      shiftBY: 0,
      sourceADirectoryURL: nil,
      flowGain: 0,
      flowRadius: 4,
      backend: .cpu,
      projectURL: nil
    )
    // Route reads bass.audio-rms but the declared "bass" carries no WAV.
    request.modulationRoutes = [
      ModulationRouteSpec(target: "shift_r_x", source: "audio-rms", scale: 8, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(name: "bass", audioURL: nil, framesURL: nil),
    ]

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddChannelShiftSequenceArguments(request: request)
    )
  }

  func testQueuedChannelShiftSequenceArgumentsRejectDuplicateNamedModulator() {
    var request = ChannelShiftSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/channel-shift-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/channel-shift", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      shiftRX: 0,
      shiftRY: 0,
      shiftGX: 0,
      shiftGY: 0,
      shiftBX: 0,
      shiftBY: 0,
      sourceADirectoryURL: nil,
      flowGain: 0,
      flowRadius: 4,
      backend: .cpu,
      projectURL: nil
    )
    // Two declared entries share the routed name "bass" (a UI rename collision);
    // emitting both would produce duplicate --named-modulator-audio flags the
    // CLI rejects, so the bridge refuses first with its own error.
    request.modulationRoutes = [
      ModulationRouteSpec(target: "shift_r_x", source: "audio-rms", scale: 8, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(
        name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass-a.wav"), framesURL: nil),
      NamedModulatorMediaSpec(
        name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass-b.wav"), framesURL: nil),
    ]

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddChannelShiftSequenceArguments(request: request)
    )

    // An unreferenced duplicate stays harmless — nothing is emitted for it.
    request.modulationRoutes = [
      ModulationRouteSpec(target: "shift_r_x", source: "audio-rms", scale: 8, offset: 0),
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/default-modulator.wav")
    XCTAssertNoThrow(
      try RustBridgePlaceholder.queueAddChannelShiftSequenceArguments(request: request)
    )
  }

  // Named-modulator threading across the swept panels. The `--named-modulator-*`
  // emission itself is exercised in depth by the channel-shift tests; each panel
  // test below only proves its own queue-add arg builder forwards
  // `request.namedModulators` into the shared append (a prefixed route spec plus
  // its `name=path` value token).

  func testQueuedFeedbackSequenceArgumentsCarryNamedModulators() throws {
    var request = FeedbackSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/feedback-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      carrierAmount: 1.0, feedbackAmount: 1.0, feedbackMix: 0.5, decay: 0.9,
      iterations: 1, structureMix: 0.0, outputBitDepth: .png8, temporalSupersampling: 1,
      maxFrames: nil, resetAtFrame: nil, frameRate: 24.0, writesFlowCache: false,
      backend: .cpu, flowSource: .opticalFlow, projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "feedback_amount", source: "audio-rms", scale: 2, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass.wav"), framesURL: nil),
    ]

    let arguments = try RustBridgePlaceholder.queueAddFeedbackSequenceArguments(request: request)
    XCTAssertTrue(arguments.contains("feedback_amount=bass.audio-rms:2,0"))
    guard let index = arguments.firstIndex(of: "--named-modulator-audio") else {
      return XCTFail("expected a --named-modulator-audio flag")
    }
    XCTAssertEqual(arguments[index + 1], "bass=/tmp/bass.wav")
  }

  func testQueuedFluidAdvectSequenceArgumentsCarryNamedModulators() throws {
    var request = FluidAdvectSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/fluid-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/fluid", isDirectory: true),
      frames: 36, frameRate: 24.0, advect: 12.0, turbulenceScale: 0.008,
      turbulenceSpeed: 0.06, detail: 0.1, reinject: 0.05, seed: 42, backend: .cpu, projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "advect", source: "audio-rms", scale: 8, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass.wav"), framesURL: nil),
    ]

    let arguments = try RustBridgePlaceholder.queueAddFluidAdvectSequenceArguments(request: request)
    XCTAssertTrue(arguments.contains("advect=bass.audio-rms:8,0"))
    guard let index = arguments.firstIndex(of: "--named-modulator-audio") else {
      return XCTFail("expected a --named-modulator-audio flag")
    }
    XCTAssertEqual(arguments[index + 1], "bass=/tmp/bass.wav")
  }

  func testQueuedRetroStaticSequenceArgumentsCarryNamedModulators() throws {
    var request = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/retro-static-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/retro-static", isDirectory: true),
      frames: 96, frameRate: 24.0, realBpp: 4, assumedBpp: 3, filter: .paeth, strength: 1.0,
      backend: .cpu, projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "strength", source: "audio-rms", scale: 1, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass.wav"), framesURL: nil),
    ]

    let arguments = try RustBridgePlaceholder.queueAddRetroStaticSequenceArguments(request: request)
    XCTAssertTrue(arguments.contains("strength=bass.audio-rms:1,0"))
    guard let index = arguments.firstIndex(of: "--named-modulator-audio") else {
      return XCTFail("expected a --named-modulator-audio flag")
    }
    XCTAssertEqual(arguments[index + 1], "bass=/tmp/bass.wav")
  }

  func testQueuedDatamoshSequenceArgumentsCarryNamedModulators() throws {
    var request = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/datamosh-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      keyframeInterval: 4, amount: 0.75, blockSize: 16, residualGain: 0.5, residualDecay: 0.8,
      blockRefreshThreshold: 1.5, vectorRemix: .none, preset: .custom, remixSeed: 0,
      maxFrames: nil, backend: .cpu, projectURL: nil, flowCacheDirectoryURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "amount", source: "audio-rms", scale: 1, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass.wav"), framesURL: nil),
    ]

    let arguments = try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: request)
    XCTAssertTrue(arguments.contains("amount=bass.audio-rms:1,0"))
    guard let index = arguments.firstIndex(of: "--named-modulator-audio") else {
      return XCTFail("expected a --named-modulator-audio flag")
    }
    XCTAssertEqual(arguments[index + 1], "bass=/tmp/bass.wav")
  }

  func testQueuedPixelSortSequenceArgumentsCarryNamedModulators() throws {
    var request = PixelSortSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/pixel-sort-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/pixel-sort", isDirectory: true),
      axis: .row, key: .luma, direction: .asc, thresholdLow: 0.25, thresholdHigh: 0.8,
      maxSpan: 0, maskSource: .selfMask, flowRadius: 4, backend: .cpu, projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "threshold_low", source: "audio-rms", scale: 0.5, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass.wav"), framesURL: nil),
    ]

    let arguments = try RustBridgePlaceholder.queueAddPixelSortSequenceArguments(request: request)
    XCTAssertTrue(arguments.contains("threshold_low=bass.audio-rms:0.5,0"))
    guard let index = arguments.firstIndex(of: "--named-modulator-audio") else {
      return XCTFail("expected a --named-modulator-audio flag")
    }
    XCTAssertEqual(arguments[index + 1], "bass=/tmp/bass.wav")
  }

  func testQueuedPaletteQuantizeSequenceArgumentsCarryNamedModulators() throws {
    var request = PaletteQuantizeSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/palette-quantize-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/palette-quantize", isDirectory: true),
      frames: 96, frameRate: 24.0, mode: .posterize, levels: 8, backend: .cpu, projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "levels", source: "audio-rms", scale: 8, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass.wav"), framesURL: nil),
    ]

    let arguments = try RustBridgePlaceholder.queueAddPaletteQuantizeSequenceArguments(request: request)
    XCTAssertTrue(arguments.contains("levels=bass.audio-rms:8,0"))
    guard let index = arguments.firstIndex(of: "--named-modulator-audio") else {
      return XCTFail("expected a --named-modulator-audio flag")
    }
    XCTAssertEqual(arguments[index + 1], "bass=/tmp/bass.wav")
  }

  func testQueuedPaletteQuantizeSequenceArgumentsIncludeModeAndLevels() throws {
    let request = PaletteQuantizeSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/palette-quantize-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(
        fileURLWithPath: "/tmp/output-root/palette-quantize", isDirectory: true
      ),
      frames: 96,
      frameRate: 24.0,
      mode: .posterize,
      levels: 8,
      backend: .metal,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddPaletteQuantizeSequenceArguments(
      request: request
    )

    XCTAssertEqual(
      arguments.prefix(8),
      [
        "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
        "queue-add-palette-quantize-sequence"
      ]
    )
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    guard let modeIndex = arguments.firstIndex(of: "--mode") else {
      return XCTFail("expected a --mode flag")
    }
    XCTAssertEqual(arguments[modeIndex + 1], "posterize")
    guard let levelsIndex = arguments.firstIndex(of: "--levels") else {
      return XCTFail("expected a --levels flag")
    }
    XCTAssertEqual(arguments[levelsIndex + 1], "8")
    XCTAssertTrue(arguments.contains("--backend"))
    XCTAssertTrue(arguments.contains("metal"))
    XCTAssertTrue(arguments.contains("--project-path"))
    // No active mod slot ⇒ the exact unmodulated CLI path.
    XCTAssertFalse(arguments.contains("--modulate"))
    XCTAssertFalse(arguments.contains("--modulation-sampling"))
  }

  func testQueuedPaletteQuantizeSequenceArgumentsCarryModulationRoutes() throws {
    var request = PaletteQuantizeSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/palette-quantize-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(
        fileURLWithPath: "/tmp/output-root/palette-quantize", isDirectory: true
      ),
      frames: 96,
      frameRate: 24.0,
      mode: .palette,
      levels: 256,
      backend: .cpu,
      projectURL: nil
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "levels", source: "audio-rms", scale: -254, offset: 256)
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    request.modulationSampling = .smooth

    let arguments = try RustBridgePlaceholder.queueAddPaletteQuantizeSequenceArguments(
      request: request
    )

    guard let modeIndex = arguments.firstIndex(of: "--mode") else {
      return XCTFail("expected a --mode flag")
    }
    XCTAssertEqual(arguments[modeIndex + 1], "palette")
    XCTAssertTrue(arguments.contains("levels=audio-rms:-254,256"))
    XCTAssertTrue(arguments.contains("--modulator-audio"))
    guard let samplingIndex = arguments.firstIndex(of: "--modulation-sampling") else {
      return XCTFail("expected a --modulation-sampling flag")
    }
    XCTAssertEqual(arguments[samplingIndex + 1], "smooth")
  }

  func testQueuedPaletteQuantizeSequenceArgumentsRejectInvalidLevels() {
    let invalid = PaletteQuantizeSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/palette-quantize-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(
        fileURLWithPath: "/tmp/output-root/palette-quantize", isDirectory: true
      ),
      frames: 96,
      frameRate: 24.0,
      mode: .posterize,
      levels: 1,
      backend: .cpu,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddPaletteQuantizeSequenceArguments(request: invalid)
    )

    // Palette mode ignores levels, so an out-of-range value is not rejected there.
    let paletteMode = PaletteQuantizeSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/palette-quantize-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(
        fileURLWithPath: "/tmp/output-root/palette-quantize", isDirectory: true
      ),
      frames: 96,
      frameRate: 24.0,
      mode: .palette,
      levels: 1,
      backend: .cpu,
      projectURL: nil
    )
    XCTAssertNoThrow(
      try RustBridgePlaceholder.queueAddPaletteQuantizeSequenceArguments(request: paletteMode)
    )
  }

  private func makeRuttEtraRequest() -> RuttEtraSequenceRenderQueueCommandRequest {
    RuttEtraSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/rutt-etra-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/rutt-etra", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      linePitch: 8,
      displacementDepth: 48.0,
      lineThickness: 1,
      mono: false,
      projectURL: nil
    )
  }

  func testQueuedRuttEtraSequenceArgumentsIncludeKnobs() throws {
    let request = RuttEtraSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/rutt-etra-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/rutt-etra", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      linePitch: 4,
      displacementDepth: -64.0,
      lineThickness: 2,
      mono: true,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)

    // Pin the exact unmodulated token sequence (default backend = cpu).
    XCTAssertEqual(
      arguments,
      [
        "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
        "queue-add-rutt-etra-sequence",
        "/tmp/rutt-etra-queue.json",
        "/tmp/source-b-frames",
        "/tmp/output-root/rutt-etra",
        "--frames", "96",
        "--frame-rate", "24",
        "--line-pitch", "4",
        "--displacement-depth=-64",
        "--line-thickness", "2",
        "--mono",
        "--backend", "cpu",
        "--project-path", "/tmp/project.morphogen.json"
      ]
    )
    // No active mod slot ⇒ the exact unmodulated CLI path.
    XCTAssertFalse(arguments.contains("--modulate"))
    XCTAssertFalse(arguments.contains("--modulation-sampling"))
    XCTAssertTrue(arguments.contains("--backend"))
  }

  func testQueuedRuttEtraSequenceArgumentsPinCpuBackendTokens() throws {
    // Pin: default backend emits `--backend cpu`.
    var request = makeRuttEtraRequest()
    request.backend = .cpu
    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)
    let backendIdx = try XCTUnwrap(arguments.firstIndex(of: "--backend"))
    XCTAssertEqual(arguments[backendIdx + 1], "cpu")
  }

  func testQueuedRuttEtraSequenceArgumentsPinMetalBackendTokens() throws {
    // Pin: Metal backend emits `--backend metal`.
    var request = makeRuttEtraRequest()
    request.backend = .metal
    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)
    let backendIdx = try XCTUnwrap(arguments.firstIndex(of: "--backend"))
    XCTAssertEqual(arguments[backendIdx + 1], "metal")
  }

  func testQueuedRuttEtraSequenceOmitsSourceAWhenSingleSource() throws {
    // Single-source (default nil Source A) must not emit `--source-a-dir`.
    let request = makeRuttEtraRequest()
    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)
    XCTAssertFalse(arguments.contains("--source-a-dir"))
  }

  func testQueuedRuttEtraSequenceEmitsSourceADirWhenTwoSource() throws {
    // Two-source: `--source-a-dir <path>` is emitted with the modulator directory.
    var request = makeRuttEtraRequest()
    request.sourceADirectoryURL = URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true)
    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)
    let idx = try XCTUnwrap(arguments.firstIndex(of: "--source-a-dir"))
    XCTAssertEqual(arguments[idx + 1], "/tmp/source-a-frames")
    // Source B (the carrier positional) is still present and distinct.
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
  }

  func testDownscaleFramesArgumentsPinTokenSequence() throws {
    // Preview-loop milestone Slice 3: the exact downscale token sequence.
    let arguments = try RustBridgePlaceholder.downscaleFramesArguments(
      inputDirectoryURL: URL(fileURLWithPath: "/tmp/proxy-b-frames", isDirectory: true),
      outputDirectoryURL: URL(fileURLWithPath: "/tmp/preview/downscaled-carrier", isDirectory: true),
      scale: 4,
      maxFrames: 48
    )
    XCTAssertEqual(
      arguments,
      [
        "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
        "downscale-frames",
        "/tmp/proxy-b-frames",
        "/tmp/preview/downscaled-carrier",
        "--scale", "4",
        "--max-frames", "48"
      ]
    )

    // Without a frame cap the flag is omitted entirely.
    let uncapped = try RustBridgePlaceholder.downscaleFramesArguments(
      inputDirectoryURL: URL(fileURLWithPath: "/tmp/proxy-b-frames", isDirectory: true),
      outputDirectoryURL: URL(fileURLWithPath: "/tmp/preview/downscaled-carrier", isDirectory: true),
      scale: 2,
      maxFrames: nil
    )
    XCTAssertFalse(uncapped.contains("--max-frames"))
  }

  func testDownscaleFramesArgumentsRejectIdentityAndInvalidValues() {
    // Scale 1 never reaches the bridge — the preview flow skips the
    // downscale entirely (identity anchor) — so the builder treats it as a
    // programmer error rather than emitting a wasted render.
    for scale in [1, 0, -2] {
      XCTAssertThrowsError(
        try RustBridgePlaceholder.downscaleFramesArguments(
          inputDirectoryURL: URL(fileURLWithPath: "/tmp/in"),
          outputDirectoryURL: URL(fileURLWithPath: "/tmp/out"),
          scale: scale,
          maxFrames: nil
        )
      )
    }
    XCTAssertThrowsError(
      try RustBridgePlaceholder.downscaleFramesArguments(
        inputDirectoryURL: URL(fileURLWithPath: "/tmp/in"),
        outputDirectoryURL: URL(fileURLWithPath: "/tmp/out"),
        scale: 4,
        maxFrames: 0
      )
    )
  }

  func testRuttEtraArgumentAssemblyUnchangedApartFromInputPath() throws {
    // The same-engine invariant made visible (preview-loop milestone): a
    // preview reroutes ONLY the input directory; every other token of the
    // effect render's argument assembly is byte-identical.
    let fullRes = makeRuttEtraRequest()  // carrier /tmp/source-b-frames
    let preview = RuttEtraSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/rutt-etra-queue.json"),
      carrierDirectoryURL: URL(
        fileURLWithPath: "/tmp/preview/downscaled-carrier", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/rutt-etra", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      linePitch: 8,
      displacementDepth: 48.0,
      lineThickness: 1,
      mono: false,
      projectURL: nil
    )

    let fullArguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(
      request: fullRes)
    let previewArguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(
      request: preview)

    XCTAssertEqual(fullArguments.count, previewArguments.count)
    let differingTokens = zip(fullArguments, previewArguments).filter { $0 != $1 }
    XCTAssertEqual(differingTokens.map { $0.0 }, ["/tmp/source-b-frames"])
    XCTAssertEqual(differingTokens.map { $0.1 }, ["/tmp/preview/downscaled-carrier"])
  }

  func testQueuedRuttEtraSequenceArgumentsCarryModulationRoutes() throws {
    var request = makeRuttEtraRequest()
    request.modulationRoutes = [
      ModulationRouteSpec(target: "displacement_depth", source: "audio-rms", scale: 96, offset: -16),
      ModulationRouteSpec(
        target: "line_pitch", source: "luma", scale: 8, offset: 4,
        sampling: ModulationSamplingOption.smooth
      ),
    ]
    request.modulatorAudioURL = URL(fileURLWithPath: "/tmp/modulator.wav")
    request.modulatorFramesURL = URL(fileURLWithPath: "/tmp/modulator-frames", isDirectory: true)
    request.modulationSampling = .smooth

    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("displacement_depth=audio-rms:96,-16"))
    // Per-route sampling override rides the route spec as an @suffix.
    XCTAssertTrue(arguments.contains("line_pitch=luma:8,4@smooth"))
    guard let audioIndex = arguments.firstIndex(of: "--modulator-audio") else {
      return XCTFail("expected a --modulator-audio flag")
    }
    XCTAssertEqual(arguments[audioIndex + 1], "/tmp/modulator.wav")
    guard let framesIndex = arguments.firstIndex(of: "--modulator-frames") else {
      return XCTFail("expected a --modulator-frames flag")
    }
    XCTAssertEqual(arguments[framesIndex + 1], "/tmp/modulator-frames")
    guard let samplingIndex = arguments.firstIndex(of: "--modulation-sampling") else {
      return XCTFail("expected a --modulation-sampling flag")
    }
    XCTAssertEqual(arguments[samplingIndex + 1], "smooth")
  }

  func testQueuedRuttEtraSequenceArgumentsCarryNamedModulators() throws {
    var request = makeRuttEtraRequest()
    request.modulationRoutes = [
      ModulationRouteSpec(
        target: "displacement_depth", source: "audio-rms", scale: 96, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(
        name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass.wav"), framesURL: nil),
    ]

    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)
    XCTAssertTrue(arguments.contains("displacement_depth=bass.audio-rms:96,0"))
    guard let index = arguments.firstIndex(of: "--named-modulator-audio") else {
      return XCTFail("expected a --named-modulator-audio flag")
    }
    XCTAssertEqual(arguments[index + 1], "bass=/tmp/bass.wav")
  }

  func testQueuedRuttEtraSequenceArgumentsRejectDuplicateNamedModulator() {
    var request = makeRuttEtraRequest()
    // Two declared entries share the routed name "bass" (a UI rename collision);
    // emitting both would produce duplicate --named-modulator-audio flags the
    // CLI rejects, so the bridge refuses first with its own error.
    request.modulationRoutes = [
      ModulationRouteSpec(
        target: "displacement_depth", source: "audio-rms", scale: 96, offset: 0, modulator: "bass"),
    ]
    request.namedModulators = [
      NamedModulatorMediaSpec(
        name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass-a.wav"), framesURL: nil),
      NamedModulatorMediaSpec(
        name: "bass", audioURL: URL(fileURLWithPath: "/tmp/bass-b.wav"), framesURL: nil),
    ]

    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)
    )
  }

  func testQueuedRuttEtraSequenceArgumentsCarryLfoRouteWithoutMediaFlags() throws {
    var request = makeRuttEtraRequest()
    // A pure-LFO route set needs no modulator media at all — the point of the
    // milestone (docs/LFO_MODULATION_MILESTONE.md, criterion 9).
    request.modulationRoutes = [
      ModulationRouteSpec(
        target: "displacement_depth", source: "lfo(sine,0.5,0.25)", scale: 64, offset: -16),
    ]

    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("--modulate"))
    XCTAssertTrue(arguments.contains("displacement_depth=lfo(sine,0.5,0.25):64,-16"))
    XCTAssertFalse(arguments.contains("--modulator-audio"))
    XCTAssertFalse(arguments.contains("--modulator-frames"))
    XCTAssertFalse(arguments.contains("--named-modulator-audio"))
    XCTAssertFalse(arguments.contains("--named-modulator-frames"))
  }

  func testQueuedRuttEtraSequenceArgumentsCarryLfoAndMediaRoutesCoexisting() throws {
    var request = makeRuttEtraRequest()
    request.modulationRoutes = [
      ModulationRouteSpec(
        target: "displacement_depth", source: "lfo(saw,2,0)", scale: 96, offset: 0),
      ModulationRouteSpec(target: "line_pitch", source: "luma", scale: 8, offset: 4),
    ]
    request.modulatorFramesURL = URL(fileURLWithPath: "/tmp/modulator-frames", isDirectory: true)

    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("displacement_depth=lfo(saw,2,0):96,0"))
    XCTAssertTrue(arguments.contains("line_pitch=luma:8,4"))
    // Media flags cover the luma route only; the LFO route demands nothing.
    guard let framesIndex = arguments.firstIndex(of: "--modulator-frames") else {
      return XCTFail("expected a --modulator-frames flag for the luma route")
    }
    XCTAssertEqual(arguments[framesIndex + 1], "/tmp/modulator-frames")
    XCTAssertFalse(arguments.contains("--modulator-audio"))
  }

  func testQueuedRuttEtraSequenceArgumentsCarryCapturedRouteWithoutMediaFlags() throws {
    var request = makeRuttEtraRequest()
    // A captured take emits an inline breakpoints(...) clause — no media, no
    // modulator name (docs/PERFORMANCE_CAPTURE_MILESTONE.md anchor 1: this is
    // string-identical to a hand-written breakpoints route of the same knots,
    // so the offline render byte-matches by construction).
    let spec = try XCTUnwrap(
      capturedSourceSpec([
        GestureKnot(t: 0.0, v: 0.0),
        GestureKnot(t: 2.0, v: 1.0),
      ])
    )
    request.modulationRoutes = [
      ModulationRouteSpec(target: "displacement_depth", source: spec, scale: 96, offset: 0)
    ]

    let arguments = try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: request)

    XCTAssertTrue(arguments.contains("--modulate"))
    XCTAssertTrue(arguments.contains("displacement_depth=breakpoints(0:0;2:1):96,0"))
    XCTAssertFalse(arguments.contains("--modulator-audio"))
    XCTAssertFalse(arguments.contains("--modulator-frames"))
    XCTAssertFalse(arguments.contains("--named-modulator-audio"))
    XCTAssertFalse(arguments.contains("--named-modulator-frames"))
  }

  func testLfoSourceSpecFormatsAndValidates() {
    // Valid params spell the exact route-grammar clause.
    XCTAssertEqual(
      lfoSourceSpec(shape: .saw, rate: 0.5, phase: 0.25), "lfo(saw,0.5,0.25)")
    XCTAssertEqual(lfoSourceSpec(shape: .sine, rate: 1.0, phase: 0.0), "lfo(sine,1,0)")
    XCTAssertEqual(
      lfoSourceSpec(shape: .triangle, rate: 2.0, phase: 0.5), "lfo(triangle,2,0.5)")
    // Invalid rate/phase mirror the CLI parse rules (rate finite > 0,
    // phase finite) — rejected app-side before any launch.
    XCTAssertNil(lfoSourceSpec(shape: .sine, rate: 0, phase: 0))
    XCTAssertNil(lfoSourceSpec(shape: .sine, rate: -1, phase: 0))
    XCTAssertNil(lfoSourceSpec(shape: .sine, rate: .infinity, phase: 0))
    XCTAssertNil(lfoSourceSpec(shape: .sine, rate: 1, phase: .nan))
  }

  func testQueuedRuttEtraSequenceArgumentsRejectInvalidKnobs() {
    let invalidPitch = RuttEtraSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/rutt-etra-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/rutt-etra", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      linePitch: 0,
      displacementDepth: 48.0,
      lineThickness: 1,
      mono: false,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: invalidPitch)
    )

    let invalidDepth = RuttEtraSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/rutt-etra-queue.json"),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/rutt-etra", isDirectory: true),
      frames: 96,
      frameRate: 24.0,
      linePitch: 8,
      displacementDepth: .infinity,
      lineThickness: 1,
      mono: false,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddRuttEtraSequenceArguments(request: invalidDepth)
    )
  }

  func testQueuedRetroStaticSequenceArgumentsRejectInvalidValues() {
    let invalid = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/retro-static-queue.json"),
      sourceDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root/retro-static", isDirectory: true),
      frames: 0,
      frameRate: 24.0,
      realBpp: 4,
      assumedBpp: 3,
      filter: .none,
      strength: 1.0,
      backend: .cpu,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddRetroStaticSequenceArguments(request: invalid)
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-granular-mosaic-pool-sequence"]
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-video-vocoder-sequence"]
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-spectral-cross-synth"]
    )
    XCTAssertEqual(arguments[8], "/tmp/cross-synth-queue.json")
    XCTAssertEqual(arguments[9], "/tmp/source-a.wav")
    XCTAssertEqual(arguments[10], "/tmp/source-b.wav")
    XCTAssertEqual(Self.value(after: "--mode", in: arguments), "filter")
    XCTAssertEqual(Self.value(after: "--amount", in: arguments), "0.75")
    XCTAssertEqual(Self.value(after: "--filter-type", in: arguments), "highpass")
    XCTAssertEqual(Self.value(after: "--rms-window", in: arguments), "2048")
    XCTAssertEqual(Self.value(after: "--rms-hop", in: arguments), "512")
    XCTAssertEqual(Self.value(after: "--fft-size", in: arguments), "1024")
    XCTAssertEqual(Self.value(after: "--stft-hop", in: arguments), "256")
    XCTAssertEqual(Self.value(after: "--window", in: arguments), "hamming")
    // Non-vocode modes never emit the vocode-only knob — the pre-vocode arg
    // array shape, byte for byte.
    XCTAssertFalse(arguments.contains("--vocode-bands"))
  }

  func testQueuedSpectralCrossSynthVocodeArgumentsCarryBands() throws {
    let request = SpectralCrossSynthRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cross-synth-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      mode: .vocode,
      amount: 1.0,
      filterType: .lowpass,
      rmsWindow: 2048,
      rmsHop: 512,
      fftSize: 1024,
      stftHop: 256,
      window: .hann,
      vocodeBands: 24,
      projectURL: nil
    )

    let arguments = try RustBridgePlaceholder.queueAddSpectralCrossSynthArguments(request: request)

    XCTAssertEqual(Self.value(after: "--mode", in: arguments), "vocode")
    XCTAssertEqual(Self.value(after: "--vocode-bands", in: arguments), "24")
  }

  func testQueuedSpectralCrossSynthVocodeArgumentsRejectInvalidBandsAndHop() {
    let base = SpectralCrossSynthRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cross-synth-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      mode: .vocode,
      amount: 1.0,
      filterType: .lowpass,
      rmsWindow: 2048,
      rmsHop: 512,
      fftSize: 1024,
      stftHop: 256,
      window: .hann,
      vocodeBands: 513, // > fftSize / 2
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddSpectralCrossSynthArguments(request: base)
    )

    let badHop = SpectralCrossSynthRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/cross-synth-queue.json"),
      modulatorWAVURL: URL(fileURLWithPath: "/tmp/source-a.wav"),
      carrierWAVURL: URL(fileURLWithPath: "/tmp/source-b.wav"),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      mode: .vocode,
      amount: 1.0,
      filterType: .lowpass,
      rmsWindow: 2048,
      rmsHop: 512,
      fftSize: 1024,
      stftHop: 1024, // > fftSize / 2 — fine for gain/filter, invalid for vocode
      window: .hann,
      vocodeBands: 32,
      projectURL: nil
    )
    XCTAssertThrowsError(
      try RustBridgePlaceholder.queueAddSpectralCrossSynthArguments(request: badHop)
    )
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-video-audio-route"]
    )
    XCTAssertEqual(arguments[8], "/tmp/video-audio-route-queue.json")
    XCTAssertEqual(arguments[9], "/tmp/source-a-frames")
    XCTAssertEqual(arguments[10], "/tmp/source-b.wav")
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
      arguments.prefix(8),
      ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "queue-add-audio-impulse-convolution"]
    )
    XCTAssertEqual(arguments[8], "/tmp/impulse-conv-queue.json")
    XCTAssertEqual(arguments[9], "/tmp/source-a.wav")
    XCTAssertEqual(arguments[10], "/tmp/source-b.wav")
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
      arguments.prefix(8),
      [
        "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
        "queue-add-audio-video-route-sequence"
      ]
    )
    XCTAssertEqual(arguments[8], "/tmp/audio-route-queue.json")
    XCTAssertEqual(arguments[9], "/tmp/source-a.wav")
    XCTAssertEqual(arguments[10], "/tmp/source-b-frames")
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
      projectURL: nil,
      flowCacheDirectoryURL: URL(fileURLWithPath: "/tmp/datamosh-flow-cache", isDirectory: true)
    )

    let arguments = try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: request)

    XCTAssertEqual(
      arguments.prefix(8),
      [
        "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
        "queue-add-datamosh-sequence"
      ]
    )
    XCTAssertEqual(arguments[8], "/tmp/datamosh-queue.json")
    XCTAssertEqual(arguments[9], "/tmp/source-a-frames")
    XCTAssertEqual(arguments[10], "/tmp/source-b-frames")
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
    XCTAssertEqual(Self.value(after: "--flow-cache-dir", in: arguments), "/tmp/datamosh-flow-cache")
  }

  func testQueuedDatamoshArgumentsOmitFlowCacheWhenUnset() throws {
    let request = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/datamosh-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      keyframeInterval: 0,
      amount: 1.0,
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

    let arguments = try RustBridgePlaceholder.queueAddDatamoshSequenceArguments(request: request)
    XCTAssertFalse(arguments.contains("--flow-cache-dir"))
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
      arguments.prefix(8),
      [
        "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
        "queue-add-convolutional-blend-sequence"
      ]
    )
    XCTAssertEqual(arguments[8], "/tmp/conv-blend-queue.json")
    XCTAssertEqual(arguments[9], "/tmp/source-a-frames")
    XCTAssertEqual(arguments[10], "/tmp/source-b-frames")
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

    XCTAssertEqual(arguments.frameExtraction.prefix(8), ["cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--", "extract-frames"])
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

final class EnumModulationMappingTests: XCTestCase {
  func testFromToMappingSpansPartialAndReversedRanges() {
    // Full sweep over the 5-variant filter: scale N−1, offset 0.
    var mapping = enumModulationMapping(
      from: RetroStaticFilterOption.none, to: RetroStaticFilterOption.paeth
    )
    XCTAssertEqual(mapping.scale, 4)
    XCTAssertEqual(mapping.offset, 0)

    // Reversed sweep: negative scale, offset at the high end.
    mapping = enumModulationMapping(
      from: RetroStaticFilterOption.paeth, to: RetroStaticFilterOption.none
    )
    XCTAssertEqual(mapping.scale, -4)
    XCTAssertEqual(mapping.offset, 4)

    // Partial range: sub (1) → average (3).
    mapping = enumModulationMapping(
      from: RetroStaticFilterOption.sub, to: RetroStaticFilterOption.average
    )
    XCTAssertEqual(mapping.scale, 2)
    XCTAssertEqual(mapping.offset, 1)

    // From == To is the continuity identity: scale 0 holds the variant.
    mapping = enumModulationMapping(
      from: PixelSortDirectionOption.desc, to: PixelSortDirectionOption.desc
    )
    XCTAssertEqual(mapping.scale, 0)
    XCTAssertEqual(mapping.offset, 1)

    // Two-variant knobs: the full sweep is scale 1, offset 0.
    mapping = enumModulationMapping(
      from: PixelSortAxisOption.row, to: PixelSortAxisOption.col
    )
    XCTAssertEqual(mapping.scale, 1)
    XCTAssertEqual(mapping.offset, 0)

    mapping = enumModulationMapping(
      from: PaletteQuantizeModeOption.posterize, to: PaletteQuantizeModeOption.palette
    )
    XCTAssertEqual(mapping.scale, 1)
    XCTAssertEqual(mapping.offset, 0)
  }

  func testOptionEnumCaseOrderMatchesTheContractVariantTables() {
    // The mapping indexes `allCases`, so each option enum's declared order
    // must mirror the engine's contract variant order (milestone table).
    XCTAssertEqual(
      RetroStaticFilterOption.allCases.map(\.cliValue),
      ["none", "sub", "up", "average", "paeth"]
    )
    XCTAssertEqual(PixelSortDirectionOption.allCases.map(\.cliValue), ["asc", "desc"])
    XCTAssertEqual(PixelSortAxisOption.allCases.map(\.cliValue), ["row", "col"])
    XCTAssertEqual(
      PaletteQuantizeModeOption.allCases.map(\.cliValue),
      ["posterize", "palette"]
    )
  }
}
