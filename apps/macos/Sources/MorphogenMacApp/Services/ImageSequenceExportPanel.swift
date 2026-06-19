import AppKit
import Foundation
import UniformTypeIdentifiers

enum ImageSequenceExportPanel {
  static func chooseFrameDirectory(
    title: String = "Choose PNG Frame Sequence",
    message: String = "Select a directory containing PNG frames."
  ) -> URL? {
    let panel = NSOpenPanel()
    panel.title = title
    panel.message = message
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

  static func chooseFrameSequenceOutputDirectory(
    defaultName: String = "morphogen-two-source-frames"
  ) -> URL? {
    let panel = NSSavePanel()
    panel.title = "Choose Frame Sequence Output"
    panel.message = "Choose the output directory path for rendered PNG frames."
    panel.prompt = "Choose"
    panel.nameFieldStringValue = defaultName
    panel.canCreateDirectories = true

    guard panel.runModal() == .OK else {
      return nil
    }

    return panel.url
  }

  static func chooseMediaProxyOutputDirectory(
    defaultName: String = "morphogen-media-proxies"
  ) -> URL? {
    let panel = NSSavePanel()
    panel.title = "Choose Media Proxy Output"
    panel.message = "Choose where extracted PNG frame and WAV proxies are written."
    panel.prompt = "Choose"
    panel.nameFieldStringValue = defaultName
    panel.canCreateDirectories = true

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
