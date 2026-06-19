import AppKit
import Foundation

enum MediaFilePicker {
  static func chooseMediaFile(for role: SourceRole) -> URL? {
    let panel = NSOpenPanel()
    panel.title = "Choose \(role.rawValue) Source"
    panel.message = "Select an audiovisual, video, or audio source for Morphogen AV."
    panel.prompt = "Choose"
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    panel.resolvesAliases = true

    guard panel.runModal() == .OK else {
      return nil
    }

    return panel.url
  }
}
