import SwiftUI

/// The detail half of the `NavigationSplitView` shell: routes the sidebar's
/// `selection` to exactly one effect's controls. Each category's cases are
/// filled in as that category is migrated (see `docs/UI_REDESIGN_MILESTONE.md`
/// phased plan) — anything not yet migrated falls through to `placeholder`.
struct EffectDetailView: View {
  @ObservedObject var state: AppState
  let selection: EffectListing

  var body: some View {
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
        default:
          placeholder
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(20)
    }
  }

  private var placeholder: some View {
    VStack(alignment: .leading, spacing: 8) {
      Label(selection.title, systemImage: selection.systemImage)
        .font(.title2.weight(.semibold))
      Text("Not migrated to the new detail pane yet.")
        .font(.callout)
        .foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
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
