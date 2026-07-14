import Dispatch
import Foundation

enum RustBridgePlaceholder {
  static let intendedBridgeOptions = [
    "C ABI/staticlib for a narrow stable engine boundary",
    "UniFFI once the Rust API shape settles",
    "Swift calling the local CLI during early development",
    "Later direct engine binding for render jobs and preview"
  ]

  static func currentStatus() -> String {
    "Rust is not directly linked into the SwiftUI shell yet. The dev bridge invokes morphogen-cli."
  }

  static func defaultRenderOutputURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("morphogen-test.png")
  }

  static func defaultRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("morphogen-render-queue.json")
  }

  static func defaultRenderQueueOutputRootURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-render-output",
      isDirectory: true
    )
  }

  static func defaultFrameSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-frame-sequence-queue.json"
    )
  }

  static func defaultFeedbackSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-feedback-sequence-queue.json"
    )
  }

  static func defaultFluidAdvectSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-fluid-advect-sequence-queue.json"
    )
  }

  static func defaultFluidAdvectTwoSourceSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-fluid-advect-two-source-sequence-queue.json"
    )
  }

  static func defaultOpticalFlowAdvectSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-optical-flow-advect-sequence-queue.json"
    )
  }

  static func defaultFieldParticlesSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-field-particles-sequence-queue.json"
    )
  }

  static func defaultCascadeTrailsSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-cascade-trails-sequence-queue.json"
    )
  }

  static func defaultCascadeCollageSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-cascade-collage-sequence-queue.json"
    )
  }

  static func defaultRetroStaticSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-retro-static-sequence-queue.json"
    )
  }

  static func defaultGranularMosaicPoolSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-granular-pool-sequence-queue.json"
    )
  }

  static func defaultVideoVocoderSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-video-vocoder-sequence-queue.json"
    )
  }

  static func defaultSpectralCrossSynthRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-spectral-cross-synth-queue.json"
    )
  }

  static func defaultAudioImpulseConvolutionRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-audio-impulse-convolution-queue.json"
    )
  }

  static func defaultAudioVideoRouteSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-audio-video-route-sequence-queue.json"
    )
  }

  static func defaultDatamoshSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-datamosh-sequence-queue.json"
    )
  }

  static func defaultBitstreamDatamoshRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-bitstream-datamosh-queue.json"
    )
  }

  static func defaultShowcasePreviewOutputURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-showcase-preview",
      isDirectory: true
    )
  }

  static func defaultEffectPreviewOutputRootURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-effect-preview",
      isDirectory: true
    )
  }

  /// Durable default location for full effect renders so a render can run without
  /// the user first picking an output folder. Renders are deliverables, so this
  /// lives under ~/Movies (or the home directory if Movies is unavailable),
  /// never the temp directory the OS may reclaim.
  static func defaultFrameSequenceOutputRootURL() -> URL {
    let base = FileManager.default.urls(for: .moviesDirectory, in: .userDomainMask).first
      ?? FileManager.default.homeDirectoryForCurrentUser
    return base.appendingPathComponent("Morphogen Renders", isDirectory: true)
  }

  static func defaultVideoAudioRouteRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-video-audio-route-queue.json"
    )
  }

  static func defaultConvolutionalBlendSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-convolutional-blend-sequence-queue.json"
    )
  }

  static func defaultPixelSortSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-pixel-sort-sequence-queue.json"
    )
  }

  static func defaultChannelShiftSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-channel-shift-sequence-queue.json"
    )
  }

  static func defaultPaletteQuantizeSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-palette-quantize-sequence-queue.json"
    )
  }

  static func defaultRuttEtraSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-rutt-etra-sequence-queue.json"
    )
  }

  static func defaultMorphogenesisSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-morphogenesis-sequence-queue.json"
    )
  }

  static func defaultMediaProxyRootURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-media-proxies",
      isDirectory: true
    )
  }

  /// Stable, source-independent location for datamosh optical-flow sidecars. A
  /// single shared directory is safe because the CLI validates each cached frame
  /// against the modulator's checksum + dimensions and recomputes on mismatch, so
  /// changing Source A invalidates stale entries automatically. Persisting it
  /// across renders (rather than the CLI's per-job default) is what lets knob
  /// tweaks reuse the flow.
  static func defaultDatamoshFlowCacheRootURL() -> URL {
    defaultMediaProxyRootURL().appendingPathComponent(
      "datamosh-flow-cache",
      isDirectory: true
    )
  }

  static func defaultQueuedTestRenderBundleURL() -> URL {
    defaultRenderQueueOutputRootURL().appendingPathComponent("job-0001", isDirectory: true)
  }

  static func runRenderTest(outputURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "render-test",
      outputURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  /// Token sequence for the deterministic preview downscale
  /// (`box_downscale_cpu_v1`); pinned in the bridge tests. Callers skip the
  /// command entirely at scale 1 (the identity anchor — the preview reads
  /// the original directories instead), so a scale below 2 here is a
  /// programmer error, not a passthrough.
  static func downscaleFramesArguments(
    inputDirectoryURL: URL,
    outputDirectoryURL: URL,
    scale: Int,
    maxFrames: Int?
  ) throws -> [String] {
    guard scale >= 2 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "downscale scale must be >= 2 (scale 1 skips the downscale)"
      )
    }
    if let maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "max frame count must be greater than zero"
      )
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "downscale-frames",
      inputDirectoryURL.path,
      outputDirectoryURL.path,
      "--scale",
      String(scale)
    ]
    if let maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    return arguments
  }

  static func runDownscaleFrames(
    inputDirectoryURL: URL,
    outputDirectoryURL: URL,
    scale: Int,
    maxFrames: Int?
  ) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    return try runCommand(
      arguments: try downscaleFramesArguments(
        inputDirectoryURL: inputDirectoryURL,
        outputDirectoryURL: outputDirectoryURL,
        scale: scale,
        maxFrames: maxFrames
      ),
      currentDirectoryURL: repoRoot
    )
  }

  static func runShowcasePreview(
    request: ShowcaseRenderCommandRequest
  ) throws -> ShowcaseRenderCommandResult {
    let repoRoot = try resolveRepoRoot()
    let result = try runCommand(
      arguments: try renderShowcaseArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let mp4URL = request.encodeMP4
      ? request.outputDirectoryURL.appendingPathComponent("showcase.mp4")
      : nil
    return ShowcaseRenderCommandResult(
      outputDirectoryURL: request.outputDirectoryURL,
      frameDirectoryURL: request.outputDirectoryURL.appendingPathComponent(
        "frames",
        isDirectory: true
      ),
      contactSheetURL: request.outputDirectoryURL.appendingPathComponent("contact_sheet.png"),
      mp4URL: mp4URL,
      commandSummary: result.summary
    )
  }

  static func runQueuedFrameSequenceRender(
    request: FrameSequenceRenderQueueCommandRequest
  ) throws -> FrameSequenceRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddFrameSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-frame-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FrameSequenceRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedFeedbackSequenceRender(
    request: FeedbackSequenceRenderQueueCommandRequest
  ) throws -> FeedbackSequenceRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddFeedbackSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-feedback-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FeedbackSequenceRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedFluidAdvectSequenceRender(
    request: FluidAdvectSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddFluidAdvectSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-fluid-advect-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedFluidAdvectTwoSourceSequenceRender(
    request: FluidAdvectTwoSourceSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddFluidAdvectTwoSourceSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-fluid-advect-two-source-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedOpticalFlowAdvectSequenceRender(
    request: OpticalFlowAdvectSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddOpticalFlowAdvectSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-optical-flow-advect-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedFieldParticlesSequenceRender(
    request: FieldParticlesSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddFieldParticlesSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-field-particles-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedCascadeTrailsSequenceRender(
    request: CascadeTrailsSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddCascadeTrailsSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-cascade-trails-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedCascadeCollageSequenceRender(
    request: CascadeCollageSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddCascadeCollageSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-cascade-collage-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddCascadeCollageSequenceArguments(
    request: CascadeCollageSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    try validateFluidNumbers([
      ("scribble amount", request.scribAmpScale),
      ("edge strength", request.edgeStrength),
      ("face strength", request.faceStrength),
      ("edge detect", request.edgeDetect),
      ("tile scale", request.tileScale),
      ("block opacity", request.blockOpacity)
    ])

    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-cascade-collage-sequence",
      request.queueURL.path,
      request.sourceDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--scrib-amp-scale",
      cliNumber(request.scribAmpScale),
      "--edge-strength",
      cliNumber(request.edgeStrength),
      "--face-strength",
      cliNumber(request.faceStrength),
      "--edge-detect",
      cliNumber(request.edgeDetect),
      "--tile-scale",
      cliNumber(request.tileScale),
      "--detail-tiles",
      String(request.detailTiles),
      "--hue-rotate",
      cliNumber(request.hueRotate),
      "--block-blend",
      request.blockBlend.cliValue,
      "--block-opacity",
      cliNumber(request.blockOpacity),
      "--seed",
      String(request.seed)
    ]

    var withProject = arguments
    if let projectURL = request.projectURL {
      withProject.append("--project-path")
      withProject.append(projectURL.path)
    }
    try appendModulationArguments(
      &withProject,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    return withProject
  }

  static func runQueuedRetroStaticSequenceRender(
    request: RetroStaticSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddRetroStaticSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-retro-static-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddRetroStaticSequenceArguments(
    request: RetroStaticSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    guard request.realBpp > 0, request.assumedBpp > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("real/assumed bpp must be greater than zero")
    }
    try validateFluidNumbers([("strength", request.strength)])
    guard (0...1).contains(request.strength) else {
      throw RustBridgeError.invalidFrameSequenceRequest("strength must be in [0, 1]")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-retro-static-sequence",
      request.queueURL.path,
      request.sourceDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--real-bpp",
      String(request.realBpp),
      "--assumed-bpp",
      String(request.assumedBpp),
      "--filter",
      request.filter.cliValue,
      "--strength",
      cliNumber(request.strength),
      "--backend",
      request.backend.cliValue
    ]
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    return arguments
  }

  static func queueAddCascadeTrailsSequenceArguments(
    request: CascadeTrailsSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    guard request.tileSize > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("tile size must be greater than zero")
    }
    guard request.gridSpacing > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("grid spacing must be greater than zero")
    }
    try validateFluidNumbers([
      ("advect", request.advect),
      ("turbulence scale", request.turbulenceScale),
      ("detail", request.detail)
    ])

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-cascade-trails-sequence",
      request.queueURL.path,
      request.sourceDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--tile-size",
      String(request.tileSize),
      "--grid-spacing",
      String(request.gridSpacing),
      "--advect",
      cliNumber(request.advect),
      "--turbulence-scale",
      cliNumber(request.turbulenceScale),
      "--detail",
      cliNumber(request.detail),
      "--seed",
      String(request.seed),
      "--field",
      request.field,
      "--river-direction",
      cliNumber(request.riverDirection),
      "--river-speed",
      cliNumber(request.riverSpeed),
      "--river-turbulence",
      cliNumber(request.riverTurbulence)
    ]

    if !request.liveRefresh {
      arguments.append("--no-live-refresh")
    }
    if request.temporalTiles {
      arguments.append("--temporal-tiles")
    }
    if request.decay > 0 {
      arguments.append("--decay")
      arguments.append(cliNumber(request.decay))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )

    return arguments
  }

  static func queueAddFluidAdvectSequenceArguments(
    request: FluidAdvectSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    try validateFluidNumbers([
      ("advect", request.advect),
      ("turbulence scale", request.turbulenceScale),
      ("turbulence speed", request.turbulenceSpeed),
      ("detail", request.detail),
      ("reinject", request.reinject),
      ("reinject blotch", request.reinjectBlotch),
      ("warp", request.warp),
      ("diffuse", request.diffuse),
      ("shade", request.shade)
    ])
    try validateFluidSubsteps(request.substeps)

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-fluid-advect-sequence",
      request.queueURL.path,
      request.sourceDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--advect",
      cliNumber(request.advect),
      "--turbulence-scale",
      cliNumber(request.turbulenceScale),
      "--turbulence-speed",
      cliNumber(request.turbulenceSpeed),
      "--detail",
      cliNumber(request.detail),
      "--reinject",
      cliNumber(request.reinject),
      "--substeps",
      String(request.substeps),
      "--reinject-blotch",
      cliNumber(request.reinjectBlotch),
      "--warp",
      cliNumber(request.warp),
      "--diffuse",
      cliNumber(request.diffuse),
      "--shade",
      cliNumber(request.shade),
      "--seed",
      String(request.seed),
      "--backend",
      request.backend.cliValue
    ]

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )

    return arguments
  }

  static func queueAddFluidAdvectTwoSourceSequenceArguments(
    request: FluidAdvectTwoSourceSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    try validateFluidNumbers([
      ("advect", request.advect),
      ("reinject", request.reinject),
      ("diffuse", request.diffuse),
      ("shade", request.shade)
    ])
    try validateFluidSubsteps(request.substeps)

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-fluid-advect-two-source-sequence",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--advect",
      cliNumber(request.advect),
      "--reinject",
      cliNumber(request.reinject),
      "--substeps",
      String(request.substeps),
      "--diffuse",
      cliNumber(request.diffuse),
      "--shade",
      cliNumber(request.shade),
      "--backend",
      request.backend.cliValue
    ]

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )

    return arguments
  }

  static func queueAddOpticalFlowAdvectSequenceArguments(
    request: OpticalFlowAdvectSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    try validateFluidNumbers([
      ("advect", request.advect),
      ("reinject", request.reinject),
      ("diffuse", request.diffuse),
      ("shade", request.shade)
    ])
    try validateFluidSubsteps(request.substeps)

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-optical-flow-advect-sequence",
      request.queueURL.path,
      request.sourceDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--advect",
      cliNumber(request.advect),
      "--reinject",
      cliNumber(request.reinject),
      "--substeps",
      String(request.substeps),
      "--diffuse",
      cliNumber(request.diffuse),
      "--shade",
      cliNumber(request.shade),
      "--backend",
      request.backend.cliValue
    ]

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )

    return arguments
  }

  static func queueAddFieldParticlesSequenceArguments(
    request: FieldParticlesSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    guard request.spacing > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("particle spacing must be greater than zero")
    }
    guard request.particleSize > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("particle size must be greater than zero")
    }
    try validateFluidNumbers([
      ("advect", request.advect),
      ("turbulence scale", request.turbulenceScale),
      ("turbulence speed", request.turbulenceSpeed),
      ("detail", request.detail)
    ])

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-field-particles-sequence",
      request.queueURL.path,
      request.sourceDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--spacing",
      String(request.spacing),
      "--particle-size",
      String(request.particleSize),
      "--advect",
      cliNumber(request.advect),
      "--turbulence-scale",
      cliNumber(request.turbulenceScale),
      "--turbulence-speed",
      cliNumber(request.turbulenceSpeed),
      "--detail",
      cliNumber(request.detail),
      "--seed",
      String(request.seed),
      "--backend",
      request.backend.cliValue
    ]

    if request.liveColour {
      arguments.append("--live-colour")
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    return arguments
  }

  static func runQueuedGranularMosaicPoolSequenceRender(
    request: GranularMosaicPoolSequenceRenderQueueCommandRequest
  ) throws -> GranularMosaicPoolSequenceRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddGranularMosaicPoolSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-granular-mosaic-pool-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return GranularMosaicPoolSequenceRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddGranularMosaicPoolSequenceArguments(
    request: GranularMosaicPoolSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.grainSize > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("grain size must be greater than zero")
    }
    for (name, value) in [
      ("rearrangement", request.rearrangement),
      ("variation", request.variation),
      ("audio weight", request.audioWeight),
      ("frame rate", request.frameRate)
    ] {
      guard value.isFinite else {
        throw RustBridgeError.invalidFrameSequenceRequest("\(name) must be finite")
      }
    }
    guard (0...1).contains(request.rearrangement) else {
      throw RustBridgeError.invalidFrameSequenceRequest("rearrangement must be between zero and one")
    }
    guard request.variation >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "variation must be greater than or equal to zero"
      )
    }
    guard request.audioWeight >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "audio weight must be greater than or equal to zero"
      )
    }
    guard request.frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame rate must be positive")
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("max frame count must be greater than zero")
    }
    guard (request.modulatorRMSCacheURL == nil) == (request.carrierRMSCacheURL == nil) else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "audio matching needs both Source A and Source B RMS caches, or neither"
      )
    }
    guard (request.modulatorCentroidCacheURL == nil) == (request.carrierCentroidCacheURL == nil) else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "centroid matching needs both Source A and Source B STFT caches, or neither"
      )
    }
    for (name, value) in [
      ("texture weight", request.textureWeight),
      ("anti-repeat weight", request.antiRepeatWeight),
      ("coherence weight", request.coherenceWeight),
      ("spatial coherence weight", request.spatialCoherenceWeight)
    ] {
      guard value.isFinite && value >= 0 else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "\(name) must be finite and greater than or equal to zero"
        )
      }
    }
    guard request.poolWindow >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("pool window must be zero or greater")
    }
    guard request.antiRepeatCooldown >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("anti-repeat cooldown must be zero or greater")
    }
    guard request.coherenceReach >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("coherence reach must be zero or greater")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-granular-mosaic-pool-sequence",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--grain-size",
      String(request.grainSize),
      "--rearrangement",
      cliNumber(request.rearrangement),
      "--variation",
      cliNumber(request.variation),
      "--seed",
      String(request.seed),
      "--audio-weight",
      cliNumber(request.audioWeight),
      "--texture-weight",
      cliNumber(request.textureWeight),
      "--pool-window",
      String(request.poolWindow),
      "--anti-repeat-weight",
      cliNumber(request.antiRepeatWeight),
      "--anti-repeat-cooldown",
      String(request.antiRepeatCooldown),
      "--coherence-weight",
      cliNumber(request.coherenceWeight),
      "--coherence-reach",
      String(request.coherenceReach),
      "--spatial-coherence-weight",
      cliNumber(request.spatialCoherenceWeight),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--backend",
      request.backend.cliValue
    ]

    if let modulatorRMSCacheURL = request.modulatorRMSCacheURL {
      arguments.append("--modulator-rms-cache")
      arguments.append(modulatorRMSCacheURL.path)
    }
    if let carrierRMSCacheURL = request.carrierRMSCacheURL {
      arguments.append("--carrier-rms-cache")
      arguments.append(carrierRMSCacheURL.path)
    }
    if let modulatorCentroidCacheURL = request.modulatorCentroidCacheURL {
      arguments.append("--modulator-centroid-cache")
      arguments.append(modulatorCentroidCacheURL.path)
    }
    if let carrierCentroidCacheURL = request.carrierCentroidCacheURL {
      arguments.append("--carrier-centroid-cache")
      arguments.append(carrierCentroidCacheURL.path)
    }
    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func runQueuedVideoVocoderSequenceRender(
    request: VideoVocoderSequenceRenderQueueCommandRequest
  ) throws -> VideoVocoderSequenceRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddVideoVocoderSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-video-vocoder-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return VideoVocoderSequenceRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddVideoVocoderSequenceArguments(
    request: VideoVocoderSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.bands > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("band count must be greater than zero")
    }
    guard request.amount.isFinite && request.amount >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "amount must be finite and greater than or equal to zero"
      )
    }
    guard request.frameRate.isFinite && request.frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame rate must be positive and finite")
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("max frame count must be greater than zero")
    }
    guard !(request.backend == .metal && request.mode == .gain) else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "the Metal backend is only available in Match mode; use Gain mode on the CPU backend"
      )
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-video-vocoder-sequence",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--bands",
      String(request.bands),
      "--amount",
      cliNumber(request.amount),
      "--mode",
      request.mode.cliValue,
      "--frame-rate",
      cliNumber(request.frameRate),
      "--backend",
      request.backend.cliValue
    ]

    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func runQueuedSpectralCrossSynthRender(
    request: SpectralCrossSynthRenderQueueCommandRequest
  ) throws -> SpectralCrossSynthRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddSpectralCrossSynthArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-spectral-cross-synth",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return SpectralCrossSynthRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddSpectralCrossSynthArguments(
    request: SpectralCrossSynthRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.amount.isFinite && request.amount >= 0 && request.amount <= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest("amount must be finite and within [0, 1]")
    }
    guard request.rmsWindow > 0 && request.rmsHop > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "RMS window and hop must be greater than zero"
      )
    }
    guard request.fftSize > 0 && (request.fftSize & (request.fftSize - 1)) == 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("FFT size must be a power of two")
    }
    guard request.stftHop > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("STFT hop must be greater than zero")
    }
    if request.mode == .vocode {
      // Mirror the CLI's add-time vocode checks so rejection happens app-side.
      guard request.stftHop <= request.fftSize / 2 else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "STFT hop must be at most half the FFT size for vocode mode"
        )
      }
      guard request.vocodeBands >= 1 && request.vocodeBands <= request.fftSize / 2 else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "vocode bands must be between 1 and half the FFT size"
        )
      }
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-spectral-cross-synth",
      request.queueURL.path,
      request.modulatorWAVURL.path,
      request.carrierWAVURL.path,
      request.outputRootDirectoryURL.path,
      "--mode",
      request.mode.cliValue,
      "--amount",
      cliNumber(request.amount),
      "--filter-type",
      request.filterType.cliValue,
      "--rms-window",
      String(request.rmsWindow),
      "--rms-hop",
      String(request.rmsHop),
      "--fft-size",
      String(request.fftSize),
      "--stft-hop",
      String(request.stftHop),
      "--window",
      request.window.cliValue
    ]

    // Vocode-only knob — omitted otherwise so gain/filter arg arrays stay
    // byte-identical to their pre-vocode form.
    if request.mode == .vocode {
      arguments.append("--vocode-bands")
      arguments.append(String(request.vocodeBands))
    }

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func runQueuedVideoAudioRouteRender(
    request: VideoAudioRouteRenderQueueCommandRequest
  ) throws -> VideoAudioRouteRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddVideoAudioRouteArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-video-audio-route",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return VideoAudioRouteRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddVideoAudioRouteArguments(
    request: VideoAudioRouteRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.amount.isFinite && request.amount >= 0 && request.amount <= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest("amount must be finite and within [0, 1]")
    }
    guard request.fps.isFinite && request.fps > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("fps must be finite and greater than zero")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-video-audio-route",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierWAVURL.path,
      request.outputRootDirectoryURL.path,
      "--descriptor",
      request.descriptor.cliValue,
      "--mode",
      request.mode.cliValue,
      "--filter-type",
      request.filterType.cliValue,
      "--sampling",
      request.sampling.cliValue,
      "--amount",
      cliNumber(request.amount),
      "--fps",
      cliNumber(request.fps)
    ]

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func runQueuedAudioImpulseConvolutionRender(
    request: AudioImpulseConvolutionRenderQueueCommandRequest
  ) throws -> AudioImpulseConvolutionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddAudioImpulseConvolutionArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-audio-impulse-convolution",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return AudioImpulseConvolutionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddAudioImpulseConvolutionArguments(
    request: AudioImpulseConvolutionRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.amount.isFinite && request.amount >= 0 && request.amount <= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest("amount must be finite and within [0, 1]")
    }
    if let maxImpulseSamples = request.maxImpulseSamples {
      guard maxImpulseSamples > 0 else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "max impulse samples must be greater than zero"
        )
      }
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-audio-impulse-convolution",
      request.queueURL.path,
      request.modulatorWAVURL.path,
      request.carrierWAVURL.path,
      request.outputRootDirectoryURL.path,
      "--amount",
      cliNumber(request.amount)
    ]

    if let maxImpulseSamples = request.maxImpulseSamples {
      arguments.append("--max-impulse-samples")
      arguments.append(String(maxImpulseSamples))
    }

    if request.useFFT {
      arguments.append("--method")
      arguments.append("fft")
    }

    if request.resampleImpulse {
      arguments.append("--resample-impulse")
    }

    if request.usePerChannelIR {
      arguments.append("--ir-mode")
      arguments.append("per-channel")
    }

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func runQueuedAudioVideoRouteSequenceRender(
    request: AudioVideoRouteSequenceRenderQueueCommandRequest
  ) throws -> AudioVideoRouteSequenceRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddAudioVideoRouteSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-audio-video-route-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return AudioVideoRouteSequenceRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddAudioVideoRouteSequenceArguments(
    request: AudioVideoRouteSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.amount.isFinite && request.amount >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "amount must be finite and greater than or equal to zero"
      )
    }
    guard request.shiftX.isFinite && request.shiftY.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("shift X and Y must be finite")
    }
    guard request.rmsWindow > 0 && request.rmsHop > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "RMS window and hop must be greater than zero"
      )
    }
    guard request.frameRate.isFinite && request.frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame rate must be positive and finite")
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("max frame count must be greater than zero")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-audio-video-route-sequence",
      request.queueURL.path,
      request.modulatorWAVURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--amount",
      cliNumber(request.amount),
      "--shift-x",
      cliNumber(request.shiftX),
      "--shift-y",
      cliNumber(request.shiftY),
      "--rms-window",
      String(request.rmsWindow),
      "--rms-hop",
      String(request.rmsHop),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--backend",
      request.backend.cliValue
    ]

    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func runQueuedDatamoshSequenceRender(
    request: DatamoshSequenceRenderQueueCommandRequest
  ) throws -> DatamoshSequenceRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddDatamoshSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-datamosh-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return DatamoshSequenceRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddDatamoshSequenceArguments(
    request: DatamoshSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.amount.isFinite && request.amount >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "amount must be finite and greater than or equal to zero"
      )
    }
    guard request.blockSize >= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "macroblock size must be greater than or equal to one"
      )
    }
    guard request.residualGain.isFinite && request.residualGain >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "residual gain must be finite and greater than or equal to zero"
      )
    }
    guard request.residualDecay.isFinite && request.residualDecay >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "residual decay must be finite and greater than or equal to zero"
      )
    }
    guard request.blockRefreshThreshold.isFinite && request.blockRefreshThreshold >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "block refresh threshold must be finite and greater than or equal to zero"
      )
    }
    guard request.remixSeed >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "vector-remix seed must be greater than or equal to zero"
      )
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("max frame count must be greater than zero")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-datamosh-sequence",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--keyframe-interval",
      String(request.keyframeInterval),
      "--amount",
      cliNumber(request.amount),
      "--block-size",
      String(request.blockSize),
      "--residual-gain",
      cliNumber(request.residualGain),
      "--residual-decay",
      cliNumber(request.residualDecay),
      "--block-refresh-threshold",
      cliNumber(request.blockRefreshThreshold),
      "--vector-remix",
      request.vectorRemix.cliValue,
      "--preset",
      request.preset.cliValue,
      "--remix-seed",
      String(request.remixSeed),
      "--backend",
      request.backend.cliValue
    ]

    if let flowCacheDirectoryURL = request.flowCacheDirectoryURL {
      arguments.append("--flow-cache-dir")
      arguments.append(flowCacheDirectoryURL.path)
    }
    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )

    return arguments
  }

  static func queueAddBitstreamDatamoshArguments(
    request: BitstreamDatamoshRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.fps > 0 && request.fps.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "fps must be positive and finite"
      )
    }
    if request.operation == .motionTransfer && request.carrierVideoURL == nil {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "motion transfer requires a carrier video"
      )
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-datamosh-bitstream",
      request.queueURL.path,
      request.inputVideoURL.path,
      request.outputRootDirectoryURL.path,
      "--fps",
      cliNumber(request.fps),
      "--operation",
      request.operation.cliValue,
      "--p-frame-index",
      String(request.pFrameIndex),
      "--duplicate-count",
      String(request.duplicateCount),
      "--carrier-keyframes",
      String(request.carrierKeyframes),
      // Equals form so negative values are not mistaken for flags by clap.
      "--mv-pan-x=\(request.mvPanX)",
      "--mv-pan-y=\(request.mvPanY)",
      "--mv-scale=\(cliNumber(request.mvScale))",
      "--mv-sine-amp=\(cliNumber(request.mvSineAmp))",
      "--mv-sine-period=\(cliNumber(request.mvSinePeriod))",
      "--preset",
      request.preset.cliValue
    ]

    if let carrierURL = request.carrierVideoURL {
      arguments.append("--carrier-video")
      arguments.append(carrierURL.path)
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func runQueuedBitstreamDatamoshRender(
    request: BitstreamDatamoshRenderQueueCommandRequest
  ) throws -> BitstreamDatamoshRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddBitstreamDatamoshArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-datamosh-bitstream",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return BitstreamDatamoshRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedConvolutionalBlendSequenceRender(
    request: ConvolutionalBlendSequenceRenderQueueCommandRequest
  ) throws -> ConvolutionalBlendSequenceRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddConvolutionalBlendSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-convolutional-blend-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return ConvolutionalBlendSequenceRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func runQueuedPixelSortSequenceRender(
    request: PixelSortSequenceRenderQueueCommandRequest
  ) throws -> PixelSortSequenceRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddPixelSortSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-pixel-sort-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return PixelSortSequenceRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddPixelSortSequenceArguments(
    request: PixelSortSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-pixel-sort-sequence",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--axis",
      request.axis.cliValue,
      "--key",
      request.key.cliValue,
      "--direction",
      request.direction.cliValue,
      "--threshold-low",
      cliNumber(request.thresholdLow),
      "--threshold-high",
      cliNumber(request.thresholdHigh),
      "--mask-source",
      request.maskSource.cliValue,
      "--backend",
      request.backend.cliValue
    ]

    if request.maxSpan > 0 {
      arguments.append("--max-span")
      arguments.append(String(request.maxSpan))
    }
    if request.maskSource == .aFlow {
      arguments.append("--flow-radius")
      arguments.append(String(request.flowRadius))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )

    return arguments
  }

  static func runQueuedChannelShiftSequenceRender(
    request: ChannelShiftSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddChannelShiftSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-channel-shift-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddChannelShiftSequenceArguments(
    request: ChannelShiftSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    let shifts: [(flag: String, value: Double)] = [
      ("--shift-r-x", request.shiftRX),
      ("--shift-r-y", request.shiftRY),
      ("--shift-g-x", request.shiftGX),
      ("--shift-g-y", request.shiftGY),
      ("--shift-b-x", request.shiftBX),
      ("--shift-b-y", request.shiftBY)
    ]
    for shift in shifts {
      guard shift.value.isFinite else {
        throw RustBridgeError.invalidFrameSequenceRequest("\(shift.flag) must be finite")
      }
    }
    guard request.flowGain.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("flow gain must be finite")
    }
    if request.flowGain != 0 {
      guard request.sourceADirectoryURL != nil else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "flow-driven channel shift requires a Source A frame directory"
        )
      }
      guard request.backend == .cpu else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "flow-driven channel shift is CPU-only; pick the CPU backend"
        )
      }
      guard request.flowRadius > 0 else {
        throw RustBridgeError.invalidFrameSequenceRequest("flow radius must be greater than zero")
      }
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-channel-shift-sequence",
      request.queueURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate)
    ]
    // `--flag=value` single-token form so negative shifts survive clap parsing.
    for shift in shifts {
      arguments.append("\(shift.flag)=\(cliNumber(shift.value))")
    }
    if request.flowGain != 0, let sourceAURL = request.sourceADirectoryURL {
      arguments.append("--source-a-dir")
      arguments.append(sourceAURL.path)
      arguments.append("--flow-gain=\(cliNumber(request.flowGain))")
      arguments.append("--radius")
      arguments.append(String(request.flowRadius))
    }
    arguments.append("--backend")
    arguments.append(request.backend.cliValue)
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    try appendMatteArguments(
      &arguments,
      source: request.matteSource,
      framesURL: request.matteFramesURL,
      hasSourceAFallback: request.flowGain != 0 && request.sourceADirectoryURL != nil,
      gain: request.matteGain
    )

    return arguments
  }

  static func runQueuedPaletteQuantizeSequenceRender(
    request: PaletteQuantizeSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddPaletteQuantizeSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-palette-quantize-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddPaletteQuantizeSequenceArguments(
    request: PaletteQuantizeSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    if request.mode == .posterize {
      guard (2...256).contains(request.levels) else {
        throw RustBridgeError.invalidFrameSequenceRequest("levels must be in [2, 256]")
      }
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-palette-quantize-sequence",
      request.queueURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--mode",
      request.mode.cliValue,
      "--levels",
      String(request.levels),
      "--backend",
      request.backend.cliValue
    ]
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    try appendMatteArguments(
      &arguments,
      source: request.matteSource,
      framesURL: request.matteFramesURL,
      hasSourceAFallback: false,
      gain: request.matteGain
    )
    return arguments
  }

  static func runQueuedRuttEtraSequenceRender(
    request: RuttEtraSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddRuttEtraSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-rutt-etra-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddRuttEtraSequenceArguments(
    request: RuttEtraSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    guard request.linePitch >= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest("line pitch must be >= 1")
    }
    guard request.lineThickness >= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest("line thickness must be >= 1")
    }
    guard request.displacementDepth.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("displacement depth must be finite")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-rutt-etra-sequence",
      request.queueURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--line-pitch",
      String(request.linePitch),
      // `=`-joined so a negative depth is not mistaken for a flag (clap).
      "--displacement-depth=\(cliNumber(request.displacementDepth))",
      "--line-thickness",
      String(request.lineThickness)
    ]
    if let sourceADirectoryURL = request.sourceADirectoryURL {
      arguments.append("--source-a-dir")
      arguments.append(sourceADirectoryURL.path)
    }
    if request.mono {
      arguments.append("--mono")
    }
    arguments.append("--backend")
    arguments.append(request.backend.cliValue)
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      modulatorMidiURL: request.modulatorMidiURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    try appendMatteArguments(
      &arguments,
      source: request.matteSource,
      framesURL: request.matteFramesURL,
      hasSourceAFallback: request.sourceADirectoryURL != nil,
      gain: request.matteGain
    )
    return arguments
  }

  // MARK: - Morphogenesis (Tier "Morphogenesis" S4, docs/MORPHOGENESIS_MILESTONE.md)

  /// Queue a Gray-Scott reaction-diffusion sequence job then run it — the
  /// standard add→run flow every effect panel uses. Single-source (Source B
  /// only, CPU-only — no backend picker, unlike Rutt-Etra).
  static func runQueuedMorphogenesisSequenceRender(
    request: MorphogenesisSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddMorphogenesisSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-morphogenesis-sequence",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddMorphogenesisSequenceArguments(
    request: MorphogenesisSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    guard request.simScale >= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest("sim scale must be >= 1")
    }
    guard request.paramMapStrength.isFinite && request.paramMapStrength >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("param map strength must be finite and >= 0")
    }
    guard request.patternMix.isFinite && (0...1).contains(request.patternMix) else {
      throw RustBridgeError.invalidFrameSequenceRequest("pattern mix must be finite and in [0, 1]")
    }
    guard request.displace.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("displace must be finite")
    }
    guard request.seedThreshold.isFinite && (0...1).contains(request.seedThreshold) else {
      throw RustBridgeError.invalidFrameSequenceRequest("seed threshold must be finite and in [0, 1]")
    }
    guard request.inject.isFinite && (0...1).contains(request.inject) else {
      throw RustBridgeError.invalidFrameSequenceRequest("inject must be finite and in [0, 1]")
    }
    guard request.erode.isFinite && (0...1).contains(request.erode) else {
      throw RustBridgeError.invalidFrameSequenceRequest("erode must be finite and in [0, 1]")
    }
    guard request.coverageTarget.isFinite && (0...1).contains(request.coverageTarget) else {
      throw RustBridgeError.invalidFrameSequenceRequest("coverage target must be finite and in [0, 1]")
    }
    guard request.shade.isFinite && (0...1).contains(request.shade) else {
      throw RustBridgeError.invalidFrameSequenceRequest("shade must be finite and in [0, 1]")
    }
    guard request.shadeHeight.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("shade height must be finite")
    }
    guard request.shadeAzimuth.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("shade azimuth must be finite")
    }
    guard request.shadeElevation.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("shade elevation must be finite")
    }
    guard request.shadeSpecular.isFinite && (0...1).contains(request.shadeSpecular) else {
      throw RustBridgeError.invalidFrameSequenceRequest("shade specular must be finite and in [0, 1]")
    }
    guard request.shadeShininess.isFinite && request.shadeShininess > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("shade shininess must be finite and > 0")
    }
    guard request.fhnEpsilon.isFinite && request.fhnEpsilon > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("FHN epsilon must be finite and > 0")
    }
    guard request.fhnA.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("FHN a must be finite")
    }
    guard request.fhnB.isFinite && request.fhnB > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("FHN b must be finite and > 0")
    }
    guard request.fhnStimulus.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("FHN stimulus must be finite")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-morphogenesis-sequence",
      request.queueURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frames",
      String(request.frames),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--preset",
      request.preset.cliValue,
      "--param-map-strength",
      cliNumber(request.paramMapStrength),
      "--seed-threshold",
      cliNumber(request.seedThreshold),
      "--sim-scale",
      String(request.simScale),
      "--substeps",
      String(request.substeps),
      "--pattern-mix",
      cliNumber(request.patternMix),
      // `=`-joined so a negative displacement is not mistaken for a flag (clap).
      "--displace=\(cliNumber(request.displace))",
      "--pattern-hue",
      cliNumber(request.patternHue),
      "--pattern-color-mode",
      request.patternColorMode.cliValue
    ]
    // Track A1: only emit `--model`/`--fhn-*` when non-default, so an
    // unmodified panel keeps the exact byte-identical pre-A1 argument array.
    if request.model != .grayScott {
      arguments.append("--model")
      arguments.append(request.model.cliValue)
    }
    if request.fhnPreset != .pulse {
      arguments.append("--fhn-preset")
      arguments.append(request.fhnPreset.cliValue)
    }
    if request.fhnEpsilon != 0.08 {
      arguments.append("--fhn-epsilon")
      arguments.append(cliNumber(request.fhnEpsilon))
    }
    if request.fhnA != 0.7 {
      arguments.append("--fhn-a=\(cliNumber(request.fhnA))")
    }
    if request.fhnB != 0.8 {
      arguments.append("--fhn-b")
      arguments.append(cliNumber(request.fhnB))
    }
    if request.fhnStimulus != 2.5 {
      arguments.append("--fhn-stimulus=\(cliNumber(request.fhnStimulus))")
    }
    // Field View milestone: only emit a flag when it differs from the
    // pre-milestone default, so an unmodified panel keeps the exact
    // byte-identical composite-only argument array.
    if request.outputView != .composite {
      arguments.append("--output-view")
      arguments.append(request.outputView.cliValue)
    }
    // Live Coupling L-S3: only emit a flag when it differs from the
    // pre-Live-Coupling default, so an unmodified panel keeps the exact
    // byte-identical unmodulated argument array (pinned by
    // testQueuedMorphogenesisSequenceNoModulationKeepsArgumentsByteIdentical).
    if request.inject > 0 {
      arguments.append("--inject")
      arguments.append(cliNumber(request.inject))
    }
    if request.erode > 0 {
      arguments.append("--erode")
      arguments.append(cliNumber(request.erode))
    }
    // `--inject-source` is only meaningful when inject or erode is active;
    // still gated on non-default so an untouched picker never appends it.
    if (request.inject > 0 || request.erode > 0) && request.injectSource != .motion {
      arguments.append("--inject-source")
      arguments.append(request.injectSource.cliValue)
    }
    if request.coverageTarget > 0 {
      arguments.append("--coverage-target")
      arguments.append(cliNumber(request.coverageTarget))
    }
    // Track B1: only emitted when non-default, so an unmodified panel keeps
    // the exact byte-identical unshaded argument array.
    if request.shade > 0 {
      arguments.append("--shade")
      arguments.append(cliNumber(request.shade))
    }
    if request.shadeHeight != 3.0 {
      arguments.append("--shade-height")
      arguments.append(cliNumber(request.shadeHeight))
    }
    if request.shadeAzimuth != 0.0 {
      arguments.append("--shade-azimuth")
      arguments.append(cliNumber(request.shadeAzimuth))
    }
    if request.shadeElevation != 0.15 {
      arguments.append("--shade-elevation")
      arguments.append(cliNumber(request.shadeElevation))
    }
    if request.shadeSpecular > 0 {
      arguments.append("--shade-specular")
      arguments.append(cliNumber(request.shadeSpecular))
    }
    if request.shadeShininess != 16.0 {
      arguments.append("--shade-shininess")
      arguments.append(cliNumber(request.shadeShininess))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      modulatorMidiURL: request.modulatorMidiURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    return arguments
  }

  // MARK: - Composition timeline (docs/COMPOSITION_MILESTONE.md)

  static func defaultCompositionRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-composition-queue.json"
    )
  }

  /// Queue a composition spec (queue-add-composition) then run it
  /// (queue-run-composition) — the same add→run flow every effect panel uses.
  /// The whole spec is validated at add time; a rejected spec throws before the
  /// run. Sources are per-scene inside the spec, so there is no top-level input.
  static func runQueuedCompositionRender(
    request: CompositionRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: queueAddCompositionArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "queue-run-composition",
        request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddCompositionArguments(
    request: CompositionRenderQueueCommandRequest
  ) -> [String] {
    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-composition",
      request.queueURL.path,
      request.specURL.path,
      request.outputRootDirectoryURL.path
    ]
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    return arguments
  }

  // MARK: - Coagulated flow blend (Tier 1.1 modulation)

  static func defaultCoagulatedBlendSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-coagulated-blend-queue.json"
    )
  }

  static func runQueuedCoagulatedBlendSequenceRender(
    request: CoagulatedBlendSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddCoagulatedBlendSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: [
        "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
        "queue-run-coagulated-blend-sequence", request.queueURL.path
      ],
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddCoagulatedBlendSequenceArguments(
    request: CoagulatedBlendSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.frameRate.isFinite && request.frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame rate must be positive and finite")
    }
    guard request.patchSize >= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest("patch size must be >= 1")
    }

    var arguments = [
      "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
      "queue-add-coagulated-blend-sequence",
      request.queueURL.path,
      request.sourceADirectoryURL.path,
      request.sourceBDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--frame-rate", cliNumber(request.frameRate),
      "--patch-size", String(request.patchSize),
      "--color-weight", cliNumber(request.colorWeight),
      "--texture-weight", cliNumber(request.textureWeight),
      "--coherence-passes", String(request.coherencePasses),
      "--coherence-strength", cliNumber(request.coherenceStrength),
      "--randomness", cliNumber(request.randomness),
      "--coagulation-strength", cliNumber(request.coagulationStrength),
      "--edge-hardness", cliNumber(request.edgeHardness),
      "--edge-dither", cliNumber(request.edgeDither),
      "--block-jitter", cliNumber(request.blockJitter),
      "--bias", cliNumber(request.bias),
      "--seed", String(request.seed),
      "--advect-source", request.advectSource.cliValue,
      "--advect-amount", cliNumber(request.advectAmount),
      "--refresh", cliNumber(request.refresh),
      "--turbulence", cliNumber(request.turbulence),
      "--smear", cliNumber(request.smear),
      "--smear-decay", cliNumber(request.smearDecay),
      "--backend", request.backend.cliValue
    ]
    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    return arguments
  }

  static func defaultDispersionBlendSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("dispersion-blend-queue.json")
  }

  static func runQueuedDispersionBlendSequenceRender(
    request: DispersionBlendSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddDispersionBlendSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: try queueRunDispersionBlendSequenceArguments(queueURL: request.queueURL),
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputRootDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddDispersionBlendSequenceArguments(
    request: DispersionBlendSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.blockSize >= 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest("block size must be >= 1")
    }
    guard request.dispersionRamp >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("dispersion ramp must be >= 0")
    }

    var arguments = [
      "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
      "queue-add-dispersion-blend-sequence",
      request.queueURL.path,
      request.sourceADirectoryURL.path,
      request.sourceBDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--block-size", String(request.blockSize),
      "--coagulation-strength", cliNumber(Double(request.coagulationStrength)),
      "--bias", cliNumber(Double(request.bias)),
      "--scatter-amount", cliNumber(Double(request.scatterAmount)),
      "--damping", cliNumber(Double(request.damping)),
      "--dispersion-ramp", String(request.dispersionRamp),
      "--ownership-refresh", cliNumber(Double(request.ownershipRefresh)),
      "--smear", cliNumber(Double(request.smear)),
    ]
    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    return arguments
  }

  static func queueRunDispersionBlendSequenceArguments(queueURL: URL) throws -> [String] {
    [
      "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
      "queue-run-dispersion-blend-sequence",
      queueURL.path,
    ]
  }

  // MARK: — Fluid Mosaic

  static func defaultFluidMosaicSequenceRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("fluid-mosaic-queue.json")
  }

  static func runQueuedFluidMosaicSequenceRender(
    request: FluidMosaicSequenceRenderQueueCommandRequest
  ) throws -> FluidAdvectionRenderQueueCommandResult {
    let repoRoot = try resolveRepoRoot()
    if !FileManager.default.fileExists(atPath: request.queueURL.path) {
      _ = try queueInit(queueURL: request.queueURL)
    }

    let addResult = try runCommand(
      arguments: try queueAddFluidMosaicSequenceArguments(request: request),
      currentDirectoryURL: repoRoot
    )
    let jobID = try queuedJobID(from: addResult)
    let runResult = try runCommand(
      arguments: queueRunFluidMosaicSequenceArguments(queueURL: request.queueURL),
      currentDirectoryURL: repoRoot
    )

    return FluidAdvectionRenderQueueCommandResult(
      queueURL: request.queueURL,
      bundleURL: request.outputDirectoryURL.appendingPathComponent(jobID, isDirectory: true),
      commandSummary: [addResult.summary, runResult.summary].joined(separator: " ")
    )
  }

  static func queueAddFluidMosaicSequenceArguments(
    request: FluidMosaicSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    var arguments = [
      "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
      "queue-add-fluid-mosaic-sequence",
      request.queueURL.path,
      request.sourceADirectoryURL.path,
      request.sourceBDirectoryURL.path,
      request.outputDirectoryURL.path,
      "--frames", String(request.frames),
      "--tile-size", String(request.tileSize),
      "--color-bins", String(request.colorBins),
      "--cohesion", cliNumber(Double(request.cohesion)),
      "--repulsion", cliNumber(Double(request.repulsion)),
      "--fluid-strength", cliNumber(Double(request.fluidStrength)),
      "--damping", cliNumber(Double(request.damping)),
      "--settle-iterations", String(request.settleIterations),
      "--jitter", cliNumber(Double(request.jitter)),
      "--turbulence", cliNumber(Double(request.turbulence)),
    ]
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )
    return arguments
  }

  static func queueRunFluidMosaicSequenceArguments(queueURL: URL) -> [String] {
    [
      "cargo", "run", "--quiet", "--release", "-p", "morphogen-cli", "--",
      "queue-run-fluid-mosaic-sequence",
      queueURL.path,
    ]
  }

  static func queueAddConvolutionalBlendSequenceArguments(
    request: ConvolutionalBlendSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.kernelSize >= 1 && request.kernelSize % 2 == 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "kernel size must be odd and greater than or equal to one"
      )
    }
    guard request.amount.isFinite && request.amount >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "amount must be finite and greater than or equal to zero"
      )
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("max frame count must be greater than zero")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-convolutional-blend-sequence",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--kernel-size",
      String(request.kernelSize),
      "--amount",
      cliNumber(request.amount),
      "--backend",
      request.backend.cliValue
    ]

    if request.useColorKernels {
      arguments.append("--kernel-mode")
      arguments.append("color")
    }
    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func queueAddFrameSequenceArguments(
    request: FrameSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    guard request.amount.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("amount must be finite")
    }
    guard request.frameRate.isFinite && request.frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame rate must be positive and finite")
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("max frame count must be greater than zero")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-frame-sequence",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--amount",
      cliNumber(request.amount),
      "--frame-rate",
      cliNumber(request.frameRate)
    ]

    if !request.writesFlowCache {
      arguments.append("--no-flow-cache")
    }

    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func queueAddFeedbackSequenceArguments(
    request: FeedbackSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    for (name, value) in [
      ("carrier amount", request.carrierAmount),
      ("feedback amount", request.feedbackAmount),
      ("feedback mix", request.feedbackMix),
      ("decay", request.decay),
      ("structure mix", request.structureMix),
      ("frame rate", request.frameRate)
    ] {
      guard value.isFinite else {
        throw RustBridgeError.invalidFrameSequenceRequest("\(name) must be finite")
      }
    }
    guard (0...1).contains(request.feedbackMix) else {
      throw RustBridgeError.invalidFrameSequenceRequest("feedback mix must be between zero and one")
    }
    guard request.decay >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("decay must be greater than or equal to zero")
    }
    guard request.structureMix >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "structure mix must be greater than or equal to zero"
      )
    }
    guard request.frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame rate must be positive")
    }
    guard request.iterations == 1 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "the first flow-feedback renderer supports exactly one iteration"
      )
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("max frame count must be greater than zero")
    }
    guard request.temporalSupersampling > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "temporal supersampling must use at least one sample"
      )
    }
    if let resetAtFrame = request.resetAtFrame, resetAtFrame < 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("reset frame must not be negative")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-feedback-sequence",
      request.queueURL.path,
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputRootDirectoryURL.path,
      "--carrier-amount",
      cliNumber(request.carrierAmount),
      "--feedback-amount",
      cliNumber(request.feedbackAmount),
      "--feedback-mix",
      cliNumber(request.feedbackMix),
      "--decay",
      cliNumber(request.decay),
      "--iterations",
      String(request.iterations),
      "--structure-mix",
      cliNumber(request.structureMix),
      "--output-bit-depth",
      request.outputBitDepth.cliValue,
      "--temporal-supersampling",
      String(request.temporalSupersampling),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--backend",
      request.backend.cliValue,
      "--flow-source",
      request.flowSource.cliValue
    ]

    if !request.writesFlowCache {
      arguments.append("--no-flow-cache")
    }
    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }
    if let resetAtFrame = request.resetAtFrame {
      arguments.append("--reset-at-frame")
      arguments.append(String(resetAtFrame))
    }
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    try appendModulationArguments(
      &arguments,
      routes: request.modulationRoutes,
      modulatorAudioURL: request.modulatorAudioURL,
      modulatorFramesURL: request.modulatorFramesURL,
      sampling: request.modulationSampling,
      namedModulators: request.namedModulators
    )

    return arguments
  }

  static func renderShowcaseArguments(
    request: ShowcaseRenderCommandRequest
  ) throws -> [String] {
    guard request.framesPerEffect > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "showcase frames per effect must be greater than zero"
      )
    }
    guard request.frameRate.isFinite && request.frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "showcase frame rate must be a positive finite number"
      )
    }
    guard request.granularGrainSize > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "showcase granular grain size must be greater than zero"
      )
    }
    guard request.seed >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "showcase seed must be greater than or equal to zero"
      )
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "render-showcase",
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputDirectoryURL.path,
      "--intensity",
      request.intensity.cliValue,
      "--frames-per-effect",
      String(request.framesPerEffect),
      "--frame-rate",
      cliNumber(request.frameRate),
      "--granular-grain-size",
      String(request.granularGrainSize),
      "--seed",
      String(request.seed),
      "--backend",
      request.backend.cliValue
    ]
    if !request.encodeMP4 {
      arguments.append("--no-mp4")
    }
    return arguments
  }

  static func extractMediaProxies(
    request: MediaProxyExtractionCommandRequest
  ) throws -> MediaProxyExtractionCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = try mediaProxyExtractionArguments(request: request)
    _ = try runCommand(arguments: arguments.frameExtraction, currentDirectoryURL: repoRoot)
    _ = try runCommand(arguments: arguments.audioExtraction, currentDirectoryURL: repoRoot)
    _ = try runCommand(arguments: arguments.rmsCacheGeneration, currentDirectoryURL: repoRoot)
    _ = try runCommand(arguments: arguments.stftCacheGeneration, currentDirectoryURL: repoRoot)

    let analysisDirectoryURL = request.proxyDirectoryURL.appendingPathComponent("analysis", isDirectory: true)
    return MediaProxyExtractionCommandResult(
      sourceURL: request.sourceURL,
      proxyDirectoryURL: request.proxyDirectoryURL,
      frameDirectoryURL: request.proxyDirectoryURL.appendingPathComponent("frames", isDirectory: true),
      audioWAVURL: request.proxyDirectoryURL.appendingPathComponent("audio.wav"),
      rmsCacheURL: analysisDirectoryURL.appendingPathComponent("rms.json"),
      stftCacheURL: analysisDirectoryURL.appendingPathComponent("stft.json")
    )
  }

  static func registerProjectSourceProxy(
    projectURL: URL,
    sourceRole: SourceRole,
    proxy: MediaProxyExtractionCommandResult
  ) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    return try runCommand(
      arguments: projectSourceProxyRegistrationArguments(
        projectURL: projectURL,
        sourceRole: sourceRole,
        proxy: proxy
      ),
      currentDirectoryURL: repoRoot
    )
  }

  static func projectSourceProxyRegistrationArguments(
    projectURL: URL,
    sourceRole: SourceRole,
    proxy: MediaProxyExtractionCommandResult
  ) -> [String] {
    let sourceRoleArgument: String
    switch sourceRole {
    case .modulator:
      sourceRoleArgument = "modulator"
    case .carrier:
      sourceRoleArgument = "carrier"
    }

    return [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "project-register-proxy",
      projectURL.path,
      "--source-role",
      sourceRoleArgument,
      "--frame-dir",
      proxy.frameDirectoryURL.path,
      "--audio",
      proxy.audioWAVURL.path,
      "--analysis-cache",
      "audio_rms=\(proxy.rmsCacheURL.path)",
      "--analysis-cache",
      "stft=\(proxy.stftCacheURL.path)"
    ]
  }

  static func mediaProxyExtractionArguments(
    request: MediaProxyExtractionCommandRequest
  ) throws -> MediaProxyExtractionArguments {
    guard request.framesPerSecond.isFinite && request.framesPerSecond > 0 else {
      throw RustBridgeError.invalidMediaProxyRequest("frame rate must be positive and finite")
    }
    guard request.sampleRate > 0 else {
      throw RustBridgeError.invalidMediaProxyRequest("sample rate must be greater than zero")
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidMediaProxyRequest("max frame count must be greater than zero")
    }

    let frameDirectoryURL = request.proxyDirectoryURL.appendingPathComponent("frames", isDirectory: true)
    let audioWAVURL = request.proxyDirectoryURL.appendingPathComponent("audio.wav")
    let analysisDirectoryURL = request.proxyDirectoryURL.appendingPathComponent("analysis", isDirectory: true)
    let rmsCacheURL = analysisDirectoryURL.appendingPathComponent("rms.json")
    let stftCacheURL = analysisDirectoryURL.appendingPathComponent("stft.json")
    var frameExtraction = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "extract-frames",
      request.sourceURL.path,
      frameDirectoryURL.path,
      "--fps",
      cliNumber(request.framesPerSecond)
    ]
    if let maxFrames = request.maxFrames {
      frameExtraction.append("--max-frames")
      frameExtraction.append(String(maxFrames))
    }

    var audioExtraction = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "extract-audio",
      request.sourceURL.path,
      audioWAVURL.path,
      "--sample-rate",
      String(request.sampleRate)
    ]
    if let maxFrames = request.maxFrames {
      audioExtraction.append("--max-duration-seconds")
      audioExtraction.append(cliNumber(Double(maxFrames) / request.framesPerSecond))
    }

    return MediaProxyExtractionArguments(
      frameExtraction: frameExtraction,
      audioExtraction: audioExtraction,
      rmsCacheGeneration: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "cache-rms",
        audioWAVURL.path,
        rmsCacheURL.path,
        "--window-size",
        "2048",
        "--hop-size",
        "512"
      ],
      stftCacheGeneration: [
        "cargo",
        "run",
        "--quiet",
        "--release",
        "-p",
        "morphogen-cli",
        "--",
        "cache-stft",
        audioWAVURL.path,
        stftCacheURL.path,
        "--fft-size",
        "1024",
        "--hop-size",
        "256"
      ]
    )
  }

  static func runFreshQueuedTestRender(projectURL: URL?) throws -> QueuedRenderCommandResult {
    let queueURL = defaultRenderQueueURL()
    let outputRootURL = defaultRenderQueueOutputRootURL()
    let bundleURL = defaultQueuedTestRenderBundleURL()
    let initResult = try queueInit(queueURL: queueURL)
    let addResult = try queueAddTest(queueURL: queueURL, projectURL: projectURL)
    let runResult = try queueRunTest(queueURL: queueURL, outputRootURL: outputRootURL)

    return QueuedRenderCommandResult(
      queueURL: queueURL,
      outputRootURL: outputRootURL,
      bundleURL: bundleURL,
      commandSummary: [
        initResult.summary,
        addResult.summary,
        runResult.summary
      ].joined(separator: " ")
    )
  }

  static func queueInit(queueURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-init",
      queueURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func queueAddTest(queueURL: URL, projectURL: URL?) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-test",
      queueURL.path
    ]
    if let projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func queueRunTest(queueURL: URL, outputRootURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "queue-run-test",
      queueURL.path,
      outputRootURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func probeMedia(mediaURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "probe",
      mediaURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func createExampleProject(outputURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "init-example",
      outputURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func inspectProject(projectURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "--release",
      "-p",
      "morphogen-cli",
      "--",
      "inspect-project",
      projectURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  private static func queuedJobID(from result: RustCommandResult) throws -> String {
    let tokens = result.summary.split(whereSeparator: { $0.isWhitespace })
    guard let jobID = tokens.first(where: { $0.hasPrefix("job-") }) else {
      throw RustBridgeError.invalidQueueResponse(result.summary)
    }
    return String(jobID)
  }

  private static func resolveRepoRoot() throws -> URL {
    var candidate = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)

    for _ in 0..<8 {
      if FileManager.default.fileExists(atPath: candidate.appendingPathComponent("Cargo.toml").path),
         FileManager.default.fileExists(atPath: candidate.appendingPathComponent("Package.swift").path) {
        return candidate
      }

      let parent = candidate.deletingLastPathComponent()
      if parent.path == candidate.path {
        break
      }
      candidate = parent
    }

    throw RustBridgeError.repoRootNotFound
  }

  private static func cliNumber(_ value: Double) -> String {
    String(format: "%.6g", locale: Locale(identifier: "en_US_POSIX"), value)
  }

  /// Append the modulation-matrix flag set shared by the modulatable
  /// `queue-add-…` commands. No routes ⇒ no flags (the exact unmodulated path).
  private static func appendModulationArguments(
    _ arguments: inout [String],
    routes: [ModulationRouteSpec],
    modulatorAudioURL: URL?,
    modulatorFramesURL: URL?,
    // Default modulator MIDI file; defaulted nil so the panels without a MIDI
    // story keep their exact argument arrays (docs/MIDI_MODULATION_MILESTONE.md S3).
    modulatorMidiURL: URL? = nil,
    sampling: ModulationSamplingOption,
    namedModulators: [NamedModulatorMediaSpec] = []
  ) throws {
    guard !routes.isEmpty else { return }
    for route in routes {
      guard route.scale.isFinite && route.offset.isFinite else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "modulation scale and offset must be finite for target \(route.target)"
        )
      }
      arguments.append("--modulate")
      let prefix = route.modulator.flatMap { $0.isEmpty ? nil : "\($0)." } ?? ""
      var spec = "\(route.target)=\(prefix)\(route.source):\(cliNumber(route.scale)),\(cliNumber(route.offset))"
      if let override = route.sampling {
        spec += "@\(override.cliValue)"
      }
      arguments.append(spec)
    }
    // Default `--modulator-*` media only covers unnamed routes; a route bound
    // to a named modulator draws from its own `--named-modulator-*` entry.
    func isDefault(_ route: ModulationRouteSpec) -> Bool {
      route.modulator.map { $0.isEmpty } ?? true
    }
    if routes.contains(where: { isDefault($0) && $0.source.hasPrefix("audio-") }) {
      guard let audioURL = modulatorAudioURL else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "audio-* modulation sources require a modulator WAV"
        )
      }
      arguments.append("--modulator-audio")
      arguments.append(audioURL.path)
    }
    if routes.contains(where: { isDefault($0) && ($0.source == "luma" || $0.source == "flow") }) {
      guard let framesURL = modulatorFramesURL else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "luma/flow modulation sources require a modulator frame directory"
        )
      }
      arguments.append("--modulator-frames")
      arguments.append(framesURL.path)
    }
    if routes.contains(where: { isDefault($0) && $0.source.hasPrefix("midi-") }) {
      guard let midiURL = modulatorMidiURL else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "midi-* modulation sources require a modulator MIDI file"
        )
      }
      arguments.append("--modulator-midi")
      arguments.append(midiURL.path)
    }
    // Emit `--named-modulator-*` only for names an actual route references,
    // and only for the media kind that route needs. A referenced name must
    // resolve to exactly one declared entry — duplicates would emit duplicate
    // `--named-modulator-*` flags, which the CLI rejects.
    for route in routes {
      guard let name = route.modulator, !name.isEmpty else { continue }
      if namedModulators.filter({ $0.name == name }).count > 1 {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "named modulator '\(name)' is declared more than once"
        )
      }
    }
    for modulator in namedModulators where !modulator.name.isEmpty {
      let usesAudio = routes.contains {
        $0.modulator == modulator.name && $0.source.hasPrefix("audio-")
      }
      let usesFrames = routes.contains {
        $0.modulator == modulator.name && ($0.source == "luma" || $0.source == "flow")
      }
      if usesAudio {
        guard let audioURL = modulator.audioURL else {
          throw RustBridgeError.invalidFrameSequenceRequest(
            "named modulator '\(modulator.name)' is routed to an audio source but has no WAV selected"
          )
        }
        arguments.append("--named-modulator-audio")
        arguments.append("\(modulator.name)=\(audioURL.path)")
      }
      if usesFrames {
        guard let framesURL = modulator.framesURL else {
          throw RustBridgeError.invalidFrameSequenceRequest(
            "named modulator '\(modulator.name)' is routed to a luma/flow source but has no frame directory selected"
          )
        }
        arguments.append("--named-modulator-frames")
        arguments.append("\(modulator.name)=\(framesURL.path)")
      }
      let usesMidi = routes.contains {
        $0.modulator == modulator.name && $0.source.hasPrefix("midi-")
      }
      if usesMidi {
        guard let midiURL = modulator.midiURL else {
          throw RustBridgeError.invalidFrameSequenceRequest(
            "named modulator '\(modulator.name)' is routed to a midi-* source but has no MIDI file selected"
          )
        }
        arguments.append("--named-modulator-midi")
        arguments.append("\(modulator.name)=\(midiURL.path)")
      }
    }
    arguments.append("--modulation-sampling")
    arguments.append(sampling.cliValue)
  }

  /// Append `--matte`/`--matte-frames`/`--matte-gain` when a matte source is
  /// active (Tier 5.4 S2, docs/SPATIAL_MATTE_MILESTONE.md). `.off` emits
  /// nothing, so no-matte call sites keep byte-identical argument arrays.
  /// `hasSourceAFallback` mirrors the CLI's own default (rutt-etra/channel-
  /// shift fall back to `--source-a-dir` when `--matte-frames` is unset;
  /// palette-quantize has no such fallback and must pass `false`).
  private static func appendMatteArguments(
    _ arguments: inout [String],
    source: MatteSourceOption,
    framesURL: URL?,
    hasSourceAFallback: Bool,
    gain: Double
  ) throws {
    guard let cliValue = source.cliValue else { return }
    guard framesURL != nil || hasSourceAFallback else {
      throw RustBridgeError.invalidFrameSequenceRequest(
        "--matte requires matte frames (or a Source A directory, when available)"
      )
    }
    guard gain.isFinite && gain >= 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("matte gain must be finite and >= 0")
    }
    arguments.append("--matte")
    arguments.append(cliValue)
    if let framesURL = framesURL {
      arguments.append("--matte-frames")
      arguments.append(framesURL.path)
    }
    arguments.append("--matte-gain=\(cliNumber(gain))")
  }

  private static func validateFluidSequenceFrames(_ frames: Int, frameRate: Double) throws {
    guard frames > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame count must be greater than zero")
    }
    guard frameRate.isFinite && frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame rate must be positive and finite")
    }
  }

  private static func validateFluidNumbers(_ values: [(String, Double)]) throws {
    for (name, value) in values {
      guard value.isFinite && value >= 0 else {
        throw RustBridgeError.invalidFrameSequenceRequest(
          "\(name) must be finite and greater than or equal to zero"
        )
      }
    }
  }

  // Mirrors the CLI bound (0 = auto, explicit counts up to FLUID_ADVECT_MAX_SUBSTEPS).
  private static func validateFluidSubsteps(_ substeps: Int) throws {
    guard (0...64).contains(substeps) else {
      throw RustBridgeError.invalidFrameSequenceRequest("substeps must be between 0 and 64")
    }
  }

  private static func runCommand(
    arguments: [String],
    currentDirectoryURL: URL
  ) throws -> RustCommandResult {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = arguments
    process.currentDirectoryURL = currentDirectoryURL

    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr

    let stdoutDrain = PipeDrain(pipe: stdout)
    let stderrDrain = PipeDrain(pipe: stderr)
    let outputGroup = DispatchGroup()
    let outputQueue = DispatchQueue(
      label: "dev.morphogen-av.rust-bridge-output",
      qos: .userInitiated,
      attributes: .concurrent
    )

    try process.run()
    stdoutDrain.start(on: outputQueue, group: outputGroup)
    stderrDrain.start(on: outputQueue, group: outputGroup)
    process.waitUntilExit()
    outputGroup.wait()

    let stdoutText = stdoutDrain.text()
    let stderrText = stderrDrain.text()
    let result = RustCommandResult(
      command: arguments.joined(separator: " "),
      exitCode: process.terminationStatus,
      stdout: stdoutText,
      stderr: stderrText
    )

    guard process.terminationStatus == 0 else {
      throw RustBridgeError.commandFailed(result)
    }

    return result
  }
}

