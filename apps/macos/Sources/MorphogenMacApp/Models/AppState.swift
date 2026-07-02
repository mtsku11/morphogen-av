import AppKit
import Combine
import Dispatch
import Foundation
import Metal

final class AppState: ObservableObject {
  // Persist render-backend choices across launches so a selected backend (e.g. Metal)
  // stays "sticky"; first launch falls back to the per-effect default.
  private static func stickyBackend(
    _ key: String,
    default fallback: FeedbackRenderBackendOption
  ) -> FeedbackRenderBackendOption {
    guard let raw = UserDefaults.standard.string(forKey: key),
      let value = FeedbackRenderBackendOption(rawValue: raw)
    else { return fallback }
    return value
  }

  private static func persistBackend(_ key: String, _ value: FeedbackRenderBackendOption) {
    UserDefaults.standard.set(value.rawValue, forKey: key)
  }

  @Published var sourceAPath = "No modulator selected"
  @Published var sourceBPath = "No carrier selected"
  @Published var sourceAProbeSummary = "Probe not run"
  @Published var sourceBProbeSummary = "Probe not run"
  @Published var sourceAPreviewSummary = "Preview not run"
  @Published var sourceBPreviewSummary = "Preview not run"
  @Published var sourceAPreviewImage: NSImage?
  @Published var sourceBPreviewImage: NSImage?
  @Published var renderQuality: RenderQualityOption = .highQualityOffline
  @Published var exportFormat: ExportFormatOption = .pngSequence
  @Published var proResFrameRate: ProResFrameRateOption = .fps24 {
    didSet {
      refreshProResPlanPreview()
    }
  }
  @Published var proResProfile: ProResExportProfile = .proRes422HQ {
    didSet {
      refreshProResPlanPreview()
    }
  }
  @Published var projectPath = "No project loaded"
  @Published var projectSummary = "Project schema idle"
  @Published var renderQueueSummary = "No queue output bundle yet"
  @Published var proResPlanSummary = VideoToolboxProResExportPlanner.defaultPlanSummary()
  @Published var proResExportSummary = "No ProRes movie exported"
  @Published var previewProbeSummary = "No preview frame decoded"
  @Published var frameSequenceModulatorPath = "No modulator frame directory selected"
  @Published var frameSequenceCarrierPath = "No carrier frame directory selected"
  @Published var frameSequenceOutputPath =
    "Default: \(RustBridgePlaceholder.defaultFrameSequenceOutputRootURL().path)"
  @Published var frameSequenceSummary = "No two-source frame sequence rendered"
  @Published var frameSequenceAmount = 16.0
  @Published var frameSequenceMaxFrames = 120
  @Published var frameSequenceWritesFlowCache = true
  @Published var feedbackPreset: FeedbackPresetOption = .stableTrails {
    didSet {
      applyFeedbackPreset(feedbackPreset)
    }
  }
  @Published var feedbackCarrierAmount = 1.0
  @Published var feedbackAmount = 1.5
  @Published var feedbackMix = 0.68
  @Published var feedbackDecay = 0.99
  @Published var feedbackIterations = 1
  @Published var feedbackStructureMix = 0.0
  @Published var feedbackOutputBitDepth: FeedbackOutputBitDepthOption = .png16
  @Published var feedbackTemporalSupersampling = 1
  @Published var feedbackFlowSource: FeedbackFlowSourceOption = .opticalFlow
  @Published var feedbackBackend = AppState.stickyBackend("backend.feedback", default: .metal) {
    didSet { AppState.persistBackend("backend.feedback", feedbackBackend) }
  }
  @Published var feedbackWritesFlowCache = true
  @Published var feedbackResetEnabled = false
  @Published var feedbackResetAtFrame = 48
  @Published var feedbackSummary = "No temporal flow-feedback sequence rendered"
  @Published var fluidProceduralAdvect = 12.0
  @Published var fluidMotionAdvect = 1.0
  @Published var fluidReinject = 0.08
  @Published var fluidTurbulenceScale = 0.008
  @Published var fluidTurbulenceSpeed = 0.06
  @Published var fluidDetail = 0.1
  @Published var fluidSeed = 0
  @Published var fieldParticleSpacing = 8
  @Published var fieldParticleSize = 8
  @Published var fieldParticleAdvect = 6.0
  @Published var fieldParticleLiveColour = true
  @Published var fluidBackend = AppState.stickyBackend("backend.fluid", default: .metal) {
    didSet { AppState.persistBackend("backend.fluid", fluidBackend) }
  }
  @Published var fluidAdvectionSummary = "No fluid/advection sequence rendered"
  // Trail Cascade — tuned sparse-ribbon defaults (the one-click preset).
  @Published var cascadeTileSize = 28
  @Published var cascadeGridSpacing = 60
  @Published var cascadeAdvect = 1.6
  @Published var cascadeTurbulenceScale = 0.008
  @Published var cascadeDetail = 0.1
  @Published var cascadeLiveRefresh = true
  @Published var cascadeSeed = 0
  @Published var cascadeFieldType: CascadeFieldOption = .vortex
  @Published var cascadeRiverDirection = 0.0
  @Published var cascadeRiverSpeed = 3.0
  @Published var cascadeRiverTurbulence = 0.8
  @Published var cascadeTemporalTiles = false
  @Published var cascadeDecay = 0.0
  // Cascade Collage — scribbled-edge tile cascade (locked default composition).
  @Published var cascadeCollageTileScale = 1.0
  @Published var cascadeCollageDetailTiles = 4
  @Published var cascadeCollageHueRotate = 0.0
  @Published var cascadeCollageScribAmpScale = 1.0
  @Published var cascadeCollageEdgeStrength = 0.85
  @Published var cascadeCollageFaceStrength = 0.55
  @Published var cascadeCollageEdgeDetect = 0.0
  @Published var cascadeCollageBlockBlend: CascadeCollageBlendOption = .normal
  @Published var cascadeCollageBlockOpacity = 1.0
  @Published var cascadeCollageSeed = 71
  @Published var cascadeCollageSummary = "No cascade-collage sequence rendered"
  // Retro Static — deliberate scanline-filter misread glitch.
  @Published var retroStaticRealBpp = 4
  @Published var retroStaticAssumedBpp = 3
  @Published var retroStaticFilter: RetroStaticFilterOption = .paeth
  @Published var retroStaticStrength = 1.0
  @Published var retroStaticBackend = AppState.stickyBackend("backend.retroStatic", default: .metal) {
    didSet { AppState.persistBackend("backend.retroStatic", retroStaticBackend) }
  }
  @Published var retroStaticSummary = "No retro-static sequence rendered"
  @Published var retroStaticModStrengthSource = ModulationSourceOption.off
  @Published var retroStaticModStrengthScale = 1.0
  @Published var retroStaticModStrengthOffset = 0.0
  @Published var retroStaticModulatorAudioURL: URL?
  @Published var retroStaticModulatorFramesURL: URL?
  @Published var retroStaticModSampling = ModulationSamplingOption.hold
  // Channel Shift — constant per-channel RGB offsets + optional A-flow row shifts.
  @Published var channelShiftRX = 0.0
  @Published var channelShiftRY = 0.0
  @Published var channelShiftGX = 0.0
  @Published var channelShiftGY = 0.0
  @Published var channelShiftBX = 0.0
  @Published var channelShiftBY = 0.0
  @Published var channelShiftFlowGain = 0.0
  @Published var channelShiftFlowRadius = 4
  // CPU default: flow-driven mode is CPU-only, so the out-of-box state keeps
  // every knob combination valid; picking Metal is sticky like the other effects.
  @Published var channelShiftBackend = AppState.stickyBackend("backend.channelShift", default: .cpu) {
    didSet { AppState.persistBackend("backend.channelShift", channelShiftBackend) }
  }
  @Published var channelShiftSummary = "No channel-shift sequence rendered"
  @Published var channelShiftModRXSource = ModulationSourceOption.off
  @Published var channelShiftModRXScale = 1.0
  @Published var channelShiftModRXOffset = 0.0
  @Published var channelShiftModRYSource = ModulationSourceOption.off
  @Published var channelShiftModRYScale = 1.0
  @Published var channelShiftModRYOffset = 0.0
  @Published var channelShiftModGXSource = ModulationSourceOption.off
  @Published var channelShiftModGXScale = 1.0
  @Published var channelShiftModGXOffset = 0.0
  @Published var channelShiftModGYSource = ModulationSourceOption.off
  @Published var channelShiftModGYScale = 1.0
  @Published var channelShiftModGYOffset = 0.0
  @Published var channelShiftModBXSource = ModulationSourceOption.off
  @Published var channelShiftModBXScale = 1.0
  @Published var channelShiftModBXOffset = 0.0
  @Published var channelShiftModBYSource = ModulationSourceOption.off
  @Published var channelShiftModBYScale = 1.0
  @Published var channelShiftModBYOffset = 0.0
  @Published var channelShiftModulatorAudioURL: URL?
  @Published var channelShiftModulatorFramesURL: URL?
  @Published var channelShiftModSampling = ModulationSamplingOption.hold
  // Palette Quantize — posterize levels / neon-palette colour collapse.
  // Levels default 8 (visible posterize) rather than the CLI's 256 passthrough
  // so the first Run shows the effect; 256 stays reachable as the off case.
  @Published var paletteQuantizeMode = PaletteQuantizeModeOption.posterize
  @Published var paletteQuantizeLevels = 8
  @Published var paletteQuantizeBackend = AppState.stickyBackend("backend.paletteQuantize", default: .metal) {
    didSet { AppState.persistBackend("backend.paletteQuantize", paletteQuantizeBackend) }
  }
  @Published var paletteQuantizeSummary = "No palette-quantize sequence rendered"
  // Mod slot for the integer `levels` target only — `mode` is an enum target
  // and enum mod slots are deferred (need an enum-aware presentation).
  @Published var paletteQuantizeModLevelsSource = ModulationSourceOption.off
  @Published var paletteQuantizeModLevelsScale = 1.0
  @Published var paletteQuantizeModLevelsOffset = 0.0
  @Published var paletteQuantizeModulatorAudioURL: URL?
  @Published var paletteQuantizeModulatorFramesURL: URL?
  @Published var paletteQuantizeModSampling = ModulationSamplingOption.hold
  @Published var granularPoolGrainSize = 32
  @Published var granularPoolRearrangement = 1.0
  @Published var granularPoolVariation = 0.25
  @Published var granularPoolSeed = 0
  @Published var granularPoolAudioWeight = 1.0
  @Published var granularPoolTextureWeight = 0.0
  @Published var granularPoolAudioWeighted = true
  @Published var granularPoolCentroidEnabled = false
  @Published var granularPoolWindow = 0
  @Published var granularPoolAntiRepeatWeight = 0.0
  @Published var granularPoolAntiRepeatCooldown = 8
  @Published var granularPoolCoherenceWeight = 0.0
  @Published var granularPoolCoherenceReach = 8
  @Published var granularPoolSpatialCoherenceWeight = 0.0
  @Published var granularPoolBackend = AppState.stickyBackend(
    "backend.granularPool", default: .cpu
  ) {
    didSet { AppState.persistBackend("backend.granularPool", granularPoolBackend) }
  }
  @Published var granularPoolSummary = "No temporal grain pool sequence rendered"
  @Published var vocoderMode: VideoVocoderModeOption = .match
  @Published var vocoderBands = 8
  @Published var vocoderAmount = 1.0
  @Published var vocoderBackend = AppState.stickyBackend("backend.vocoder", default: .cpu) {
    didSet { AppState.persistBackend("backend.vocoder", vocoderBackend) }
  }
  @Published var vocoderSummary = "No video vocoder sequence rendered"

