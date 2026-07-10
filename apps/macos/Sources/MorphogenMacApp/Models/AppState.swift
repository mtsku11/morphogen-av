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
  // Composition timeline (spec-file runner; docs/COMPOSITION_MILESTONE.md).
  @Published var compositionSpecURL: URL?
  @Published var compositionSpecPath = "No composition spec selected"
  @Published var compositionOutputURL: URL?
  @Published var compositionOutputPath = "No composition output directory selected"
  @Published var compositionSummary = "No composition rendered yet"
  // Coagulated flow blend (Tier 1.1). Two-source: A/B come from the shared
  // Source A/B slots; modulation drives coagulation_strength/edge_hardness/bias.
  @Published var coagOutputURL: URL?
  @Published var coagOutputPath = "No coagulated-blend output directory selected"
  @Published var coagSummary = "No coagulated blend rendered yet"
  @Published var coagPatchSize = 16
  @Published var coagColorWeight = 1.0
  @Published var coagTextureWeight = 0.0
  @Published var coagCoherencePasses = 2
  @Published var coagCoherenceStrength = 0.5
  @Published var coagRandomness = 0.0
  @Published var coagCoagulationStrength = 0.0
  @Published var coagEdgeHardness = 0.0
  @Published var coagEdgeDither = 0.0
  @Published var coagBlockJitter = 0.0
  @Published var coagBias = 0.0
  @Published var coagSeed = 0
  @Published var coagAdvectSource = CoagulationFlowSourceOption.aFlow
  @Published var coagAdvectAmount = 0.0
  @Published var coagRefresh = 1.0
  @Published var coagTurbulence = 1.0
  @Published var coagSmear = 0.0
  @Published var coagSmearDecay = 0.9
  @Published var coagBackend = FeedbackRenderBackendOption.cpu
  @Published var coagMaxFrames = 120
  @Published var coagStrengthModSource = ModulationSourceOption.off
  @Published var coagStrengthModScale = 1.0
  @Published var coagStrengthModOffset = 0.0
  @Published var coagStrengthModSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var coagStrengthModModulator = ""
  @Published var coagEdgeModSource = ModulationSourceOption.off
  @Published var coagEdgeModScale = 1.0
  @Published var coagEdgeModOffset = 0.0
  @Published var coagEdgeModSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var coagEdgeModModulator = ""
  @Published var coagBiasModSource = ModulationSourceOption.off
  @Published var coagBiasModScale = 1.0
  @Published var coagBiasModOffset = 0.0
  @Published var coagBiasModSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var coagBiasModModulator = ""
  @Published var coagModulatorAudioURL: URL?
  @Published var coagModulatorFramesURL: URL?
  @Published var coagModulationSampling = ModulationSamplingOption.hold
  @Published var coagNamedModulators: [NamedModulatorEntry] = []
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
  // Feedback mod slots — stateful: the routes join the render's checkpoint
  // contract, so a resumed job must keep them unchanged.
  @Published var feedbackModCarrierAmountSource = ModulationSourceOption.off
  @Published var feedbackModCarrierAmountScale = 1.0
  @Published var feedbackModCarrierAmountOffset = 0.0
  @Published var feedbackModCarrierAmountSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var feedbackModCarrierAmountModulator = ""
  @Published var feedbackModAmountSource = ModulationSourceOption.off
  @Published var feedbackModAmountScale = 1.0
  @Published var feedbackModAmountOffset = 0.0
  @Published var feedbackModAmountSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var feedbackModAmountModulator = ""
  @Published var feedbackModMixSource = ModulationSourceOption.off
  @Published var feedbackModMixScale = 1.0
  @Published var feedbackModMixOffset = 0.0
  @Published var feedbackModMixSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var feedbackModMixModulator = ""
  @Published var feedbackModDecaySource = ModulationSourceOption.off
  @Published var feedbackModDecayScale = 1.0
  @Published var feedbackModDecayOffset = 0.0
  @Published var feedbackModDecaySamplingOverride = ModulationSamplingOverrideOption.default
  @Published var feedbackModDecayModulator = ""
  @Published var feedbackModStructureMixSource = ModulationSourceOption.off
  @Published var feedbackModStructureMixScale = 1.0
  @Published var feedbackModStructureMixOffset = 0.0
  @Published var feedbackModStructureMixSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var feedbackModStructureMixModulator = ""
  @Published var feedbackModulatorAudioURL: URL?
  @Published var feedbackModulatorFramesURL: URL?
  @Published var feedbackModSampling = ModulationSamplingOption.hold
  @Published var feedbackNamedModulators: [NamedModulatorEntry] = []
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
  // Field-particles mod slots (advect / turbulence_scale / turbulence_speed / detail).
  // Shares the fluid panel's modulator audio/frames pickers and named modulators.
  @Published var particleModAdvectSource = ModulationSourceOption.off
  @Published var particleModAdvectScale = 1.0
  @Published var particleModAdvectOffset = 0.0
  @Published var particleModAdvectSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var particleModAdvectModulator = ""
  @Published var particleModTurbScaleSource = ModulationSourceOption.off
  @Published var particleModTurbScaleScale = 1.0
  @Published var particleModTurbScaleOffset = 0.0
  @Published var particleModTurbScaleSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var particleModTurbScaleModulator = ""
  @Published var particleModTurbSpeedSource = ModulationSourceOption.off
  @Published var particleModTurbSpeedScale = 1.0
  @Published var particleModTurbSpeedOffset = 0.0
  @Published var particleModTurbSpeedSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var particleModTurbSpeedModulator = ""
  @Published var particleModDetailSource = ModulationSourceOption.off
  @Published var particleModDetailScale = 1.0
  @Published var particleModDetailOffset = 0.0
  @Published var particleModDetailSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var particleModDetailModulator = ""
  @Published var fluidBackend = AppState.stickyBackend("backend.fluid", default: .metal) {
    didSet { AppState.persistBackend("backend.fluid", fluidBackend) }
  }
  @Published var fluidAdvectionSummary = "No fluid/advection sequence rendered"
  // Fluid mod slots shared by the three advect runs. Procedural Fluid consumes
  // the procedural-advect + turbulence/detail slots; A-to-B Fluid and Self-Flow
  // consume only the flow-advect + reinject slots (their CLI commands have no
  // turbulence targets).
  @Published var fluidModProceduralAdvectSource = ModulationSourceOption.off
  @Published var fluidModProceduralAdvectScale = 1.0
  @Published var fluidModProceduralAdvectOffset = 0.0
  @Published var fluidModProceduralAdvectSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var fluidModProceduralAdvectModulator = ""
  @Published var fluidModMotionAdvectSource = ModulationSourceOption.off
  @Published var fluidModMotionAdvectScale = 1.0
  @Published var fluidModMotionAdvectOffset = 0.0
  @Published var fluidModMotionAdvectSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var fluidModMotionAdvectModulator = ""
  @Published var fluidModTurbulenceScaleSource = ModulationSourceOption.off
  @Published var fluidModTurbulenceScaleScale = 0.008
  @Published var fluidModTurbulenceScaleOffset = 0.0
  @Published var fluidModTurbulenceScaleSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var fluidModTurbulenceScaleModulator = ""
  @Published var fluidModTurbulenceSpeedSource = ModulationSourceOption.off
  @Published var fluidModTurbulenceSpeedScale = 0.06
  @Published var fluidModTurbulenceSpeedOffset = 0.0
  @Published var fluidModTurbulenceSpeedSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var fluidModTurbulenceSpeedModulator = ""
  @Published var fluidModDetailSource = ModulationSourceOption.off
  @Published var fluidModDetailScale = 1.0
  @Published var fluidModDetailOffset = 0.0
  @Published var fluidModDetailSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var fluidModDetailModulator = ""
  @Published var fluidModReinjectSource = ModulationSourceOption.off
  @Published var fluidModReinjectScale = 1.0
  @Published var fluidModReinjectOffset = 0.0
  @Published var fluidModReinjectSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var fluidModReinjectModulator = ""
  @Published var fluidModulatorAudioURL: URL?
  @Published var fluidModulatorFramesURL: URL?
  @Published var fluidModSampling = ModulationSamplingOption.hold
  @Published var fluidNamedModulators: [NamedModulatorEntry] = []
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
  // Cascade Trails mod slots (advect / turbulence_scale / detail / decay).
  @Published var cascadeTrailsModulatorAudioURL: URL? = nil
  @Published var cascadeTrailsModulatorFramesURL: URL? = nil
  @Published var cascadeTrailsModSampling: ModulationSamplingOption = .hold
  @Published var cascadeTrailsNamedModulators: [NamedModulatorEntry] = []
  @Published var trailsModAdvectSource = ModulationSourceOption.off
  @Published var trailsModAdvectScale = 1.0
  @Published var trailsModAdvectOffset = 0.0
  @Published var trailsModAdvectSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var trailsModAdvectModulator = ""
  @Published var trailsModTurbScaleSource = ModulationSourceOption.off
  @Published var trailsModTurbScaleScale = 1.0
  @Published var trailsModTurbScaleOffset = 0.0
  @Published var trailsModTurbScaleSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var trailsModTurbScaleModulator = ""
  @Published var trailsModDetailSource = ModulationSourceOption.off
  @Published var trailsModDetailScale = 1.0
  @Published var trailsModDetailOffset = 0.0
  @Published var trailsModDetailSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var trailsModDetailModulator = ""
  @Published var trailsModDecaySource = ModulationSourceOption.off
  @Published var trailsModDecayScale = 1.0
  @Published var trailsModDecayOffset = 0.0
  @Published var trailsModDecaySamplingOverride = ModulationSamplingOverrideOption.default
  @Published var trailsModDecayModulator = ""
  // Cascade Collage mod slots (scrib_amp_scale / morph_rate / edge_strength / face_strength).
  @Published var cascadeCollageModulatorAudioURL: URL? = nil
  @Published var cascadeCollageModulatorFramesURL: URL? = nil
  @Published var cascadeCollageModSampling: ModulationSamplingOption = .hold
  @Published var cascadeCollageNamedModulators: [NamedModulatorEntry] = []
  @Published var collageModScribSource = ModulationSourceOption.off
  @Published var collageModScribScale = 1.0
  @Published var collageModScribOffset = 0.0
  @Published var collageModScribSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var collageModScribModulator = ""
  @Published var collageModMorphSource = ModulationSourceOption.off
  @Published var collageModMorphScale = 1.0
  @Published var collageModMorphOffset = 0.0
  @Published var collageModMorphSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var collageModMorphModulator = ""
  @Published var collageModEdgeSource = ModulationSourceOption.off
  @Published var collageModEdgeScale = 1.0
  @Published var collageModEdgeOffset = 0.0
  @Published var collageModEdgeSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var collageModEdgeModulator = ""
  @Published var collageModFaceSource = ModulationSourceOption.off
  @Published var collageModFaceScale = 1.0
  @Published var collageModFaceOffset = 0.0
  @Published var collageModFaceSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var collageModFaceModulator = ""
  // Dispersion blend — colour-group tile dispersion two-source.
  @Published var disperseOutputURL: URL?
  @Published var disperseOutputPath = "No dispersion-blend output directory selected"
  @Published var disperseSummary = "No dispersion blend rendered yet"
  @Published var disperseBlockSize = 8
  @Published var disperseCoagulationStrength = 1.6
  @Published var disperseBias = 0.4
  @Published var disperseScatterAmount = 3.0
  @Published var disperseDamping = 0.9
  @Published var disperseDispersionRamp = 24
  @Published var disperseOwnershipRefresh = 0.4
  @Published var disperseSmear = 0.0
  @Published var disperseMaxFrames = 120
  @Published var disperseModSampling = ModulationSamplingOption.hold
  @Published var disperseModulatorAudioURL: URL?
  @Published var disperseModulatorFramesURL: URL?
  @Published var disperseNamedModulators: [NamedModulatorEntry] = []
  @Published var disperseModStrengthSource = ModulationSourceOption.off
  @Published var disperseModStrengthScale = 1.0
  @Published var disperseModStrengthOffset = 0.0
  @Published var disperseModStrengthSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var disperseModStrengthModulator = ""
  @Published var disperseModBiasSource = ModulationSourceOption.off
  @Published var disperseModBiasScale = 1.0
  @Published var disperseModBiasOffset = 0.0
  @Published var disperseModBiasSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var disperseModBiasModulator = ""
  @Published var disperseModScatterSource = ModulationSourceOption.off
  @Published var disperseModScatterScale = 1.0
  @Published var disperseModScatterOffset = 0.0
  @Published var disperseModScatterSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var disperseModScatterModulator = ""
  @Published var disperseModDampingSource = ModulationSourceOption.off
  @Published var disperseModDampingScale = 1.0
  @Published var disperseModDampingOffset = 0.0
  @Published var disperseModDampingSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var disperseModDampingModulator = ""
  // Fluid mosaic — colour-group tile simulation (two-source, CPU only).
  @Published var mosaicOutputURL: URL?
  @Published var mosaicOutputPath = "No fluid-mosaic output directory selected"
  @Published var mosaicSummary = "No fluid mosaic rendered yet"
  @Published var mosaicTileSize = 8
  @Published var mosaicColorBins = 5
  @Published var mosaicCohesion = 0.035
  @Published var mosaicRepulsion = 1.4
  @Published var mosaicFluidStrength = 0.5
  @Published var mosaicDamping = 0.88
  @Published var mosaicSettleIterations = 60
  @Published var mosaicJitter = 0.03
  @Published var mosaicTurbulence = 0.0
  @Published var mosaicFrames = 120
  @Published var mosaicModSampling = ModulationSamplingOption.hold
  @Published var mosaicModulatorAudioURL: URL?
  @Published var mosaicModulatorFramesURL: URL?
  @Published var mosaicNamedModulators: [NamedModulatorEntry] = []
  @Published var mosaicModCohesionSource = ModulationSourceOption.off
  @Published var mosaicModCohesionScale = 0.05
  @Published var mosaicModCohesionOffset = 0.0
  @Published var mosaicModCohesionSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var mosaicModCohesionModulator = ""
  @Published var mosaicModRepulsionSource = ModulationSourceOption.off
  @Published var mosaicModRepulsionScale = 2.0
  @Published var mosaicModRepulsionOffset = 0.0
  @Published var mosaicModRepulsionSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var mosaicModRepulsionModulator = ""
  @Published var mosaicModFluidSource = ModulationSourceOption.off
  @Published var mosaicModFluidScale = 1.0
  @Published var mosaicModFluidOffset = 0.0
  @Published var mosaicModFluidSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var mosaicModFluidModulator = ""
  @Published var mosaicModTurbulenceSource = ModulationSourceOption.off
  @Published var mosaicModTurbulenceScale = 1.0
  @Published var mosaicModTurbulenceOffset = 0.0
  @Published var mosaicModTurbulenceSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var mosaicModTurbulenceModulator = ""
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
  @Published var retroStaticModStrengthSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var retroStaticModStrengthModulator = ""
  // Enum slot (From→To variant pickers): envelope 0 → From, envelope 1 → To.
  // Defaults span the full variant list so activating the slot sweeps it all.
  @Published var retroStaticModFilterSource = ModulationSourceOption.off
  @Published var retroStaticModFilterFrom = RetroStaticFilterOption.none
  @Published var retroStaticModFilterTo = RetroStaticFilterOption.paeth
  @Published var retroStaticModFilterSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var retroStaticModFilterModulator = ""
  @Published var retroStaticModulatorAudioURL: URL?
  @Published var retroStaticModulatorFramesURL: URL?
  @Published var retroStaticModSampling = ModulationSamplingOption.hold
  @Published var retroStaticNamedModulators: [NamedModulatorEntry] = []
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
  @Published var channelShiftModRXSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var channelShiftModRXModulator = ""
  @Published var channelShiftModRYSource = ModulationSourceOption.off
  @Published var channelShiftModRYScale = 1.0
  @Published var channelShiftModRYOffset = 0.0
  @Published var channelShiftModRYSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var channelShiftModRYModulator = ""
  @Published var channelShiftModGXSource = ModulationSourceOption.off
  @Published var channelShiftModGXScale = 1.0
  @Published var channelShiftModGXOffset = 0.0
  @Published var channelShiftModGXSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var channelShiftModGXModulator = ""
  @Published var channelShiftModGYSource = ModulationSourceOption.off
  @Published var channelShiftModGYScale = 1.0
  @Published var channelShiftModGYOffset = 0.0
  @Published var channelShiftModGYSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var channelShiftModGYModulator = ""
  @Published var channelShiftModBXSource = ModulationSourceOption.off
  @Published var channelShiftModBXScale = 1.0
  @Published var channelShiftModBXOffset = 0.0
  @Published var channelShiftModBXSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var channelShiftModBXModulator = ""
  @Published var channelShiftModBYSource = ModulationSourceOption.off
  @Published var channelShiftModBYScale = 1.0
  @Published var channelShiftModBYOffset = 0.0
  @Published var channelShiftModBYSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var channelShiftModBYModulator = ""
  @Published var channelShiftModulatorAudioURL: URL?
  @Published var channelShiftModulatorFramesURL: URL?
  @Published var channelShiftModSampling = ModulationSamplingOption.hold
  // Named modulators: extra modulator media a slot can bind to instead of the
  // single default `channelShiftModulator*URL`. Empty = only the default is
  // available (panels predating this stay visually unchanged).
  @Published var channelShiftNamedModulators: [NamedModulatorEntry] = []
  // Spatial matte (Tier 5.4 S2): gate the effect's blend per-pixel instead of
  // uniformly. Off = no matte (byte-identical to pre-slice behaviour). Frames
  // default to Source A (the flow-driven mode's modulator dir) when unset.
  @Published var channelShiftMatteSource = MatteSourceOption.off
  @Published var channelShiftMatteFramesURL: URL?
  @Published var channelShiftMatteGain = 1.0
  // Palette Quantize — posterize levels / neon-palette colour collapse.
  // Levels default 8 (visible posterize) rather than the CLI's 256 passthrough
  // so the first Run shows the effect; 256 stays reachable as the off case.
  @Published var paletteQuantizeMode = PaletteQuantizeModeOption.posterize
  @Published var paletteQuantizeLevels = 8
  @Published var paletteQuantizeBackend = AppState.stickyBackend("backend.paletteQuantize", default: .metal) {
    didSet { AppState.persistBackend("backend.paletteQuantize", paletteQuantizeBackend) }
  }
  @Published var paletteQuantizeSummary = "No palette-quantize sequence rendered"
  @Published var paletteQuantizeModLevelsSource = ModulationSourceOption.off
  @Published var paletteQuantizeModLevelsScale = 1.0
  @Published var paletteQuantizeModLevelsOffset = 0.0
  @Published var paletteQuantizeModLevelsSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var paletteQuantizeModLevelsModulator = ""
  // Enum slot (From→To variant pickers): envelope 0 → From, envelope 1 → To.
  @Published var paletteQuantizeModModeSource = ModulationSourceOption.off
  @Published var paletteQuantizeModModeFrom = PaletteQuantizeModeOption.posterize
  @Published var paletteQuantizeModModeTo = PaletteQuantizeModeOption.palette
  @Published var paletteQuantizeModModeSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var paletteQuantizeModModeModulator = ""
  @Published var paletteQuantizeModulatorAudioURL: URL?
  @Published var paletteQuantizeModulatorFramesURL: URL?
  @Published var paletteQuantizeModSampling = ModulationSamplingOption.hold
  @Published var paletteQuantizeNamedModulators: [NamedModulatorEntry] = []
  // Spatial matte (Tier 5.4 S2). No Source A concept on this single-source
  // command — matte frames must be chosen explicitly.
  @Published var paletteQuantizeMatteSource = MatteSourceOption.off
  @Published var paletteQuantizeMatteFramesURL: URL?
  @Published var paletteQuantizeMatteGain = 1.0
  // Rutt-Etra — luma-displaced scanlines on black (CPU-only; no backend
  // picker until the Metal slice lands).
  @Published var ruttEtraLinePitch = 8
  @Published var ruttEtraDisplacementDepth = 48.0
  @Published var ruttEtraLineThickness = 1
  @Published var ruttEtraMono = false
  /// When on, the shared Source A (modulator) drives the scanline displacement
  /// while Source B supplies the colour — two-source cross-synthesis. Persisted
  /// so the choice survives relaunch (the backend-picker precedent).
  @Published var ruttEtraUseTwoSource =
    UserDefaults.standard.bool(forKey: "ruttEtra.twoSource")
  {
    didSet { UserDefaults.standard.set(ruttEtraUseTwoSource, forKey: "ruttEtra.twoSource") }
  }
  @Published var ruttEtraSummary = "No rutt-etra sequence rendered"
  @Published var ruttEtraModDepthSource = ModulationSourceOption.off
  @Published var ruttEtraModDepthScale = 48.0
  @Published var ruttEtraModDepthOffset = 0.0
  @Published var ruttEtraModDepthSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var ruttEtraModDepthModulator = ""
  @Published var ruttEtraModDepthLfoShape = LfoShapeOption.sine
  @Published var ruttEtraModDepthLfoRate = 1.0
  @Published var ruttEtraModDepthLfoPhase = 0.0
  @Published var ruttEtraModPitchSource = ModulationSourceOption.off
  @Published var ruttEtraModPitchScale = 1.0
  @Published var ruttEtraModPitchOffset = 0.0
  @Published var ruttEtraModPitchSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var ruttEtraModPitchModulator = ""
  @Published var ruttEtraModPitchLfoShape = LfoShapeOption.sine
  @Published var ruttEtraModPitchLfoRate = 1.0
  @Published var ruttEtraModPitchLfoPhase = 0.0
  @Published var ruttEtraModThicknessSource = ModulationSourceOption.off
  @Published var ruttEtraModThicknessScale = 1.0
  @Published var ruttEtraModThicknessOffset = 0.0
  @Published var ruttEtraModThicknessSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var ruttEtraModThicknessModulator = ""
  @Published var ruttEtraModThicknessLfoShape = LfoShapeOption.sine
  @Published var ruttEtraModThicknessLfoRate = 1.0
  @Published var ruttEtraModThicknessLfoPhase = 0.0
  @Published var ruttEtraModulatorAudioURL: URL?
  @Published var ruttEtraModulatorFramesURL: URL?
  @Published var ruttEtraModulatorMidiURL: URL?
  // Per-slot CC controller numbers for the .midiCc source (0–127; 74 = the
  // classic filter-cutoff CC, docs/MIDI_MODULATION_MILESTONE.md S3).
  @Published var ruttEtraModDepthMidiCc = 74
  @Published var ruttEtraModPitchMidiCc = 74
  @Published var ruttEtraModThicknessMidiCc = 74
  @Published var ruttEtraModSampling = ModulationSamplingOption.hold
  @Published var ruttEtraNamedModulators: [NamedModulatorEntry] = []
  // Spatial matte (Tier 5.4 S2). Frames default to Source A (the two-source
  // modulator dir) when unset and Two-Source is on.
  @Published var ruttEtraMatteSource = MatteSourceOption.off
  @Published var ruttEtraMatteFramesURL: URL?
  @Published var ruttEtraMatteGain = 1.0
  // Performance capture (docs/PERFORMANCE_CAPTURE_MILESTONE.md): recorded
  // takes keyed by Rutt-Etra target name; re-recording replaces (the MVP edit
  // story). The strip state lives here (not the view) so takes survive view
  // reconstruction and the recorder rules stay unit-testable.
  @Published var ruttEtraCapturedTakes: [String: [GestureKnot]] = [:]
  @Published var captureSlider = 0.5
  @Published var captureTargetSelection = ""
  @Published var isCapturing = false
  private var captureRecorder: GestureRecorder?
  @Published var ruttEtraBackend = AppState.stickyBackend("backend.ruttEtra", default: .cpu) {
    didSet { AppState.persistBackend("backend.ruttEtra", ruttEtraBackend) }
  }
  // Morphogenesis — Gray-Scott reaction-diffusion (Tier "Morphogenesis" S4;
  // docs/MORPHOGENESIS_MILESTONE.md). CPU-only (stateful checkpoint; no Metal
  // slice yet) — no backend picker, unlike Rutt-Etra.
  @Published var morphogenesisPreset = MorphogenesisPresetOption.coral
  // Field View milestone (docs/MORPHOGENESIS_FIELD_VIEW_MILESTONE.md):
  // Composite (default) or Field (the raw V field, upsampled to carrier
  // resolution). Composite knobs below stay legal but inert in field view.
  @Published var morphogenesisOutputView = MorphogenesisOutputViewOption.composite
  @Published var morphogenesisPatternMix = 0.85
  @Published var morphogenesisDisplace = 0.0
  @Published var morphogenesisPatternHue = 0.02
  @Published var morphogenesisPatternColorMode = MorphogenesisColorModeOption.hue
  @Published var morphogenesisParamMapStrength = 1.0
  @Published var morphogenesisSeedThreshold = 0.5
  @Published var morphogenesisSimScale = 2
  @Published var morphogenesisSubsteps = 12
  // Live Coupling L-S3 (docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md) — the
  // "Alive" knobs; all default off/0, matching a pre-milestone render.
  @Published var morphogenesisInject = 0.0
  @Published var morphogenesisErode = 0.0
  @Published var morphogenesisInjectSource = MorphogenesisInjectSourceOption.motion
  @Published var morphogenesisCoverageTarget = 0.0
  // Track B1 relief shading (docs/MORPHOGENESIS_RELIEF_SHADING_MILESTONE.md);
  // all default off/pinned, matching a pre-slice render.
  @Published var morphogenesisShade = 0.0
  @Published var morphogenesisShadeHeight = 3.0
  @Published var morphogenesisShadeAzimuth = 0.0
  @Published var morphogenesisShadeElevation = 0.15
  @Published var morphogenesisShadeSpecular = 0.0
  @Published var morphogenesisShadeShininess = 16.0
  @Published var morphogenesisSummary = "No morphogenesis sequence rendered"
  @Published var morphogenesisModFeedSource = ModulationSourceOption.off
  @Published var morphogenesisModFeedScale = 0.014
  @Published var morphogenesisModFeedOffset = 0.0
  @Published var morphogenesisModFeedSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var morphogenesisModFeedModulator = ""
  @Published var morphogenesisModFeedLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModFeedLfoRate = 1.0
  @Published var morphogenesisModFeedLfoPhase = 0.0
  @Published var morphogenesisModFeedMidiCc = 74
  @Published var morphogenesisModKillSource = ModulationSourceOption.off
  @Published var morphogenesisModKillScale = 0.008
  @Published var morphogenesisModKillOffset = 0.0
  @Published var morphogenesisModKillSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var morphogenesisModKillModulator = ""
  @Published var morphogenesisModKillLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModKillLfoRate = 1.0
  @Published var morphogenesisModKillLfoPhase = 0.0
  @Published var morphogenesisModKillMidiCc = 74
  @Published var morphogenesisModParamMapStrengthSource = ModulationSourceOption.off
  @Published var morphogenesisModParamMapStrengthScale = 1.0
  @Published var morphogenesisModParamMapStrengthOffset = 0.0
  @Published var morphogenesisModParamMapStrengthSamplingOverride =
    ModulationSamplingOverrideOption.default
  @Published var morphogenesisModParamMapStrengthModulator = ""
  @Published var morphogenesisModParamMapStrengthLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModParamMapStrengthLfoRate = 1.0
  @Published var morphogenesisModParamMapStrengthLfoPhase = 0.0
  @Published var morphogenesisModParamMapStrengthMidiCc = 74
  @Published var morphogenesisModPatternMixSource = ModulationSourceOption.off
  @Published var morphogenesisModPatternMixScale = 1.0
  @Published var morphogenesisModPatternMixOffset = 0.0
  @Published var morphogenesisModPatternMixSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var morphogenesisModPatternMixModulator = ""
  @Published var morphogenesisModPatternMixLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModPatternMixLfoRate = 1.0
  @Published var morphogenesisModPatternMixLfoPhase = 0.0
  @Published var morphogenesisModPatternMixMidiCc = 74
  @Published var morphogenesisModDisplaceSource = ModulationSourceOption.off
  @Published var morphogenesisModDisplaceScale = 8.0
  @Published var morphogenesisModDisplaceOffset = 0.0
  @Published var morphogenesisModDisplaceSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var morphogenesisModDisplaceModulator = ""
  @Published var morphogenesisModDisplaceLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModDisplaceLfoRate = 1.0
  @Published var morphogenesisModDisplaceLfoPhase = 0.0
  @Published var morphogenesisModDisplaceMidiCc = 74
  @Published var morphogenesisModInjectSource = ModulationSourceOption.off
  @Published var morphogenesisModInjectScale = 1.0
  @Published var morphogenesisModInjectOffset = 0.0
  @Published var morphogenesisModInjectSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var morphogenesisModInjectModulator = ""
  @Published var morphogenesisModInjectLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModInjectLfoRate = 1.0
  @Published var morphogenesisModInjectLfoPhase = 0.0
  @Published var morphogenesisModInjectMidiCc = 74
  @Published var morphogenesisModErodeSource = ModulationSourceOption.off
  @Published var morphogenesisModErodeScale = 1.0
  @Published var morphogenesisModErodeOffset = 0.0
  @Published var morphogenesisModErodeSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var morphogenesisModErodeModulator = ""
  @Published var morphogenesisModErodeLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModErodeLfoRate = 1.0
  @Published var morphogenesisModErodeLfoPhase = 0.0
  @Published var morphogenesisModErodeMidiCc = 74
  @Published var morphogenesisModCoverageTargetSource = ModulationSourceOption.off
  @Published var morphogenesisModCoverageTargetScale = 1.0
  @Published var morphogenesisModCoverageTargetOffset = 0.0
  @Published var morphogenesisModCoverageTargetSamplingOverride =
    ModulationSamplingOverrideOption.default
  @Published var morphogenesisModCoverageTargetModulator = ""
  @Published var morphogenesisModCoverageTargetLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModCoverageTargetLfoRate = 1.0
  @Published var morphogenesisModCoverageTargetLfoPhase = 0.0
  @Published var morphogenesisModCoverageTargetMidiCc = 74
  @Published var morphogenesisModShadeSource = ModulationSourceOption.off
  @Published var morphogenesisModShadeScale = 1.0
  @Published var morphogenesisModShadeOffset = 0.0
  @Published var morphogenesisModShadeSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var morphogenesisModShadeModulator = ""
  @Published var morphogenesisModShadeLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModShadeLfoRate = 1.0
  @Published var morphogenesisModShadeLfoPhase = 0.0
  @Published var morphogenesisModShadeMidiCc = 74
  @Published var morphogenesisModShadeAzimuthSource = ModulationSourceOption.off
  @Published var morphogenesisModShadeAzimuthScale = 1.0
  @Published var morphogenesisModShadeAzimuthOffset = 0.0
  @Published var morphogenesisModShadeAzimuthSamplingOverride =
    ModulationSamplingOverrideOption.default
  @Published var morphogenesisModShadeAzimuthModulator = ""
  @Published var morphogenesisModShadeAzimuthLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModShadeAzimuthLfoRate = 1.0
  @Published var morphogenesisModShadeAzimuthLfoPhase = 0.0
  @Published var morphogenesisModShadeAzimuthMidiCc = 74
  @Published var morphogenesisModShadeHeightSource = ModulationSourceOption.off
  @Published var morphogenesisModShadeHeightScale = 1.0
  @Published var morphogenesisModShadeHeightOffset = 0.0
  @Published var morphogenesisModShadeHeightSamplingOverride =
    ModulationSamplingOverrideOption.default
  @Published var morphogenesisModShadeHeightModulator = ""
  @Published var morphogenesisModShadeHeightLfoShape = LfoShapeOption.sine
  @Published var morphogenesisModShadeHeightLfoRate = 1.0
  @Published var morphogenesisModShadeHeightLfoPhase = 0.0
  @Published var morphogenesisModShadeHeightMidiCc = 74
  @Published var morphogenesisModulatorAudioURL: URL?
  @Published var morphogenesisModulatorFramesURL: URL?
  @Published var morphogenesisModulatorMidiURL: URL?
  @Published var morphogenesisModSampling = ModulationSamplingOption.hold
  @Published var morphogenesisNamedModulators: [NamedModulatorEntry] = []
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
  @Published var crossSynthVocodeBands = 32
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
  // Datamosh mod slots — stateful: the routes join the render's checkpoint
  // contract. Tier activation follows the routed value per frame (a residual
  // gain envelope pulses the residual tier over a plain block base).
  @Published var datamoshModAmountSource = ModulationSourceOption.off
  @Published var datamoshModAmountScale = 1.0
  @Published var datamoshModAmountOffset = 0.0
  @Published var datamoshModAmountSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var datamoshModAmountModulator = ""
  @Published var datamoshModResidualGainSource = ModulationSourceOption.off
  @Published var datamoshModResidualGainScale = 1.0
  @Published var datamoshModResidualGainOffset = 0.0
  @Published var datamoshModResidualGainSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var datamoshModResidualGainModulator = ""
  @Published var datamoshModResidualDecaySource = ModulationSourceOption.off
  @Published var datamoshModResidualDecayScale = 1.0
  @Published var datamoshModResidualDecayOffset = 0.0
  @Published var datamoshModResidualDecaySamplingOverride = ModulationSamplingOverrideOption.default
  @Published var datamoshModResidualDecayModulator = ""
  @Published var datamoshModRefreshThresholdSource = ModulationSourceOption.off
  @Published var datamoshModRefreshThresholdScale = 1.0
  @Published var datamoshModRefreshThresholdOffset = 0.0
  @Published var datamoshModRefreshThresholdSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var datamoshModRefreshThresholdModulator = ""
  @Published var datamoshModulatorAudioURL: URL?
  @Published var datamoshModulatorFramesURL: URL?
  @Published var datamoshModSampling = ModulationSamplingOption.hold
  @Published var datamoshNamedModulators: [NamedModulatorEntry] = []
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
  @Published var pixelSortModLowSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var pixelSortModLowModulator = ""
  @Published var pixelSortModHighSource = ModulationSourceOption.off
  @Published var pixelSortModHighScale = 1.0
  @Published var pixelSortModHighOffset = 0.0
  @Published var pixelSortModHighSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var pixelSortModHighModulator = ""
  // Enum slots (From→To variant pickers): envelope 0 → From, envelope 1 → To.
  @Published var pixelSortModDirectionSource = ModulationSourceOption.off
  @Published var pixelSortModDirectionFrom = PixelSortDirectionOption.asc
  @Published var pixelSortModDirectionTo = PixelSortDirectionOption.desc
  @Published var pixelSortModDirectionSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var pixelSortModDirectionModulator = ""
  @Published var pixelSortModAxisSource = ModulationSourceOption.off
  @Published var pixelSortModAxisFrom = PixelSortAxisOption.row
  @Published var pixelSortModAxisTo = PixelSortAxisOption.col
  @Published var pixelSortModAxisSamplingOverride = ModulationSamplingOverrideOption.default
  @Published var pixelSortModAxisModulator = ""
  @Published var pixelSortModulatorAudioURL: URL?
  @Published var pixelSortModulatorFramesURL: URL?
  @Published var pixelSortModSampling = ModulationSamplingOption.hold
  @Published var pixelSortNamedModulators: [NamedModulatorEntry] = []

  @Published var mediaProxyOutputPath = RustBridgePlaceholder.defaultMediaProxyRootURL().path
  @Published var mediaProxySummary = "No source proxies extracted"
  @Published var mediaProxyFrameRate = 12.0
  @Published var mediaProxyMaxFrames = 120
  @Published var statusMessage = "Analysis cache idle. Offline queue empty."

  /// Preview downscale factor (box average, `downscale-frames`): 1 = full
  /// resolution (identity — no downscale run), 4 = the quarter-res default.
  @Published var previewScale = 4
  /// Seconds of motion a quick preview covers; the frame cap is
  /// `previewSeconds × proxy fps`, computed when the preview begins.
  @Published var previewSeconds = 4
  /// The proxy fps recorded when the current/last preview began — playback
  /// must use this, not the live proxy setting, so changing the extraction
  /// fps after the fact cannot shift an already-rendered preview's rate.
  @Published private(set) var previewPlaybackFps = 12.0
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
  /// invoked next and, because `previewSession` is set, it writes a capped frame
  /// count into a temp directory instead of the user's chosen output — reading
  /// its inputs from downscaled copies of the source proxies (the quarter-res
  /// fast path; same engine, only the input paths change). Returns `false`
  /// (and reports why) when the required sources are not loaded yet or the
  /// downscale fails.
  func beginEffectPreview(requiresModulator: Bool) -> Bool {
    // Read the RAW stored proxy dirs here (not the effective helpers): a
    // still-active previous session must never chain-downscale its own copies.
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frames before previewing."
      return false
    }
    if requiresModulator && frameSequenceModulatorURL == nil {
      statusMessage = "Select Source A frames before previewing."
      return false
    }

    let outputRoot = RustBridgePlaceholder.defaultEffectPreviewOutputRootURL()
    let fps = mediaProxyFrameRate
    let cap = previewFrameCap(seconds: previewSeconds, fps: fps)
    let scale = previewScale

    // Scale 1 is the identity anchor: both overrides are nil, so the preview
    // renders from the ORIGINAL proxy directories and no downscale runs.
    let carrierOverride = previewInputOverrideURL(
      previewRoot: outputRoot, scale: scale, label: "carrier"
    )
    let modulatorOverride = requiresModulator
      ? previewInputOverrideURL(previewRoot: outputRoot, scale: scale, label: "modulator")
      : nil

    do {
      if let carrierOverride {
        // Clear stale frames so a longer previous preview cannot leak extra
        // frames into this one, then downscale synchronously (the render
        // that consumes the copies is invoked immediately after).
        try? FileManager.default.removeItem(at: carrierOverride)
        _ = try RustBridgePlaceholder.runDownscaleFrames(
          inputDirectoryURL: carrierURL,
          outputDirectoryURL: carrierOverride,
          scale: scale,
          maxFrames: cap
        )
      }
      if let modulatorOverride, let modulatorURL = frameSequenceModulatorURL {
        try? FileManager.default.removeItem(at: modulatorOverride)
        _ = try RustBridgePlaceholder.runDownscaleFrames(
          inputDirectoryURL: modulatorURL,
          outputDirectoryURL: modulatorOverride,
          scale: scale,
          maxFrames: cap
        )
      }
    } catch {
      statusMessage = "Preview downscale failed: \(error.localizedDescription)"
      return false
    }

    previewSession = EffectPreviewSession(
      outputRootURL: outputRoot,
      maxFrames: cap,
      fps: fps,
      carrierInputOverrideURL: carrierOverride,
      modulatorInputOverrideURL: modulatorOverride
    )
    previewPlaybackFps = fps
    previewFrames = []
    isRenderingPreview = true
    previewSummary = "Rendering \(cap)-frame preview…"
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

  /// Carrier input directory a render should read: the preview session's
  /// downscaled copy while a preview is active (the same-engine invariant —
  /// only the input paths change), else the stored proxy directory.
  /// Mirrors `effectiveOutputRoot`. Nil override (scale 1) = original dir.
  private func effectiveCarrierURL() -> URL? {
    previewSession?.carrierInputOverrideURL ?? frameSequenceCarrierURL
  }

  /// Modulator counterpart of `effectiveCarrierURL()`.
  private func effectiveModulatorURL() -> URL? {
    previewSession?.modulatorInputOverrideURL ?? frameSequenceModulatorURL
  }

  private func finishPreviewIfNeeded(frameDirectory: URL) {
    guard let session = previewSession else {
      return
    }
    previewSession = nil
    let limit = session.maxFrames
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

  // MARK: - Coagulated flow blend (Tier 1.1)

  func chooseCoagulatedOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Coagulated-blend output selection cancelled."
      return
    }
    coagOutputURL = url
    coagOutputPath = url.path
    statusMessage = "Coagulated-blend output selected: \(url.lastPathComponent)"
  }

  func chooseCoagModulatorAudio() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Audio-* coagulated modulation routes read this WAV."
    ) else {
      statusMessage = "Coagulated modulator audio selection cancelled."
      return
    }
    coagModulatorAudioURL = url
    statusMessage = "Coagulated modulator audio: \(url.lastPathComponent)"
  }

  func runCoagulatedBlendSequenceRender() {
    guard let sourceAURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering coagulated blend."
      return
    }
    guard let sourceBURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering coagulated blend."
      return
    }
    guard let outputURL = effectiveOutputRoot(coagOutputURL) else {
      statusMessage = "Choose a coagulated-blend output directory before rendering."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        (
          "coagulation_strength", coagStrengthModSource,
          coagStrengthModScale, coagStrengthModOffset, coagStrengthModSamplingOverride
        ),
        (
          "edge_hardness", coagEdgeModSource,
          coagEdgeModScale, coagEdgeModOffset, coagEdgeModSamplingOverride
        ),
        (
          "bias", coagBiasModSource,
          coagBiasModScale, coagBiasModOffset, coagBiasModSamplingOverride
        )
      ],
      modulatorAudioURL: coagModulatorAudioURL,
      modulatorFramesURL: coagModulatorFramesURL,
      namedModulators: coagNamedModulators,
      slotModulators: [coagStrengthModModulator, coagEdgeModModulator, coagBiasModModulator],
      effectLabel: "coagulated blend"
    ) else { return }

    let request = CoagulatedBlendSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultCoagulatedBlendSequenceRenderQueueURL(),
      sourceADirectoryURL: sourceAURL,
      sourceBDirectoryURL: sourceBURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("coagulated-blend", isDirectory: true),
      frameRate: proResFrameRate.framesPerSecond,
      patchSize: coagPatchSize,
      colorWeight: coagColorWeight,
      textureWeight: coagTextureWeight,
      coherencePasses: coagCoherencePasses,
      coherenceStrength: coagCoherenceStrength,
      randomness: coagRandomness,
      coagulationStrength: coagCoagulationStrength,
      edgeHardness: coagEdgeHardness,
      edgeDither: coagEdgeDither,
      blockJitter: coagBlockJitter,
      bias: coagBias,
      seed: UInt64(max(0, coagSeed)),
      advectSource: coagAdvectSource,
      advectAmount: coagAdvectAmount,
      refresh: coagRefresh,
      turbulence: coagTurbulence,
      smear: coagSmear,
      smearDecay: coagSmearDecay,
      backend: coagBackend,
      maxFrames: coagMaxFrames > 0 ? coagMaxFrames : nil,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: coagModulatorAudioURL,
      modulatorFramesURL: coagModulatorFramesURL,
      modulationSampling: coagModulationSampling,
      namedModulators: namedModulatorSpecs(coagNamedModulators)
    )

    runFluidAdvectionQueue(
      label: "Coagulated blend",
      requestDescription: "Queueing coagulated blend through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedCoagulatedBlendSequenceRender(request: request)
    }
  }

  // MARK: - Composition timeline (docs/COMPOSITION_MILESTONE.md)

  func chooseCompositionSpecFile() {
    guard let url = ProjectFilePanel.chooseProjectFile() else {
      statusMessage = "Composition spec selection cancelled."
      return
    }
    compositionSpecURL = url
    compositionSpecPath = url.path
    statusMessage = "Composition spec selected: \(url.lastPathComponent)"
  }

  func chooseCompositionOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Composition output selection cancelled."
      return
    }
    compositionOutputURL = url
    compositionOutputPath = url.path
    statusMessage = "Composition output selected: \(url.lastPathComponent)"
  }

  /// Queue-add + queue-run a composition spec through the CLI bridge, then load
  /// the assembled timeline (`frames/`) into the preview so the finished piece
  /// can be scrubbed. Sources are per-scene inside the spec.
  func runComposition() {
    guard let specURL = compositionSpecURL else {
      statusMessage = "Choose a composition spec file before rendering."
      compositionSummary = "No composition spec selected."
      return
    }
    guard let outputURL = compositionOutputURL else {
      statusMessage = "Choose a composition output directory before rendering."
      compositionSummary = "No composition output directory selected."
      return
    }

    let request = CompositionRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultCompositionRenderQueueURL(),
      specURL: specURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("composition", isDirectory: true),
      projectURL: projectURL
    )

    compositionSummary = "Queueing composition…"
    statusMessage = "Queueing composition \(specURL.lastPathComponent) through morphogen-cli…"

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedCompositionRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let frames = Self.loadPreviewFrames(from: bundle.frameDirectory, limit: 480)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.previewFrames = frames
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.proResExportSummary =
            "Queued composition ready for ProRes export: \(bundle.bundleURL.path)"
          self.compositionSummary =
            "\(bundle.frameCount) timeline frame(s) at \(bundle.frameDirectory.path)"
          self.statusMessage = "Composition render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.compositionSummary = "Composition render failed: \(error.localizedDescription)"
          self.statusMessage = "Composition render failed: \(error.localizedDescription)"
        }
      }
    }
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
    guard let modulatorURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering."
      return
    }
    guard let carrierURL = effectiveCarrierURL() else {
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
    guard let modulatorURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering flow feedback."
      return
    }
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering flow feedback."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering flow feedback."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        (
          "carrier_amount", feedbackModCarrierAmountSource,
          feedbackModCarrierAmountScale, feedbackModCarrierAmountOffset,
          feedbackModCarrierAmountSamplingOverride
        ),
        (
          "feedback_amount", feedbackModAmountSource, feedbackModAmountScale, feedbackModAmountOffset,
          feedbackModAmountSamplingOverride
        ),
        (
          "feedback_mix", feedbackModMixSource, feedbackModMixScale, feedbackModMixOffset,
          feedbackModMixSamplingOverride
        ),
        (
          "decay", feedbackModDecaySource, feedbackModDecayScale, feedbackModDecayOffset,
          feedbackModDecaySamplingOverride
        ),
        (
          "structure_mix", feedbackModStructureMixSource,
          feedbackModStructureMixScale, feedbackModStructureMixOffset,
          feedbackModStructureMixSamplingOverride
        )
      ],
      modulatorAudioURL: feedbackModulatorAudioURL,
      modulatorFramesURL: feedbackModulatorFramesURL,
      namedModulators: feedbackNamedModulators,
      slotModulators: [
        feedbackModCarrierAmountModulator, feedbackModAmountModulator,
        feedbackModMixModulator, feedbackModDecayModulator,
        feedbackModStructureMixModulator
      ],
      effectLabel: "flow feedback"
    ) else { return }

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
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: feedbackModulatorAudioURL,
      modulatorFramesURL: feedbackModulatorFramesURL,
      modulationSampling: feedbackModSampling,
      namedModulators: namedModulatorSpecs(feedbackNamedModulators)
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
    guard let modulatorURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering a showcase preview."
      return
    }
    guard let carrierURL = effectiveCarrierURL() else {
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
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering procedural fluid advection."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering procedural fluid advection."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        (
          "advect", fluidModProceduralAdvectSource,
          fluidModProceduralAdvectScale, fluidModProceduralAdvectOffset,
          fluidModProceduralAdvectSamplingOverride
        ),
        (
          "turbulence_scale", fluidModTurbulenceScaleSource,
          fluidModTurbulenceScaleScale, fluidModTurbulenceScaleOffset,
          fluidModTurbulenceScaleSamplingOverride
        ),
        (
          "turbulence_speed", fluidModTurbulenceSpeedSource,
          fluidModTurbulenceSpeedScale, fluidModTurbulenceSpeedOffset,
          fluidModTurbulenceSpeedSamplingOverride
        ),
        (
          "detail", fluidModDetailSource, fluidModDetailScale, fluidModDetailOffset,
          fluidModDetailSamplingOverride
        ),
        (
          "reinject", fluidModReinjectSource, fluidModReinjectScale, fluidModReinjectOffset,
          fluidModReinjectSamplingOverride
        )
      ],
      modulatorAudioURL: fluidModulatorAudioURL,
      modulatorFramesURL: fluidModulatorFramesURL,
      namedModulators: fluidNamedModulators,
      slotModulators: [
        fluidModProceduralAdvectModulator, fluidModTurbulenceScaleModulator,
        fluidModTurbulenceSpeedModulator, fluidModDetailModulator,
        fluidModReinjectModulator
      ],
      effectLabel: "procedural fluid advection"
    ) else { return }

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
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: fluidModulatorAudioURL,
      modulatorFramesURL: fluidModulatorFramesURL,
      modulationSampling: fluidModSampling,
      namedModulators: namedModulatorSpecs(fluidNamedModulators)
    )

    runFluidAdvectionQueue(
      label: "Procedural fluid",
      requestDescription: "Queueing procedural fluid advection through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedFluidAdvectSequenceRender(request: request)
    }
  }

  func runTwoSourceFluidAdvectSequenceRender() {
    guard let modulatorURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering two-source fluid advection."
      return
    }
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering two-source fluid advection."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering two-source fluid advection."
      return
    }
    // Only the flow-advect + reinject slots apply — the flow-driven commands
    // have no turbulence targets.
    guard let routes = modulationRoutes(
      slots: [
        (
          "advect", fluidModMotionAdvectSource, fluidModMotionAdvectScale, fluidModMotionAdvectOffset,
          fluidModMotionAdvectSamplingOverride
        ),
        (
          "reinject", fluidModReinjectSource, fluidModReinjectScale, fluidModReinjectOffset,
          fluidModReinjectSamplingOverride
        )
      ],
      modulatorAudioURL: fluidModulatorAudioURL,
      modulatorFramesURL: fluidModulatorFramesURL,
      namedModulators: fluidNamedModulators,
      slotModulators: [fluidModMotionAdvectModulator, fluidModReinjectModulator],
      effectLabel: "A-to-B fluid advection"
    ) else { return }

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
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: fluidModulatorAudioURL,
      modulatorFramesURL: fluidModulatorFramesURL,
      modulationSampling: fluidModSampling,
      namedModulators: namedModulatorSpecs(fluidNamedModulators)
    )

    runFluidAdvectionQueue(
      label: "A-to-B fluid",
      requestDescription: "Queueing A-to-B fluid advection through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedFluidAdvectTwoSourceSequenceRender(request: request)
    }
  }

  func runOpticalFlowAdvectSequenceRender() {
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering self-flow advection."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering self-flow advection."
      return
    }
    // Only the flow-advect + reinject slots apply — the flow-driven commands
    // have no turbulence targets.
    guard let routes = modulationRoutes(
      slots: [
        (
          "advect", fluidModMotionAdvectSource, fluidModMotionAdvectScale, fluidModMotionAdvectOffset,
          fluidModMotionAdvectSamplingOverride
        ),
        (
          "reinject", fluidModReinjectSource, fluidModReinjectScale, fluidModReinjectOffset,
          fluidModReinjectSamplingOverride
        )
      ],
      modulatorAudioURL: fluidModulatorAudioURL,
      modulatorFramesURL: fluidModulatorFramesURL,
      namedModulators: fluidNamedModulators,
      slotModulators: [fluidModMotionAdvectModulator, fluidModReinjectModulator],
      effectLabel: "self-flow advection"
    ) else { return }

    let request = OpticalFlowAdvectSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultOpticalFlowAdvectSequenceRenderQueueURL(),
      sourceDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("optical-flow-advect", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      advect: fluidMotionAdvect,
      reinject: fluidReinject,
      backend: fluidBackend,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: fluidModulatorAudioURL,
      modulatorFramesURL: fluidModulatorFramesURL,
      modulationSampling: fluidModSampling,
      namedModulators: namedModulatorSpecs(fluidNamedModulators)
    )

    runFluidAdvectionQueue(
      label: "Self-flow advection",
      requestDescription: "Queueing self-flow advection through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedOpticalFlowAdvectSequenceRender(request: request)
    }
  }

  func runFieldParticlesSequenceRender() {
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering field particles."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering field particles."
      return
    }

    guard let routes = modulationRoutes(
      slots: [
        ("advect", particleModAdvectSource, particleModAdvectScale, particleModAdvectOffset,
         particleModAdvectSamplingOverride),
        ("turbulence_scale", particleModTurbScaleSource, particleModTurbScaleScale, particleModTurbScaleOffset,
         particleModTurbScaleSamplingOverride),
        ("turbulence_speed", particleModTurbSpeedSource, particleModTurbSpeedScale, particleModTurbSpeedOffset,
         particleModTurbSpeedSamplingOverride),
        ("detail", particleModDetailSource, particleModDetailScale, particleModDetailOffset,
         particleModDetailSamplingOverride),
      ],
      modulatorAudioURL: fluidModulatorAudioURL,
      modulatorFramesURL: fluidModulatorFramesURL,
      namedModulators: fluidNamedModulators,
      slotModulators: [
        particleModAdvectModulator, particleModTurbScaleModulator,
        particleModTurbSpeedModulator, particleModDetailModulator,
      ],
      effectLabel: "field particles"
    ) else { return }

    var request = FieldParticlesSequenceRenderQueueCommandRequest(
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
    request.modulationRoutes = routes
    request.modulatorAudioURL = fluidModulatorAudioURL
    request.modulatorFramesURL = fluidModulatorFramesURL
    request.modulationSampling = fluidModSampling
    request.namedModulators = namedModulatorSpecs(fluidNamedModulators)

    runFluidAdvectionQueue(
      label: "Field particles",
      requestDescription: "Queueing field particles through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedFieldParticlesSequenceRender(request: request)
    }
  }

  func runTrailCascadeSequenceRender() {
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering the trail cascade."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering the trail cascade."
      return
    }

    guard let routes = modulationRoutes(
      slots: [
        ("advect", trailsModAdvectSource, trailsModAdvectScale, trailsModAdvectOffset, trailsModAdvectSamplingOverride),
        ("turbulence_scale", trailsModTurbScaleSource, trailsModTurbScaleScale, trailsModTurbScaleOffset, trailsModTurbScaleSamplingOverride),
        ("detail", trailsModDetailSource, trailsModDetailScale, trailsModDetailOffset, trailsModDetailSamplingOverride),
        ("decay", trailsModDecaySource, trailsModDecayScale, trailsModDecayOffset, trailsModDecaySamplingOverride),
      ],
      modulatorAudioURL: cascadeTrailsModulatorAudioURL,
      modulatorFramesURL: cascadeTrailsModulatorFramesURL,
      namedModulators: cascadeTrailsNamedModulators,
      slotModulators: [trailsModAdvectModulator, trailsModTurbScaleModulator, trailsModDetailModulator, trailsModDecayModulator],
      effectLabel: "cascade trails"
    ) else { return }
    var request = CascadeTrailsSequenceRenderQueueCommandRequest(
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
    request.modulationRoutes = routes
    request.modulatorAudioURL = cascadeTrailsModulatorAudioURL
    request.modulatorFramesURL = cascadeTrailsModulatorFramesURL
    request.modulationSampling = cascadeTrailsModSampling
    request.namedModulators = namedModulatorSpecs(cascadeTrailsNamedModulators)

    runFluidAdvectionQueue(
      label: "Trail cascade",
      requestDescription: "Queueing trail cascade through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedCascadeTrailsSequenceRender(request: request)
    }
  }

  func runCascadeCollageSequenceRender() {
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering the cascade collage."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering the cascade collage."
      return
    }

    guard let routes = modulationRoutes(
      slots: [
        ("scrib_amp_scale", collageModScribSource, collageModScribScale, collageModScribOffset, collageModScribSamplingOverride),
        ("morph_rate", collageModMorphSource, collageModMorphScale, collageModMorphOffset, collageModMorphSamplingOverride),
        ("edge_strength", collageModEdgeSource, collageModEdgeScale, collageModEdgeOffset, collageModEdgeSamplingOverride),
        ("face_strength", collageModFaceSource, collageModFaceScale, collageModFaceOffset, collageModFaceSamplingOverride),
      ],
      modulatorAudioURL: cascadeCollageModulatorAudioURL,
      modulatorFramesURL: cascadeCollageModulatorFramesURL,
      namedModulators: cascadeCollageNamedModulators,
      slotModulators: [collageModScribModulator, collageModMorphModulator, collageModEdgeModulator, collageModFaceModulator],
      effectLabel: "cascade collage"
    ) else { return }
    var request = CascadeCollageSequenceRenderQueueCommandRequest(
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
    request.modulationRoutes = routes
    request.modulatorAudioURL = cascadeCollageModulatorAudioURL
    request.modulatorFramesURL = cascadeCollageModulatorFramesURL
    request.modulationSampling = cascadeCollageModSampling
    request.namedModulators = namedModulatorSpecs(cascadeCollageNamedModulators)

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
    slots: [(
      target: String, source: ModulationSourceOption, scale: Double, offset: Double,
      sampling: ModulationSamplingOverrideOption
    )],
    modulatorAudioURL: URL?,
    modulatorFramesURL: URL?,
    // Default modulator MIDI file; nil (the default) leaves panels without a
    // MIDI story unchanged (docs/MIDI_MODULATION_MILESTONE.md S3).
    modulatorMidiURL: URL? = nil,
    namedModulators: [NamedModulatorEntry] = [],
    // Parallel to `slots`: the named modulator each slot binds to ("" = default).
    // Empty (the default) leaves every slot on the default modulator, so callers
    // predating named modulators need no change.
    slotModulators: [String] = [],
    // Parallel to `slots`: the LFO params for a slot whose source is `.lfo`.
    // Empty (the default) leaves every slot media-sourced, so callers whose
    // panels don't opt in to LFO need no change.
    slotLfos: [(shape: LfoShapeOption, rate: Double, phase: Double)?] = [],
    // Parallel to `slots`: the pre-formatted `breakpoints(...)` clause for a
    // slot whose source is `.captured` (via `capturedSourceSpec`). Empty (the
    // default) leaves every slot untouched, so callers whose panels don't opt
    // in to capture need no change — the `slotLfos` churn-avoider precedent.
    slotCaptures: [String?] = [],
    // Parallel to `slots`: the CC controller number for a slot whose source is
    // `.midiCc`. Empty (the default) leaves every slot untouched.
    slotMidiCcNumbers: [Int] = [],
    effectLabel: String
  ) -> [ModulationRouteSpec]? {
    var routes: [ModulationRouteSpec] = []
    for (index, slot) in slots.enumerated() {
      if slot.source == .captured {
        // No media, no modulator name — the knots live in the source clause.
        guard let spec = index < slotCaptures.count ? slotCaptures[index] : nil else {
          statusMessage =
            "Record a capture take for the \(effectLabel) \(slot.target) slot in the Preview band."
          return nil
        }
        routes.append(
          ModulationRouteSpec(
            target: slot.target, source: spec, scale: slot.scale, offset: slot.offset,
            sampling: slot.sampling.spec,
            modulator: nil
          )
        )
        continue
      }
      if slot.source == .lfo {
        // No media, no modulator name — the params live in the source clause.
        guard let lfo = index < slotLfos.count ? slotLfos[index] : nil else {
          statusMessage = "LFO is not available on the \(effectLabel) \(slot.target) slot."
          return nil
        }
        guard let source = lfoSourceSpec(shape: lfo.shape, rate: lfo.rate, phase: lfo.phase) else {
          statusMessage =
            "LFO rate must be finite and greater than 0 (and phase finite) on the \(effectLabel) \(slot.target) slot."
          return nil
        }
        routes.append(
          ModulationRouteSpec(
            target: slot.target, source: source, scale: slot.scale, offset: slot.offset,
            sampling: slot.sampling.spec,
            modulator: nil
          )
        )
        continue
      }
      let source: String
      if slot.source == .midiCc {
        let controller = index < slotMidiCcNumbers.count ? slotMidiCcNumbers[index] : 74
        guard let spec = midiCcSourceSpec(controller: controller) else {
          statusMessage =
            "MIDI CC number must be 0–127 on the \(effectLabel) \(slot.target) slot."
          return nil
        }
        source = spec
      } else if let cli = slot.source.cliValue {
        source = cli
      } else {
        continue
      }
      let modulator = index < slotModulators.count ? slotModulators[index] : ""
      // Resolve the media this slot reads: the default modulator (empty name)
      // or the same-named declared entry.
      let audioURL: URL?
      let framesURL: URL?
      let midiURL: URL?
      if modulator.isEmpty {
        audioURL = modulatorAudioURL
        framesURL = modulatorFramesURL
        midiURL = modulatorMidiURL
      } else {
        guard let entry = namedModulators.first(where: { $0.name == modulator }) else {
          statusMessage =
            "Declare a modulator named \"\(modulator)\" before rendering \(effectLabel)."
          return nil
        }
        audioURL = entry.audioURL
        framesURL = entry.framesURL
        midiURL = entry.midiURL
      }
      let mediaLabel = modulator.isEmpty ? effectLabel : "modulator \"\(modulator)\""
      if slot.source.needsAudio && audioURL == nil {
        statusMessage = "Pick a modulator WAV for \(mediaLabel) before rendering with an audio source."
        return nil
      }
      if slot.source.needsFrames && framesURL == nil {
        statusMessage =
          "Pick a modulator frame directory for \(mediaLabel) before rendering with a luma/flow source."
        return nil
      }
      if slot.source.needsMidi && midiURL == nil {
        statusMessage =
          "Pick a modulator MIDI file for \(mediaLabel) before rendering with a MIDI source."
        return nil
      }
      routes.append(
        ModulationRouteSpec(
          target: slot.target, source: source, scale: slot.scale, offset: slot.offset,
          sampling: slot.sampling.spec,
          modulator: modulator.isEmpty ? nil : modulator
        )
      )
    }
    return routes
  }

  /// Declared named modulators mapped to the bridge's media spec. Empty-named
  /// entries are dropped (they are indistinguishable from the default).
  private func namedModulatorSpecs(_ entries: [NamedModulatorEntry]) -> [NamedModulatorMediaSpec] {
    entries.filter { !$0.name.isEmpty }.map {
      NamedModulatorMediaSpec(
        name: $0.name, audioURL: $0.audioURL, framesURL: $0.framesURL, midiURL: $0.midiURL)
    }
  }

  // Named-modulator list mutations shared by every panel's thin per-panel
  // wrappers (channel-shift keeps its own inline copies as the prototype). The
  // per-panel slot resets on removal stay in the wrappers since the slot fields
  // are panel-specific.

  /// Append a new named-modulator row to `list`, seeding a unique default name.
  private func appendNamedModulator(to list: inout [NamedModulatorEntry]) {
    let existing = Set(list.map(\.name))
    var index = list.count + 1
    var name = "mod\(index)"
    while existing.contains(name) {
      index += 1
      name = "mod\(index)"
    }
    list.append(NamedModulatorEntry(name: name))
    statusMessage = "Added named modulator \"\(name)\"."
  }

  private func pickNamedModulatorWAV(in list: inout [NamedModulatorEntry], id: UUID) {
    guard let index = list.firstIndex(where: { $0.id == id }) else { return }
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Named Modulator WAV",
      message: "Select the audio whose analysis envelope drives slots bound to this modulator."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    list[index].audioURL = url
    statusMessage = "Named modulator \"\(list[index].name)\" WAV selected: \(url.lastPathComponent)"
  }

  private func pickNamedModulatorMIDI(in list: inout [NamedModulatorEntry], id: UUID) {
    guard let index = list.firstIndex(where: { $0.id == id }) else { return }
    guard let url = MediaFilePicker.chooseMIDIFile(
      title: "Choose Named Modulator MIDI",
      message: "Select the MIDI file whose CC/note envelopes drive slots bound to this modulator."
    ) else {
      statusMessage = "Modulator MIDI selection cancelled."
      return
    }
    list[index].midiURL = url
    statusMessage = "Named modulator \"\(list[index].name)\" MIDI selected: \(url.lastPathComponent)"
  }

  private func pickNamedModulatorFrames(in list: inout [NamedModulatorEntry], id: UUID) {
    guard let index = list.firstIndex(where: { $0.id == id }) else { return }
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Named Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives slots bound to this modulator."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    list[index].framesURL = url
    statusMessage = "Named modulator \"\(list[index].name)\" frames selected: \(url.lastPathComponent)"
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

  func chooseFeedbackModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knob."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    feedbackModulatorAudioURL = url
    statusMessage = "Feedback modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseFeedbackModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knob."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    feedbackModulatorFramesURL = url
    statusMessage = "Feedback modulator frames selected: \(url.lastPathComponent)"
  }

  func chooseFluidModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knob."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    fluidModulatorAudioURL = url
    statusMessage = "Fluid modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseFluidModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knob."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    fluidModulatorFramesURL = url
    statusMessage = "Fluid modulator frames selected: \(url.lastPathComponent)"
  }

  func chooseDatamoshModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knob."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    datamoshModulatorAudioURL = url
    statusMessage = "Datamosh modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseDatamoshModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knob."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    datamoshModulatorFramesURL = url
    statusMessage = "Datamosh modulator frames selected: \(url.lastPathComponent)"
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

  func chooseChannelShiftMatteFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Matte Frames",
      message: "Select the frame directory analyzed for the spatial matte field."
    ) else {
      statusMessage = "Matte frames selection cancelled."
      return
    }
    channelShiftMatteFramesURL = url
    statusMessage = "Channel-shift matte frames selected: \(url.lastPathComponent)"
  }

  /// Non-empty declared names for the channel-shift slots' Modulator pickers.
  var channelShiftDeclaredModulatorNames: [String] {
    channelShiftNamedModulators.map(\.name).filter { !$0.isEmpty }
  }

  /// Append a new named-modulator row, seeding a unique default name.
  func addChannelShiftNamedModulator() {
    let existing = Set(channelShiftNamedModulators.map(\.name))
    var index = channelShiftNamedModulators.count + 1
    var name = "mod\(index)"
    while existing.contains(name) {
      index += 1
      name = "mod\(index)"
    }
    channelShiftNamedModulators.append(NamedModulatorEntry(name: name))
    statusMessage = "Added named modulator \"\(name)\"."
  }

  /// Remove a named-modulator row, resetting any slot that bound to it back to
  /// the default so no route dangles at render time.
  func removeChannelShiftNamedModulator(id: UUID) {
    guard let entry = channelShiftNamedModulators.first(where: { $0.id == id }) else { return }
    channelShiftNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if channelShiftModRXModulator == name { channelShiftModRXModulator = "" }
    if channelShiftModRYModulator == name { channelShiftModRYModulator = "" }
    if channelShiftModGXModulator == name { channelShiftModGXModulator = "" }
    if channelShiftModGYModulator == name { channelShiftModGYModulator = "" }
    if channelShiftModBXModulator == name { channelShiftModBXModulator = "" }
    if channelShiftModBYModulator == name { channelShiftModBYModulator = "" }
  }

  func chooseChannelShiftNamedModulatorWAV(id: UUID) {
    guard let index = channelShiftNamedModulators.firstIndex(where: { $0.id == id }) else { return }
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Named Modulator WAV",
      message: "Select the audio whose analysis envelope drives slots bound to this modulator."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    channelShiftNamedModulators[index].audioURL = url
    statusMessage =
      "Named modulator \"\(channelShiftNamedModulators[index].name)\" WAV selected: \(url.lastPathComponent)"
  }

  func chooseChannelShiftNamedModulatorFrames(id: UUID) {
    guard let index = channelShiftNamedModulators.firstIndex(where: { $0.id == id }) else { return }
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Named Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives slots bound to this modulator."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    channelShiftNamedModulators[index].framesURL = url
    statusMessage =
      "Named modulator \"\(channelShiftNamedModulators[index].name)\" frames selected: \(url.lastPathComponent)"
  }

  // MARK: - Named modulators (swept panels)
  // Each panel exposes the same four actions as channel-shift; add/choose
  // delegate to the shared helpers, remove stays per-panel to reset its own
  // slot Modulator bindings back to Default.

  var feedbackDeclaredModulatorNames: [String] {
    feedbackNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addFeedbackNamedModulator() { appendNamedModulator(to: &feedbackNamedModulators) }
  func chooseFeedbackNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &feedbackNamedModulators, id: id) }
  func chooseFeedbackNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &feedbackNamedModulators, id: id) }
  func removeFeedbackNamedModulator(id: UUID) {
    guard let entry = feedbackNamedModulators.first(where: { $0.id == id }) else { return }
    feedbackNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if feedbackModCarrierAmountModulator == name { feedbackModCarrierAmountModulator = "" }
    if feedbackModAmountModulator == name { feedbackModAmountModulator = "" }
    if feedbackModMixModulator == name { feedbackModMixModulator = "" }
    if feedbackModDecayModulator == name { feedbackModDecayModulator = "" }
    if feedbackModStructureMixModulator == name { feedbackModStructureMixModulator = "" }
  }

  var fluidDeclaredModulatorNames: [String] {
    fluidNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addFluidNamedModulator() { appendNamedModulator(to: &fluidNamedModulators) }
  func chooseFluidNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &fluidNamedModulators, id: id) }
  func chooseFluidNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &fluidNamedModulators, id: id) }
  func removeFluidNamedModulator(id: UUID) {
    guard let entry = fluidNamedModulators.first(where: { $0.id == id }) else { return }
    fluidNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if fluidModProceduralAdvectModulator == name { fluidModProceduralAdvectModulator = "" }
    if fluidModMotionAdvectModulator == name { fluidModMotionAdvectModulator = "" }
    if fluidModTurbulenceScaleModulator == name { fluidModTurbulenceScaleModulator = "" }
    if fluidModTurbulenceSpeedModulator == name { fluidModTurbulenceSpeedModulator = "" }
    if fluidModDetailModulator == name { fluidModDetailModulator = "" }
    if fluidModReinjectModulator == name { fluidModReinjectModulator = "" }
  }

  var retroStaticDeclaredModulatorNames: [String] {
    retroStaticNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addRetroStaticNamedModulator() { appendNamedModulator(to: &retroStaticNamedModulators) }
  func chooseRetroStaticNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &retroStaticNamedModulators, id: id) }
  func chooseRetroStaticNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &retroStaticNamedModulators, id: id) }
  func removeRetroStaticNamedModulator(id: UUID) {
    guard let entry = retroStaticNamedModulators.first(where: { $0.id == id }) else { return }
    retroStaticNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if retroStaticModStrengthModulator == name { retroStaticModStrengthModulator = "" }
    if retroStaticModFilterModulator == name { retroStaticModFilterModulator = "" }
  }

  var paletteQuantizeDeclaredModulatorNames: [String] {
    paletteQuantizeNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addPaletteQuantizeNamedModulator() { appendNamedModulator(to: &paletteQuantizeNamedModulators) }
  func choosePaletteQuantizeNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &paletteQuantizeNamedModulators, id: id) }
  func choosePaletteQuantizeNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &paletteQuantizeNamedModulators, id: id) }
  func removePaletteQuantizeNamedModulator(id: UUID) {
    guard let entry = paletteQuantizeNamedModulators.first(where: { $0.id == id }) else { return }
    paletteQuantizeNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if paletteQuantizeModLevelsModulator == name { paletteQuantizeModLevelsModulator = "" }
    if paletteQuantizeModModeModulator == name { paletteQuantizeModModeModulator = "" }
  }

  var ruttEtraDeclaredModulatorNames: [String] {
    ruttEtraNamedModulators.map(\.name).filter { !$0.isEmpty }
  }

  // MARK: Performance capture (docs/PERFORMANCE_CAPTURE_MILESTONE.md)

  /// Rutt-Etra targets whose slot is armed for capture (source == .captured),
  /// in slot order. The Workflow capture strip is visible while non-empty.
  var ruttEtraArmedCaptureTargets: [String] {
    var targets: [String] = []
    if ruttEtraModDepthSource == .captured { targets.append("displacement_depth") }
    if ruttEtraModPitchSource == .captured { targets.append("line_pitch") }
    if ruttEtraModThicknessSource == .captured { targets.append("line_thickness") }
    return targets
  }

  /// The stored take's spec clause for one Rutt-Etra target, or nil when no
  /// take has been recorded (the run path then refuses with a status message).
  func ruttEtraCapturedSpec(_ target: String) -> String? {
    ruttEtraCapturedTakes[target].flatMap(capturedSourceSpec)
  }

  /// Begin a take for the currently selected armed target. The caller (the
  /// capture strip) restarts preview playback from frame 0 in the same action,
  /// so recorder time 0 == frame 0. The current slider value is ingested at
  /// t = 0 so a held-still take records a constant.
  func beginCaptureTake(loopDuration: TimeInterval) {
    let armed = ruttEtraArmedCaptureTargets
    guard !armed.isEmpty else {
      statusMessage = "Arm a Rutt-Etra mod slot (source: Captured) before recording."
      return
    }
    if !armed.contains(captureTargetSelection) {
      captureTargetSelection = armed[0]
    }
    let recorder = GestureRecorder(loopDuration: loopDuration)
    recorder.ingest(t: 0, v: captureSlider)
    captureRecorder = recorder
    isCapturing = true
    statusMessage = "Recording \(captureTargetSelection) — scrub the capture slider…"
  }

  /// Offer one slider sample at the preview player's elapsed play time. Ends
  /// the take automatically once the loop duration is passed (one pass, no
  /// wrap — the recorder also drops out-of-window samples by contract).
  func ingestCaptureSample(t: TimeInterval, v: Double) {
    guard isCapturing, let recorder = captureRecorder else { return }
    if t > recorder.loopDuration {
      endCaptureTake()
      return
    }
    recorder.ingest(t: t, v: v)
  }

  /// Close the take and store it on the selected target (replacing any prior
  /// take — delete + re-record is the MVP edit story). An empty take stores
  /// nothing and leaves any existing take untouched.
  func endCaptureTake() {
    guard let recorder = captureRecorder else { return }
    recorder.finish()
    captureRecorder = nil
    isCapturing = false
    guard !recorder.knots.isEmpty else {
      statusMessage = "Capture take was empty — nothing recorded."
      return
    }
    ruttEtraCapturedTakes[captureTargetSelection] = recorder.knots
    let seconds = recorder.knots.last.map { String(format: "%.1f", $0.t) } ?? "0"
    statusMessage =
      "Captured \(recorder.knots.count) knot(s) over \(seconds)s on \(captureTargetSelection)."
  }
  func addRuttEtraNamedModulator() { appendNamedModulator(to: &ruttEtraNamedModulators) }
  func chooseRuttEtraNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &ruttEtraNamedModulators, id: id) }
  func chooseRuttEtraNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &ruttEtraNamedModulators, id: id) }
  func chooseRuttEtraNamedModulatorMIDI(id: UUID) { pickNamedModulatorMIDI(in: &ruttEtraNamedModulators, id: id) }
  func removeRuttEtraNamedModulator(id: UUID) {
    guard let entry = ruttEtraNamedModulators.first(where: { $0.id == id }) else { return }
    ruttEtraNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if ruttEtraModDepthModulator == name { ruttEtraModDepthModulator = "" }
    if ruttEtraModPitchModulator == name { ruttEtraModPitchModulator = "" }
    if ruttEtraModThicknessModulator == name { ruttEtraModThicknessModulator = "" }
  }

  var morphogenesisDeclaredModulatorNames: [String] {
    morphogenesisNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addMorphogenesisNamedModulator() { appendNamedModulator(to: &morphogenesisNamedModulators) }
  func chooseMorphogenesisNamedModulatorWAV(id: UUID) {
    pickNamedModulatorWAV(in: &morphogenesisNamedModulators, id: id)
  }
  func chooseMorphogenesisNamedModulatorFrames(id: UUID) {
    pickNamedModulatorFrames(in: &morphogenesisNamedModulators, id: id)
  }
  func chooseMorphogenesisNamedModulatorMIDI(id: UUID) {
    pickNamedModulatorMIDI(in: &morphogenesisNamedModulators, id: id)
  }
  func removeMorphogenesisNamedModulator(id: UUID) {
    guard let entry = morphogenesisNamedModulators.first(where: { $0.id == id }) else { return }
    morphogenesisNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if morphogenesisModFeedModulator == name { morphogenesisModFeedModulator = "" }
    if morphogenesisModKillModulator == name { morphogenesisModKillModulator = "" }
    if morphogenesisModParamMapStrengthModulator == name {
      morphogenesisModParamMapStrengthModulator = ""
    }
    if morphogenesisModPatternMixModulator == name { morphogenesisModPatternMixModulator = "" }
    if morphogenesisModDisplaceModulator == name { morphogenesisModDisplaceModulator = "" }
    if morphogenesisModInjectModulator == name { morphogenesisModInjectModulator = "" }
    if morphogenesisModErodeModulator == name { morphogenesisModErodeModulator = "" }
    if morphogenesisModCoverageTargetModulator == name {
      morphogenesisModCoverageTargetModulator = ""
    }
  }

  var datamoshDeclaredModulatorNames: [String] {
    datamoshNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addDatamoshNamedModulator() { appendNamedModulator(to: &datamoshNamedModulators) }
  func chooseDatamoshNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &datamoshNamedModulators, id: id) }
  func chooseDatamoshNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &datamoshNamedModulators, id: id) }
  func removeDatamoshNamedModulator(id: UUID) {
    guard let entry = datamoshNamedModulators.first(where: { $0.id == id }) else { return }
    datamoshNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if datamoshModAmountModulator == name { datamoshModAmountModulator = "" }
    if datamoshModResidualGainModulator == name { datamoshModResidualGainModulator = "" }
    if datamoshModResidualDecayModulator == name { datamoshModResidualDecayModulator = "" }
    if datamoshModRefreshThresholdModulator == name { datamoshModRefreshThresholdModulator = "" }
  }

  var pixelSortDeclaredModulatorNames: [String] {
    pixelSortNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addPixelSortNamedModulator() { appendNamedModulator(to: &pixelSortNamedModulators) }
  func choosePixelSortNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &pixelSortNamedModulators, id: id) }
  func choosePixelSortNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &pixelSortNamedModulators, id: id) }
  func removePixelSortNamedModulator(id: UUID) {
    guard let entry = pixelSortNamedModulators.first(where: { $0.id == id }) else { return }
    pixelSortNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if pixelSortModLowModulator == name { pixelSortModLowModulator = "" }
    if pixelSortModHighModulator == name { pixelSortModHighModulator = "" }
    if pixelSortModDirectionModulator == name { pixelSortModDirectionModulator = "" }
    if pixelSortModAxisModulator == name { pixelSortModAxisModulator = "" }
  }

  var cascadeTrailsDeclaredModulatorNames: [String] {
    cascadeTrailsNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addCascadeTrailsNamedModulator() { appendNamedModulator(to: &cascadeTrailsNamedModulators) }
  func chooseCascadeTrailsNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &cascadeTrailsNamedModulators, id: id) }
  func chooseCascadeTrailsNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &cascadeTrailsNamedModulators, id: id) }
  func removeCascadeTrailsNamedModulator(id: UUID) {
    guard let entry = cascadeTrailsNamedModulators.first(where: { $0.id == id }) else { return }
    cascadeTrailsNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if trailsModAdvectModulator == name { trailsModAdvectModulator = "" }
    if trailsModTurbScaleModulator == name { trailsModTurbScaleModulator = "" }
    if trailsModDetailModulator == name { trailsModDetailModulator = "" }
    if trailsModDecayModulator == name { trailsModDecayModulator = "" }
  }

  func chooseCascadeTrailsModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    cascadeTrailsModulatorAudioURL = url
    statusMessage = "Cascade trails modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseCascadeTrailsModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frame Directory"
    ) else {
      statusMessage = "Modulator frame directory selection cancelled."
      return
    }
    cascadeTrailsModulatorFramesURL = url
    statusMessage = "Cascade trails modulator frames selected: \(url.lastPathComponent)"
  }

  var cascadeCollageDeclaredModulatorNames: [String] {
    cascadeCollageNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addCascadeCollageNamedModulator() { appendNamedModulator(to: &cascadeCollageNamedModulators) }
  func chooseCascadeCollageNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &cascadeCollageNamedModulators, id: id) }
  func chooseCascadeCollageNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &cascadeCollageNamedModulators, id: id) }
  func removeCascadeCollageNamedModulator(id: UUID) {
    guard let entry = cascadeCollageNamedModulators.first(where: { $0.id == id }) else { return }
    cascadeCollageNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if collageModScribModulator == name { collageModScribModulator = "" }
    if collageModMorphModulator == name { collageModMorphModulator = "" }
    if collageModEdgeModulator == name { collageModEdgeModulator = "" }
    if collageModFaceModulator == name { collageModFaceModulator = "" }
  }

  func chooseCascadeCollageModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    cascadeCollageModulatorAudioURL = url
    statusMessage = "Cascade collage modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseCascadeCollageModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frame Directory"
    ) else {
      statusMessage = "Modulator frame directory selection cancelled."
      return
    }
    cascadeCollageModulatorFramesURL = url
    statusMessage = "Cascade collage modulator frames selected: \(url.lastPathComponent)"
  }

  var disperseDeclaredModulatorNames: [String] {
    disperseNamedModulators.map(\.name).filter { !$0.isEmpty }
  }
  func addDisperseNamedModulator() { appendNamedModulator(to: &disperseNamedModulators) }
  func chooseDisperseNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &disperseNamedModulators, id: id) }
  func chooseDisperseNamedModulatorFrames(id: UUID) { pickNamedModulatorFrames(in: &disperseNamedModulators, id: id) }
  func removeDisperseNamedModulator(id: UUID) {
    guard let entry = disperseNamedModulators.first(where: { $0.id == id }) else { return }
    disperseNamedModulators.removeAll { $0.id == id }
    let name = entry.name
    if disperseModStrengthModulator == name { disperseModStrengthModulator = "" }
    if disperseModBiasModulator == name { disperseModBiasModulator = "" }
    if disperseModScatterModulator == name { disperseModScatterModulator = "" }
    if disperseModDampingModulator == name { disperseModDampingModulator = "" }
  }

  func chooseDispersionBlendOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Dispersion Blend Output Directory"
    ) else {
      statusMessage = "Dispersion blend output directory selection cancelled."
      return
    }
    disperseOutputURL = url
    disperseOutputPath = url.path
    statusMessage = "Dispersion blend output directory selected: \(url.lastPathComponent)"
  }

  func chooseDisperseModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    disperseModulatorAudioURL = url
    statusMessage = "Dispersion modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseDisperseModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frame Directory"
    ) else {
      statusMessage = "Modulator frame directory selection cancelled."
      return
    }
    disperseModulatorFramesURL = url
    statusMessage = "Dispersion modulator frames selected: \(url.lastPathComponent)"
  }

  func runDispersionBlendRender() {
    guard let sourceAURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering dispersion blend."
      return
    }
    guard let sourceBURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering dispersion blend."
      return
    }
    guard let outputURL = effectiveOutputRoot(disperseOutputURL) else {
      statusMessage = "Choose a dispersion-blend output directory before rendering."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        ("coagulation_strength", disperseModStrengthSource, disperseModStrengthScale, disperseModStrengthOffset, disperseModStrengthSamplingOverride),
        ("bias", disperseModBiasSource, disperseModBiasScale, disperseModBiasOffset, disperseModBiasSamplingOverride),
        ("scatter_amount", disperseModScatterSource, disperseModScatterScale, disperseModScatterOffset, disperseModScatterSamplingOverride),
        ("damping", disperseModDampingSource, disperseModDampingScale, disperseModDampingOffset, disperseModDampingSamplingOverride),
      ],
      modulatorAudioURL: disperseModulatorAudioURL,
      modulatorFramesURL: disperseModulatorFramesURL,
      namedModulators: disperseNamedModulators,
      slotModulators: [disperseModStrengthModulator, disperseModBiasModulator, disperseModScatterModulator, disperseModDampingModulator],
      effectLabel: "dispersion blend"
    ) else { return }

    let request = DispersionBlendSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultDispersionBlendSequenceRenderQueueURL(),
      sourceADirectoryURL: sourceAURL,
      sourceBDirectoryURL: sourceBURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("dispersion-blend", isDirectory: true),
      blockSize: disperseBlockSize,
      coagulationStrength: Float(disperseCoagulationStrength),
      bias: Float(disperseBias),
      scatterAmount: Float(disperseScatterAmount),
      damping: Float(disperseDamping),
      dispersionRamp: disperseDispersionRamp,
      ownershipRefresh: Float(disperseOwnershipRefresh),
      smear: Float(disperseSmear),
      maxFrames: disperseMaxFrames > 0 ? disperseMaxFrames : nil,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: disperseModulatorAudioURL,
      modulatorFramesURL: disperseModulatorFramesURL,
      modulationSampling: disperseModSampling,
      namedModulators: namedModulatorSpecs(disperseNamedModulators)
    )

    runFluidAdvectionQueue(
      label: "Dispersion blend",
      requestDescription: "Queueing dispersion blend through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedDispersionBlendSequenceRender(request: request)
    }
  }

  func mosaicDeclaredModulatorNames() -> [String] { mosaicNamedModulators.map(\.name) }

  func addMosaicNamedModulator() { mosaicNamedModulators.append(NamedModulatorEntry()) }

  func removeMosaicNamedModulator(id: UUID) {
    mosaicNamedModulators.removeAll { $0.id == id }
  }

  func chooseMosaicNamedModulatorWAV(id: UUID) { pickNamedModulatorWAV(in: &mosaicNamedModulators, id: id) }

  func chooseMosaicNamedModulatorFrames(id: UUID) {
    pickNamedModulatorFrames(in: &mosaicNamedModulators, id: id)
  }

  func chooseMosaicOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Fluid Mosaic Output Directory"
    ) else {
      statusMessage = "Fluid mosaic output directory selection cancelled."
      return
    }
    mosaicOutputURL = url
    mosaicOutputPath = url.path
  }

  func chooseMosaicModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    mosaicModulatorAudioURL = url
    statusMessage = "Mosaic modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseMosaicModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frame Directory"
    ) else {
      statusMessage = "Modulator frame directory selection cancelled."
      return
    }
    mosaicModulatorFramesURL = url
    statusMessage = "Mosaic modulator frames selected: \(url.lastPathComponent)"
  }

  func runFluidMosaicRender() {
    guard let sourceAURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering fluid mosaic."
      return
    }
    guard let sourceBURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering fluid mosaic."
      return
    }
    guard let outputURL = mosaicOutputURL else {
      statusMessage = "Choose a fluid-mosaic output directory before rendering."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        ("cohesion", mosaicModCohesionSource, mosaicModCohesionScale, mosaicModCohesionOffset, mosaicModCohesionSamplingOverride),
        ("repulsion", mosaicModRepulsionSource, mosaicModRepulsionScale, mosaicModRepulsionOffset, mosaicModRepulsionSamplingOverride),
        ("fluid_strength", mosaicModFluidSource, mosaicModFluidScale, mosaicModFluidOffset, mosaicModFluidSamplingOverride),
        ("turbulence", mosaicModTurbulenceSource, mosaicModTurbulenceScale, mosaicModTurbulenceOffset, mosaicModTurbulenceSamplingOverride),
      ],
      modulatorAudioURL: mosaicModulatorAudioURL,
      modulatorFramesURL: mosaicModulatorFramesURL,
      namedModulators: mosaicNamedModulators,
      slotModulators: [mosaicModCohesionModulator, mosaicModRepulsionModulator, mosaicModFluidModulator, mosaicModTurbulenceModulator],
      effectLabel: "fluid mosaic"
    ) else { return }

    let request = FluidMosaicSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultFluidMosaicSequenceRenderQueueURL(),
      sourceADirectoryURL: sourceAURL,
      sourceBDirectoryURL: sourceBURL,
      outputDirectoryURL: outputURL.appendingPathComponent("fluid-mosaic", isDirectory: true),
      tileSize: mosaicTileSize,
      colorBins: mosaicColorBins,
      cohesion: Float(mosaicCohesion),
      repulsion: Float(mosaicRepulsion),
      fluidStrength: Float(mosaicFluidStrength),
      damping: Float(mosaicDamping),
      settleIterations: mosaicSettleIterations,
      jitter: Float(mosaicJitter),
      turbulence: Float(mosaicTurbulence),
      frames: mosaicFrames,
      modulationRoutes: routes,
      modulatorAudioURL: mosaicModulatorAudioURL,
      modulatorFramesURL: mosaicModulatorFramesURL,
      modulationSampling: mosaicModSampling,
      namedModulators: namedModulatorSpecs(mosaicNamedModulators)
    )

    runFluidAdvectionQueue(
      label: "Fluid mosaic",
      requestDescription: "Queueing fluid mosaic through morphogen-cli..."
    ) {
      try RustBridgePlaceholder.runQueuedFluidMosaicSequenceRender(request: request)
    }
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

  func choosePaletteQuantizeMatteFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Matte Frames",
      message: "Select the frame directory analyzed for the spatial matte field."
    ) else {
      statusMessage = "Matte frames selection cancelled."
      return
    }
    paletteQuantizeMatteFramesURL = url
    statusMessage = "Palette-quantize matte frames selected: \(url.lastPathComponent)"
  }

  func chooseRuttEtraModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    ruttEtraModulatorAudioURL = url
    statusMessage = "Rutt-etra modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseRuttEtraModulatorMIDI() {
    guard let url = MediaFilePicker.chooseMIDIFile(
      title: "Choose Modulator MIDI",
      message: "Select the MIDI file whose CC/note envelopes drive the routed knobs."
    ) else {
      statusMessage = "Modulator MIDI selection cancelled."
      return
    }
    ruttEtraModulatorMidiURL = url
    statusMessage = "Rutt-etra modulator MIDI selected: \(url.lastPathComponent)"
  }

  func chooseRuttEtraModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    ruttEtraModulatorFramesURL = url
    statusMessage = "Rutt-etra modulator frames selected: \(url.lastPathComponent)"
  }

  func chooseRuttEtraMatteFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Matte Frames",
      message: "Select the frame directory analyzed for the spatial matte field."
    ) else {
      statusMessage = "Matte frames selection cancelled."
      return
    }
    ruttEtraMatteFramesURL = url
    statusMessage = "Rutt-etra matte frames selected: \(url.lastPathComponent)"
  }

  func chooseMorphogenesisModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Modulator WAV",
      message: "Select the audio whose analysis envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator WAV selection cancelled."
      return
    }
    morphogenesisModulatorAudioURL = url
    statusMessage = "Morphogenesis modulator WAV selected: \(url.lastPathComponent)"
  }

  func chooseMorphogenesisModulatorMIDI() {
    guard let url = MediaFilePicker.chooseMIDIFile(
      title: "Choose Modulator MIDI",
      message: "Select the MIDI file whose CC/note envelopes drive the routed knobs."
    ) else {
      statusMessage = "Modulator MIDI selection cancelled."
      return
    }
    morphogenesisModulatorMidiURL = url
    statusMessage = "Morphogenesis modulator MIDI selected: \(url.lastPathComponent)"
  }

  func chooseMorphogenesisModulatorFrames() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Modulator Frames",
      message: "Select the frame directory whose luma/flow envelope drives the routed knobs."
    ) else {
      statusMessage = "Modulator frames selection cancelled."
      return
    }
    morphogenesisModulatorFramesURL = url
    statusMessage = "Morphogenesis modulator frames selected: \(url.lastPathComponent)"
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
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering retro static."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering retro static."
      return
    }
    let filterMapping = enumModulationMapping(
      from: retroStaticModFilterFrom, to: retroStaticModFilterTo
    )
    guard let routes = modulationRoutes(
      slots: [
        (
          "strength", retroStaticModStrengthSource,
          retroStaticModStrengthScale, retroStaticModStrengthOffset,
          retroStaticModStrengthSamplingOverride
        ),
        (
          "filter", retroStaticModFilterSource, filterMapping.scale, filterMapping.offset,
          retroStaticModFilterSamplingOverride
        ),
      ],
      modulatorAudioURL: retroStaticModulatorAudioURL,
      modulatorFramesURL: retroStaticModulatorFramesURL,
      namedModulators: retroStaticNamedModulators,
      slotModulators: [retroStaticModStrengthModulator, retroStaticModFilterModulator],
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
      modulationSampling: retroStaticModSampling,
      namedModulators: namedModulatorSpecs(retroStaticNamedModulators)
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
    guard let carrierURL = effectiveCarrierURL() else {
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
    let channelShiftSourceADirectoryURL = channelShiftFlowGain != 0 ? effectiveModulatorURL() : nil
    if channelShiftMatteSource != .off && channelShiftMatteFramesURL == nil
      && channelShiftSourceADirectoryURL == nil {
      statusMessage =
        "Select matte frames (or a Source A directory) before rendering a matted channel shift."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        (
          "shift_r_x", channelShiftModRXSource, channelShiftModRXScale, channelShiftModRXOffset,
          channelShiftModRXSamplingOverride
        ),
        (
          "shift_r_y", channelShiftModRYSource, channelShiftModRYScale, channelShiftModRYOffset,
          channelShiftModRYSamplingOverride
        ),
        (
          "shift_g_x", channelShiftModGXSource, channelShiftModGXScale, channelShiftModGXOffset,
          channelShiftModGXSamplingOverride
        ),
        (
          "shift_g_y", channelShiftModGYSource, channelShiftModGYScale, channelShiftModGYOffset,
          channelShiftModGYSamplingOverride
        ),
        (
          "shift_b_x", channelShiftModBXSource, channelShiftModBXScale, channelShiftModBXOffset,
          channelShiftModBXSamplingOverride
        ),
        (
          "shift_b_y", channelShiftModBYSource, channelShiftModBYScale, channelShiftModBYOffset,
          channelShiftModBYSamplingOverride
        )
      ],
      modulatorAudioURL: channelShiftModulatorAudioURL,
      modulatorFramesURL: channelShiftModulatorFramesURL,
      namedModulators: channelShiftNamedModulators,
      slotModulators: [
        channelShiftModRXModulator, channelShiftModRYModulator,
        channelShiftModGXModulator, channelShiftModGYModulator,
        channelShiftModBXModulator, channelShiftModBYModulator
      ],
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
      sourceADirectoryURL: channelShiftSourceADirectoryURL,
      flowGain: channelShiftFlowGain,
      flowRadius: channelShiftFlowRadius,
      backend: channelShiftBackend,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: channelShiftModulatorAudioURL,
      modulatorFramesURL: channelShiftModulatorFramesURL,
      modulationSampling: channelShiftModSampling,
      namedModulators: namedModulatorSpecs(channelShiftNamedModulators),
      matteSource: channelShiftMatteSource,
      matteFramesURL: channelShiftMatteFramesURL,
      matteGain: channelShiftMatteGain
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
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering palette quantize."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering palette quantize."
      return
    }
    // No Source A concept on this single-source command — matte frames must
    // be chosen explicitly.
    if paletteQuantizeMatteSource != .off && paletteQuantizeMatteFramesURL == nil {
      statusMessage = "Select matte frames before rendering a matted palette quantize."
      return
    }
    let modeMapping = enumModulationMapping(
      from: paletteQuantizeModModeFrom, to: paletteQuantizeModModeTo
    )
    guard let routes = modulationRoutes(
      slots: [
        (
          "levels", paletteQuantizeModLevelsSource,
          paletteQuantizeModLevelsScale, paletteQuantizeModLevelsOffset,
          paletteQuantizeModLevelsSamplingOverride
        ),
        (
          "mode", paletteQuantizeModModeSource, modeMapping.scale, modeMapping.offset,
          paletteQuantizeModModeSamplingOverride
        ),
      ],
      modulatorAudioURL: paletteQuantizeModulatorAudioURL,
      modulatorFramesURL: paletteQuantizeModulatorFramesURL,
      namedModulators: paletteQuantizeNamedModulators,
      slotModulators: [paletteQuantizeModLevelsModulator, paletteQuantizeModModeModulator],
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
      modulationSampling: paletteQuantizeModSampling,
      namedModulators: namedModulatorSpecs(paletteQuantizeNamedModulators),
      matteSource: paletteQuantizeMatteSource,
      matteFramesURL: paletteQuantizeMatteFramesURL,
      matteGain: paletteQuantizeMatteGain
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

  func runRuttEtraSequenceRender() {
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering rutt-etra."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering rutt-etra."
      return
    }
    // Two-source: Source A (the shared modulator directory) drives displacement.
    var sourceADirectoryURL: URL? = nil
    if ruttEtraUseTwoSource {
      guard let modulatorURL = effectiveModulatorURL() else {
        statusMessage =
          "Select Source A frame directory (the modulator) for two-source rutt-etra, "
          + "or turn off Two-Source."
        return
      }
      sourceADirectoryURL = modulatorURL
    }
    if ruttEtraMatteSource != .off && ruttEtraMatteFramesURL == nil && sourceADirectoryURL == nil {
      statusMessage =
        "Select matte frames (or turn on Two-Source) before rendering a matted rutt-etra."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        (
          "displacement_depth", ruttEtraModDepthSource,
          ruttEtraModDepthScale, ruttEtraModDepthOffset,
          ruttEtraModDepthSamplingOverride
        ),
        (
          "line_pitch", ruttEtraModPitchSource,
          ruttEtraModPitchScale, ruttEtraModPitchOffset,
          ruttEtraModPitchSamplingOverride
        ),
        (
          "line_thickness", ruttEtraModThicknessSource,
          ruttEtraModThicknessScale, ruttEtraModThicknessOffset,
          ruttEtraModThicknessSamplingOverride
        ),
      ],
      modulatorAudioURL: ruttEtraModulatorAudioURL,
      modulatorFramesURL: ruttEtraModulatorFramesURL,
      modulatorMidiURL: ruttEtraModulatorMidiURL,
      namedModulators: ruttEtraNamedModulators,
      slotModulators: [
        ruttEtraModDepthModulator, ruttEtraModPitchModulator, ruttEtraModThicknessModulator,
      ],
      slotLfos: [
        (ruttEtraModDepthLfoShape, ruttEtraModDepthLfoRate, ruttEtraModDepthLfoPhase),
        (ruttEtraModPitchLfoShape, ruttEtraModPitchLfoRate, ruttEtraModPitchLfoPhase),
        (
          ruttEtraModThicknessLfoShape, ruttEtraModThicknessLfoRate,
          ruttEtraModThicknessLfoPhase
        ),
      ],
      slotCaptures: [
        ruttEtraCapturedSpec("displacement_depth"),
        ruttEtraCapturedSpec("line_pitch"),
        ruttEtraCapturedSpec("line_thickness"),
      ],
      slotMidiCcNumbers: [
        ruttEtraModDepthMidiCc, ruttEtraModPitchMidiCc, ruttEtraModThicknessMidiCc,
      ],
      effectLabel: "rutt-etra"
    ) else { return }

    var request = RuttEtraSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultRuttEtraSequenceRenderQueueURL(),
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("rutt-etra", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      linePitch: ruttEtraLinePitch,
      displacementDepth: ruttEtraDisplacementDepth,
      lineThickness: ruttEtraLineThickness,
      mono: ruttEtraMono,
      projectURL: projectURL,
      modulationRoutes: routes,
      modulatorAudioURL: ruttEtraModulatorAudioURL,
      modulatorFramesURL: ruttEtraModulatorFramesURL,
      modulationSampling: ruttEtraModSampling,
      namedModulators: namedModulatorSpecs(ruttEtraNamedModulators)
    )
    request.backend = ruttEtraBackend
    request.sourceADirectoryURL = sourceADirectoryURL
    request.modulatorMidiURL = ruttEtraModulatorMidiURL
    request.matteSource = ruttEtraMatteSource
    request.matteFramesURL = ruttEtraMatteFramesURL
    request.matteGain = ruttEtraMatteGain

    statusMessage = "Queueing rutt-etra through morphogen-cli..."
    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedRuttEtraSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.ruttEtraSummary =
            "\(bundle.frameCount) rutt-etra frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary =
            "Queued rutt-etra sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Rutt-etra render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.ruttEtraSummary = "Rutt-etra render failed: \(error.localizedDescription)"
          self.statusMessage = "Rutt-etra render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runMorphogenesisSequenceRender() {
    guard let carrierURL = effectiveCarrierURL() else {
      statusMessage = "Select Source B frame directory before rendering morphogenesis."
      return
    }
    guard let outputURL = effectiveOutputRoot(frameSequenceOutputURL) else {
      statusMessage = "Choose a frame sequence output directory before rendering morphogenesis."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        (
          "feed", morphogenesisModFeedSource,
          morphogenesisModFeedScale, morphogenesisModFeedOffset,
          morphogenesisModFeedSamplingOverride
        ),
        (
          "kill", morphogenesisModKillSource,
          morphogenesisModKillScale, morphogenesisModKillOffset,
          morphogenesisModKillSamplingOverride
        ),
        (
          "param_map_strength", morphogenesisModParamMapStrengthSource,
          morphogenesisModParamMapStrengthScale, morphogenesisModParamMapStrengthOffset,
          morphogenesisModParamMapStrengthSamplingOverride
        ),
        (
          "pattern_mix", morphogenesisModPatternMixSource,
          morphogenesisModPatternMixScale, morphogenesisModPatternMixOffset,
          morphogenesisModPatternMixSamplingOverride
        ),
        (
          "displace", morphogenesisModDisplaceSource,
          morphogenesisModDisplaceScale, morphogenesisModDisplaceOffset,
          morphogenesisModDisplaceSamplingOverride
        ),
        (
          "inject", morphogenesisModInjectSource,
          morphogenesisModInjectScale, morphogenesisModInjectOffset,
          morphogenesisModInjectSamplingOverride
        ),
        (
          "erode", morphogenesisModErodeSource,
          morphogenesisModErodeScale, morphogenesisModErodeOffset,
          morphogenesisModErodeSamplingOverride
        ),
        (
          "coverage_target", morphogenesisModCoverageTargetSource,
          morphogenesisModCoverageTargetScale, morphogenesisModCoverageTargetOffset,
          morphogenesisModCoverageTargetSamplingOverride
        ),
        (
          "shade", morphogenesisModShadeSource,
          morphogenesisModShadeScale, morphogenesisModShadeOffset,
          morphogenesisModShadeSamplingOverride
        ),
        (
          "shade_azimuth", morphogenesisModShadeAzimuthSource,
          morphogenesisModShadeAzimuthScale, morphogenesisModShadeAzimuthOffset,
          morphogenesisModShadeAzimuthSamplingOverride
        ),
        (
          "shade_height", morphogenesisModShadeHeightSource,
          morphogenesisModShadeHeightScale, morphogenesisModShadeHeightOffset,
          morphogenesisModShadeHeightSamplingOverride
        ),
      ],
      modulatorAudioURL: morphogenesisModulatorAudioURL,
      modulatorFramesURL: morphogenesisModulatorFramesURL,
      modulatorMidiURL: morphogenesisModulatorMidiURL,
      namedModulators: morphogenesisNamedModulators,
      slotModulators: [
        morphogenesisModFeedModulator, morphogenesisModKillModulator,
        morphogenesisModParamMapStrengthModulator, morphogenesisModPatternMixModulator,
        morphogenesisModDisplaceModulator,
        morphogenesisModInjectModulator, morphogenesisModErodeModulator,
        morphogenesisModCoverageTargetModulator,
        morphogenesisModShadeModulator, morphogenesisModShadeAzimuthModulator,
        morphogenesisModShadeHeightModulator,
      ],
      slotLfos: [
        (morphogenesisModFeedLfoShape, morphogenesisModFeedLfoRate, morphogenesisModFeedLfoPhase),
        (morphogenesisModKillLfoShape, morphogenesisModKillLfoRate, morphogenesisModKillLfoPhase),
        (
          morphogenesisModParamMapStrengthLfoShape, morphogenesisModParamMapStrengthLfoRate,
          morphogenesisModParamMapStrengthLfoPhase
        ),
        (
          morphogenesisModPatternMixLfoShape, morphogenesisModPatternMixLfoRate,
          morphogenesisModPatternMixLfoPhase
        ),
        (
          morphogenesisModDisplaceLfoShape, morphogenesisModDisplaceLfoRate,
          morphogenesisModDisplaceLfoPhase
        ),
        (
          morphogenesisModInjectLfoShape, morphogenesisModInjectLfoRate,
          morphogenesisModInjectLfoPhase
        ),
        (
          morphogenesisModErodeLfoShape, morphogenesisModErodeLfoRate,
          morphogenesisModErodeLfoPhase
        ),
        (
          morphogenesisModCoverageTargetLfoShape, morphogenesisModCoverageTargetLfoRate,
          morphogenesisModCoverageTargetLfoPhase
        ),
        (
          morphogenesisModShadeLfoShape, morphogenesisModShadeLfoRate,
          morphogenesisModShadeLfoPhase
        ),
        (
          morphogenesisModShadeAzimuthLfoShape, morphogenesisModShadeAzimuthLfoRate,
          morphogenesisModShadeAzimuthLfoPhase
        ),
        (
          morphogenesisModShadeHeightLfoShape, morphogenesisModShadeHeightLfoRate,
          morphogenesisModShadeHeightLfoPhase
        ),
      ],
      slotMidiCcNumbers: [
        morphogenesisModFeedMidiCc, morphogenesisModKillMidiCc,
        morphogenesisModParamMapStrengthMidiCc, morphogenesisModPatternMixMidiCc,
        morphogenesisModDisplaceMidiCc,
        morphogenesisModInjectMidiCc, morphogenesisModErodeMidiCc,
        morphogenesisModCoverageTargetMidiCc,
        morphogenesisModShadeMidiCc, morphogenesisModShadeAzimuthMidiCc,
        morphogenesisModShadeHeightMidiCc,
      ],
      effectLabel: "morphogenesis"
    ) else { return }

    let request = MorphogenesisSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultMorphogenesisSequenceRenderQueueURL(),
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL.appendingPathComponent("morphogenesis", isDirectory: true),
      frames: effectiveMaxFrames(frameSequenceMaxFrames),
      frameRate: proResFrameRate.framesPerSecond,
      preset: morphogenesisPreset,
      paramMapStrength: morphogenesisParamMapStrength,
      seedThreshold: morphogenesisSeedThreshold,
      simScale: morphogenesisSimScale,
      substeps: morphogenesisSubsteps,
      patternMix: morphogenesisPatternMix,
      displace: morphogenesisDisplace,
      patternHue: morphogenesisPatternHue,
      patternColorMode: morphogenesisPatternColorMode,
      projectURL: projectURL,
      outputView: morphogenesisOutputView,
      inject: morphogenesisInject,
      erode: morphogenesisErode,
      injectSource: morphogenesisInjectSource,
      coverageTarget: morphogenesisCoverageTarget,
      shade: morphogenesisShade,
      shadeHeight: morphogenesisShadeHeight,
      shadeAzimuth: morphogenesisShadeAzimuth,
      shadeElevation: morphogenesisShadeElevation,
      shadeSpecular: morphogenesisShadeSpecular,
      shadeShininess: morphogenesisShadeShininess,
      modulationRoutes: routes,
      modulatorAudioURL: morphogenesisModulatorAudioURL,
      modulatorFramesURL: morphogenesisModulatorFramesURL,
      modulatorMidiURL: morphogenesisModulatorMidiURL,
      modulationSampling: morphogenesisModSampling,
      namedModulators: namedModulatorSpecs(morphogenesisNamedModulators)
    )

    statusMessage = "Queueing morphogenesis through morphogen-cli..."
    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedMorphogenesisSequenceRender(
          request: request
        )
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.morphogenesisSummary =
            "\(bundle.frameCount) morphogenesis frame(s) at \(bundle.frameDirectory.path)"
          self.proResExportSummary =
            "Queued morphogenesis sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Morphogenesis render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.failPreviewIfNeeded(message: error.localizedDescription)
          self.morphogenesisSummary = "Morphogenesis render failed: \(error.localizedDescription)"
          self.statusMessage = "Morphogenesis render failed: \(error.localizedDescription)"
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
    guard let modulatorURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering the grain pool."
      return
    }
    guard let carrierURL = effectiveCarrierURL() else {
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
    guard let modulatorURL = effectiveModulatorURL() else {
      statusMessage = "Select Source A frame directory before rendering the video vocoder."
      return
    }
    guard let carrierURL = effectiveCarrierURL() else {
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
      vocodeBands: crossSynthVocodeBands,
      projectURL: projectURL
    )

    statusMessage = "Queueing spectral cross-synth render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedSpectralCrossSynthRender(request: request)
        let modeText =
          switch self.crossSynthMode {
          case .gain: "RMS-gain"
          case .filter: "centroid-filter"
          case .vocode: "phase-vocoder"
          }
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
    guard let modulatorURL = datamoshModulatorURL ?? effectiveModulatorURL() else {
      statusMessage = "Select a Source A frame directory before rendering the datamosh."
      return
    }
    guard let carrierURL = datamoshCarrierURL ?? effectiveCarrierURL() else {
      statusMessage = "Select a Source B frame directory before rendering the datamosh."
      return
    }
    guard let outputURL = effectiveOutputRoot(datamoshOutputURL ?? frameSequenceOutputURL) else {
      statusMessage = "Choose an output directory before rendering the datamosh."
      return
    }
    guard let routes = modulationRoutes(
      slots: [
        (
          "amount", datamoshModAmountSource, datamoshModAmountScale, datamoshModAmountOffset,
          datamoshModAmountSamplingOverride
        ),
        (
          "residual_gain", datamoshModResidualGainSource,
          datamoshModResidualGainScale, datamoshModResidualGainOffset,
          datamoshModResidualGainSamplingOverride
        ),
        (
          "residual_decay", datamoshModResidualDecaySource,
          datamoshModResidualDecayScale, datamoshModResidualDecayOffset,
          datamoshModResidualDecaySamplingOverride
        ),
        (
          "refresh_threshold", datamoshModRefreshThresholdSource,
          datamoshModRefreshThresholdScale, datamoshModRefreshThresholdOffset,
          datamoshModRefreshThresholdSamplingOverride
        )
      ],
      modulatorAudioURL: datamoshModulatorAudioURL,
      modulatorFramesURL: datamoshModulatorFramesURL,
      namedModulators: datamoshNamedModulators,
      slotModulators: [
        datamoshModAmountModulator, datamoshModResidualGainModulator,
        datamoshModResidualDecayModulator, datamoshModRefreshThresholdModulator
      ],
      effectLabel: "datamosh"
    ) else { return }

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
        : nil,
      modulationRoutes: routes,
      modulatorAudioURL: datamoshModulatorAudioURL,
      modulatorFramesURL: datamoshModulatorFramesURL,
      modulationSampling: datamoshModSampling,
      namedModulators: namedModulatorSpecs(datamoshNamedModulators)
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
    let directionMapping = enumModulationMapping(
      from: pixelSortModDirectionFrom, to: pixelSortModDirectionTo
    )
    let axisMapping = enumModulationMapping(
      from: pixelSortModAxisFrom, to: pixelSortModAxisTo
    )
    guard let routes = modulationRoutes(
      slots: [
        (
          "threshold_low", pixelSortModLowSource, pixelSortModLowScale, pixelSortModLowOffset,
          pixelSortModLowSamplingOverride
        ),
        (
          "threshold_high", pixelSortModHighSource, pixelSortModHighScale, pixelSortModHighOffset,
          pixelSortModHighSamplingOverride
        ),
        (
          "direction", pixelSortModDirectionSource, directionMapping.scale, directionMapping.offset,
          pixelSortModDirectionSamplingOverride
        ),
        (
          "axis", pixelSortModAxisSource, axisMapping.scale, axisMapping.offset,
          pixelSortModAxisSamplingOverride
        ),
      ],
      modulatorAudioURL: pixelSortModulatorAudioURL,
      modulatorFramesURL: pixelSortModulatorFramesURL,
      namedModulators: pixelSortNamedModulators,
      slotModulators: [
        pixelSortModLowModulator, pixelSortModHighModulator,
        pixelSortModDirectionModulator, pixelSortModAxisModulator
      ],
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
      modulationSampling: pixelSortModSampling,
      namedModulators: namedModulatorSpecs(pixelSortNamedModulators)
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
  /// Proxy fps recorded when the preview began; playback runs at this rate.
  let fps: Double
  /// Downscaled input copies the render reads instead of the source proxies;
  /// nil = no override (scale 1 identity), render from the original dirs.
  let carrierInputOverrideURL: URL?
  let modulatorInputOverrideURL: URL?
}

/// Preview frame cap: `seconds × fps` rounded to nearest (ties away from
/// zero), floored at one frame; invalid fps or non-positive seconds also
/// yield the 1-frame floor. Free function for testability (the
/// `enumModulationMapping` precedent).
func previewFrameCap(seconds: Int, fps: Double) -> Int {
  guard seconds > 0, fps.isFinite, fps > 0 else { return 1 }
  return max(1, Int((Double(seconds) * fps).rounded()))
}

/// Where a preview session reroutes one input directory: nil at scale 1 —
/// the identity anchor skips the downscale entirely and the preview renders
/// from the ORIGINAL directory — else the fixed downscale destination under
/// the preview temp root. Free function for testability.
func previewInputOverrideURL(previewRoot: URL, scale: Int, label: String) -> URL? {
  guard scale > 1 else { return nil }
  return previewRoot.appendingPathComponent("downscaled-\(label)", isDirectory: true)
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
/// A user-declared named modulator: a `name` plus whichever media it carries.
/// A mod slot binds to one by name (`target=name.source`); the default
/// modulator stays the unnamed `channelShiftModulator*URL` pair.
struct NamedModulatorEntry: Identifiable, Equatable {
  let id = UUID()
  var name: String = ""
  var audioURL: URL? = nil
  var framesURL: URL? = nil
  var midiURL: URL? = nil
}

enum ModulationSourceOption: String, CaseIterable, Identifiable {
  case off = "Off"
  case audioRms = "Audio RMS"
  case audioOnset = "Audio Onset"
  case audioCentroid = "Audio Centroid"
  case luma = "Luma"
  case flow = "Flow"
  /// Internal deterministic modulator — no media. Only slot rows that opt in
  /// (pass LFO bindings) offer it; `modulationRoutes` spells it per-slot via
  /// `lfoSourceSpec`, so it never goes through `cliValue`.
  case lfo = "LFO"
  /// A recorded performance-capture take — no media. Only slot rows that opt
  /// in (`captureAvailable`) offer it; `modulationRoutes` spells it per-slot
  /// via `capturedSourceSpec` (a `breakpoints(...)` clause), so it never goes
  /// through `cliValue`. See `docs/PERFORMANCE_CAPTURE_MILESTONE.md`.
  case captured = "Captured"
  /// MIDI-file sources (docs/MIDI_MODULATION_MILESTONE.md S3). Only slot rows
  /// that opt in (`midiAvailable`) offer them. `midiCc` needs a per-slot
  /// controller number, spelled via `midiCcSourceSpec`; the other three go
  /// through `cliValue` like any media source.
  case midiCc = "MIDI CC"
  case midiVelocity = "MIDI Velocity"
  case midiNoteDensity = "MIDI Density"
  case midiPitch = "MIDI Pitch"

  var id: String { rawValue }

  /// CLI route-grammar spelling; `nil` for `off` (and for `lfo`/`captured`/
  /// `midiCc`, whose spelling is per-slot — see `lfoSourceSpec`/
  /// `capturedSourceSpec`/`midiCcSourceSpec`).
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
    case .lfo:
      return nil
    case .captured:
      return nil
    case .midiCc:
      return nil
    case .midiVelocity:
      return "midi-velocity"
    case .midiNoteDensity:
      return "midi-note-density"
    case .midiPitch:
      return "midi-pitch"
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

  var needsMidi: Bool {
    switch self {
    case .midiCc, .midiVelocity, .midiNoteDensity, .midiPitch:
      return true
    default:
      return false
    }
  }

  var isMidi: Bool { needsMidi }
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

/// Per-slot override of a route's sampling; `.default` inherits the panel-level
/// `ModulationSamplingOption` picker (no `@hold`/`@smooth` suffix emitted).
enum ModulationSamplingOverrideOption: String, CaseIterable, Identifiable {
  case `default` = "Default"
  case hold = "Hold"
  case smooth = "Smooth"

  var id: String { rawValue }

  var spec: ModulationSamplingOption? {
    switch self {
    case .default:
      return nil
    case .hold:
      return .hold
    case .smooth:
      return .smooth
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

/// From→To enum mod-slot mapping: envelope 0 selects `from`, envelope 1
/// selects `to`, so the emitted affine route is `offset = fromIndex`,
/// `scale = toIndex − fromIndex` over the option enum's declared case order —
/// which mirrors the engine's contract variant order (milestone table).
/// `from == to` emits `scale 0` = the continuity identity (a constant
/// override of the static knob).
func enumModulationMapping<Option: CaseIterable & Equatable>(
  from: Option, to: Option
) -> (scale: Double, offset: Double) {
  let all = Array(Option.allCases)
  let fromIndex = all.firstIndex(of: from) ?? 0
  let toIndex = all.firstIndex(of: to) ?? 0
  return (Double(toIndex - fromIndex), Double(fromIndex))
}

/// LFO waveform for an LFO mod slot. Case order mirrors the engine's
/// `LfoShape` declaration; `cliValue` is the route-grammar spelling.
enum LfoShapeOption: String, CaseIterable, Identifiable {
  case sine = "Sine"
  case triangle = "Triangle"
  case square = "Square"
  case saw = "Saw"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .sine:
      return "sine"
    case .triangle:
      return "triangle"
    case .square:
      return "square"
    case .saw:
      return "saw"
    }
  }
}

/// The exact `lfo(<shape>,<rate_hz>,<phase>)` source clause for the CLI route
/// grammar, or `nil` when the params are invalid (mirrors the CLI parse
/// rules: rate finite and > 0, phase finite). Free function for testability,
/// the `enumModulationMapping` precedent.
func lfoSourceSpec(shape: LfoShapeOption, rate: Double, phase: Double) -> String? {
  guard rate.isFinite, rate > 0, phase.isFinite else { return nil }
  func number(_ value: Double) -> String {
    String(format: "%.6g", locale: Locale(identifier: "en_US_POSIX"), value)
  }
  return "lfo(\(shape.cliValue),\(number(rate)),\(number(phase)))"
}

/// The exact `midi-cc(<n>)` source clause for the CLI route grammar, or `nil`
/// when the controller number is out of the MIDI range (mirrors the CLI parse
/// rule: 0–127). Free function for testability — the `lfoSourceSpec` precedent.
func midiCcSourceSpec(controller: Int) -> String? {
  guard (0...127).contains(controller) else { return nil }
  return "midi-cc(\(controller))"
}

/// The exact `breakpoints(<t>:<v>[;<t>:<v>...])` source clause for a recorded
/// performance-capture take, or `nil` for an empty take. Knots are emitted
/// sorted ascending by `t` (the recorder already ingests in order; sorting is
/// the parser's contract, so it is enforced here too). Free function for
/// testability — the `lfoSourceSpec` precedent.
func capturedSourceSpec(_ knots: [GestureKnot]) -> String? {
  guard !knots.isEmpty else { return nil }
  func number(_ value: Double) -> String {
    String(format: "%.6g", locale: Locale(identifier: "en_US_POSIX"), value)
  }
  let pairs = knots.sorted { $0.t < $1.t }.map { "\(number($0.t)):\(number($0.v))" }
  return "breakpoints(\(pairs.joined(separator: ";")))"
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

/// Named Gray-Scott parameter-atlas presets (Tier "Morphogenesis" S1;
/// docs/MORPHOGENESIS_MILESTONE.md) — most of `(feed, kill)` space is dead
/// (uniform grey), so the panel offers presets rather than raw numbers.
enum MorphogenesisPresetOption: String, CaseIterable, Identifiable {
  case coral = "Coral"
  case mitosis = "Mitosis"
  case worms = "Worms"
  case spots = "Spots"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .coral: return "coral"
    case .mitosis: return "mitosis"
    case .worms: return "worms"
    case .spots: return "spots"
    }
  }
}

/// `--pattern-color-mode`: how the pattern-mix tint colour is chosen.
enum MorphogenesisColorModeOption: String, CaseIterable, Identifiable {
  case hue = "Hue"
  case inherit = "Inherit"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .hue: return "hue"
    case .inherit: return "inherit"
    }
  }
}

/// `--output-view` (Field View milestone,
/// docs/MORPHOGENESIS_FIELD_VIEW_MILESTONE.md): which representation
/// `render-morphogenesis-sequence` writes as its output frame.
enum MorphogenesisOutputViewOption: String, CaseIterable, Identifiable {
  case composite = "Composite"
  case field = "Field"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .composite: return "composite"
    case .field: return "field"
    }
  }
}

