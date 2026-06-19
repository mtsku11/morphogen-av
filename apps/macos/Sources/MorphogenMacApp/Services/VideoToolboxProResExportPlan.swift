import CoreMedia
import CoreVideo
import Foundation
import VideoToolbox

enum ProResExportProfile: String, CaseIterable, Identifiable {
  case proRes422Proxy
  case proRes422LT
  case proRes422
  case proRes422HQ
  case proRes4444
  case proRes4444XQ

  var id: String { rawValue }

  var displayName: String {
    switch self {
    case .proRes422Proxy:
      return "Apple ProRes 422 Proxy"
    case .proRes422LT:
      return "Apple ProRes 422 LT"
    case .proRes422:
      return "Apple ProRes 422"
    case .proRes422HQ:
      return "Apple ProRes 422 HQ"
    case .proRes4444:
      return "Apple ProRes 4444"
    case .proRes4444XQ:
      return "Apple ProRes 4444 XQ"
    }
  }

  var codecType: CMVideoCodecType {
    switch self {
    case .proRes422Proxy:
      return kCMVideoCodecType_AppleProRes422Proxy
    case .proRes422LT:
      return kCMVideoCodecType_AppleProRes422LT
    case .proRes422:
      return kCMVideoCodecType_AppleProRes422
    case .proRes422HQ:
      return kCMVideoCodecType_AppleProRes422HQ
    case .proRes4444:
      return kCMVideoCodecType_AppleProRes4444
    case .proRes4444XQ:
      return kCMVideoCodecType_AppleProRes4444XQ
    }
  }
}

struct ProResExportPlan {
  let profile: ProResExportProfile
  let dimensions: PixelDimensions
  let frameRate: Double
  let sourcePixelFormat: OSType
  let requiresHardwareEncoder: Bool

  var encoderSpecification: [String: Any] {
    var specification = [
      kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder as String: true
    ] as [String: Any]
    if requiresHardwareEncoder {
      specification[kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder as String] = true
    }
    return specification
  }

  var sourceImageBufferAttributes: [String: Any] {
    [
      kCVPixelBufferPixelFormatTypeKey as String: NSNumber(value: sourcePixelFormat),
      kCVPixelBufferWidthKey as String: NSNumber(value: dimensions.width),
      kCVPixelBufferHeightKey as String: NSNumber(value: dimensions.height),
      kCVPixelBufferMetalCompatibilityKey as String: true,
      kCVPixelBufferIOSurfacePropertiesKey as String: [:] as [String: Any]
    ]
  }

  var compactSummary: String {
    let hardwareMode = requiresHardwareEncoder ? "hardware required" : "hardware preferred"
    return String(
      format: "%@ (%@) via VideoToolbox, %dx%d @ %.3f fps, source %@, %@",
      profile.displayName,
      FourCharacterCodeFormatter.string(from: profile.codecType),
      dimensions.width,
      dimensions.height,
      frameRate,
      FourCharacterCodeFormatter.string(from: sourcePixelFormat),
      hardwareMode
    )
  }
}

struct VideoToolboxEncoderSummary: Identifiable {
  let codecType: CMVideoCodecType
  let codecName: String
  let encoderID: String?
  let displayName: String
  let isHardwareAccelerated: Bool
  let gpuRegistryID: UInt64?

  var id: String {
    "\(codecType)-\(encoderID ?? displayName)"
  }

  var codecFourCC: String {
    FourCharacterCodeFormatter.string(from: codecType)
  }
}

struct VideoToolboxProResSupport {
  let profile: ProResExportProfile
  let dimensions: PixelDimensions
  let status: OSStatus
  let encoderID: String?
  let supportedPropertyCount: Int

  var isSupported: Bool {
    status == noErr
  }

  var compactSummary: String {
    if isSupported {
      let encoder = encoderID ?? "system-selected encoder"
      return "VideoToolbox supports \(profile.displayName) with \(encoder); \(supportedPropertyCount) properties reported"
    }
    return "VideoToolbox support check failed for \(profile.displayName) at \(dimensions.width)x\(dimensions.height) with status \(status)"
  }
}

enum VideoToolboxProResExportPlanner {
  static func defaultPlanSummary() -> String {
    do {
      return try makePlan(
        width: 1920,
        height: 1080,
        frameRate: 24.0,
        profile: .proRes422HQ
      ).compactSummary
    } catch {
      return "ProRes plan unavailable: \(error.localizedDescription)"
    }
  }