  @Published var crossSynthModulatorURL: URL?
  @Published var crossSynthCarrierURL: URL?
  @Published var crossSynthOutputURL: URL?
  @Published var crossSynthMode: CrossSynthModeOption = .gain
  @Published var crossSynthAmount = 1.0
  @Published var crossSynthFilterType: CrossSynthFilterTypeOption = .lowpass
  @Published var crossSynthRmsWindow = 2048
  @Published var crossSynthRmsHop = 512
  @Published var crossSynthFFTSize = 1024
  @Published var crossSynthSTFTHop = 256
  @Published var crossSynthWindow: CrossSynthWindowOption = .hann
  @Published var crossSynthSummary = "No spectral cross-synth rendered"

  @Published var impulseConvModulatorURL: URL?
  @Published var impulseConvCarrierURL: URL?
  @Published var impulseConvOutputURL: URL?
  @Published var impulseConvAmount = 1.0
  @Published var impulseConvMaxSamples = 0
  @Published var impulseConvUseFFT = false
  @Published var impulseConvResample = false
  @Published var impulseConvPerChannel = false
  @Published var impulseConvSummary = "No audio impulse convolution rendered"

  @Published var audioRouteModulatorURL: URL?
  @Published var audioRouteCarrierURL: URL?
  @Published var audioRouteOutputURL: URL?
  @Published var audioRouteAmount = 1.0
  @Published var audioRouteShiftX = 8.0
  @Published var audioRouteShiftY = 0.0
  @Published var audioRouteRmsWindow = 2048
  @Published var audioRouteRmsHop = 512
  @Published var audioRouteFrameRate = 30.0
  @Published var audioRouteBackend = AppState.stickyBackend("backend.audioRoute", default: .cpu) {
    didSet { AppState.persistBackend("backend.audioRoute", audioRouteBackend) }
  }
  @Published var audioRouteSummary = "No audio→video route rendered"
  @Published var datamoshModulatorURL: URL?
  @Published var datamoshCarrierURL: URL?
  @Published var datamoshOutputURL: URL?
  @Published var datamoshKeyframeInterval = 0
  @Published var datamoshAmount = 1.0
  @Published var datamoshBlockSize = 1
  @Published var datamoshResidualGain = 0.0
  @Published var datamoshResidualDecay = 0.9
  @Published var datamoshBlockRefreshThreshold = 0.0
  @Published var datamoshVectorRemix: DatamoshVectorRemixOption = .none
  @Published var datamoshPreset: DatamoshPresetOption = .custom
  @Published var datamoshRemixSeed = 0
  @Published var datamoshBackend = AppState.stickyBackend("backend.datamosh", default: .cpu) {
    didSet { AppState.persistBackend("backend.datamosh", datamoshBackend) }
  }
  /// Reuse a shared optical-flow cache across datamosh renders so changing knobs
  /// (which don't affect the flow) skips recomputing the dominant per-frame cost.
  @Published var datamoshReuseFlowCache = true
  @Published var datamoshSummary = "No datamosh rendered"
  @Published var bitstreamInputVideoURL: URL?
  @Published var bitstreamCarrierVideoURL: URL?
  @Published var bitstreamOutputURL: URL?
  @Published var bitstreamOperation: BitstreamOperationOption = .pframeDuplicate
  @Published var bitstreamFps = 24.0
  @Published var bitstreamPFrameIndex = 0
  @Published var bitstreamDuplicateCount = 8
  @Published var bitstreamCarrierKeyframes = 1
  @Published var bitstreamPreset: BitstreamPresetOption = .custom
  @Published var bitstreamSummary = "No bitstream datamosh rendered"
  @Published var showcaseSummary = "No showcase preview rendered"
  @Published var showcaseIntensity: ShowcaseIntensityOption = .destructive
  @Published var videoAudioRouteModulatorURL: URL?
  @Published var videoAudioRouteCarrierURL: URL?
  @Published var videoAudioRouteOutputURL: URL?
  @Published var videoAudioRouteDescriptor: VideoAudioRouteDescriptorOption = .luma
  @Published var videoAudioRouteMode: VideoAudioRouteModeOption = .gain
  @Published var videoAudioRouteFilterType: VideoAudioRouteFilterTypeOption = .lowpass
  @Published var videoAudioRouteSampling: VideoAudioRouteSamplingOption = .hold
  @Published var videoAudioRouteAmount = 1.0
  @Published var videoAudioRouteFPS = 30.0
  @Published var videoAudioRouteSummary = "No video→audio route rendered"

  @Published var convBlendModulatorURL: URL?
  @Published var convBlendCarrierURL: URL?
  @Published var convBlendOutputURL: URL?
  @Published var convBlendKernelSize = 3
  @Published var convBlendAmount = 1.0
  @Published var convBlendColorMode = false
  @Published var convBlendBackend = AppState.stickyBackend("backend.convBlend", default: .cpu) {
    didSet { AppState.persistBackend("backend.convBlend", convBlendBackend) }
  }
  @Published var convBlendSummary = "No convolutional blend rendered"

  // Pixel sort
  @Published var pixelSortModulatorURL: URL?
  @Published var pixelSortCarrierURL: URL?
  @Published var pixelSortOutputURL: URL?
  @Published var pixelSortAxis = PixelSortAxisOption.row
  @Published var pixelSortKey = PixelSortKeyOption.luma
  @Published var pixelSortDirection = PixelSortDirectionOption.asc
  @Published var pixelSortThresholdLow = 0.25
  @Published var pixelSortThresholdHigh = 0.80
  @Published var pixelSortMaxSpan = 0
  @Published var pixelSortMaskSource = PixelSortMaskSourceOption.selfMask
  @Published var pixelSortFlowRadius = 4
  @Published var pixelSortBackend = AppState.stickyBackend("backend.pixelSort", default: .cpu) {
    didSet { AppState.persistBackend("backend.pixelSort", pixelSortBackend) }
  }
  @Published var pixelSortSummary = "No pixel sort rendered"
  @Published var pixelSortModLowSource = ModulationSourceOption.off
  @Published var pixelSortModLowScale = 1.0
  @Published var pixelSortModLowOffset = 0.0
  @Published var pixelSortModHighSource = ModulationSourceOption.off
  @Published var pixelSortModHighScale = 1.0
  @Published var pixelSortModHighOffset = 0.0
  @Published var pixelSortModulatorAudioURL: URL?
  @Published var pixelSortModulatorFramesURL: URL?
  @Published var pixelSortModSampling = ModulationSamplingOption.hold

  @Published var mediaProxyOutputPath = RustBridgePlaceholder.defaultMediaProxyRootURL().path
  @Published var mediaProxySummary = "No source proxies extracted"
  @Published var mediaProxyFrameRate = 12.0
  @Published var mediaProxyMaxFrames = 120
  @Published var statusMessage = "Analysis cache idle. Offline queue empty."

  /// Number of frames rendered for a quick effect preview (a short look at the
  /// selected effect before committing to the full clip).
  let previewFrameCount = 8
  @Published var previewFrames: [NSImage] = []
  @Published var previewSummary = "No preview rendered"
  @Published var isRenderingPreview = false

  /// Number of proxy extractions currently in flight. Picking Source A then B
  /// can run two concurrently, so this is a counter rather than a flag.
  @Published private var extractingProxyCount = 0

  /// True while any source proxy is still being extracted; the render controls
  /// disable on this so a render can't fire before its inputs exist.
  var isExtractingProxies: Bool { extractingProxyCount > 0 }

  private var sourceAURL: URL?
  private var sourceBURL: URL?
  private var projectURL: URL?
  private var lastRenderQueueBundleURL: URL?
  private var frameSequenceModulatorURL: URL?
  private var frameSequenceCarrierURL: URL?
  private var frameSequenceOutputURL: URL?
  private var lastFrameSequenceOutputURL: URL? {
    didSet {
      if let lastFrameSequenceOutputURL {
        finishPreviewIfNeeded(frameDirectory: lastFrameSequenceOutputURL)
      }
    }
  }
  private var previewSession: EffectPreviewSession?
  private var sourceARMSCacheURL: URL?
  private var sourceBRMSCacheURL: URL?
  private var sourceASTFTCacheURL: URL?
  private var sourceBSTFTCacheURL: URL?
  private var mediaProxyOutputURL = RustBridgePlaceholder.defaultMediaProxyRootURL()

  func setSource(_ role: SourceRole, url: URL) {
    switch role {
    case .modulator:
      sourceAURL = url
      sourceAPath = url.path
      sourceAProbeSummary = "Probe not run"
      sourceAPreviewSummary = "Preview not run"
      sourceAPreviewImage = nil
    case .carrier:
      sourceBURL = url
      sourceBPath = url.path
      sourceBProbeSummary = "Probe not run"
      sourceBPreviewSummary = "Preview not run"
      sourceBPreviewImage = nil
    }

    statusMessage = "\(role.rawValue) source selected: \(url.lastPathComponent) — extracting proxy…"

    // Auto-extract the just-picked source so the render-input frame directory is
    // populated without a manual "Extract Proxies" step. The manual button stays
    // for re-extraction at different fps / frame-limit settings.
    extractProxies(for: [(role, url)])
  }

  /// Begin a quick preview of the selected effect: the matching render method is
  /// invoked next and, because `previewSession` is set, it writes a small frame
  /// cap into a temp directory instead of the user's chosen output. Returns
  /// `false` (and reports why) when the required sources are not loaded yet.
  func beginEffectPreview(requiresModulator: Bool) -> Bool {
    guard frameSequenceCarrierURL != nil else {
      statusMessage = "Select Source B frames before previewing."
      return false
    }
    if requiresModulator && frameSequenceModulatorURL == nil {
      statusMessage = "Select Source A frames before previewing."
      return false
    }

    previewSession = EffectPreviewSession(
      outputRootURL: RustBridgePlaceholder.defaultEffectPreviewOutputRootURL(),
      maxFrames: previewFrameCount
    )
    previewFrames = []
    isRenderingPreview = true
    previewSummary = "Rendering \(previewFrameCount)-frame preview…"
    return true
  }

  /// Output root a render should use: the preview temp directory while a preview
  /// is active, then the user's chosen directory, then a durable default under
  /// ~/Movies so a render can run without an explicit "Choose Output" step.
  private func effectiveOutputRoot(_ chosen: URL?) -> URL? {
    if let session = previewSession {
      return session.outputRootURL
    }
    if let chosen {
      return chosen
    }
    let fallback = RustBridgePlaceholder.defaultFrameSequenceOutputRootURL()
    try? FileManager.default.createDirectory(
      at: fallback, withIntermediateDirectories: true
    )
    return fallback
  }

  private func effectiveMaxFrames(_ chosen: Int) -> Int {
    previewSession?.maxFrames ?? chosen
  }

  private func effectiveOptionalMaxFrames(_ chosen: Int?) -> Int? {
    previewSession?.maxFrames ?? chosen
  }

  private func finishPreviewIfNeeded(frameDirectory: URL) {
    guard previewSession != nil else {
      return
    }
    previewSession = nil
    let limit = previewFrameCount
    DispatchQueue.global(qos: .userInitiated).async {
      let images = Self.loadPreviewFrames(from: frameDirectory, limit: limit)
      DispatchQueue.main.async {
        self.previewFrames = images
        self.isRenderingPreview = false
        self.previewSummary = images.isEmpty
          ? "Preview produced no frames."
          : "Preview: \(images.count) frame(s) of the selected effect."
        self.statusMessage = "Effect preview complete."
      }
    }
  }

  private func failPreviewIfNeeded(message: String) {
    guard previewSession != nil else {
      return
    }
    previewSession = nil
    isRenderingPreview = false
    previewSummary = "Preview failed: \(message)"
  }

  private static func loadPreviewFrames(from frameDirectory: URL, limit: Int) -> [NSImage] {
    guard let urls = try? ProResImageSequenceExporter.collectPNGFrameURLs(in: frameDirectory) else {
      return []
    }
    return urls.prefix(limit).compactMap { NSImage(contentsOf: $0) }
  }

