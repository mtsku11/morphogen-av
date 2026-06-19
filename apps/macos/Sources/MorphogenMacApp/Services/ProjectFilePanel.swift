import AppKit
import Foundation
import UniformTypeIdentifiers

enum ProjectFilePanel {
  static func chooseProjectFile() -> URL? {
    let panel = NSOpenPanel()
    panel.title = "Open Morphogen Project"
    panel.message = "Select a .morphogen.json project file."
    panel.prompt = "Open"
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    panel.allowedContentTypes = [.json]
    panel.resolvesAliases = true

    guard panel.runModal() == .OK else {
      return nil
    }

    return panel.url
  }

  static func chooseProjectSaveLocation() -> URL? {
    let panel = NSSavePanel()
    panel.title = "Create Test Morphogen Project"
    panel.message = "Choose where to write the example Morphogen project."
    panel.prompt = "Create"
    panel.nameFieldStringValue = "two-source-flow-displace.morphogen.json"
    panel.allowedContentTypes = [.json]
    panel.canCreateDirectories = true

    guard panel.runModal() == .OK else {
      return nil
    }

    return panel.url
  }
}
