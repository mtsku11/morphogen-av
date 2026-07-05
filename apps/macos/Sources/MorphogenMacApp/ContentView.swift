import SwiftUI

struct ContentView: View {
  @StateObject private var state = AppState()

  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      header

      ScrollView(.vertical) {
        VStack(alignment: .leading, spacing: 16) {
          HStack(alignment: .top, spacing: 16) {
            SourceSlotView(
              title: "Source A",
              role: .modulator,
              path: $state.sourceAPath,
              probeSummary: state.sourceAProbeSummary,
              previewSummary: state.sourceAPreviewSummary,
              previewImage: state.sourceAPreviewImage,
              onChoose: { chooseSource(.modulator) }
            )

            SourceSlotView(
              title: "Source B",
              role: .carrier,
              path: $state.sourceBPath,
              probeSummary: state.sourceBProbeSummary,
              previewSummary: state.sourceBPreviewSummary,
              previewImage: state.sourceBPreviewImage,
              onChoose: { chooseSource(.carrier) }
            )
          }

          WorkflowPanelView(state: state)

          DisclosureGroup {
            VStack(alignment: .leading, spacing: 16) {
              NodeGraphPlaceholderView()
              AnalysisPanelView()
              CompositionPanelView(state: state)
              CoagulatedBlendPanelView(state: state)
              DispersionBlendPanelView(state: state)
              FluidMosaicPanelView(state: state)
              RenderPanelView(state: state)
            }
            .padding(.top, 8)
          } label: {
            Label("Advanced render queue, diagnostics, and experimental controls", systemImage: "slider.horizontal.3")
              .font(.headline)
          }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, 8)
      }
    }
    .padding(20)
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: 4) {
      Text("Morphogen AV")
        .font(.system(size: 28, weight: .semibold, design: .rounded))
      Text("Load source material, route modulation, choose an effect, then render deterministically.")
        .foregroundStyle(.secondary)
    }
  }

  private func chooseSource(_ role: SourceRole) {
    guard let url = MediaFilePicker.chooseMediaFile(for: role) else {
      state.statusMessage = "\(role.rawValue) source selection cancelled."
      return
    }

    state.setSource(role, url: url)
  }
}
