import SwiftUI

/// The detail half of the `NavigationSplitView` shell: routes the sidebar's
/// `selection` to exactly one effect's controls. Every `EffectListing` case
/// is covered — see `docs/UI_REDESIGN_MILESTONE.md` for the catalog and the
/// per-category view files this switches into.
struct EffectDetailView: View {
  @ObservedObject var state: AppState
  let selection: EffectListing

  var body: some View {
    VStack(spacing: 0) {
      // The centralized preview, pinned at the top of the detail pane and
      // shared by every eligible effect (replaces the former per-effect bands
      // that sat at the bottom of each view). Effects with no preview
      // (`previewConfiguration == nil`) show only their controls below.
      if let config = state.previewConfiguration(for: selection) {
        QuickPreviewBand(
          state: state,
          requiresModulator: config.requiresModulator,
          runEffect: config.run
        )
        .padding([.horizontal, .top], 20)
        .padding(.bottom, 12)

        Divider()
      }

      ScrollView(.vertical) {
        Group {
          switch selection {
        case .flowDisplace:
          FlowDisplaceDetailView(state: state)
        case .flowFeedback:
          FlowFeedbackDetailView(state: state)
        case .ruttEtra:
          RuttEtraDetailView(state: state)
        case .fluidAdvection:
          FluidAdvectionDetailView(state: state)
        case .convBlend:
          ConvBlendDetailView(state: state)
        case .coagulatedBlend:
          CoagulatedBlendPanelView(state: state)
        case .dispersionBlend:
          DispersionBlendPanelView(state: state)
        case .fluidMosaic:
          FluidMosaicPanelView(state: state)
        case .datamosh:
          DatamoshDetailView(state: state)
        case .bitstreamDatamosh:
          BitstreamDatamoshDetailView(state: state)
        case .cascadeCollage:
          CascadeCollageDetailView(state: state)
        case .trailCascade:
          TrailCascadeDetailView(state: state)
        case .morphogenesis:
          MorphogenesisDetailView(state: state)
        case .granularMosaic:
          GranularMosaicDetailView(state: state)
        case .retroStatic:
          RetroStaticDetailView(state: state)
        case .channelShift:
          ChannelShiftDetailView(state: state)
        case .paletteQuantize:
          PaletteQuantizeDetailView(state: state)
        case .pixelSort:
          PixelSortDetailView(state: state)
        case .videoVocoder:
          VideoVocoderDetailView(state: state)
        case .spectralCrossSynth:
          SpectralCrossSynthDetailView(state: state)
        case .audioImpulseConvolution:
          AudioImpulseConvolutionDetailView(state: state)
        case .audioVideoRoute:
          AudioVideoRouteDetailView(state: state)
        case .videoAudioRoute:
          VideoAudioRouteDetailView(state: state)
        case .composition:
          CompositionPanelView(state: state)
        case .analysis:
          AnalysisPanelView()
        case .nodeGraph:
          NodeGraphPlaceholderView()
        }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
      }
    }
  }
}

/// Shown in the detail pane before any sidebar row is selected.
struct EffectDetailPlaceholderView: View {
  var body: some View {
    VStack(spacing: 10) {
      Image(systemName: "sidebar.left")
        .font(.system(size: 36))
        .foregroundStyle(.secondary)
      Text("Select an effect")
        .font(.title3)
        .foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }
}