/// `--inject-source` (Tier "Morphogenesis Live Coupling" L-S1/L-S3,
/// docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md): which weight field
/// `--inject`/`--erode` read.
enum MorphogenesisInjectSourceOption: String, CaseIterable, Identifiable {
  case luma = "Luma"
  case motion = "Motion"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .luma: return "luma"
    case .motion: return "motion"
    }
  }
}

/// Spatial matte source (Tier 5.4, docs/SPATIAL_MATTE_MILESTONE.md): gates a
/// stateless effect's blend per-pixel instead of uniformly. `.off` means no
/// `--matte` flag at all (byte-identical to pre-slice behaviour) — it has no
/// `cliValue` for that reason, unlike the other CLI-mapped option enums here.
enum MatteSourceOption: String, CaseIterable, Identifiable {
  case off = "Off"
  case aLuma = "A-Luma"
  case aFlow = "A-Flow"
  case aEdge = "A-Edge"

  var id: String { rawValue }

  var cliValue: String? {
    switch self {
    case .off:
      return nil
    case .aLuma:
      return "a-luma"
    case .aFlow:
      return "a-flow"
    case .aEdge:
      return "a-edge"
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
  case vocode = "Vocode (A's spectrum → B)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .gain:
      return "gain"
    case .filter:
      return "filter"
    case .vocode:
      return "vocode"
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
