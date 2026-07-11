import SwiftUI

struct ContentView: View {
  @StateObject private var state = AppState()
  @State private var selection: EffectListing?

  var body: some View {
    VStack(spacing: 0) {
      header
        .padding(16)

      Divider()

      NavigationSplitView {
        EffectSidebarView(selection: $selection)
      } detail: {
        if let selection {
          EffectDetailView(state: state, selection: selection)
        } else {
          EffectDetailPlaceholderView()
        }
      }
      .navigationSplitViewColumnWidth(min: 220, ideal: 260, max: 340)
    }
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: 12) {
      title

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

      GlobalRenderSettingsView(state: state)
    }
  }

  private var title: some View {
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