struct FrameSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let amount: Double
  let maxFrames: Int?
  let frameRate: Double
  let writesFlowCache: Bool
  let projectURL: URL?
}

struct FrameSequenceRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct FeedbackSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let carrierAmount: Double
  let feedbackAmount: Double
  let feedbackMix: Double
  let decay: Double
  let iterations: Int
  let structureMix: Double
  let outputBitDepth: FeedbackOutputBitDepthOption
  let temporalSupersampling: Int
  let maxFrames: Int?
  let resetAtFrame: Int?
  let frameRate: Double
  let writesFlowCache: Bool
  let backend: FeedbackRenderBackendOption
  let flowSource: FeedbackFlowSourceOption
  let projectURL: URL?
  // Modulation-matrix routes; defaulted off so call sites predating the
  // stateful exposure keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct FeedbackSequenceRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct ShowcaseRenderCommandRequest {
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputDirectoryURL: URL
  let intensity: ShowcaseIntensityOption
  let framesPerEffect: Int
  let frameRate: Double
  let granularGrainSize: Int
  let seed: Int
  let backend: FeedbackRenderBackendOption
  let encodeMP4: Bool
}

struct ShowcaseRenderCommandResult {
  let outputDirectoryURL: URL
  let frameDirectoryURL: URL
  let contactSheetURL: URL
  let mp4URL: URL?
  let commandSummary: String
}