  func probeSelectedSources() {
    let selectedSources = [
      (SourceRole.modulator, sourceAURL),
      (SourceRole.carrier, sourceBURL)
    ].compactMap { role, url -> (SourceRole, URL)? in
      guard let url else {
        return nil
      }
      return (role, url)
    }

    guard !selectedSources.isEmpty else {
      statusMessage = "Select Source A or Source B before probing media."
      return
    }

    statusMessage = "Probing selected media through morphogen-cli..."

    Task {
      var results: [(SourceRole, String)] = []
      for (role, url) in selectedSources {
        let summary: String
        do {
          let appleProbe = try await AppleMediaProbe.probeMedia(mediaURL: url)
          summary = appleProbe.compactSummary
        } catch {
          summary = Self.fallbackProbeSummary(mediaURL: url, appleError: error)
        }
        results.append((role, summary))
      }

      let probeResults = results
      await MainActor.run {
        for result in probeResults {
          switch result.0 {
          case .modulator:
            self.sourceAProbeSummary = result.1
          case .carrier:
            self.sourceBProbeSummary = result.1
          }
        }

        self.statusMessage = "Media probe complete."
      }
    }
  }

  func probePreviewFrames() {
    let selectedSources = [
      (SourceRole.modulator, sourceAURL),
      (SourceRole.carrier, sourceBURL)
    ].compactMap { role, url -> (SourceRole, URL)? in
      guard let url else {
        return nil
      }
      return (role, url)
    }

    guard !selectedSources.isEmpty else {
      statusMessage = "Select Source A or Source B before probing preview frames."
      return
    }

    guard let device = MTLCreateSystemDefaultDevice() else {
      previewProbeSummary = "No Metal device available for source preview."
      statusMessage = "Preview probe failed: no Metal device available."
      return
    }

    statusMessage = "Decoding first source frame into a Metal texture..."

    Task {
      var results: [(SourceRole, String, NSImage?)] = []
      for (role, url) in selectedSources {
        let summary: String
        let previewImage: NSImage?
        do {
          let result = try await SourcePreviewFrameProbe.decodeFirstVideoFrame(
            mediaURL: url,
            device: device
          )
          summary = result.compactSummary
          previewImage = result.previewImage
        } catch {
          summary = "Preview failed: \(error.localizedDescription)"
          previewImage = nil
        }
        results.append((role, summary, previewImage))
      }

      let previewResults = results
      await MainActor.run {
        for result in previewResults {
          switch result.0 {
          case .modulator:
            self.sourceAPreviewSummary = result.1
            self.sourceAPreviewImage = result.2
          case .carrier:
            self.sourceBPreviewSummary = result.1
            self.sourceBPreviewImage = result.2
          }
        }

        self.previewProbeSummary = previewResults
          .map { "\($0.0.rawValue): \($0.1)" }
          .joined(separator: " | ")
        self.statusMessage = "Preview frame probe complete."
      }
    }
  }

  func runCpuReferenceRender() {
    let outputURL = RustBridgePlaceholder.defaultRenderOutputURL()
    statusMessage = "Running CPU reference render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let commandResult = try RustBridgePlaceholder.runRenderTest(outputURL: outputURL)
        DispatchQueue.main.async {
          self.statusMessage = "Rendered \(outputURL.path). \(commandResult.summary)"
        }
      } catch {
        DispatchQueue.main.async {
          self.statusMessage = "Render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runQueuedTestRender() {
    let projectURL = self.projectURL
    statusMessage = "Running deterministic queued test render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let commandResult = try RustBridgePlaceholder.runFreshQueuedTestRender(projectURL: projectURL)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: commandResult.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.statusMessage = "Queued render output ready: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.renderQueueSummary = "Queued render failed: \(error.localizedDescription)"
          self.statusMessage = "Queued render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func chooseFrameSequenceModulatorDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source A Frames",
      message: "Select modulator PNG frames."
    ) else {
      statusMessage = "Source A frame selection cancelled."
      return
    }

    frameSequenceModulatorURL = url
    frameSequenceModulatorPath = url.path
    statusMessage = "Source A frame directory selected: \(url.lastPathComponent)"
  }

  func chooseFrameSequenceCarrierDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source B Frames",
      message: "Select carrier PNG frames."
    ) else {
      statusMessage = "Source B frame selection cancelled."
      return
    }

    frameSequenceCarrierURL = url
    frameSequenceCarrierPath = url.path
    statusMessage = "Source B frame directory selected: \(url.lastPathComponent)"
  }

  func chooseFrameSequenceOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Frame sequence output selection cancelled."
      return
    }

    frameSequenceOutputURL = url
    frameSequenceOutputPath = url.path
    statusMessage = "Frame sequence output selected: \(url.lastPathComponent)"
  }

  func chooseCrossSynthModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source A WAV",
      message: "Select the modulator audio (analysis source)."
    ) else {
      statusMessage = "Source A WAV selection cancelled."
      return
    }

    crossSynthModulatorURL = url
    statusMessage = "Cross-synth Source A WAV selected: \(url.lastPathComponent)"
  }

  func chooseCrossSynthCarrierWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source B WAV",
      message: "Select the carrier audio (material to reshape)."
    ) else {
      statusMessage = "Source B WAV selection cancelled."
      return
    }

    crossSynthCarrierURL = url
    statusMessage = "Cross-synth Source B WAV selected: \(url.lastPathComponent)"
  }

  func chooseCrossSynthOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Cross-synth output selection cancelled."
      return
    }

    crossSynthOutputURL = url
    statusMessage = "Cross-synth output selected: \(url.lastPathComponent)"
  }

  func chooseImpulseConvModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source A WAV (impulse response)",
      message: "Select the impulse response audio (convolution kernel)."
    ) else {
      statusMessage = "Impulse response WAV selection cancelled."
      return
    }

    impulseConvModulatorURL = url
    statusMessage = "Impulse-convolution Source A WAV selected: \(url.lastPathComponent)"
  }

  func chooseImpulseConvCarrierWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source B WAV",
      message: "Select the carrier audio (material to convolve)."
    ) else {
      statusMessage = "Source B WAV selection cancelled."
      return
    }

    impulseConvCarrierURL = url
    statusMessage = "Impulse-convolution Source B WAV selected: \(url.lastPathComponent)"
  }

  func chooseImpulseConvOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Impulse-convolution output selection cancelled."
      return
    }

    impulseConvOutputURL = url
    statusMessage = "Impulse-convolution output selected: \(url.lastPathComponent)"
  }

  func chooseAudioRouteModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source A WAV",
      message: "Select the modulator audio whose RMS drives displacement."
    ) else {
      statusMessage = "Source A WAV selection cancelled."
      return
    }

    audioRouteModulatorURL = url
    statusMessage = "Audio-route Source A WAV selected: \(url.lastPathComponent)"
  }

  func chooseAudioRouteCarrierDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source B Frames",
      message: "Select the carrier PNG frames to displace."
    ) else {
      statusMessage = "Source B frame selection cancelled."
      return
    }

    audioRouteCarrierURL = url
    statusMessage = "Audio-route Source B frame directory selected: \(url.lastPathComponent)"
  }

  func chooseAudioRouteOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Audio-route output selection cancelled."
      return
    }

    audioRouteOutputURL = url
    statusMessage = "Audio-route output selected: \(url.lastPathComponent)"
  }

  func chooseDatamoshModulatorDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source A Frames",
      message: "Select the modulator PNG frames whose motion drives the mosh."
    ) else {
      statusMessage = "Source A frame selection cancelled."
      return
    }

    datamoshModulatorURL = url
    statusMessage = "Datamosh Source A frame directory selected: \(url.lastPathComponent)"
  }

  func chooseDatamoshCarrierDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source B Frames",
      message: "Select the carrier PNG frames to mosh."
    ) else {
      statusMessage = "Source B frame selection cancelled."
      return
    }

    datamoshCarrierURL = url
    statusMessage = "Datamosh Source B frame directory selected: \(url.lastPathComponent)"
  }

  func chooseDatamoshOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Datamosh output selection cancelled."
      return
    }

    datamoshOutputURL = url
    statusMessage = "Datamosh output selected: \(url.lastPathComponent)"
  }

  func chooseBitstreamInputVideo() {
    guard let url = MediaFilePicker.chooseMediaFile(for: .modulator) else {
      statusMessage = "Bitstream input video selection cancelled."
      return
    }
    bitstreamInputVideoURL = url
    statusMessage = "Bitstream input video selected: \(url.lastPathComponent)"
  }

  func chooseBitstreamCarrierVideo() {
    guard let url = MediaFilePicker.chooseMediaFile(for: .carrier) else {
      statusMessage = "Bitstream carrier video selection cancelled."
      return
    }
    bitstreamCarrierVideoURL = url
    statusMessage = "Bitstream carrier video selected: \(url.lastPathComponent)"
  }

  func chooseBitstreamOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Bitstream output selection cancelled."
      return
    }
    bitstreamOutputURL = url
    statusMessage = "Bitstream output selected: \(url.lastPathComponent)"
  }

  func chooseVideoAudioRouteModulatorDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source A Frames",
      message: "Select the modulator PNG frames whose luma drives the audio."
    ) else {
      statusMessage = "Source A frame selection cancelled."
      return
    }

    videoAudioRouteModulatorURL = url
    statusMessage = "Video-route Source A frame directory selected: \(url.lastPathComponent)"
  }

  func chooseVideoAudioRouteCarrierWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source B WAV",
      message: "Select the carrier audio (material to reshape)."
    ) else {
      statusMessage = "Source B WAV selection cancelled."
      return
    }

    videoAudioRouteCarrierURL = url
    statusMessage = "Video-route Source B WAV selected: \(url.lastPathComponent)"
  }

  func chooseVideoAudioRouteOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Video-route output selection cancelled."
      return
    }

    videoAudioRouteOutputURL = url
    statusMessage = "Video-route output selected: \(url.lastPathComponent)"
  }

  func chooseMediaProxyOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseMediaProxyOutputDirectory() else {
      statusMessage = "Media proxy output selection cancelled."
      return
    }

    mediaProxyOutputURL = url
    mediaProxyOutputPath = url.path
    statusMessage = "Media proxy output selected: \(url.lastPathComponent)"
  }

  func extractSelectedSourceProxies() {
    let selectedSources = [
      (SourceRole.modulator, sourceAURL),
      (SourceRole.carrier, sourceBURL)
    ].compactMap { role, url -> (SourceRole, URL)? in
      guard let url else {
        return nil
      }
      return (role, url)
    }
    guard !selectedSources.isEmpty else {
      statusMessage = "Select Source A or Source B before extracting proxies."
      return
    }

    extractProxies(for: selectedSources)
  }

  /// Extract PNG/WAV proxies and analysis caches for the given roles, populating
  /// the render-input frame directories. Shared by the manual "Extract Proxies"
  /// button (all selected sources) and the automatic extract on source pick (the
  /// single newly chosen source).
  private func extractProxies(for selectedSources: [(SourceRole, URL)]) {
    let outputRootURL = mediaProxyOutputURL
    let frameRate = mediaProxyFrameRate
    let maxFrames = mediaProxyMaxFrames
    let selectedProjectURL = projectURL
    statusMessage = "Extracting PNG and WAV source proxies through morphogen-cli..."
    extractingProxyCount += 1

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        var results: [(SourceRole, MediaProxyExtractionCommandResult)] = []
        for (role, sourceURL) in selectedSources {
          let proxyName: String
          switch role {
          case .modulator:
            proxyName = "source-a"
          case .carrier:
            proxyName = "source-b"
          }
          let result = try RustBridgePlaceholder.extractMediaProxies(
            request: MediaProxyExtractionCommandRequest(
              sourceURL: sourceURL,
              proxyDirectoryURL: outputRootURL.appendingPathComponent(proxyName, isDirectory: true),
              framesPerSecond: frameRate,
              maxFrames: maxFrames,
              sampleRate: 48_000
            )
          )
          results.append((role, result))
        }

        var projectSummary: String?
        if let selectedProjectURL {
          for (role, result) in results {
            _ = try RustBridgePlaceholder.registerProjectSourceProxy(
              projectURL: selectedProjectURL,
              sourceRole: role,
              proxy: result
            )
          }
          let inspectResult = try RustBridgePlaceholder.inspectProject(projectURL: selectedProjectURL)
          projectSummary = Self.compactProjectSummary(inspectResult.summary)
        }

        DispatchQueue.main.async {
          for (role, result) in results {
            switch role {
            case .modulator:
              self.frameSequenceModulatorURL = result.frameDirectoryURL
              self.frameSequenceModulatorPath = result.frameDirectoryURL.path
              self.sourceARMSCacheURL = result.rmsCacheURL
              self.sourceASTFTCacheURL = result.stftCacheURL
            case .carrier:
              self.frameSequenceCarrierURL = result.frameDirectoryURL
              self.frameSequenceCarrierPath = result.frameDirectoryURL.path
              self.sourceBRMSCacheURL = result.rmsCacheURL
              self.sourceBSTFTCacheURL = result.stftCacheURL
            }
          }
          if let projectSummary {
            self.projectSummary = projectSummary
          }
          let projectText = selectedProjectURL == nil ? "" : " and recorded in the project"
          self.mediaProxySummary = "\(results.count) source proxy set(s) with RMS + STFT analysis caches at \(outputRootURL.path)\(projectText)"
          self.statusMessage = "Source proxy extraction and analysis caching complete\(projectText)."
          self.extractingProxyCount -= 1
        }
      } catch {
        DispatchQueue.main.async {
          self.mediaProxySummary = "Media proxy extraction failed: \(error.localizedDescription)"
          self.statusMessage = "Media proxy extraction failed: \(error.localizedDescription)"
          self.extractingProxyCount -= 1
        }
      }
    }
  }

  func runTwoSourceFrameSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering."
      return
    }

    let request = FrameSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultFrameSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      amount: frameSequenceAmount,
      maxFrames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      writesFlowCache: frameSequenceWritesFlowCache,
      projectURL: projectURL
    )

    statusMessage = "Queueing two-source frame sequence render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedFrameSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let cacheText = request.writesFlowCache ? ", flow cache persisted" : ""
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.frameSequenceSummary = "\(bundle.frameCount) PNG frame(s) at \(bundle.frameDirectory.path)\(cacheText)"
          self.proResExportSummary = "Queued frame sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Two-source frame sequence render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.frameSequenceSummary = "Two-source frame sequence render failed: \(error.localizedDescription)"
          self.statusMessage = "Two-source frame sequence render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runFlowFeedbackSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering flow feedback."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering flow feedback."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering flow feedback."
      return
    }

    let request = FeedbackSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultFeedbackSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      carrierAmount: feedbackCarrierAmount,
      feedbackAmount: feedbackAmount,
      feedbackMix: feedbackMix,
      decay: feedbackDecay,
      iterations: feedbackIterations,
      structureMix: feedbackStructureMix,
      outputBitDepth: feedbackOutputBitDepth,
      temporalSupersampling: feedbackTemporalSupersampling,
      maxFrames: effectiveMaxFrames(frameSequenceMaxFrames),
      resetAtFrame: feedbackResetEnabled ? feedbackResetAtFrame : nil,
      frameRate: proResFrameRate.framesPerSecond,
      writesFlowCache: feedbackWritesFlowCache,
      backend: feedbackBackend,
      flowSource: feedbackFlowSource,
      projectURL: projectURL
    )

    statusMessage = "Queueing temporal flow-feedback render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedFeedbackSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let resetText = request.resetAtFrame.map { ", reset at frame \($0)" } ?? ""
        let cacheText = request.writesFlowCache ? ", flow cache persisted" : ""
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.feedbackSummary = "\(bundle.frameCount) \(request.outputBitDepth.rawValue) feedback frame(s), \(request.temporalSupersampling)x temporal samples at \(bundle.frameDirectory.path)\(cacheText)\(resetText)"
          self.proResExportSummary = "Queued flow-feedback sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Temporal flow-feedback render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.feedbackSummary = "Temporal flow-feedback render failed: \(error.localizedDescription)"
          self.statusMessage = "Temporal flow-feedback render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runShowcasePreviewRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering a showcase preview."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering a showcase preview."
      return
    }
    guard let outputRootURL = frameSequenceOutputURL else {
      statusMessage = "Choose an output directory before rendering a showcase preview."
      return
    }

    let outputURL = outputRootURL.appendingPathComponent("showcase-preview", isDirectory: true)
    let request = ShowcaseRenderCommandRequest(
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputDirectoryURL: outputURL,
      intensity: showcaseIntensity,
      framesPerEffect: max(1, min(frameSequenceMaxFrames, 15)),
      frameRate: proResFrameRate.framesPerSecond,
      granularGrainSize: 48,
      seed: 20260625,
      backend: .cpu,
      encodeMP4: true
    )

    statusMessage = "Rendering showcase preview through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runShowcasePreview(request: request)
        DispatchQueue.main.async {
          self.lastFrameSequenceOutputURL = result.frameDirectoryURL
          self.showcaseSummary = "Showcase preview at \(result.outputDirectoryURL.path)"
          self.frameSequenceSummary = "Showcase PNG sequence at \(result.frameDirectoryURL.path)"
          self.proResExportSummary = "Showcase frames ready for ProRes export: \(result.frameDirectoryURL.path)"
          self.statusMessage = "Showcase preview complete: \(result.outputDirectoryURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.showcaseSummary = "Showcase preview failed: \(error.localizedDescription)"
          self.statusMessage = "Showcase preview failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runProceduralFluidAdvectSequenceRender() {
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering procedural fluid advection."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering procedural fluid advection."
      return
    }

    let request = FluidAdvectSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultFluidAdvectSequenceRenderQueueURL(),
      sourceDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("fluid-advect", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      advect: fluidProceduralAdvect,
      turbulenceScale: fluidTurbulenceScale,
      turbulenceSpeed: fluidTurbulenceSpeed,
      detail: fluidDetail,
      reinject: fluidReinject,
      seed: UInt64(max(0, fluidSeed)),
      backend: fluidBackend,
      projectURL: projectURL
    )

    runFluidAdvectionQueue(
      label: "Procedural fluid",
      requestDescription: "Queueing procedural fluid advection through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedFluidAdvectSequenceRender(request: request)
    }
  }

  func runTwoSourceFluidAdvectSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering two-source fluid advection."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering two-source fluid advection."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering two-source fluid advection."
      return
    }

    let request = FluidAdvectTwoSourceSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultFluidAdvectTwoSourceSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("fluid-advect-two-source", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      advect: fluidMotionAdvect,
      reinject: fluidReinject,
      backend: fluidBackend,
      projectURL: projectURL
    )

    runFluidAdvectionQueue(
      label: "A-to-B fluid",
      requestDescription: "Queueing A-to-B fluid advection through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedFluidAdvectTwoSourceSequenceRender(request: request)
    }
  }

  func runOpticalFlowAdvectSequenceRender() {
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering self-flow advection."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering self-flow advection."
      return
    }

    let request = OpticalFlowAdvectSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultOpticalFlowAdvectSequenceRenderQueueURL(),
      sourceDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("optical-flow-advect", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      advect: fluidMotionAdvect,
      reinject: fluidReinject,
      backend: fluidBackend,
      projectURL: projectURL
    )

    runFluidAdvectionQueue(
      label: "Self-flow advection",
      requestDescription: "Queueing self-flow advection through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedOpticalFlowAdvectSequenceRender(request: request)
    }
  }

  func runFieldParticlesSequenceRender() {
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering field particles."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering field particles."
      return
    }

    let request = FieldParticlesSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultFieldParticlesSequenceRenderQueueURL(),
      sourceDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("field-particles", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      spacing: fieldParticleSpacing,
      particleSize: fieldParticleSize,
      advect: fieldParticleAdvect,
      turbulenceScale: fluidTurbulenceScale,
      turbulenceSpeed: fluidTurbulenceSpeed,
      detail: fluidDetail,
      liveColour: fieldParticleLiveColour,
      seed: UInt64(max(0, fluidSeed)),
      backend: fluidBackend,
      projectURL: projectURL
    )

    runFluidAdvectionQueue(
      label: "Field particles",
      requestDescription: "Queueing field particles through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedFieldParticlesSequenceRender(request: request)
    }
  }

  func runTrailCascadeSequenceRender() {
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering the trail cascade."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering the trail cascade."
      return
    }

    let request = CascadeTrailsSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultCascadeTrailsSequenceRenderQueueURL(),
      sourceDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("trail-cascade", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      tileSize: cascadeTileSize,
      gridSpacing: cascadeGridSpacing,
      advect: cascadeAdvect,
      turbulenceScale: cascadeTurbulenceScale,
      detail: cascadeDetail,
      liveRefresh: cascadeLiveRefresh,
      seed: UInt64(max(0, cascadeSeed)),
      field: cascadeFieldType.cliValue,
      riverDirection: cascadeRiverDirection,
      riverSpeed: cascadeRiverSpeed,
      riverTurbulence: cascadeRiverTurbulence,
      temporalTiles: cascadeTemporalTiles,
      decay: cascadeDecay,
      projectURL: projectURL
    )

    runFluidAdvectionQueue(
      label: "Trail cascade",
      requestDescription: "Queueing trail cascade through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedCascadeTrailsSequenceRender(request: request)
    }
  }

  func runCascadeCollageSequenceRender() {
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering the cascade collage."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering the cascade collage."
      return
    }

    let request = CascadeCollageSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultCascadeCollageSequenceRenderQueueURL(),
      sourceDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("cascade-collage", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      scribAmpScale: cascadeCollageScribAmpScale,
      edgeStrength: cascadeCollageEdgeStrength,
      faceStrength: cascadeCollageFaceStrength,
      edgeDetect: cascadeCollageEdgeDetect,
      tileScale: cascadeCollageTileScale,
      detailTiles: cascadeCollageDetailTiles,
      hueRotate: cascadeCollageHueRotate,
      blockBlend: cascadeCollageBlockBlend,
      blockOpacity: cascadeCollageBlockOpacity,
      seed: UInt64(max(0, cascadeCollageSeed)),
      projectURL: projectURL
    )

    statusMessage = "Queueing cascade collage through morphogen-cli..."
    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedCascadeCollageSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.cascadeCollageSummary = "\(bundle.frameCount) cascade collage frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued cascade collage sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Cascade collage render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.cascadeCollageSummary = "Cascade collage render failed: \(error.localizedDescription)"
          self.statusMessage = "Cascade collage render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  /// Build the route list from per-knob mod slots, validating that the chosen
  /// sources have their modulator media picked. Returns `nil` (with a status
  /// message) when a required modulator is missing.
  private func modulationRoutes(
    slots: [(target: String, source: ModulationSourceOption, scale: Double, offset: Double)],
    modulatorAudioURL: URL?,
    modulatorFramesURL: URL?,
    effectLabel: String
  ) -> [ModulationRouteSpec]? {
    var routes: [ModulationRouteSpec] = []
    for slot in slots {
      guard let source = slot.source.cliValue else { continue }
      if slot.source.needsAudio && modulatorAudioURL == nil {
        statusMessage = "Pick a modulator WAV before rendering \(effectLabel) with an audio source."
        return nil
      }
      if slot.source.needsFrames && modulatorFramesURL == nil {
        statusMessage =
          "Pick a modulator frame directory before rendering \(effectLabel) with a luma/flow source."
        return nil
      }
      routes.append(
        ModulationRouteSpec(
          target: slot.target, source: source, scale: slot.scale, offset: slot.offset
        )
      )
    }
    return routes
  }

  func chooseRetroStaticModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knob."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    retroStaticModulatorAudioURL = url
    statusMessage = "Retro-static modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseRetroStaticModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knob."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    retroStaticModulatorFramesURL = url
    statusMessage = "Retro-static modulator frames selected: \(url.lastPathComponent)"
  }

  func chooseChannelShiftModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    channelShiftModulatorAudioURL = url
    statusMessage = "Channel-shift modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseChannelShiftModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    channelShiftModulatorFramesURL = url
    statusMessage = "Channel-shift modulator frames selected: \(url.lastPathComponent)"
  }

  func choosePaletteQuantizeModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    paletteQuantizeModulatorAudioURL = url
    statusMessage = "Palette-quantize modulator WAV selected: \(url.lastPathComponent)"
  }

  func choosePaletteQuantizeModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    paletteQuantizeModulatorFramesURL = url
    statusMessage = "Palette-quantize modulator frames selected: \(url.lastPathComponent)"
  }

  func choosePixelSortModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    pixelSortModulatorAudioURL = url
    statusMessage = "Pixel-sort modulator WAV selected: \(url.lastPathComponent)"
  }

  func choosePixelSortModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    pixelSortModulatorFramesURL = url
    statusMessage = "Pixel-sort modulator frames selected: \(url.lastPathComponent)"
  }

  func runRetroStaticSequenceRender() {
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering retro static."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering retro static."
      return
    }
    guard let routes = modulationRoutes(
      slots: [(
        "strength", retroStaticModStrengthSource,
        retroStaticModStrengthScale, retroStaticModStrengthOffset
      )],
      modulatorAudioURL: retroStaticModulatorAudioURL,
      modulatorFramesURL: retroStaticModulatorFramesURL,
      effectLabel: "retro static"
    ) else { return }

    let request = RetroStaticSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultRetroStaticSequenceRenderQueueURL(),
      sourceDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("retro-static", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      realBpp: retroStaticRealBpp,
      assumedBpp: retroStaticAssumedBpp,
      filter: retroStaticFilter,
      strength: retroStaticStrength,
      backend: retroStaticBackend,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: retroStaticModulatorAudioURL,
      modulatorFramesURL: retroStaticModulatorFramesURL,
      modulationSampling: retroStaticModSampling
    )

    statusMessage = "Queueing retro static through morphogen-cli..."
    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedRetroStaticSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.retroStaticSummary = "\(bundle.frameCount) retro-static frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued retro static sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Retro static render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.retroStaticSummary = "Retro static render failed: \(error.localizedDescription)"
          self.statusMessage = "Retro static render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runChannelShiftSequenceRender() {
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering channel shift."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering channel shift."
      return
    }
    if channelShiftFlowGain != 0 && frameSequenceModulatorURL == nil {
      statusMessage = "Select Source A frame directory before rendering flow-driven channel shift."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        ("shift_r_x", channelShiftModRXSource, channelShiftModRXScale, channelShiftModRXOffset),
        ("shift_r_y", channelShiftModRYSource, channelShiftModRYScale, channelShiftModRYOffset),
        ("shift_g_x", channelShiftModGXSource, channelShiftModGXScale, channelShiftModGXOffset),
        ("shift_g_y", channelShiftModGYSource, channelShiftModGYScale, channelShiftModGYOffset),
        ("shift_b_x", channelShiftModBXSource, channelShiftModBXScale, channelShiftModBXOffset),
        ("shift_b_y", channelShiftModBYSource, channelShiftModBYScale, channelShiftModBYOffset)
      ],
      modulatorAudioURL: channelShiftModulatorAudioURL,
      modulatorFramesURL: channelShiftModulatorFramesURL,
      effectLabel: "channel shift"
    ) else { return }

    let request = ChannelShiftSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultChannelShiftSequenceRenderQueueURL(),
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("channel-shift", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      shiftRX: channelShiftRX,
      shiftRY: channelShiftRY,
      shiftGX: channelShiftGX,
      shiftGY: channelShiftGY,
      shiftBX: channelShiftBX,
      shiftBY: channelShiftBY,
      sourceADirectoryURL: channelShiftFlowGain != 0 ? frameSequenceModulatorURL : nil,
      flowGain: channelShiftFlowGain,
      flowRadius: channelShiftFlowRadius,
      backend: channelShiftBackend,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: channelShiftModulatorAudioURL,
      modulatorFramesURL: channelShiftModulatorFramesURL,
      modulationSampling: channelShiftModSampling
    )

    statusMessage = "Queueing channel shift through morphogen-cli..."
    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedChannelShiftSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.channelShiftSummary = "\(bundle.frameCount) channel-shift frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued channel shift sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Channel shift render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.channelShiftSummary = "Channel shift render failed: \(error.localizedDescription)"
          self.statusMessage = "Channel shift render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runPaletteQuantizeSequenceRender() {
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering palette quantize."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering palette quantize."
      return
    }
    guard let routes = modulationRoutes(
      slots: [(
        "levels", paletteQuantizeModLevelsSource,
        paletteQuantizeModLevelsScale, paletteQuantizeModLevelsOffset
      )],
      modulatorAudioURL: paletteQuantizeModulatorAudioURL,
      modulatorFramesURL: paletteQuantizeModulatorFramesURL,
      effectLabel: "palette quantize"
    ) else { return }

    let request = PaletteQuantizeSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultPaletteQuantizeSequenceRenderQueueURL(),
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent(
        "palette-quantize", isDirectory: true
      ),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      mode: paletteQuantizeMode,
      levels: paletteQuantizeLevels,
      backend: paletteQuantizeBackend,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: paletteQuantizeModulatorAudioURL,
      modulatorFramesURL: paletteQuantizeModulatorFramesURL,
      modulationSampling: paletteQuantizeModSampling
    )

    statusMessage = "Queueing palette quantize through morphogen-cli..."
    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedPaletteQuantizeSequenceRender(
          request: request
        )
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.paletteQuantizeSummary =
            "\(bundle.frameCount) palette-quantize frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary =
            "Queued palette quantize sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Palette quantize render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.paletteQuantizeSummary =
            "Palette quantize render failed: \(error.localizedDescription)"
          self.statusMessage = "Palette quantize render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  private func runFluidAdvectionQueue(
    label: String,
    requestDescription: String,
    run: @escaping () throws -> FluidAdvectionRenderQueueCommandResult
  ) {
    statusMessage = requestDescription

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try run()
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.fluidAdvectionSummary = "\(bundle.frameCount) \(label.lowercased()) frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued \(label.lowercased()) sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "\(label) render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.fluidAdvectionSummary = "\(label) render failed: \(error.localizedDescription)"
          self.statusMessage = "\(label) render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runGranularMosaicPoolSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering the grain pool."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering the grain pool."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering the grain pool."
      return
    }

    let audioWeighted = granularPoolAudioWeighted
    let modulatorRMSCacheURL = audioWeighted ? sourceARMSCacheURL : nil
    let carrierRMSCacheURL = audioWeighted ? sourceBRMSCacheURL : nil
    if audioWeighted && (modulatorRMSCacheURL == nil || carrierRMSCacheURL == nil) {
      statusMessage = "Extract source proxies first to generate the RMS caches audio matching needs, or turn off Audio-Weighted."
      return
    }

    let centroidEnabled = granularPoolCentroidEnabled
    let modulatorCentroidCacheURL = centroidEnabled ? sourceASTFTCacheURL : nil
    let carrierCentroidCacheURL = centroidEnabled ? sourceBSTFTCacheURL : nil
    if centroidEnabled && (modulatorCentroidCacheURL == nil || carrierCentroidCacheURL == nil) {
      statusMessage = "Extract source proxies first to generate the STFT caches spectral-centroid matching needs, or turn off Spectral Centroid."
      return
    }

    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultGranularMosaicPoolSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      grainSize: granularPoolGrainSize,
      rearrangement: granularPoolRearrangement,
      variation: granularPoolVariation,
      seed: UInt64(max(0, granularPoolSeed)),
      audioWeight: granularPoolAudioWeight,
      textureWeight: granularPoolTextureWeight,
      modulatorRMSCacheURL: modulatorRMSCacheURL,
      carrierRMSCacheURL: carrierRMSCacheURL,
      modulatorCentroidCacheURL: modulatorCentroidCacheURL,
      carrierCentroidCacheURL: carrierCentroidCacheURL,
      poolWindow: max(0, granularPoolWindow),
      antiRepeatWeight: granularPoolAntiRepeatWeight,
      antiRepeatCooldown: max(0, granularPoolAntiRepeatCooldown),
      coherenceWeight: granularPoolCoherenceWeight,
      coherenceReach: max(0, granularPoolCoherenceReach),
      spatialCoherenceWeight: granularPoolSpatialCoherenceWeight,
      maxFrames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      backend: granularPoolBackend,
      projectURL: projectURL
    )

    statusMessage = "Queueing temporal grain pool (joint-AV) render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedGranularMosaicPoolSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let audioText = audioWeighted ? ", audio-weighted (RMS)" : ", color-only"
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.granularPoolSummary = "\(bundle.frameCount) grain-pool frame(s)\(audioText) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued grain pool sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Temporal grain pool render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.granularPoolSummary = "Temporal grain pool render failed: \(error.localizedDescription)"
          self.statusMessage = "Temporal grain pool render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runVideoVocoderSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering the video vocoder."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering the video vocoder."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering the video vocoder."
      return
    }

    let request = VideoVocoderSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultVideoVocoderSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      bands: vocoderBands,
      amount: vocoderAmount,
      mode: vocoderMode,
      maxFrames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      backend: vocoderBackend,
      projectURL: projectURL
    )

    statusMessage = "Queueing video vocoder render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedVideoVocoderSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let modeText = self.vocoderMode == .match ? "tonal-match" : "band-gain"
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.vocoderSummary = "\(bundle.frameCount) vocoder frame(s) (\(modeText)) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued video vocoder sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Video vocoder render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.vocoderSummary = "Video vocoder render failed: \(error.localizedDescription)"
          self.statusMessage = "Video vocoder render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runSpectralCrossSynthRender() {
    guard let modulatorURL = crossSynthModulatorURL else {
      statusMessage = "Select a Source A WAV before rendering the spectral cross-synth."
      return
    }
    guard let carrierURL = crossSynthCarrierURL else {
      statusMessage = "Select a Source B WAV before rendering the spectral cross-synth."
      return
    }
    guard let outputURL = crossSynthOutputURL else {
      statusMessage = "Choose an output directory before rendering the spectral cross-synth."
      return
    }

    let request = SpectralCrossSynthRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultSpectralCrossSynthRenderQueueURL(),
      modulatorWAVURL: modulatorURL,
      carrierWAVURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      mode: crossSynthMode,
      amount: crossSynthAmount,
      filterType: crossSynthFilterType,
      rmsWindow: crossSynthRmsWindow,
      rmsHop: crossSynthRmsHop,
      fftSize: crossSynthFFTSize,
      stftHop: crossSynthSTFTHop,
      window: crossSynthWindow,
      projectURL: projectURL
    )

    statusMessage = "Queueing spectral cross-synth render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedSpectralCrossSynthRender(request: request)
        let modeText = self.crossSynthMode == .gain ? "RMS-gain" : "centroid-filter"
        DispatchQueue.main.async {
          self.crossSynthSummary = "Cross-synth (\(modeText)) bundle at \(result.bundleURL.path)"
          self.statusMessage = "Spectral cross-synth render complete: \(result.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.crossSynthSummary = "Spectral cross-synth render failed: \(error.localizedDescription)"
          self.statusMessage = "Spectral cross-synth render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runAudioImpulseConvolutionRender() {
    guard let modulatorURL = impulseConvModulatorURL else {
      statusMessage = "Select a Source A WAV (impulse response) before rendering."
      return
    }
    guard let carrierURL = impulseConvCarrierURL else {
      statusMessage = "Select a Source B WAV before rendering the audio impulse convolution."
      return
    }
    guard let outputURL = impulseConvOutputURL else {
      statusMessage = "Choose an output directory before rendering the audio impulse convolution."
      return
    }

    let request = AudioImpulseConvolutionRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultAudioImpulseConvolutionRenderQueueURL(),
      modulatorWAVURL: modulatorURL,
      carrierWAVURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      amount: impulseConvAmount,
      maxImpulseSamples: impulseConvMaxSamples > 0 ? impulseConvMaxSamples : nil,
      useFFT: impulseConvUseFFT,
      resampleImpulse: impulseConvResample,
      usePerChannelIR: impulseConvPerChannel,
      projectURL: projectURL
    )

    statusMessage = "Queueing audio impulse convolution render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedAudioImpulseConvolutionRender(
          request: request
        )
        DispatchQueue.main.async {
          self.impulseConvSummary = "Impulse-convolution bundle at \(result.bundleURL.path)"
          self.statusMessage = "Audio impulse convolution render complete: \(result.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.impulseConvSummary =
            "Audio impulse convolution render failed: \(error.localizedDescription)"
          self.statusMessage =
            "Audio impulse convolution render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runAudioVideoRouteRender() {
    guard let modulatorURL = audioRouteModulatorURL else {
      statusMessage = "Select a Source A WAV before rendering the audio→video route."
      return
    }
    guard let carrierURL = audioRouteCarrierURL else {
      statusMessage = "Select a Source B frame directory before rendering the audio→video route."
      return
    }
    guard let outputURL = audioRouteOutputURL else {
      statusMessage = "Choose an output directory before rendering the audio→video route."
      return
    }

    let request = AudioVideoRouteSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultAudioVideoRouteSequenceRenderQueueURL(),
      modulatorWAVURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      amount: audioRouteAmount,
      shiftX: audioRouteShiftX,
      shiftY: audioRouteShiftY,
      rmsWindow: audioRouteRmsWindow,
      rmsHop: audioRouteRmsHop,
      frameRate: audioRouteFrameRate,
      maxFrames: nil,
      backend: audioRouteBackend,
      projectURL: projectURL
    )

    statusMessage = "Queueing audio→video route render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedAudioVideoRouteSequenceRender(
          request: request
        )
        DispatchQueue.main.async {
          self.audioRouteSummary = "Audio→video route bundle at \(result.bundleURL.path)"
          self.statusMessage = "Audio→video route render complete: \(result.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.audioRouteSummary = "Audio→video route render failed: \(error.localizedDescription)"
          self.statusMessage = "Audio→video route render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runDatamoshRender() {
    guard let modulatorURL = datamoshModulatorURL ?? frameSequenceModulatorURL else {
      statusMessage = "Select a Source A frame directory before rendering the datamosh."
      return
    }
    guard let carrierURL = datamoshCarrierURL ?? frameSequenceCarrierURL else {
      statusMessage = "Select a Source B frame directory before rendering the datamosh."
      return
    }
    guard let outputURL = effectiveOutputRoot(datamoshOutputURL ?? frameSequenceOutputURL) else {
      statusMessage = "Choose an output directory before rendering the datamosh."
      return
    }

    let request = DatamoshSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultDatamoshSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      keyframeInterval: datamoshKeyframeInterval,
      amount: datamoshAmount,
      blockSize: datamoshBlockSize,
      residualGain: datamoshResidualGain,
      residualDecay: datamoshResidualDecay,
      blockRefreshThreshold: datamoshBlockRefreshThreshold,
      vectorRemix: datamoshVectorRemix,
      preset: datamoshPreset,
      remixSeed: datamoshRemixSeed,
      maxFrames: effectiveOptionalMaxFrames(nil),
      backend: datamoshBackend,
      projectURL: projectURL,
      flowCacheDirectoryURL: datamoshReuseFlowCache
        ? RustBridgePlaceholder.defaultDatamoshFlowCacheRootURL()
        : nil
    )

    statusMessage = "Queueing datamosh render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedDatamoshSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.datamoshSummary = "\(bundle.frameCount) datamosh frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued datamosh sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Datamosh render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.datamoshSummary = "Datamosh render failed: \(error.localizedDescription)"
          self.statusMessage = "Datamosh render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runBitstreamDatamoshRender() {
    guard let inputURL = bitstreamInputVideoURL else {
      statusMessage = "Select an input video before rendering the bitstream datamosh."
      return
    }
    guard let outputURL = effectiveOutputRoot(bitstreamOutputURL) else {
      statusMessage = "Choose an output directory before rendering the bitstream datamosh."
      return
    }
    if bitstreamOperation == .motionTransfer && bitstreamCarrierVideoURL == nil {
      statusMessage = "Motion transfer requires a carrier video (Source B)."
      return
    }

    let request = BitstreamDatamoshRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultBitstreamDatamoshRenderQueueURL(),
      inputVideoURL: inputURL,
      outputRootDirectoryURL: outputURL,
      fps: bitstreamFps,
      operation: bitstreamOperation,
      pFrameIndex: bitstreamPFrameIndex,
      duplicateCount: bitstreamDuplicateCount,
      carrierVideoURL: bitstreamCarrierVideoURL,
      carrierKeyframes: bitstreamCarrierKeyframes,
      preset: bitstreamPreset,
      projectURL: projectURL
    )

    statusMessage = "Queueing bitstream datamosh render..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedBitstreamDatamoshRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.bitstreamSummary = "\(bundle.frameCount) bitstream datamosh frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued bitstream datamosh ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Bitstream datamosh render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.bitstreamSummary = "Bitstream datamosh failed: \(error.localizedDescription)"
          self.statusMessage = "Bitstream datamosh failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runVideoAudioRouteRender() {
    guard let modulatorURL = videoAudioRouteModulatorURL else {
      statusMessage = "Select a Source A frame directory before rendering the video→audio route."
      return
    }
    guard let carrierURL = videoAudioRouteCarrierURL else {
      statusMessage = "Select a Source B WAV before rendering the video→audio route."
      return
    }
    guard let outputURL = videoAudioRouteOutputURL else {
      statusMessage = "Choose an output directory before rendering the video→audio route."
      return
    }

    let request = VideoAudioRouteRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultVideoAudioRouteRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierWAVURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      descriptor: videoAudioRouteDescriptor,
      mode: videoAudioRouteMode,
      filterType: videoAudioRouteFilterType,
      sampling: videoAudioRouteSampling,
      amount: videoAudioRouteAmount,
      fps: videoAudioRouteFPS,
      projectURL: projectURL
    )

    statusMessage = "Queueing video→audio route render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedVideoAudioRouteRender(request: request)
        let modeText = "\(self.videoAudioRouteDescriptor.cliValue)-\(self.videoAudioRouteMode.cliValue)"
        DispatchQueue.main.async {
          self.videoAudioRouteSummary = "Video→audio route (\(modeText)) bundle at \(result.bundleURL.path)"
          self.statusMessage = "Video→audio route render complete: \(result.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.videoAudioRouteSummary = "Video→audio route render failed: \(error.localizedDescription)"
          self.statusMessage = "Video→audio route render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func chooseConvBlendModulatorDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source A Frames",
      message: "Select the modulator PNG frames that supply the convolution kernel."
    ) else {
      statusMessage = "Source A frame selection cancelled."
      return
    }

    convBlendModulatorURL = url
    statusMessage = "Convolution Source A frame directory selected: \(url.lastPathComponent)"
  }

  func chooseConvBlendCarrierDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source B Frames",
      message: "Select the carrier PNG frames to convolve."
    ) else {
      statusMessage = "Source B frame selection cancelled."
      return
    }

    convBlendCarrierURL = url
    statusMessage = "Convolution Source B frame directory selected: \(url.lastPathComponent)"
  }

  func chooseConvBlendOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Convolution output selection cancelled."
      return
    }

    convBlendOutputURL = url
    statusMessage = "Convolution output selected: \(url.lastPathComponent)"
  }

  func runConvolutionalBlendRender() {
    guard let modulatorURL = convBlendModulatorURL else {
      statusMessage = "Select a Source A frame directory before rendering the convolution blend."
      return
    }
    guard let carrierURL = convBlendCarrierURL else {
      statusMessage = "Select a Source B frame directory before rendering the convolution blend."
      return
    }
    guard let outputURL = convBlendOutputURL else {
      statusMessage = "Choose an output directory before rendering the convolution blend."
      return
    }

    let request = ConvolutionalBlendSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultConvolutionalBlendSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      kernelSize: convBlendKernelSize,
      amount: convBlendAmount,
      useColorKernels: convBlendColorMode,
      maxFrames: nil,
      backend: convBlendBackend,
      projectURL: projectURL
    )

    statusMessage = "Queueing convolutional blend render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedConvolutionalBlendSequenceRender(
          request: request
        )
        DispatchQueue.main.async {
          self.convBlendSummary = "Convolution blend bundle at \(result.bundleURL.path)"
          self.statusMessage = "Convolution blend render complete: \(result.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.convBlendSummary = "Convolution blend render failed: \(error.localizedDescription)"
          self.statusMessage = "Convolution blend render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func choosePixelSortModulatorDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source A Frames",
      message: "Select the modulator PNG frames that drive the sortability mask."
    ) else {
      statusMessage = "Source A frame selection cancelled."
      return
    }
    pixelSortModulatorURL = url
    statusMessage = "Pixel sort Source A frame directory selected: \(url.lastPathComponent)"
  }

  func choosePixelSortCarrierDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source B Frames",
      message: "Select the carrier PNG frames whose pixels are sorted."
    ) else {
      statusMessage = "Source B frame selection cancelled."
      return
    }
    pixelSortCarrierURL = url
    statusMessage = "Pixel sort Source B frame directory selected: \(url.lastPathComponent)"
  }

  func choosePixelSortOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Pixel sort output selection cancelled."
      return
    }
    pixelSortOutputURL = url
    statusMessage = "Pixel sort output selected: \(url.lastPathComponent)"
  }

  func runPixelSortRender() {
    guard let modulatorURL = pixelSortModulatorURL else {
      statusMessage = "Select a Source A frame directory before rendering pixel sort."
      return
    }
    guard let carrierURL = pixelSortCarrierURL else {
      statusMessage = "Select a Source B frame directory before rendering pixel sort."
      return
    }
    guard let outputURL = pixelSortOutputURL else {
      statusMessage = "Choose an output directory before rendering pixel sort."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        ("threshold_low", pixelSortModLowSource, pixelSortModLowScale, pixelSortModLowOffset),
        ("threshold_high", pixelSortModHighSource, pixelSortModHighScale, pixelSortModHighOffset),
      ],
      modulatorAudioURL: pixelSortModulatorAudioURL,
      modulatorFramesURL: pixelSortModulatorFramesURL,
      effectLabel: "pixel sort"
    ) else { return }

    let request = PixelSortSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultPixelSortSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      axis: pixelSortAxis,
      key: pixelSortKey,
      direction: pixelSortDirection,
      thresholdLow: pixelSortThresholdLow,
      thresholdHigh: pixelSortThresholdHigh,
      maxSpan: pixelSortMaxSpan,
      maskSource: pixelSortMaskSource,
      flowRadius: pixelSortFlowRadius,
      backend: pixelSortBackend,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: pixelSortModulatorAudioURL,
      modulatorFramesURL: pixelSortModulatorFramesURL,
      modulationSampling: pixelSortModSampling
    )

    statusMessage = "Queueing pixel sort render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedPixelSortSequenceRender(request: request)
        DispatchQueue.main.async {
          self.pixelSortSummary = "Pixel sort bundle at \(result.bundleURL.path)"
          self.statusMessage = "Pixel sort render complete: \(result.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.pixelSortSummary = "Pixel sort render failed: \(error.localizedDescription)"
          self.statusMessage = "Pixel sort render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func applyFeedbackPreset(_ preset: FeedbackPresetOption) {
    guard let settings = preset.settings else {
      return
    }

    feedbackCarrierAmount = settings.carrierAmount
    feedbackAmount = settings.feedbackAmount
    feedbackMix = settings.feedbackMix
    feedbackDecay = settings.decay
    feedbackStructureMix = settings.structureMix
    feedbackFlowSource = settings.flowSource
    feedbackBackend = settings.backend
    feedbackWritesFlowCache = settings.writesFlowCache
    feedbackResetEnabled = settings.resetAtFrame != nil
    if let resetAtFrame = settings.resetAtFrame {
      feedbackResetAtFrame = min(resetAtFrame, frameSequenceMaxFrames - 1)
    }
    statusMessage = "Applied flow-feedback preset: \(preset.rawValue)."
  }

  func checkProResExportPlan() {
    let selectedFrameRate = proResFrameRate.framesPerSecond
    let selectedProfile = proResProfile
    statusMessage = "Checking ProRes export support through VideoToolbox..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let plan = try VideoToolboxProResExportPlanner.makePlan(
          width: 1920,
          height: 1080,
          frameRate: selectedFrameRate,
          profile: selectedProfile
        )
        let support = VideoToolboxProResExportPlanner.probeSupport(for: plan)
        let proResEncoderCount = try VideoToolboxProResExportPlanner.availableProResEncoderSummaries().count
        let encoderSummary = proResEncoderCount == 1
          ? "1 ProRes encoder listed"
          : "\(proResEncoderCount) ProRes encoders listed"

        DispatchQueue.main.async {
          self.proResPlanSummary = "\(plan.compactSummary) | \(support.compactSummary) | \(encoderSummary)"
          self.statusMessage = support.isSupported
            ? "ProRes VideoToolbox support check complete."
            : "ProRes support check returned status \(support.status)."
        }
      } catch {
        DispatchQueue.main.async {
          self.proResPlanSummary = "ProRes plan unavailable: \(error.localizedDescription)"
          self.statusMessage = "ProRes check failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func exportProResMovie() {
    let selectedFrameRate = proResFrameRate.framesPerSecond
    let selectedProfile = proResProfile
    guard let frameDirectory = ImageSequenceExportPanel.chooseFrameDirectory() else {
      statusMessage = "ProRes export cancelled."
      return
    }
    guard let outputURL = ImageSequenceExportPanel.chooseMovieSaveLocation() else {
      statusMessage = "ProRes export cancelled."
      return
    }

    statusMessage = "Exporting PNG sequence to ProRes MOV..."

    DispatchQueue.global(qos: .userInitiated).async {
      Task {
        do {
          let result = try await ProResImageSequenceExporter.exportPNGSequence(
            request: ProResImageSequenceExportRequest(
              frameDirectory: frameDirectory,
              outputURL: outputURL,
              frameRate: selectedFrameRate,
              profile: selectedProfile,
              requiresHardwareEncoder: false
            )
          )
          await MainActor.run {
            self.proResExportSummary = result.compactSummary
            self.statusMessage = "ProRes export complete: \(outputURL.path)"
          }
        } catch {
          await MainActor.run {
            self.proResExportSummary = "ProRes export failed: \(error.localizedDescription)"
            self.statusMessage = "ProRes export failed: \(error.localizedDescription)"
          }
        }
      }
    }
  }

  func exportLastFrameSequenceProResMovie() {
    guard let frameDirectory = lastFrameSequenceOutputURL ?? frameSequenceOutputURL else {
      statusMessage = "Run a two-source frame sequence before exporting its ProRes movie."
      return
    }

    let selectedFrameRate = proResFrameRate.framesPerSecond
    let selectedProfile = proResProfile
    let defaultMovieName = "\(frameDirectory.lastPathComponent)-prores.mov"
    guard let outputURL = ImageSequenceExportPanel.chooseMovieSaveLocation(defaultName: defaultMovieName) else {
      statusMessage = "ProRes export cancelled."
      return
    }

    statusMessage = "Exporting two-source frame sequence to ProRes MOV..."

    DispatchQueue.global(qos: .userInitiated).async {
      Task {
        do {
          let result = try await ProResImageSequenceExporter.exportPNGSequence(
            request: ProResImageSequenceExportRequest(
              frameDirectory: frameDirectory,
              outputURL: outputURL,
              frameRate: selectedFrameRate,
              profile: selectedProfile,
              requiresHardwareEncoder: false
            )
          )
          await MainActor.run {
            self.proResExportSummary = result.compactSummary
            self.statusMessage = "Two-source ProRes export complete: \(outputURL.path)"
          }
        } catch {
          await MainActor.run {
            self.proResExportSummary = "Two-source ProRes export failed: \(error.localizedDescription)"
            self.statusMessage = "Two-source ProRes export failed: \(error.localizedDescription)"
          }
        }
      }
    }
  }

  func exportRenderQueueProResMovie() {
    let defaultBundleURL = RustBridgePlaceholder.defaultQueuedTestRenderBundleURL()
    let bundleURL = lastRenderQueueBundleURL ?? defaultBundleURL

    let inspectedBundle: RenderQueueOutputBundle
    do {
      inspectedBundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: bundleURL)
      applyRenderQueueTimingDefaults(inspectedBundle)
      renderQueueSummary = "\(inspectedBundle.compactSummary) at \(inspectedBundle.bundleURL.path)"
    } catch {
      statusMessage = "Run queued test render before exporting its ProRes movie."
      renderQueueSummary = "No queue output bundle found at \(defaultBundleURL.path): \(error.localizedDescription)"
      return
    }

    let selectedFrameRate = proResFrameRate.framesPerSecond
    let selectedProfile = proResProfile
    let defaultMovieName = "\(bundleURL.lastPathComponent)-prores.mov"
    guard let outputURL = ImageSequenceExportPanel.chooseMovieSaveLocation(defaultName: defaultMovieName) else {
      statusMessage = "ProRes export cancelled."
      return
    }

    statusMessage = "Exporting render queue image sequence to ProRes MOV..."

    DispatchQueue.global(qos: .userInitiated).async {
      Task {
        do {
          let result = try await ProResImageSequenceExporter.exportRenderQueueBundle(
            request: ProResRenderQueueBundleExportRequest(
              bundleURL: bundleURL,
              outputURL: outputURL,
              frameRate: selectedFrameRate,
              profile: selectedProfile,
              requiresHardwareEncoder: false
            )
          )
          await MainActor.run {
            self.lastRenderQueueBundleURL = result.bundle.bundleURL
            self.renderQueueSummary = "\(result.bundle.compactSummary) at \(result.bundle.bundleURL.path)"
            self.proResExportSummary = result.compactSummary
            self.statusMessage = "Render queue ProRes export complete: \(outputURL.path)"
          }
        } catch {
          await MainActor.run {
            self.proResExportSummary = "Render queue ProRes export failed: \(error.localizedDescription)"
            self.statusMessage = "Render queue ProRes export failed: \(error.localizedDescription)"
          }
        }
      }
    }
  }

  func createTestProject() {
    guard let outputURL = ProjectFilePanel.chooseProjectSaveLocation() else {
      statusMessage = "Create test project cancelled."
      return
    }

    statusMessage = "Creating test project through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        _ = try RustBridgePlaceholder.createExampleProject(outputURL: outputURL)
        let inspectResult = try RustBridgePlaceholder.inspectProject(projectURL: outputURL)
        DispatchQueue.main.async {
          self.projectURL = outputURL
          self.projectPath = outputURL.path
          self.projectSummary = Self.compactProjectSummary(inspectResult.summary)
          self.statusMessage = "Created project \(outputURL.lastPathComponent)."
        }
      } catch {
        DispatchQueue.main.async {
          self.statusMessage = "Project creation failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func openProject() {
    guard let url = ProjectFilePanel.chooseProjectFile() else {
      statusMessage = "Open project cancelled."
      return
    }

    statusMessage = "Validating project through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let inspectResult = try RustBridgePlaceholder.inspectProject(projectURL: url)
        DispatchQueue.main.async {
          self.projectURL = url
          self.projectPath = url.path
          self.projectSummary = Self.compactProjectSummary(inspectResult.summary)
          self.statusMessage = "Loaded project \(url.lastPathComponent)."
        }
      } catch {
        DispatchQueue.main.async {
          self.statusMessage = "Project load failed: \(error.localizedDescription)"
        }
      }
    }
  }

  private static func compactProbeSummary(_ text: String) -> String {
    let lines = text
      .split(whereSeparator: \.isNewline)
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }

    guard !lines.isEmpty else {
      return "Probe completed."
    }

    return lines.prefix(5).joined(separator: " | ")
  }

  private static func fallbackProbeSummary(mediaURL: URL, appleError: Error) -> String {
    do {
      let commandResult = try RustBridgePlaceholder.probeMedia(mediaURL: mediaURL)
      return "FFprobe fallback: \(compactProbeSummary(commandResult.summary))"
    } catch {
      return "Probe failed: AVFoundation \(appleError.localizedDescription); FFprobe \(error.localizedDescription)"
    }
  }

  private static func compactProjectSummary(_ text: String) -> String {
    let lines = text
      .split(whereSeparator: \.isNewline)
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }

    guard !lines.isEmpty else {
      return "Project validated."
    }

    return lines.prefix(8).joined(separator: " | ")
  }

  private func refreshProResPlanPreview() {
    proResPlanSummary = VideoToolboxProResExportPlanner.defaultPlanSummary(
      frameRate: proResFrameRate.framesPerSecond,
      profile: proResProfile
    )
  }

  private func applyRenderQueueTimingDefaults(_ bundle: RenderQueueOutputBundle) {
    guard let timing = bundle.timing,
          let frameRateOption = ProResFrameRateOption.matching(timing.frameRate)
    else {
      return
    }
    proResFrameRate = frameRateOption
  }
}

