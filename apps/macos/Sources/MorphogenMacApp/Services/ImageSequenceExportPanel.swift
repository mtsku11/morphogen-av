import AppKit
import Foundation
import UniformTypeIdentifiers

enum ImageSequenceExportPanel {
  static func chooseFrameDirectory() -> URL? {
    let panel = NSOpenPanel()
    panel.title = "Choose PNG Frame Sequence"
    panel.message = "Select a directory containing PNG frames."
    panel.prompt = "Choose"
    panel.canChooseFiles = false
    panel.canChooseDirectories = true
    panel.allowsMultipleSelection = false
    panel.resolvesAliases = true

    guard panel.runModal() == .OK else {
      return nil
    }

    return panel.url
  }

  static func chooseMovieSaveLocation(defaultName: String = "morphogen-prores.mov") -> URL? {
    let panel = NSSavePanel()
    panel.title = "Export ProRes Movie"
    panel.message = "Choose where to write the ProRes .mov file."
    panel.prompt = "Export"
    panel.nameFieldStringValue = defaultName
    panel.allowedContentTypes = [.quickTimeMovie]
    panel.canCreateDirectories = true

    guard panel.runModal() == .OK else {
      return nil
    }

    return panel.url
  }
}