  static func makePlan(
    width: Int,
    height: Int,
    frameRate: Double,
    profile: ProResExportProfile,
    requiresHardwareEncoder: Bool = false
  ) throws -> ProResExportPlan {
    guard width > 0 && height > 0 && width <= Int(Int32.max) && height <= Int(Int32.max) else {
      throw VideoToolboxProResExportPlannerError.invalidDimensions(width: width, height: height)
    }
    guard frameRate.isFinite && frameRate > 0 else {
      throw VideoToolboxProResExportPlannerError.invalidFrameRate(frameRate)
    }

    return ProResExportPlan(
      profile: profile,
      dimensions: PixelDimensions(width: width, height: height),
      frameRate: frameRate,
      sourcePixelFormat: kCVPixelFormatType_32BGRA,
      requiresHardwareEncoder: requiresHardwareEncoder
    )
  }

  static func availableProResEncoderSummaries() throws -> [VideoToolboxEncoderSummary] {
    let proResCodecTypes = Set(ProResExportProfile.allCases.map(\.codecType))
    return try availableEncoderSummaries()
      .filter { proResCodecTypes.contains($0.codecType) }
  }

  static func availableEncoderSummaries() throws -> [VideoToolboxEncoderSummary] {
    VTRegisterProfessionalVideoWorkflowVideoEncoders()

    var encoderList: CFArray?
    let status = VTCopyVideoEncoderList(nil, &encoderList)
    guard status == noErr else {
      throw VideoToolboxProResExportPlannerError.encoderListUnavailable(status)
    }
    guard let dictionaries = encoderList as? [[CFString: Any]] else {
      throw VideoToolboxProResExportPlannerError.encoderListMalformed
    }

    return dictionaries.compactMap { dictionary in
      guard
        let codecNumber = dictionary[kVTVideoEncoderList_CodecType] as? NSNumber,
        let displayName = dictionary[kVTVideoEncoderList_DisplayName] as? String
      else {
        return nil
      }

      let codecName = dictionary[kVTVideoEncoderList_CodecName] as? String ?? displayName
      let encoderID = dictionary[kVTVideoEncoderList_EncoderID] as? String
      let gpuRegistryID = (dictionary[kVTVideoEncoderList_GPURegistryID] as? NSNumber)?.uint64Value

      return VideoToolboxEncoderSummary(
        codecType: codecNumber.uint32Value,
        codecName: codecName,
        encoderID: encoderID,
        displayName: displayName,
        isHardwareAccelerated: (dictionary[kVTVideoEncoderList_IsHardwareAccelerated] as? Bool) ?? false,
        gpuRegistryID: gpuRegistryID
      )
    }
  }

  static func probeSupport(for plan: ProResExportPlan) -> VideoToolboxProResSupport {
    VTRegisterProfessionalVideoWorkflowVideoEncoders()

    var encoderID: CFString?
    var supportedProperties: CFDictionary?
    let status = VTCopySupportedPropertyDictionaryForEncoder(
      width: Int32(plan.dimensions.width),
      height: Int32(plan.dimensions.height),
      codecType: plan.profile.codecType,
      encoderSpecification: plan.encoderSpecification as CFDictionary,
      encoderIDOut: &encoderID,
      supportedPropertiesOut: &supportedProperties
    )

    return VideoToolboxProResSupport(
      profile: plan.profile,
      dimensions: plan.dimensions,
      status: status,
      encoderID: encoderID.map { $0 as String },
      supportedPropertyCount: supportedProperties.map { CFDictionaryGetCount($0) } ?? 0
    )
  }
}

enum VideoToolboxProResExportPlannerError: LocalizedError, Equatable {
  case invalidDimensions(width: Int, height: Int)
  case invalidFrameRate(Double)
  case encoderListUnavailable(OSStatus)
  case encoderListMalformed

  var errorDescription: String? {
    switch self {
    case let .invalidDimensions(width, height):
      return "ProRes export dimensions must be positive Int32 values, got \(width)x\(height)."
    case let .invalidFrameRate(frameRate):
      return "ProRes export frame rate must be finite and positive, got \(frameRate)."
    case let .encoderListUnavailable(status):
      return "VideoToolbox encoder list failed with status \(status)."
    case .encoderListMalformed:
      return "VideoToolbox returned an encoder list with an unexpected shape."
    }
  }
}

private enum FourCharacterCodeFormatter {
  static func string(from code: OSType) -> String {
    let bytes = [
      UInt8((code >> 24) & 0xff),
      UInt8((code >> 16) & 0xff),
      UInt8((code >> 8) & 0xff),
      UInt8(code & 0xff)
    ]

    guard bytes.allSatisfy({ byte in byte >= 32 && byte <= 126 }) else {
      return "\(code)"
    }
    return String(bytes: bytes, encoding: .ascii) ?? "\(code)"
  }
}
