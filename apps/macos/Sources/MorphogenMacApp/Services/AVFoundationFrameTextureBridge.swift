import AVFoundation
import CoreMedia
import CoreVideo
import Foundation
import Metal

struct DecodedVideoPixelBufferFrame {
  let pixelBuffer: CVPixelBuffer
  let presentationTime: CMTime
  let dimensions: PixelDimensions
}

struct DecodedVideoTextureFrame {
  let texture: MTLTexture
  let presentationTime: CMTime
  let dimensions: PixelDimensions
}

enum AVFoundationFrameTextureBridge {
  static func copyFirstVideoFramePixelBuffer(
    mediaURL: URL
  ) async throws -> DecodedVideoPixelBufferFrame {
    let asset = AVURLAsset(url: mediaURL)
    let tracks = try await asset.load(.tracks)
    guard let videoTrack = tracks.first(where: { $0.mediaType == .video }) else {
      throw AVFoundationFrameTextureBridgeError.noVideoTrack(mediaURL)
    }

    let reader = try AVAssetReader(asset: asset)
    let output = AVAssetReaderTrackOutput(
      track: videoTrack,
      outputSettings: pixelBufferOutputSettings()
    )
    output.alwaysCopiesSampleData = false

    guard reader.canAdd(output) else {
      throw AVFoundationFrameTextureBridgeError.cannotAddTrackOutput
    }
    reader.add(output)

    guard reader.startReading() else {
      throw AVFoundationFrameTextureBridgeError.readerStartFailed(reader.error)
    }
    guard let sampleBuffer = output.copyNextSampleBuffer() else {
      throw AVFoundationFrameTextureBridgeError.noSampleBuffer(
        status: reader.status,
        underlyingError: reader.error
      )
    }
    guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else {
      throw AVFoundationFrameTextureBridgeError.missingPixelBuffer
    }

    return DecodedVideoPixelBufferFrame(
      pixelBuffer: pixelBuffer,
      presentationTime: CMSampleBufferGetPresentationTimeStamp(sampleBuffer),
      dimensions: try CoreVideoMetalTextureBridge.textureDimensions(for: pixelBuffer)
    )
  }

  static func copyFirstVideoFrameTexture(
    mediaURL: URL,
    device: MTLDevice
  ) async throws -> DecodedVideoTextureFrame {
    let cache = try CoreVideoMetalTextureBridge.makeTextureCache(device: device)
    return try await copyFirstVideoFrameTexture(mediaURL: mediaURL, cache: cache)
  }

  static func copyFirstVideoFrameTexture(
    mediaURL: URL,
    cache: CVMetalTextureCache
  ) async throws -> DecodedVideoTextureFrame {
    let decodedFrame = try await copyFirstVideoFramePixelBuffer(mediaURL: mediaURL)
    let texture = try CoreVideoMetalTextureBridge.makeTexture(
      from: decodedFrame.pixelBuffer,
      cache: cache,
      pixelFormat: .bgra8Unorm
    )

    return DecodedVideoTextureFrame(
      texture: texture,
      presentationTime: decodedFrame.presentationTime,
      dimensions: decodedFrame.dimensions
    )
  }

  private static func pixelBufferOutputSettings() -> [String: Any] {
    [
      kCVPixelBufferPixelFormatTypeKey as String: NSNumber(value: kCVPixelFormatType_32BGRA),
      kCVPixelBufferMetalCompatibilityKey as String: true,
      kCVPixelBufferIOSurfacePropertiesKey as String: [:] as [String: Any]
    ]
  }
}

enum AVFoundationFrameTextureBridgeError: LocalizedError {
  case noVideoTrack(URL)
  case cannotAddTrackOutput
  case readerStartFailed(Error?)
  case noSampleBuffer(status: AVAssetReader.Status, underlyingError: Error?)
  case missingPixelBuffer

  var errorDescription: String? {
    switch self {
    case let .noVideoTrack(url):
      return "AVFoundation found no video track in \(url.lastPathComponent)."
    case .cannotAddTrackOutput:
      return "AVFoundation could not add a BGRA video track output for frame decoding."
    case let .readerStartFailed(error):
      return "AVFoundation asset reader could not start: \(error?.localizedDescription ?? "unknown error")."
    case let .noSampleBuffer(status, error):
      return "AVFoundation produced no video sample buffer, reader status \(status.rawValue): \(error?.localizedDescription ?? "no underlying error")."
    case .missingPixelBuffer:
      return "AVFoundation returned a video sample without a CoreVideo pixel buffer."
    }
  }
}
