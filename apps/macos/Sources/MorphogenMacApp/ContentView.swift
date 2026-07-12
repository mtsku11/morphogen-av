import SwiftUI

struct ContentView: View {
  @StateObject private var state = AppState()

  var body: some View {
    VStack(spacing: 0) {
      header
        .padding(16)

      Divider()

      NavigationSplitView {
        EffectSidebarView(selection: $state.selectedEffect)
      } detail: {
        if let selection = state.selectedEffect {
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

      // Compact source row: two "Choose Source" buttons and the global
      // source-direction dropdown, replacing the former full-height cards.
      HStack(alignment: .center, spacing: 12) {
        CompactSourceButton(
          title: "Source A",
          role: .modulator,
          path: state.sourceAPath,
          previewImage: state.sourceAPreviewImage,
          onChoose: { chooseSource(.modulator) }
        )

        CompactSourceButton(
          title: "Source B",
          role: .carrier,
          path: state.sourceBPath,
          previewImage: state.sourceBPreviewImage,
          onChoose: { chooseSource(.carrier) }
        )

        relationshipPicker

        Spacer(minLength: 0)
      }

      GlobalRenderSettingsView(state: state)
    }
  }

  /// Global "who modifies whom" control. Disabled for single-source effects,
  /// which ignore the relationship (see `EffectListing.supportsSourceDirection`).
  private var relationshipPicker: some View {
    let supported = state.selectedEffect?.supportsSourceDirection ?? true
    return VStack(alignment: .leading, spacing: 2) {
      Text("Relationship")
        .font(.caption)
        .foregroundStyle(.secondary)
      Picker("Relationship", selection: $state.sourceRelationship) {
        ForEach(SourceRelationship.allCases) { relationship in
          Text(relationship.rawValue).tag(relationship)
        }
      }
      .labelsHidden()
      .frame(width: 190)
      .disabled(!supported)
      .help(supported
        ? "Which loaded source modulates which, for two-source effects."
        : "This effect uses a single source, so the relationship doesn't apply.")
    }
  }

  private var title: some View {
    VStack(alignment: .leading, spacing: 4) {
      Text("Morphogen AV")
        .font(.system(size: 24, weight: .semibold, design: .rounded))
      Text("Load two sources, pick who modifies whom, choose an effect, then render.")
        .font(.subheadline)
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
