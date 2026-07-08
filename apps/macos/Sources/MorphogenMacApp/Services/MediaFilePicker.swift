import AppKit
import Foundation
import UniformTypeIdentifiers

enum MediaFilePicker {
  static func chooseWAVFile(title: String, message: String) -> URL? {
    let panel = NSOpenPanel()
    panel.title = title
    panel.message = message
    panel.prompt = "Choose"
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    panel.resolvesAliases = true
    panel.allowedContentTypes = [.wav]

    guard panel.runModal() == .OK else {
      return nil
    }

    return panel.url
  }

  static func chooseMIDIFile(title: String, message: String) -> URL? {
    let panel = NSOpenPanel()
    panel.title = title
    panel.message = message
    panel.prompt = "Choose"
    panel.canChooseFiles = true
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    panel.resolvesAliases = true
    panel.allowedContentTypes = [.midi]

    guard panel.runModal() == .OK else {
      return nil
    }

    return panel.url
  }

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