struct FluidAdvectSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let advect: Double
  let turbulenceScale: Double
  let turbulenceSpeed: Double
  let detail: Double
  let reinject: Double
  let seed: UInt64
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  // v3 shader-look knobs; defaulted off/auto so call sites predating them
  // keep their meaning.
  var substeps = 0
  var reinjectBlotch = 0.0
  var warp = 0.0
  var diffuse = 0.0
  var shade = 0.0
  // Modulation-matrix routes; defaulted off so call sites predating the
  // stateful exposure keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct FluidAdvectTwoSourceSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let advect: Double
  let reinject: Double
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  // v2 knobs; defaulted off/auto so call sites predating them keep their meaning.
  var substeps = 0
  var diffuse = 0.0
  var shade = 0.0
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct OpticalFlowAdvectSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let advect: Double
  let reinject: Double
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  // v2 knobs; defaulted off/auto so call sites predating them keep their meaning.
  var substeps = 0
  var diffuse = 0.0
  var shade = 0.0
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct FieldParticlesSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let spacing: Int
  let particleSize: Int
  let advect: Double
  let turbulenceScale: Double
  let turbulenceSpeed: Double
  let detail: Double
  let liveColour: Bool
  let seed: UInt64
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct CascadeTrailsSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let tileSize: Int
  let gridSpacing: Int
  let advect: Double
  let turbulenceScale: Double
  let detail: Double
  let liveRefresh: Bool
  let seed: UInt64
  let field: String
  let riverDirection: Double
  let riverSpeed: Double
  let riverTurbulence: Double
  let temporalTiles: Bool
  let decay: Double
  let projectURL: URL?
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct CascadeCollageSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let scribAmpScale: Double
  let edgeStrength: Double
  let faceStrength: Double
  let edgeDetect: Double
  let tileScale: Double
  let detailTiles: Int
  let hueRotate: Double
  let blockBlend: CascadeCollageBlendOption
  let blockOpacity: Double
  let seed: UInt64
  let projectURL: URL?
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