private struct EffectPreviewSession {
  let outputRootURL: URL
  let maxFrames: Int
}

enum RenderQualityOption: String, CaseIterable, Identifiable {
  case draftPreview = "Draft Preview"
  case highQualityOffline = "High Quality Offline"
  case floatMaster = "Float Master"

  var id: String { rawValue }
}

enum ExportFormatOption: String, CaseIterable, Identifiable {
  case pngSequence = "PNG Sequence"
  case exrSequence = "EXR Sequence"
  case proRes = "ProRes"
  case wavStems = "WAV Stems"

  var id: String { rawValue }
}

enum FeedbackFlowSourceOption: String, CaseIterable, Identifiable {
  case opticalFlow = "Optical Flow"
  case luminance = "Luminance Gradient"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .opticalFlow:
      return "optical-flow"
    case .luminance:
      return "luminance"
    }
  }
}

enum FeedbackRenderBackendOption: String, CaseIterable, Identifiable {
  case cpu = "CPU Reference"
  case metal = "Metal"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .cpu:
      return "cpu"
    case .metal:
      return "metal"
    }
  }
}

enum DatamoshVectorRemixOption: String, CaseIterable, Identifiable {
  case none = "None"
  case sort = "Sort (pool motion)"
  case shuffle = "Shuffle (scramble)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .none:
      return "none"
    case .sort:
      return "sort"
    case .shuffle:
      return "shuffle"
    }
  }
}

