import CoreVideo
import Foundation
import Metal

enum CoreVideoMetalTextureBridge {
  static func makeTextureCache(device: MTLDevice) throws -> CVMetalTextureCache {
    var cache: CVMetalTextureCache?
    let status = CVMetalTextureCacheCreate(kCFAllocatorDefault, nil, device, nil, &cache)
    guard status == kCVReturnSuccess, let cache else {
      throw CoreVideoMetalTextureBridgeError.textureCacheCreationFailed(status)
    }
    return cache
  }

  static func makeTexture(
    from pixelBuffer: CVPixelBuffer,
    device: MTLDevice,
    pixelFormat: MTLPixelFormat,
    planeIndex: Int = 0
  ) throws -> MTLTexture {
    let cache = try makeTextureCache(device: device)
    return try makeTexture(
      from: pixelBuffer,
      cache: cache,
      pixelFormat: pixelFormat,
      planeIndex: planeIndex
    )
  }

  static func makeTexture(
    from pixelBuffer: CVPixelBuffer,
    cache: CVMetalTextureCache,
    pixelFormat: MTLPixelFormat,
    planeIndex: Int = 0
  ) throws -> MTLTexture {
    let dimensions = try textureDimensions(for: pixelBuffer, planeIndex: planeIndex)
    var cvTexture: CVMetalTexture?
    let status = CVMetalTextureCacheCreateTextureFromImage(
      kCFAllocatorDefault,
      cache,
      pixelBuffer,
      nil,
      pixelFormat,
      dimensions.width,
      dimensions.height,
      planeIndex,
      &cvTexture
    )
    guard status == kCVReturnSuccess, let cvTexture else {
      throw CoreVideoMetalTextureBridgeError.textureCreationFailed(status)
    }
    guard let texture = CVMetalTextureGetTexture(cvTexture) else {
      throw CoreVideoMetalTextureBridgeError.textureMissing
    }
    return texture
  }

  static func textureDimensions(
    for pixelBuffer: CVPixelBuffer,
    planeIndex: Int = 0
  ) throws -> PixelDimensions {
    let planeCount = CVPixelBufferGetPlaneCount(pixelBuffer)
    if planeCount == 0 {
      guard planeIndex == 0 else {
        throw CoreVideoMetalTextureBridgeError.invalidPlaneIndex(
          index: planeIndex,
          planeCount: planeCount
        )
      }
      return PixelDimensions(
        width: CVPixelBufferGetWidth(pixelBuffer),
        height: CVPixelBufferGetHeight(pixelBuffer)
      )
    }

    guard planeIndex >= 0 && planeIndex < planeCount else {
      throw CoreVideoMetalTextureBridgeError.invalidPlaneIndex(
        index: planeIndex,
        planeCount: planeCount
      )
    }
    return PixelDimensions(
      width: CVPixelBufferGetWidthOfPlane(pixelBuffer, planeIndex),
      height: CVPixelBufferGetHeightOfPlane(pixelBuffer, planeIndex)
    )
  }
}

enum CoreVideoMetalTextureBridgeError: LocalizedError, Equatable {
  case invalidPlaneIndex(index: Int, planeCount: Int)
  case textureCacheCreationFailed(OSStatus)
  case textureCreationFailed(OSStatus)
  case textureMissing

  var errorDescription: String? {
    switch self {
    case let .invalidPlaneIndex(index, planeCount):
      return "Pixel buffer plane \(index) is invalid for \(planeCount) available planes."
    case let .textureCacheCreationFailed(status):
      return "CoreVideo Metal texture cache creation failed with status \(status)."
    case let .textureCreationFailed(status):
      return "CoreVideo Metal texture creation failed with status \(status)."
    case .textureMissing:
      return "CoreVideo returned a texture wrapper without an MTLTexture."
    }
  }
}