/// One modulation-matrix patch cable passed to `queue-add-…` as a
/// `--modulate "<target>=<source>:<scale>,<offset>"` flag.
struct ModulationRouteSpec: Equatable {
  let target: String
  /// CLI source spelling: audio-rms, audio-onset, audio-centroid, luma, flow.
  let source: String
  let scale: Double
  let offset: Double
  // Per-route sampling override; nil inherits the panel-level
  // `--modulation-sampling` default (no `@hold`/`@smooth` suffix emitted).
  // Defaulted so call sites predating this override keep their meaning.
  var sampling: ModulationSamplingOption? = nil
  // Named modulator this route reads from; nil/empty reads the default
  // `--modulator-*` media and emits a bare `source` (no `name.` prefix).
  // Defaulted so call sites predating named modulators keep their meaning.
  var modulator: String? = nil
}

/// A declared named modulator: its `name` plus whichever media it carries.
/// The bridge emits `--named-modulator-audio name=path` (and/or `-frames`)
/// only for names that at least one route actually references.
struct NamedModulatorMediaSpec: Equatable {
  let name: String
  let audioURL: URL?
  let framesURL: URL?
  // MIDI media (docs/MIDI_MODULATION_MILESTONE.md S3); defaulted so call
  // sites predating MIDI are unchanged.
  var midiURL: URL? = nil
}