enum DatamoshPresetOption: String, CaseIterable, Identifiable {
  case custom = "Custom"
  case codecBloom = "Codec Bloom"
  case structuredMelt = "Structured Melt"
  case macroblockRot = "Macroblock Rot"
  case vectorShuffle = "Vector Shuffle"
  case scanlineSmear = "Scanline Smear"
  case codecEngrave = "Codec Engrave"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .custom:
      return "custom"
    case .codecBloom:
      return "codec-bloom"
    case .structuredMelt:
      return "structured-melt"
    case .macroblockRot:
      return "macroblock-rot"
    case .vectorShuffle:
      return "vector-shuffle"
    case .scanlineSmear:
      return "scanline-smear"
    case .codecEngrave:
      return "codec-engrave"
    }
  }
}

/// Modulation-matrix source choice for one knob's mod slot. `off` = the knob
/// stays a constant (no route emitted).
enum ModulationSourceOption: String, CaseIterable, Identifiable {
  case off = "Off"
  case audioRms = "Audio RMS"
  case audioOnset = "Audio Onset"
  case audioCentroid = "Audio Centroid"
  case luma = "Luma"
  case flow = "Flow"

  var id: String { rawValue }

  /// CLI route-grammar spelling; `nil` for `off`.
  var cliValue: String? {
    switch self {
    case .off:
      return nil
    case .audioRms:
      return "audio-rms"
    case .audioOnset:
      return "audio-onset"
    case .audioCentroid:
      return "audio-centroid"
    case .luma:
      return "luma"
    case .flow:
      return "flow"
    }
  }

