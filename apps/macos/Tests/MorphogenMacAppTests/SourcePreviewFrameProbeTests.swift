import AVFoundation
import CoreGraphics
import Foundation
import ImageIO
import Metal
@testable import MorphogenMacApp
import UniformTypeIdentifiers
import XCTest

final class SourcePreviewFrameProbeTests: XCTestCase {
  func testDecodesFirstMovieFrameIntoMetalTextureWhenSupported() async throws {
    guard let device = MTLCreateSystemDefaultDevice() else {
      throw XCTSkip("No Metal device available for preview-frame probe test.")
    }

    let directory = try makeTemporaryDirectory()
    try writePNGFrame(directory.appendingPathComponent("frame_000000.png"))
    let movieURL = directory.appendingPathComponent("preview-source.mov")

    let plan = try VideoToolboxProResExportPlanner.makePlan(
      width: 16,
      height: 16,
      frameRate: 24.0,
      profile: .proRes422HQ
    )
    let support = VideoToolboxProResExportPlanner.probeSupport(for: plan)
    try XCTSkipUnless(support.isSupported, support.compactSummary)

    _ = try await ProResImageSequenceExporter.exportPNGSequence(
      request: ProResImageSequenceExportRequest(
        frameDirectory: directory,
        outputURL: movieURL,
        frameRate: 24.0,
        profile: .proRes422HQ,
        requiresHardwareEncoder: false
      )
    )

    let result = try await SourcePreviewFrameProbe.decodeFirstVideoFrame(
      mediaURL: movieURL,
      device: device
    )

    XCTAssertEqual(result.sourceDimensions.width, 16)
    XCTAssertEqual(result.sourceDimensions.height, 16)
    XCTAssertEqual(result.textureDimensions.width, 16)
    XCTAssertEqual(result.textureDimensions.height, 16)
    XCTAssertEqual(result.presentationTimeSeconds ?? -1.0, 0.0, accuracy: 0.001)
    XCTAssertEqual(result.previewImage.size.width, 16)
    XCTAssertEqual(result.previewImage.size.height, 16)
    XCTAssertTrue(result.compactSummary.contains("Metal"))
  }

  func testPixelFormatNameFallsBackToRawValue() {
    XCTAssertEqual(SourcePreviewFrameProbe.pixelFormatName(.bgra8Unorm), "bgra8Unorm")
    XCTAssertEqual(
      SourcePreviewFrameProbe.pixelFormatName(.r8Unorm),
      "pixelFormat(\(MTLPixelFormat.r8Unorm.rawValue))"
    )
  }

  private func makeTemporaryDirectory() throws -> URL {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-preview-tests-\(UUID().uuidString)",
      isDirectory: true
    )
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
  }

  private func writePNGFrame(_ url: URL) throws {
    let width = 16
    let height = 16
    let bytesPerPixel = 4
    let bytesPerRow = width * bytesPerPixel
    var pixels = [UInt8](repeating: 0, count: width * height * bytesPerPixel)

    for y in 0..<height {
      for x in 0..<width {
        let offset = (y * bytesPerRow) + (x * bytesPerPixel)
        pixels[offset] = UInt8((x * 11) % 255)
        pixels[offset + 1] = UInt8((y * 19) % 255)
        pixels[offset + 2] = UInt8((x * 3 + y * 5) % 255)
        pixels[offset + 3] = 255
      }
    }

    let data = Data(pixels)
    guard let provider = CGDataProvider(data: data as CFData) else {
      XCTFail("Could not create image data provider")
      return
    }
    guard let image = CGImage(
      width: width,
      height: height,
      bitsPerComponent: 8,
      bitsPerPixel: 32,
      bytesPerRow: bytesPerRow,
      space: CGColorSpaceCreateDeviceRGB(),
      bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
      provider: provider,
      decode: nil,
      shouldInterpolate: false,
      intent: .defaultIntent
    ) else {
      XCTFail("Could not create test image")
      return
    }
    guard let destination = CGImageDestinationCreateWithURL(
      url as CFURL,
      UTType.png.identifier as CFString,
      1,
      nil
    ) else {
      XCTFail("Could not create PNG destination")
      return
    }

    CGImageDestinationAddImage(destination, image, nil)
    XCTAssertTrue(CGImageDestinationFinalize(destination))
  }
}