struct RetroStaticSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let realBpp: Int
  let assumedBpp: Int
  let filter: RetroStaticFilterOption
  let strength: Double
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  // Modulation-matrix routes; defaulted off so call sites predating slice 3
  // keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct FluidAdvectionRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct GranularMosaicPoolSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let grainSize: Int
  let rearrangement: Double
  let variation: Double
  let seed: UInt64
  let audioWeight: Double
  // Texture matching weight; defaulted off so call sites predating it keep meaning.
  var textureWeight: Double = 0
  let modulatorRMSCacheURL: URL?
  let carrierRMSCacheURL: URL?
  // Pool-selection knobs added in the queue/SwiftUI exposure sweep. Defaulted to
  // off so call sites predating the sweep keep their whole-clip / no-scheduler meaning.
  var modulatorCentroidCacheURL: URL? = nil
  var carrierCentroidCacheURL: URL? = nil
  var poolWindow: Int = 0
  var antiRepeatWeight: Double = 0
  var antiRepeatCooldown: Int = 8
  var coherenceWeight: Double = 0
  var coherenceReach: Int = 8
  var spatialCoherenceWeight: Double = 0
  let maxFrames: Int?
  let frameRate: Double
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
}

struct GranularMosaicPoolSequenceRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct VideoVocoderSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let bands: Int
  let amount: Double
  let mode: VideoVocoderModeOption
  let maxFrames: Int?
  let frameRate: Double
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
}