  var needsAudio: Bool {
    switch self {
    case .audioRms, .audioOnset, .audioCentroid:
      return true
    default:
      return false
    }
  }

  var needsFrames: Bool {
    self == .luma || self == .flow
  }
}

enum ModulationSamplingOption: String, CaseIterable, Identifiable {
  case hold = "Hold"
  case smooth = "Smooth"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .hold:
      return "hold"
    case .smooth:
      return "smooth"
    }
  }
}

enum RetroStaticFilterOption: String, CaseIterable, Identifiable {
  case none = "None"
  case sub = "Sub"
  case up = "Up"
  case average = "Average"
  case paeth = "Paeth"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .none:
      return "none"
    case .sub:
      return "sub"
    case .up:
      return "up"
    case .average:
      return "average"
    case .paeth:
      return "paeth"
    }
  }
}

enum PaletteQuantizeModeOption: String, CaseIterable, Identifiable {
  case posterize = "Posterize"
  case palette = "Neon Palette"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .posterize:
      return "posterize"
    case .palette:
      return "palette"
    }
  }
}

enum CascadeCollageBlendOption: String, CaseIterable, Identifiable {
  case normal = "Normal"
  case multiply = "Multiply"
  case screen = "Screen"
  case average = "Average"
  case lighten = "Lighten"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .normal:
      return "normal"
    case .multiply:
      return "multiply"
    case .screen:
      return "screen"
    case .average:
      return "average"
    case .lighten:
      return "lighten"
    }
  }
}

