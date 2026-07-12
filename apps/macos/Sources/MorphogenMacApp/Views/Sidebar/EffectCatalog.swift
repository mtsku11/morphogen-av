import SwiftUI

/// Sidebar section grouping. Pure data — see `docs/UI_REDESIGN_MILESTONE.md`
/// for the catalog this mirrors.
enum EffectCategory: String, CaseIterable, Identifiable {
  case displacement = "Displacement"
  case fluidAdvection = "Fluid / Advection"
  case blendMosaic = "Blend / Mosaic"
  case feedbackDatamosh = "Feedback / Datamosh"
  case generative = "Generative"
  case postLook = "Post / Look"
  case audioCrossSynth = "Audio / Cross-Synth"
  case composition = "Composition"
  case tools = "Tools"

  var id: String { rawValue }
}

/// One sidebar row. Every case maps to exactly one detail view and (for
/// render effects) one `AppState.run*Render()` entry point. Tools entries
/// have no render function.
enum EffectListing: String, CaseIterable, Identifiable, Hashable {
  case flowDisplace
  case flowFeedback
  case ruttEtra
  case fluidAdvection
  case convBlend
  case coagulatedBlend
  case dispersionBlend
  case fluidMosaic
  case datamosh
  case bitstreamDatamosh
  case cascadeCollage
  case trailCascade
  case morphogenesis
  case granularMosaic
  case retroStatic
  case channelShift
  case paletteQuantize
  case pixelSort
  case videoVocoder
  case spectralCrossSynth
  case audioImpulseConvolution
  case audioVideoRoute
  case videoAudioRoute
  case composition
  case analysis
  case nodeGraph

  var id: String { rawValue }

  var category: EffectCategory {
    switch self {
    case .flowDisplace, .flowFeedback, .ruttEtra:
      return .displacement
    case .fluidAdvection:
      return .fluidAdvection
    case .convBlend, .coagulatedBlend, .dispersionBlend, .fluidMosaic:
      return .blendMosaic
    case .datamosh, .bitstreamDatamosh, .cascadeCollage, .trailCascade:
      return .feedbackDatamosh
    case .morphogenesis, .granularMosaic:
      return .generative
    case .retroStatic, .channelShift, .paletteQuantize, .pixelSort:
      return .postLook
    case .videoVocoder, .spectralCrossSynth, .audioImpulseConvolution, .audioVideoRoute, .videoAudioRoute:
      return .audioCrossSynth
    case .composition:
      return .composition
    case .analysis, .nodeGraph:
      return .tools
    }
  }

  var title: String {
    switch self {
    case .flowDisplace: return "Flow Displace"
    case .flowFeedback: return "Flow Feedback"
    case .ruttEtra: return "Rutt-Etra"
    case .fluidAdvection: return "Fluid Advection"
    case .convBlend: return "Conv-Blend"
    case .coagulatedBlend: return "Coagulated Flow Blend"
    case .dispersionBlend: return "Dispersion Blend"
    case .fluidMosaic: return "Fluid Mosaic"
    case .datamosh: return "Controlled Datamosh"
    case .bitstreamDatamosh: return "Bitstream Datamosh"
    case .cascadeCollage: return "Cascade Collage"
    case .trailCascade: return "Trail Cascade"
    case .morphogenesis: return "Morphogenesis"
    case .granularMosaic: return "Granular Mosaic"
    case .retroStatic: return "Retro Static"
    case .channelShift: return "Channel Shift"
    case .paletteQuantize: return "Palette Quantize"
    case .pixelSort: return "Pixel Sort"
    case .videoVocoder: return "Video Vocoder"
    case .spectralCrossSynth: return "Spectral Cross-Synthesis"
    case .audioImpulseConvolution: return "Audio Impulse Convolution"
    case .audioVideoRoute: return "Audio-to-Video Route"
    case .videoAudioRoute: return "Video-to-Audio Route"
    case .composition: return "Composition Timeline"
    case .analysis: return "Analysis"
    case .nodeGraph: return "Node Graph"
    }
  }

  /// Whether the global header source-direction control (`SourceRelationship`)
  /// applies to this effect. True for the genuinely two-source effects, where
  /// swapping which loaded clip is the modulator vs carrier — or feeding one
  /// clip to both (self) — is meaningful. Single-source effects and generators
  /// ignore it and always read Source B as their carrier.
  var supportsSourceDirection: Bool {
    switch self {
    case .flowDisplace, .flowFeedback, .ruttEtra, .fluidAdvection, .convBlend,
         .coagulatedBlend, .dispersionBlend, .fluidMosaic, .datamosh, .granularMosaic,
         .channelShift, .pixelSort, .videoVocoder, .spectralCrossSynth,
         .audioImpulseConvolution, .audioVideoRoute, .videoAudioRoute:
      return true
    case .bitstreamDatamosh, .cascadeCollage, .trailCascade, .morphogenesis,
         .retroStatic, .paletteQuantize, .composition, .analysis, .nodeGraph:
      return false
    }
  }

  var systemImage: String {
    switch self {
    case .flowDisplace: return "arrow.up.left.and.arrow.down.right"
    case .flowFeedback: return "arrow.triangle.2.circlepath"
    case .ruttEtra: return "waveform.path"
    case .fluidAdvection: return "wind"
    case .convBlend: return "square.on.square"
    case .coagulatedBlend: return "drop.triangle"
    case .dispersionBlend: return "aqi.medium"
    case .fluidMosaic: return "square.grid.2x2"
    case .datamosh: return "rectangle.stack.badge.play"
    case .bitstreamDatamosh: return "waveform.path.ecg"
    case .cascadeCollage: return "square.stack.3d.up.slash"
    case .trailCascade: return "scribble.variable"
    case .morphogenesis: return "circle.hexagongrid"
    case .granularMosaic: return "circle.grid.3x3.fill"
    case .retroStatic: return "tv"
    case .channelShift: return "camera.filters"
    case .paletteQuantize: return "paintpalette"
    case .pixelSort: return "arrow.left.arrow.right"
    case .videoVocoder: return "waveform"
    case .spectralCrossSynth: return "waveform.badge.magnifyingglass"
    case .audioImpulseConvolution: return "waveform.circle"
    case .audioVideoRoute: return "speaker.wave.2.circle"
    case .videoAudioRoute: return "video.circle"
    case .composition: return "timeline.selection"
    case .analysis: return "chart.xyaxis.line"
    case .nodeGraph: return "point.3.connected.trianglepath.dotted"
    }
  }
}