struct VideoVocoderSequenceRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct SpectralCrossSynthRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorWAVURL: URL
  let carrierWAVURL: URL
  let outputRootDirectoryURL: URL
  let mode: CrossSynthModeOption
  let amount: Double
  let filterType: CrossSynthFilterTypeOption
  let rmsWindow: Int
  let rmsHop: Int
  let fftSize: Int
  let stftHop: Int
  let window: CrossSynthWindowOption
  /// Log-band count for A's spectral envelope (vocode mode only; defaulted so
  /// pre-vocode call sites are unchanged).
  var vocodeBands: Int = 32
  let projectURL: URL?
}

struct SpectralCrossSynthRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct VideoAudioRouteRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierWAVURL: URL
  let outputRootDirectoryURL: URL
  let descriptor: VideoAudioRouteDescriptorOption
  let mode: VideoAudioRouteModeOption
  let filterType: VideoAudioRouteFilterTypeOption
  let sampling: VideoAudioRouteSamplingOption
  let amount: Double
  let fps: Double
  let projectURL: URL?
}

struct VideoAudioRouteRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct AudioImpulseConvolutionRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorWAVURL: URL
  let carrierWAVURL: URL
  let outputRootDirectoryURL: URL
  let amount: Double
  let maxImpulseSamples: Int?
  /// Use the FFT method (HQ tier) instead of the default direct convolution.
  let useFFT: Bool
  /// Resample A's IR to B's sample rate instead of erroring on a rate mismatch.
  let resampleImpulse: Bool
  /// Use a per-channel (true-stereo) IR instead of one mono downmix IR.
  let usePerChannelIR: Bool
  let projectURL: URL?
}

struct AudioImpulseConvolutionRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct AudioVideoRouteSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorWAVURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let amount: Double
  let shiftX: Double
  let shiftY: Double
  let rmsWindow: Int
  let rmsHop: Int
  let frameRate: Double
  let maxFrames: Int?
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
}

struct AudioVideoRouteSequenceRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct DatamoshSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let keyframeInterval: Int
  let amount: Double
  let blockSize: Int
  let residualGain: Double
  let residualDecay: Double
  let blockRefreshThreshold: Double
  let vectorRemix: DatamoshVectorRemixOption
  let preset: DatamoshPresetOption
  let remixSeed: Int
  let maxFrames: Int?
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  /// Stable directory for per-frame optical-flow sidecars. When set, re-renders
  /// that change only datamosh knobs (block size, amount, preset — none affect
  /// the flow) reuse the cached Lucas-Kanade flow instead of recomputing it, the
  /// dominant per-frame cost. Defaulted nil so existing call sites are unchanged.
  var flowCacheDirectoryURL: URL? = nil
  // Modulation-matrix routes; defaulted off so call sites predating the
  // stateful exposure keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct DatamoshSequenceRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct BitstreamDatamoshRenderQueueCommandRequest {
  let queueURL: URL
  let inputVideoURL: URL
  let outputRootDirectoryURL: URL
  let fps: Double
  let operation: BitstreamOperationOption
  let pFrameIndex: Int
  let duplicateCount: Int
  let carrierVideoURL: URL?
  let carrierKeyframes: Int
  let mvPanX: Int
  let mvPanY: Int
  let mvScale: Double
  let mvSineAmp: Double
  let mvSinePeriod: Double
  let preset: BitstreamPresetOption
  let projectURL: URL?
}