enum CascadeFieldOption: String, CaseIterable, Identifiable {
  case vortex = "Vortex"
  case river = "River"
  case riverRoot = "River Root"
  case centerSplit = "Centre Split"
  case oscillate = "Oscillate"
  case squarePop = "Square Pop"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .vortex:
      return "vortex"
    case .river:
      return "river"
    case .riverRoot:
      return "river-root"
    case .centerSplit:
      return "center-split"
    case .oscillate:
      return "oscillate"
    case .squarePop:
      return "square-pop"
    }
  }
}

enum BitstreamOperationOption: String, CaseIterable, Identifiable {
  case pframeDuplicate = "P-Frame Bloom"
  case removeKeyframe = "Void Mosh"
  case motionTransfer = "Motion Transfer"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .pframeDuplicate:
      return "pframe-duplicate"
    case .removeKeyframe:
      return "remove-keyframe"
    case .motionTransfer:
      return "motion-transfer"
    }
  }
}

enum BitstreamPresetOption: String, CaseIterable, Identifiable {
  case custom = "Custom"
  case bloom = "Bloom"
  case heavyMelt = "Heavy Melt"
  case voidMosh = "Void Mosh"
  case motionGraft = "Motion Graft"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .custom:
      return "custom"
    case .bloom:
      return "bloom"
    case .heavyMelt:
      return "heavy-melt"
    case .voidMosh:
      return "void-mosh"
    case .motionGraft:
      return "motion-graft"
    }
  }
}

enum ShowcaseIntensityOption: String, CaseIterable, Identifiable {
  case balanced = "Balanced"
  case destructive = "Destructive"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .balanced:
      return "balanced"
    case .destructive:
      return "destructive"
    }
  }
}

enum VideoVocoderModeOption: String, CaseIterable, Identifiable {
  case match = "Match (tonal transfer)"
  case gain = "Gain (band routing)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .match:
      return "match"
    case .gain:
      return "gain"
    }
  }
}

enum CrossSynthModeOption: String, CaseIterable, Identifiable {
  case gain = "Gain (RMS → amplitude)"
  case filter = "Filter (centroid → cutoff)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .gain:
      return "gain"
    case .filter:
      return "filter"
    }
  }
}

enum VideoAudioRouteModeOption: String, CaseIterable, Identifiable {
  case gain = "Gain (descriptor → amplitude)"
  case pan = "Pan (descriptor → stereo position)"
  case filter = "Filter (descriptor → cutoff)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .gain:
      return "gain"
    case .pan:
      return "pan"
    case .filter:
      return "filter"
    }
  }
}

enum VideoAudioRouteDescriptorOption: String, CaseIterable, Identifiable {
  case luma = "Luma (brightness)"
  case flow = "Flow (motion)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .luma:
      return "luma"
    case .flow:
      return "flow"
    }
  }
}

enum VideoAudioRouteFilterTypeOption: String, CaseIterable, Identifiable {
  case lowpass = "Lowpass"
  case highpass = "Highpass"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .lowpass:
      return "lowpass"
    case .highpass:
      return "highpass"
    }
  }
}

enum VideoAudioRouteSamplingOption: String, CaseIterable, Identifiable {
  case hold = "Hold (stepped)"
  case smooth = "Smooth (interpolated)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .hold:
      return "hold"
    case .smooth:
      return "smooth"
    }
  }
}

enum CrossSynthFilterTypeOption: String, CaseIterable, Identifiable {
  case lowpass = "Lowpass"
  case highpass = "Highpass"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .lowpass:
      return "lowpass"
    case .highpass:
      return "highpass"
    }
  }
}

enum CrossSynthWindowOption: String, CaseIterable, Identifiable {
  case hann = "Hann"
  case hamming = "Hamming"
  case rectangular = "Rectangular"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .hann:
      return "hann"
    case .hamming:
      return "hamming"
    case .rectangular:
      return "rectangular"
    }
  }
}

enum FeedbackOutputBitDepthOption: String, CaseIterable, Identifiable {
  case png8 = "PNG 8-bit"
  case png16 = "PNG 16-bit"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .png8:
      return "8"
    case .png16:
      return "16"
    }
  }
}

struct FeedbackPresetSettings: Equatable {
  let carrierAmount: Double
  let feedbackAmount: Double
  let feedbackMix: Double
  let decay: Double
  let structureMix: Double
  let flowSource: FeedbackFlowSourceOption
  let backend: FeedbackRenderBackendOption
  let writesFlowCache: Bool
  let resetAtFrame: Int?
}

enum FeedbackPresetOption: String, CaseIterable, Identifiable {
  case stableTrails = "Stable Trails"
  case aggressiveDegradation = "Aggressive Degradation"
  case resetDrivenCuts = "Reset-Driven Cuts"
  case structuredMorph = "Structured Morph"
  case custom = "Custom"

  var id: String { rawValue }

  var settings: FeedbackPresetSettings? {
    switch self {
    case .stableTrails:
      return FeedbackPresetSettings(
        carrierAmount: 1.0,
        feedbackAmount: 1.5,
        feedbackMix: 0.68,
        decay: 0.99,
        structureMix: 0.0,
        flowSource: .opticalFlow,
        backend: .metal,
        writesFlowCache: true,
        resetAtFrame: nil
      )
    case .aggressiveDegradation:
      return FeedbackPresetSettings(
        carrierAmount: 2.5,
        feedbackAmount: 7.0,
        feedbackMix: 0.92,
        decay: 0.998,
        structureMix: 0.0,
        flowSource: .opticalFlow,
        backend: .metal,
        writesFlowCache: true,
        resetAtFrame: nil
      )
    case .resetDrivenCuts:
      return FeedbackPresetSettings(
        carrierAmount: 1.25,
        feedbackAmount: 3.5,
        feedbackMix: 0.84,
        decay: 0.99,
        structureMix: 0.0,
        flowSource: .opticalFlow,
        backend: .metal,
        writesFlowCache: true,
        resetAtFrame: 48
      )
    case .structuredMorph:
      // "Beyond recognition" as a structured morph: high feedback-mix so the
      // carrier stops re-asserting its composition, but structure re-injection
      // keeps regenerating high-frequency detail instead of washing to fog.
      // Settings follow the empirical lever sweep (mix ~0.97, decay ~0.97).
      return FeedbackPresetSettings(
        carrierAmount: 2.5,
        feedbackAmount: 7.0,
        feedbackMix: 0.97,
        decay: 0.97,
        structureMix: 0.6,
        flowSource: .opticalFlow,
        backend: .metal,
        writesFlowCache: true,
        resetAtFrame: nil
      )
    case .custom:
      return nil
    }
  }
}

enum ProResFrameRateOption: String, CaseIterable, Identifiable {
  case fps12 = "12 fps"
  case fps23976 = "23.976 fps"
  case fps24 = "24 fps"
  case fps25 = "25 fps"
  case fps2997 = "29.97 fps"
  case fps30 = "30 fps"
  case fps60 = "60 fps"

  var id: String { rawValue }

  var framesPerSecond: Double {
    switch self {
    case .fps12:
      return 12.0
    case .fps23976:
      return 24_000.0 / 1_001.0
    case .fps24:
      return 24.0
    case .fps25:
      return 25.0
    case .fps2997:
      return 30_000.0 / 1_001.0
    case .fps30:
      return 30.0
    case .fps60:
      return 60.0
    }
  }

  static func matching(_ frameRate: Double) -> ProResFrameRateOption? {
    guard frameRate.isFinite && frameRate > 0 else {
      return nil
    }
    return allCases.first { option in
      abs(option.framesPerSecond - frameRate) < 0.0005
    }
  }
}

enum PixelSortAxisOption: String, CaseIterable, Identifiable {
  case row = "Row"
  case col = "Col"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .row: return "row"
    case .col: return "col"
    }
  }
}

enum PixelSortKeyOption: String, CaseIterable, Identifiable {
  case luma = "Luma"
  case hue = "Hue"
  case sat = "Sat"
  case red = "Red"
  case green = "Green"
  case blue = "Blue"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .luma: return "luma"
    case .hue: return "hue"
    case .sat: return "sat"
    case .red: return "red"
    case .green: return "green"
    case .blue: return "blue"
    }
  }
}

enum PixelSortDirectionOption: String, CaseIterable, Identifiable {
  case asc = "Asc"
  case desc = "Desc"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .asc: return "asc"
    case .desc: return "desc"
    }
  }
}

enum PixelSortMaskSourceOption: String, CaseIterable, Identifiable {
  case selfMask = "Self (B drives mask)"
  case aLuma = "A Luma"
  case aEdge = "A Edge"
  case aFlow = "A Flow"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .selfMask: return "self"
    case .aLuma: return "a-luma"
    case .aEdge: return "a-edge"
    case .aFlow: return "a-flow"
    }
  }
}
