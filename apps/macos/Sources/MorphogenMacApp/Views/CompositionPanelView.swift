import SwiftUI

/// Spec-file runner for the composition timeline (docs/COMPOSITION_MILESTONE.md).
/// A composition arranges finished render jobs (scenes — each a chain over its
/// own source) on a global timeline. Authoring the spec JSON happens in an
/// editor for now (a scene body is a chain, and the visual chain builder is not
/// built yet); this panel picks a spec + output folder, runs it through the CLI
/// bridge (queue add→run, like every effect panel), and loads the assembled
/// timeline into the preview.
struct CompositionPanelView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      Text("Composition Timeline")
        .font(.headline)
      Text("Arrange scenes (each a chain over its own source) into a piece — cuts, crossfades, and a master audio clock. Author the spec JSON in an editor, then render or queue it here.")
        .font(.caption)
        .foregroundStyle(.secondary)

      HStack {
        Button {
          state.chooseCompositionSpecFile()
        } label: {
          Label("Composition Spec (JSON)", systemImage: "doc.badge.gearshape")
        }

        Button {
          state.chooseCompositionOutputDirectory()
        } label: {
          Label("Output Folder", systemImage: "folder.badge.plus")
        }
      }

      Text(state.compositionSpecPath)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)
      Text(state.compositionOutputPath)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)

      HStack {
        Button {
          state.runComposition()
        } label: {
          Label("Render Composition", systemImage: "film.stack")
        }
        .disabled(state.compositionSpecURL == nil || state.compositionOutputURL == nil)
      }

      Text(state.compositionSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}