struct BitstreamDatamoshRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct PixelSortSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let axis: PixelSortAxisOption
  let key: PixelSortKeyOption
  let direction: PixelSortDirectionOption
  let thresholdLow: Double
  let thresholdHigh: Double
  let maxSpan: Int
  let maskSource: PixelSortMaskSourceOption
  let flowRadius: Int
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  // Modulation-matrix routes; defaulted off so call sites predating slice 3
  // keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct PixelSortSequenceRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct ChannelShiftSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let shiftRX: Double
  let shiftRY: Double
  let shiftGX: Double
  let shiftGY: Double
  let shiftBX: Double
  let shiftBY: Double
  /// Source A frames; required when `flowGain` is non-zero (A-flow row shifts, CPU-only).
  let sourceADirectoryURL: URL?
  let flowGain: Double
  let flowRadius: Int
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  // Modulation-matrix routes; defaulted off so call sites predating slice 3
  // keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  // Declared named modulators; defaulted empty so call sites predating the
  // named-modulator UI keep their single-modulator meaning.
  var namedModulators: [NamedModulatorMediaSpec] = []
  // Spatial matte (Tier 5.4 S2). `.off` = no `--matte` flag at all, so call
  // sites predating this slice keep byte-identical argument arrays.
  var matteSource: MatteSourceOption = .off
  var matteFramesURL: URL? = nil
  var matteGain: Double = 1.0
}

struct PaletteQuantizeSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let mode: PaletteQuantizeModeOption
  /// Discrete steps per channel for posterize mode (2–256; 256 = passthrough).
  let levels: Int
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
  // Modulation-matrix routes; defaulted off so call sites predating slice 3
  // keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
  // Spatial matte (Tier 5.4 S2). No Source A concept on this single-source
  // command — matte frames must be given explicitly.
  var matteSource: MatteSourceOption = .off
  var matteFramesURL: URL? = nil
  var matteGain: Double = 1.0
}

struct RuttEtraSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  /// Optional Source A (modulator) frames. When set, A's luma drives the
  /// displacement (two-source cross-synthesis) and Source B supplies the colour;
  /// when nil, Source B displaces its own scanlines (single-source). Defaulted
  /// nil so call sites predating the two-source slice keep their meaning.
  var sourceADirectoryURL: URL? = nil
  let frames: Int
  let frameRate: Double
  /// Rows between scanlines (top row always included; >= 1).
  let linePitch: Int
  /// Vertical displacement in px at luma 1.0; sign sets direction.
  let displacementDepth: Double
  /// Each filled cell extends downward by this many px (>= 1).
  let lineThickness: Int
  /// White lines instead of source colour.
  let mono: Bool
  let projectURL: URL?
  // Render backend; defaults to CPU for existing call sites.
  var backend: FeedbackRenderBackendOption = .cpu
  // Modulation-matrix routes; defaulted off so call sites predating slice 3
  // keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  // Default modulator MIDI file for midi-* routes; defaulted so call sites
  // predating MIDI keep their meaning (docs/MIDI_MODULATION_MILESTONE.md S3).
  var modulatorMidiURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
  // Spatial matte (Tier 5.4 S2). Frames default to `sourceADirectoryURL` at
  // the CLI when unset.
  var matteSource: MatteSourceOption = .off
  var matteFramesURL: URL? = nil
  var matteGain: Double = 1.0
}

/// Gray-Scott reaction-diffusion sequence (Tier "Morphogenesis" S4,
/// docs/MORPHOGENESIS_MILESTONE.md). Single-source (Source B only), CPU-only —
/// no backend/matte fields, unlike Rutt-Etra/palette-quantize.
struct MorphogenesisSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frames: Int
  let frameRate: Double
  let preset: MorphogenesisPresetOption
  let paramMapStrength: Double
  let seedThreshold: Double
  let simScale: Int
  let substeps: Int
  let patternMix: Double
  let displace: Double
  let patternHue: Double
  let patternColorMode: MorphogenesisColorModeOption
  let projectURL: URL?
  // Track A1 (docs/MORPHOGENESIS_FHN_MILESTONE.md); defaulted to Gray-Scott/
  // pulse so call sites predating this slice keep their pre-A1 meaning (the
  // flags are only emitted when non-default — see
  // queueAddMorphogenesisSequenceArguments).
  var model: MorphogenesisModelOption = .grayScott
  var fhnPreset: FhnPresetOption = .pulse
  var fhnEpsilon: Double = 0.08
  var fhnA: Double = 0.7
  var fhnB: Double = 0.8
  var fhnStimulus: Double = 2.5
  // Field View milestone (docs/MORPHOGENESIS_FIELD_VIEW_MILESTONE.md);
  // defaulted composite so call sites predating this slice keep their
  // pre-milestone meaning (the flag is only emitted when non-default — see
  // queueAddMorphogenesisSequenceArguments).
  var outputView: MorphogenesisOutputViewOption = .composite
  // Live Coupling L-S3 (docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md);
  // defaulted off so call sites predating this slice keep their unmodulated,
  // pre-Live-Coupling meaning (the flags are only emitted when nonzero/
  // non-default — see queueAddMorphogenesisSequenceArguments).
  var inject: Double = 0.0
  var erode: Double = 0.0
  var injectSource: MorphogenesisInjectSourceOption = .motion
  var coverageTarget: Double = 0.0
  // Track B1 relief shading (docs/MORPHOGENESIS_RELIEF_SHADING_MILESTONE.md);
  // defaulted off so call sites predating this slice keep their unshaded
  // meaning (the flags are only emitted when non-default — see
  // queueAddMorphogenesisSequenceArguments).
  var shade: Double = 0.0
  var shadeHeight: Double = 3.0
  var shadeAzimuth: Double = 0.0
  var shadeElevation: Double = 0.15
  var shadeSpecular: Double = 0.0
  var shadeShininess: Double = 16.0
  // Modulation-matrix routes; defaulted off so call sites predating this
  // slice keep their unmodulated meaning.
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulatorMidiURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

enum CoagulationFlowSourceOption: String, CaseIterable, Identifiable {
  case aFlow, bFlow, mixed, turbulence
  var id: String { rawValue }
  var cliValue: String {
    switch self {
    case .aFlow: return "a-flow"
    case .bFlow: return "b-flow"
    case .mixed: return "mixed"
    case .turbulence: return "turbulence"
    }
  }
  var label: String {
    switch self {
    case .aFlow: return "A Flow"
    case .bFlow: return "B Flow"
    case .mixed: return "Mixed"
    case .turbulence: return "Turbulence"
    }
  }
}

struct CoagulatedBlendSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceADirectoryURL: URL
  let sourceBDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let frameRate: Double
  let patchSize: Int
  let colorWeight: Double
  let textureWeight: Double
  let coherencePasses: Int
  let coherenceStrength: Double
  let randomness: Double
  let coagulationStrength: Double
  let edgeHardness: Double
  let edgeDither: Double
  let blockJitter: Double
  let bias: Double
  let seed: UInt64
  let advectSource: CoagulationFlowSourceOption
  let advectAmount: Double
  let refresh: Double
  let turbulence: Double
  let smear: Double
  let smearDecay: Double
  let backend: FeedbackRenderBackendOption
  let maxFrames: Int?
  let projectURL: URL?
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct DispersionBlendSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceADirectoryURL: URL
  let sourceBDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let blockSize: Int
  let coagulationStrength: Float
  let bias: Float
  let scatterAmount: Float
  let damping: Float
  let dispersionRamp: Int
  let ownershipRefresh: Float
  let smear: Float
  let maxFrames: Int?
  let projectURL: URL?
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct FluidMosaicSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let sourceADirectoryURL: URL
  let sourceBDirectoryURL: URL
  let outputDirectoryURL: URL
  let tileSize: Int
  let colorBins: Int
  let cohesion: Float
  let repulsion: Float
  let fluidStrength: Float
  let damping: Float
  let settleIterations: Int
  let jitter: Float
  let turbulence: Float
  let frames: Int
  var modulationRoutes: [ModulationRouteSpec] = []
  var modulatorAudioURL: URL? = nil
  var modulatorFramesURL: URL? = nil
  var modulationSampling: ModulationSamplingOption = .hold
  var namedModulators: [NamedModulatorMediaSpec] = []
}

struct CompositionRenderQueueCommandRequest {
  let queueURL: URL
  /// Composition spec JSON (`{"version": 1, "fps": 12, "scenes": [...]}`);
  /// sources are per-scene inside it.
  let specURL: URL
  let outputRootDirectoryURL: URL
  var projectURL: URL? = nil
}

struct ConvolutionalBlendSequenceRenderQueueCommandRequest {
  let queueURL: URL
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputRootDirectoryURL: URL
  let kernelSize: Int
  let amount: Double
  let useColorKernels: Bool
  let maxFrames: Int?
  let backend: FeedbackRenderBackendOption
  let projectURL: URL?
}

struct ConvolutionalBlendSequenceRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
}

struct MediaProxyExtractionCommandRequest {
  let sourceURL: URL
  let proxyDirectoryURL: URL
  let framesPerSecond: Double
  let maxFrames: Int?
  let sampleRate: Int
}

struct MediaProxyExtractionArguments {
  let frameExtraction: [String]
  let audioExtraction: [String]
  let rmsCacheGeneration: [String]
  let stftCacheGeneration: [String]
}

struct MediaProxyExtractionCommandResult {
  let sourceURL: URL
  let proxyDirectoryURL: URL
  let frameDirectoryURL: URL
  let audioWAVURL: URL
  let rmsCacheURL: URL
  let stftCacheURL: URL
}

struct QueuedRenderCommandResult {
  let queueURL: URL
  let outputRootURL: URL
  let bundleURL: URL
  let commandSummary: String
}

private final class PipeDrain: @unchecked Sendable {
  private let handle: FileHandle
  private let lock = NSLock()
  private var output = Data()

  init(pipe: Pipe) {
    self.handle = pipe.fileHandleForReading
  }

  func start(on queue: DispatchQueue, group: DispatchGroup) {
    group.enter()
    queue.async {
      let data = self.handle.readDataToEndOfFile()
      self.lock.lock()
      self.output = data
      self.lock.unlock()
      group.leave()
    }
  }

  func text() -> String {
    lock.lock()
    let data = output
    lock.unlock()
    return String(data: data, encoding: .utf8) ?? ""
  }
}

struct RustCommandResult {
  let command: String
  let exitCode: Int32
  let stdout: String
  let stderr: String

  var summary: String {
    let combined = [stdout, stderr]
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }
      .joined(separator: " ")

    if combined.isEmpty {
      return "Command completed."
    }

    return combined
  }
}

enum RustBridgeError: LocalizedError {
  case repoRootNotFound
  case commandFailed(RustCommandResult)
  case invalidFrameSequenceRequest(String)
  case invalidMediaProxyRequest(String)
  case invalidQueueResponse(String)

  var errorDescription: String? {
    switch self {
    case .repoRootNotFound:
      return "Could not find the repository root containing Cargo.toml and Package.swift."
    case .commandFailed(let result):
      let detail = result.summary
      if detail.isEmpty {
        return "\(result.command) exited with status \(result.exitCode)."
      }
      return "\(result.command) exited with status \(result.exitCode): \(detail)"
    case .invalidFrameSequenceRequest(let message):
      return "Invalid frame-sequence render request: \(message)."
    case .invalidMediaProxyRequest(let message):
      return "Invalid media proxy request: \(message)."
    case .invalidQueueResponse(let response):
      return "Could not read the queued job ID from morphogen-cli output: \(response)"
    }
  }
}
