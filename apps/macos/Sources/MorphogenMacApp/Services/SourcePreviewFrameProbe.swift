import AppKit
import CoreMedia
import CoreImage
import CoreVideo
import Foundation
import Metal

struct SourcePreviewFrameProbeResult {
  let mediaURL: URL
  let sourceDimensions: PixelDimensions
  let textureDimensions: PixelDimensions
  let presentationTimeSeconds: Double?
  let texturePixelFormat: String
  let previewImage: NSImage

  var compactSummary: String {
    let timeSummary: String
    if let presentationTimeSeconds {
      timeSummary = String(format: "%.3fs", presentationTimeSeconds)
    } else {
      timeSummary = "unknown time"
    }

    return "Preview: first frame \(sourceDimensions.width)x\(sourceDimensions.height) at \(timeSummary); Metal \(texturePixelFormat) texture \(textureDimensions.width)x\(textureDimensions.height)"
  }
}

enum SourcePreviewFrameProbe {
  static func decodeFirstVideoFrame(
    mediaURL: URL,
    device: MTLDevice
  ) async throws -> SourcePreviewFrameProbeResult {
    let frame = try await AVFoundationFrameTextureBridge.copyFirstVideoFramePixelBuffer(
      mediaURL: mediaURL
    )
    let texture = try CoreVideoMetalTextureBridge.makeTexture(
      from: frame.pixelBuffer,
      device: device,
      pixelFormat: .bgra8Unorm
    )

    let seconds = CMTimeGetSeconds(frame.presentationTime)
    return SourcePreviewFrameProbeResult(
      mediaURL: mediaURL,
      sourceDimensions: frame.dimensions,
      textureDimensions: PixelDimensions(width: texture.width, height: texture.height),
      presentationTimeSeconds: seconds.isFinite ? seconds : nil,
      texturePixelFormat: pixelFormatName(texture.pixelFormat),
      previewImage: try makePreviewImage(
        from: frame.pixelBuffer,
        dimensions: frame.dimensions
      )
    )
  }

  static func pixelFormatName(_ pixelFormat: MTLPixelFormat) -> String {
    switch pixelFormat {
    case .bgra8Unorm:
      return "bgra8Unorm"
    case .rgba8Unorm:
      return "rgba8Unorm"
    case .rgba16Float:
      return "rgba16Float"
    case .rgba32Float:
      return "rgba32Float"
    default:
      return "pixelFormat(\(pixelFormat.rawValue))"
    }
  }

  private static func makePreviewImage(
    from pixelBuffer: CVPixelBuffer,
    dimensions: PixelDimensions
  ) throws -> NSImage {
    let ciImage = CIImage(cvPixelBuffer: pixelBuffer)
    let context = CIContext()
    guard let cgImage = context.createCGImage(ciImage, from: ciImage.extent.integral) else {
      throw SourcePreviewFrameProbeError.previewImageCreationFailed
    }

    return NSImage(
      cgImage: cgImage,
      size: NSSize(width: dimensions.width, height: dimensions.height)
    )
  }
}

enum SourcePreviewFrameProbeError: LocalizedError {
  case previewImageCreationFailed

  var errorDescription: String? {
    switch self {
    case .previewImageCreationFailed:
      return "Could not create a preview image from the decoded CoreVideo pixel buffer."
    }
  }
}
