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

  static func defaultMediaProxyRootURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-media-proxies",
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
      "-p",
      "morphogen-cli",
      "--",
      "render-test",
      outputURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
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
    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

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
      ("reinject", request.reinject)
    ])

    var arguments = [
      "cargo",
      "run",
      "--quiet",
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
      "--seed",
      String(request.seed),
      "--backend",
      request.backend.cliValue
    ]

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func queueAddFluidAdvectTwoSourceSequenceArguments(
    request: FluidAdvectTwoSourceSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    try validateFluidNumbers([
      ("advect", request.advect),
      ("reinject", request.reinject)
    ])

    var arguments = [
      "cargo",
      "run",
      "--quiet",
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
      "--backend",
      request.backend.cliValue
    ]

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

    return arguments
  }

  static func queueAddOpticalFlowAdvectSequenceArguments(
    request: OpticalFlowAdvectSequenceRenderQueueCommandRequest
  ) throws -> [String] {
    try validateFluidSequenceFrames(request.frames, frameRate: request.frameRate)
    try validateFluidNumbers([
      ("advect", request.advect),
      ("reinject", request.reinject)
    ])

    var arguments = [
      "cargo",
      "run",
      "--quiet",
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
      "--backend",
      request.backend.cliValue
    ]

    if let projectURL = request.projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }

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

    var arguments = [
      "cargo",
      "run",
      "--quiet",
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
  let projectURL: URL?
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
  let preset: BitstreamPresetOption
  let projectURL: URL?
}

struct BitstreamDatamoshRenderQueueCommandResult {
  let queueURL: URL
  let bundleURL: URL
  let commandSummary: String
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
