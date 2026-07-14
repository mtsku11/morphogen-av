import SwiftUI
import XCTest

@testable import MorphogenMacApp

/// Offscreen visual-verification harness: renders representative effect detail
/// panels to PNGs so layout changes (knobs, ControlFlow wrapping) can be
/// inspected without launching the app. Opt-in via
/// `MORPHOGEN_RENDER_PANEL_PREVIEW=1` so the normal suite stays fast.
final class PanelPreviewRenderTests: XCTestCase {
  @MainActor
  func testRenderPanelPreviews() throws {
    guard ProcessInfo.processInfo.environment["MORPHOGEN_RENDER_PANEL_PREVIEW"] == "1" else {
      throw XCTSkip("set MORPHOGEN_RENDER_PANEL_PREVIEW=1 to render panel previews")
    }

    let state = AppState()
    let panels: [(String, AnyView)] = [
      ("datamosh", AnyView(DatamoshDetailView(state: state))),
      ("bitstream", AnyView(BitstreamDatamoshDetailView(state: state))),
      ("pixel-sort", AnyView(PixelSortDetailView(state: state))),
      ("morphogenesis", AnyView(MorphogenesisDetailView(state: state))),
      ("flow-feedback", AnyView(FlowFeedbackDetailView(state: state))),
    ]

    for (name, panel) in panels {
      let renderer = ImageRenderer(
        content: panel
          .frame(width: 1180, alignment: .topLeading)
          .padding(20)
          .background(Color(nsColor: .windowBackgroundColor))
      )
      renderer.scale = 2
      guard let image = renderer.nsImage, let tiff = image.tiffRepresentation,
        let rep = NSBitmapImageRep(data: tiff),
        let png = rep.representation(using: .png, properties: [:])
      else {
        XCTFail("could not render \(name)")
        continue
      }
      let url = URL(fileURLWithPath: "/tmp/panel-preview-\(name).png")
      try png.write(to: url)
      print("wrote \(url.path)")
    }
  }
}
