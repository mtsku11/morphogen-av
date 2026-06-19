import AVFoundation
import AudioToolbox
import CoreGraphics
import CoreMedia
import CoreVideo
import Foundation
import ImageIO
import VideoToolbox

struct ProResImageSequenceExportRequest {
  let frameDirectory: URL
  let outputURL: URL
  let frameRate: Double
  let profile: ProResExportProfile
  let requiresHardwareEncoder: Bool
  let audioStemURL: URL?

  init(
    frameDirectory: URL,
    outputURL: URL,
    frameRate: Double,
    profile: ProResExportProfile,
    requiresHardwareEncoder: Bool,
    audioStemURL: URL? = nil
  ) {
    self.frameDirectory = frameDirectory
    self.outputURL = outputURL
    self.frameRate = frameRate
    self.profile = profile
    self.requiresHardwareEncoder = requiresHardwareEncoder
    self.audioStemURL = audioStemURL
  }
}

struct ProResImageSequenceExportResult {
  let outputURL: URL
  let frameCount: Int
  let dimensions: PixelDimensions
  let frameRate: Double
  let profile: ProResExportProfile
  let audioStemURL: URL?
  let audioSampleBufferCount: Int

  var durationSeconds: Double {
    Double(frameCount) / frameRate
  }

  var audioTrackCount: Int {
    audioStemURL == nil ? 0 : 1
  }

  var compactSummary: String {
    let audioSummary = audioStemURL.map {
      ", audio \($0.lastPathComponent) (\(audioSampleBufferCount) sample buffer(s))"
    } ?? ", video only"
    return String(
      format: "Wrote %@ with %d frame(s), %dx%d @ %.3f fps, %@%@",
      outputURL.lastPathComponent,
      frameCount,
      dimensions.width,
      dimensions.height,
      frameRate,
      profile.displayName,
      audioSummary
    )
  }
}

private struct PreparedAudioStem {
  let url: URL
  let asset: AVURLAsset
  let track: AVAssetTrack
  let input: AVAssetWriterInput
  let outputSettings: [String: Any]
}

enum ProResImageSequenceExporter {
  static func exportPNGSequence(
    request: ProResImageSequenceExportRequest
  ) async throws -> ProResImageSequenceExportResult {
    guard request.frameRate.isFinite && request.frameRate > 0 else {
      throw ProResImageSequenceExporterError.invalidFrameRate(request.frameRate)
    }
    guard !FileManager.default.fileExists(atPath: request.outputURL.path) else {
      throw ProResImageSequenceExporterError.outputAlreadyExists(request.outputURL)
    }

    let frameURLs = try collectPNGFrameURLs(in: request.frameDirectory)
    guard let firstFrameURL = frameURLs.first else {
      throw ProResImageSequenceExporterError.noFramesFound(request.frameDirectory)
    }

    let firstImage = try loadCGImage(from: firstFrameURL)
    let dimensions = PixelDimensions(width: firstImage.width, height: firstImage.height)
    let plan = try VideoToolboxProResExportPlanner.makePlan(
      width: dimensions.width,
      height: dimensions.height,
      frameRate: request.frameRate,
      profile: request.profile,
      requiresHardwareEncoder: request.requiresHardwareEncoder
    )

    let support = VideoToolboxProResExportPlanner.probeSupport(for: plan)
    guard support.isSupported else {
      throw ProResImageSequenceExporterError.encoderUnsupported(support)
    }

    let writer = try AVAssetWriter(outputURL: request.outputURL, fileType: .mov)
    let outputSettings = videoOutputSettings(for: plan)
    guard writer.canApply(outputSettings: outputSettings, forMediaType: .video) else {
      throw ProResImageSequenceExporterError.outputSettingsUnsupported
    }

    let input = AVAssetWriterInput(mediaType: .video, outputSettings: outputSettings)
    input.expectsMediaDataInRealTime = false

    guard writer.canAdd(input) else {
      throw ProResImageSequenceExporterError.cannotAddInput
    }
    writer.add(input)

    let adaptor = AVAssetWriterInputPixelBufferAdaptor(
      assetWriterInput: input,
      sourcePixelBufferAttributes: plan.sourceImageBufferAttributes
    )

    let audioStem = try await prepareAudioStemIfNeeded(
      request.audioStemURL,
      writer: writer
    )

    var audioSampleBufferCount = 0
    do {
      guard writer.startWriting() else {
        throw ProResImageSequenceExporterError.startFailed(
          writerStatus: writer.status,
          underlyingError: writer.error
        )
      }
      writer.startSession(atSourceTime: .zero)

      for (index, frameURL) in frameURLs.enumerated() {
        try Task.checkCancellation()
        while !input.isReadyForMoreMediaData {
          try await Task.sleep(nanoseconds: 1_000_000)
          try Task.checkCancellation()
        }

        let image = index == 0 ? firstImage : try loadCGImage(from: frameURL)
        guard image.width == dimensions.width && image.height == dimensions.height else {
          throw ProResImageSequenceExporterError.frameDimensionsMismatch(
            frameURL,
            expected: dimensions,
            actual: PixelDimensions(width: image.width, height: image.height)
          )
        }

        let pixelBuffer = try makePixelBuffer(from: image, using: adaptor)
        let presentationTime = CMTime(
          seconds: Double(index) / request.frameRate,
          preferredTimescale: 60_000
        )

        guard adaptor.append(pixelBuffer, withPresentationTime: presentationTime) else {
          throw ProResImageSequenceExporterError.appendFailed(
            frameURL,
            writerStatus: writer.status,
            underlyingError: writer.error
          )
        }
      }

      input.markAsFinished()
      audioSampleBufferCount = try await appendAudioStemIfNeeded(audioStem, writer: writer)
      await writer.finishWriting()
      guard writer.status == .completed else {
        throw ProResImageSequenceExporterError.finishFailed(
          writerStatus: writer.status,
          underlyingError: writer.error
        )
      }
    } catch {
      writer.cancelWriting()
      throw error
    }

    return ProResImageSequenceExportResult(
      outputURL: request.outputURL,
      frameCount: frameURLs.count,
      dimensions: dimensions,
      frameRate: request.frameRate,
      profile: request.profile,
      audioStemURL: request.audioStemURL,
      audioSampleBufferCount: audioSampleBufferCount
    )
  }

  static func collectPNGFrameURLs(in directory: URL) throws -> [URL] {
    var isDirectory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: directory.path, isDirectory: &isDirectory),
          isDirectory.boolValue
    else {
      throw ProResImageSequenceExporterError.frameDirectoryMissing(directory)
    }

    let urls = try FileManager.default.contentsOfDirectory(
      at: directory,
      includingPropertiesForKeys: [.isRegularFileKey],
      options: [.skipsHiddenFiles]
    )

    let frames = try urls.filter { url in
      let values = try url.resourceValues(forKeys: [.isRegularFileKey])
      return values.isRegularFile == true && url.pathExtension.lowercased() == "png"
    }
    .sorted { lhs, rhs in
      lhs.lastPathComponent.localizedStandardCompare(rhs.lastPathComponent) == .orderedAscending
    }

    return frames
  }

  private static func videoOutputSettings(for plan: ProResExportPlan) -> [String: Any] {
    [
      AVVideoCodecKey: avVideoCodecType(for: plan.profile),
      AVVideoWidthKey: NSNumber(value: plan.dimensions.width),
      AVVideoHeightKey: NSNumber(value: plan.dimensions.height),
      AVVideoEncoderSpecificationKey: plan.encoderSpecification
    ]
  }

  private static func avVideoCodecType(for profile: ProResExportProfile) -> AVVideoCodecType {
    switch profile {
    case .proRes422Proxy:
      return .proRes422Proxy
    case .proRes422LT:
      return .proRes422LT
    case .proRes422:
      return .proRes422
    case .proRes422HQ:
      return .proRes422HQ
    case .proRes4444:
      return .proRes4444
    case .proRes4444XQ:
      return AVVideoCodecType(rawValue: "ap4x")
    }
  }

  private static func prepareAudioStemIfNeeded(
    _ audioStemURL: URL?,
    writer: AVAssetWriter
  ) async throws -> PreparedAudioStem? {
    guard let audioStemURL else {
      return nil
    }

    var isDirectory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: audioStemURL.path, isDirectory: &isDirectory),
          !isDirectory.boolValue
    else {
      throw ProResImageSequenceExporterError.audioStemMissing(audioStemURL)
    }

    let asset = AVURLAsset(url: audioStemURL)
    let audioTracks = try await asset.loadTracks(withMediaType: .audio)
    guard let audioTrack = audioTracks.first else {
      throw ProResImageSequenceExporterError.audioStemHasNoAudioTrack(audioStemURL)
    }

    let audioOutputSettings = try await pcmAudioOutputSettings(
      for: audioTrack,
      stemURL: audioStemURL
    )
    guard writer.canApply(outputSettings: audioOutputSettings, forMediaType: .audio) else {
      throw ProResImageSequenceExporterError.audioOutputSettingsUnsupported(audioStemURL)
    }

    let audioInput = AVAssetWriterInput(mediaType: .audio, outputSettings: audioOutputSettings)
    audioInput.expectsMediaDataInRealTime = false
    guard writer.canAdd(audioInput) else {
      throw ProResImageSequenceExporterError.cannotAddAudioInput(audioStemURL)
    }
    writer.add(audioInput)

    return PreparedAudioStem(
      url: audioStemURL,
      asset: asset,
      track: audioTrack,
      input: audioInput,
      outputSettings: audioOutputSettings
    )
  }

  private static func pcmAudioOutputSettings(
    for track: AVAssetTrack,
    stemURL: URL
  ) async throws -> [String: Any] {
    let formatDescriptions = try await track.load(.formatDescriptions)
    guard let formatDescription = formatDescriptions.first,
          let streamDescription = CMAudioFormatDescriptionGetStreamBasicDescription(formatDescription)
    else {
      throw ProResImageSequenceExporterError.audioFormatDescriptionMissing(stemURL)
    }

    let sampleRate = streamDescription.pointee.mSampleRate
    let channelCount = streamDescription.pointee.mChannelsPerFrame
    guard sampleRate.isFinite && sampleRate > 0 && channelCount > 0 else {
      throw ProResImageSequenceExporterError.audioFormatDescriptionMissing(stemURL)
    }

    return [
      AVFormatIDKey: NSNumber(value: kAudioFormatLinearPCM),
      AVSampleRateKey: NSNumber(value: sampleRate),
      AVNumberOfChannelsKey: NSNumber(value: channelCount),
      AVLinearPCMBitDepthKey: NSNumber(value: 32),
      AVLinearPCMIsFloatKey: true,
      AVLinearPCMIsBigEndianKey: false,
      AVLinearPCMIsNonInterleaved: false
    ]
  }

  private static func appendAudioStemIfNeeded(
    _ preparedAudioStem: PreparedAudioStem?,
    writer: AVAssetWriter
  ) async throws -> Int {
    guard let preparedAudioStem else {
      return 0
    }

    let reader = try AVAssetReader(asset: preparedAudioStem.asset)
    let output = AVAssetReaderTrackOutput(
      track: preparedAudioStem.track,
      outputSettings: preparedAudioStem.outputSettings
    )
    output.alwaysCopiesSampleData = false
    guard reader.canAdd(output) else {
      throw ProResImageSequenceExporterError.audioReaderCannotAddOutput(preparedAudioStem.url)
    }
    reader.add(output)

    guard reader.startReading() else {
      throw ProResImageSequenceExporterError.audioReaderStartFailed(
        preparedAudioStem.url,
        underlyingError: reader.error
      )
    }

    var sampleBufferCount = 0
    while true {
      try Task.checkCancellation()
      while !preparedAudioStem.input.isReadyForMoreMediaData {
        if writer.status == .failed || writer.status == .cancelled {
          throw ProResImageSequenceExporterError.audioAppendFailed(
            preparedAudioStem.url,
            writerStatus: writer.status,
            underlyingError: writer.error
          )
        }
        try await Task.sleep(nanoseconds: 1_000_000)
        try Task.checkCancellation()
      }

      guard let sampleBuffer = output.copyNextSampleBuffer() else {
        break
      }

      guard preparedAudioStem.input.append(sampleBuffer) else {
        throw ProResImageSequenceExporterError.audioAppendFailed(
          preparedAudioStem.url,
          writerStatus: writer.status,
          underlyingError: writer.error
        )
      }
      sampleBufferCount += 1
    }

    preparedAudioStem.input.markAsFinished()
    if reader.status == .failed || reader.status == .cancelled {
      throw ProResImageSequenceExporterError.audioReaderFailed(
        preparedAudioStem.url,
        underlyingError: reader.error
      )
    }

    return sampleBufferCount
  }

  private static func loadCGImage(from url: URL) throws -> CGImage {
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else {
      throw ProResImageSequenceExporterError.imageSourceCreationFailed(url)
    }
    guard let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
      throw ProResImageSequenceExporterError.imageDecodeFailed(url)
    }
    return image
  }

  private static func makePixelBuffer(
    from image: CGImage,
    using adaptor: AVAssetWriterInputPixelBufferAdaptor
  ) throws -> CVPixelBuffer {
    guard let pool = adaptor.pixelBufferPool else {
      throw ProResImageSequenceExporterError.pixelBufferPoolUnavailable
    }

    var maybePixelBuffer: CVPixelBuffer?
    let createStatus = CVPixelBufferPoolCreatePixelBuffer(nil, pool, &maybePixelBuffer)
    guard createStatus == kCVReturnSuccess, let pixelBuffer = maybePixelBuffer else {
      throw ProResImageSequenceExporterError.pixelBufferCreationFailed(createStatus)
    }

    CVPixelBufferLockBaseAddress(pixelBuffer, [])
    defer {
      CVPixelBufferUnlockBaseAddress(pixelBuffer, [])
    }

    guard let baseAddress = CVPixelBufferGetBaseAddress(pixelBuffer) else {
      throw ProResImageSequenceExporterError.pixelBufferBaseAddressMissing
    }

    let colorSpace = CGColorSpaceCreateDeviceRGB()
    let bitmapInfo = CGBitmapInfo.byteOrder32Little.rawValue |
      CGImageAlphaInfo.premultipliedFirst.rawValue
    guard let context = CGContext(
      data: baseAddress,
      width: CVPixelBufferGetWidth(pixelBuffer),
      height: CVPixelBufferGetHeight(pixelBuffer),
      bitsPerComponent: 8,
      bytesPerRow: CVPixelBufferGetBytesPerRow(pixelBuffer),
      space: colorSpace,
      bitmapInfo: bitmapInfo
    ) else {
      throw ProResImageSequenceExporterError.bitmapContextCreationFailed
    }

    let bounds = CGRect(
      x: 0,
      y: 0,
      width: CVPixelBufferGetWidth(pixelBuffer),
      height: CVPixelBufferGetHeight(pixelBuffer)
    )
    context.clear(bounds)
    context.draw(image, in: bounds)

    return pixelBuffer
  }
}

enum ProResImageSequenceExporterError: LocalizedError {
  case invalidFrameRate(Double)
  case outputAlreadyExists(URL)
  case frameDirectoryMissing(URL)
  case noFramesFound(URL)
  case imageSourceCreationFailed(URL)
  case imageDecodeFailed(URL)
  case frameDimensionsMismatch(URL, expected: PixelDimensions, actual: PixelDimensions)
  case encoderUnsupported(VideoToolboxProResSupport)
  case outputSettingsUnsupported
  case cannotAddInput
  case startFailed(writerStatus: AVAssetWriter.Status, underlyingError: Error?)
  case pixelBufferPoolUnavailable
  case pixelBufferCreationFailed(CVReturn)
  case pixelBufferBaseAddressMissing
  case bitmapContextCreationFailed
  case appendFailed(URL, writerStatus: AVAssetWriter.Status, underlyingError: Error?)
  case audioStemMissing(URL)
  case audioStemHasNoAudioTrack(URL)
  case audioFormatDescriptionMissing(URL)
  case audioOutputSettingsUnsupported(URL)
  case cannotAddAudioInput(URL)
  case audioReaderCannotAddOutput(URL)
  case audioReaderStartFailed(URL, underlyingError: Error?)
  case audioAppendFailed(URL, writerStatus: AVAssetWriter.Status, underlyingError: Error?)
  case audioReaderFailed(URL, underlyingError: Error?)
  case finishFailed(writerStatus: AVAssetWriter.Status, underlyingError: Error?)

  var errorDescription: String? {
    switch self {
    case let .invalidFrameRate(frameRate):
      return "ProRes export frame rate must be finite and positive, got \(frameRate)."
    case let .outputAlreadyExists(url):
      return "Refusing to overwrite existing movie at \(url.path)."
    case let .frameDirectoryMissing(url):
      return "Frame sequence directory does not exist: \(url.path)."
    case let .noFramesFound(url):
      return "No PNG frames found in \(url.path)."
    case let .imageSourceCreationFailed(url):
      return "Could not open image source for \(url.lastPathComponent)."
    case let .imageDecodeFailed(url):
      return "Could not decode PNG frame \(url.lastPathComponent)."
    case let .frameDimensionsMismatch(url, expected, actual):
      return "Frame \(url.lastPathComponent) is \(actual.width)x\(actual.height), expected \(expected.width)x\(expected.height)."
    case let .encoderUnsupported(support):
      return support.compactSummary
    case .outputSettingsUnsupported:
      return "AVAssetWriter cannot apply the requested ProRes output settings for a QuickTime movie."
    case .cannotAddInput:
      return "AVAssetWriter cannot add the ProRes video input."
    case let .startFailed(status, error):
      return "AVAssetWriter could not start, writer status \(status.rawValue): \(error?.localizedDescription ?? "no underlying error")."
    case .pixelBufferPoolUnavailable:
      return "AVAssetWriter did not provide a CoreVideo pixel buffer pool after writing started."
    case let .pixelBufferCreationFailed(status):
      return "CoreVideo pixel buffer allocation failed with status \(status)."
    case .pixelBufferBaseAddressMissing:
      return "CoreVideo pixel buffer has no writable base address."
    case .bitmapContextCreationFailed:
      return "CoreGraphics could not create a BGRA bitmap context for the frame."
    case let .appendFailed(url, status, error):
      return "Could not append \(url.lastPathComponent), writer status \(status.rawValue): \(error?.localizedDescription ?? "no underlying error")."
    case let .audioStemMissing(url):
      return "Audio stem does not exist: \(url.path)."
    case let .audioStemHasNoAudioTrack(url):
      return "Audio stem has no readable audio track: \(url.path)."
    case let .audioFormatDescriptionMissing(url):
      return "Audio stem format could not be read for \(url.path)."
    case let .audioOutputSettingsUnsupported(url):
      return "AVAssetWriter cannot apply PCM audio settings for \(url.lastPathComponent)."
    case let .cannotAddAudioInput(url):
      return "AVAssetWriter cannot add the audio input for \(url.lastPathComponent)."
    case let .audioReaderCannotAddOutput(url):
      return "AVAssetReader cannot add the audio output for \(url.lastPathComponent)."
    case let .audioReaderStartFailed(url, error):
      return "Audio stem reader could not start for \(url.lastPathComponent): \(error?.localizedDescription ?? "no underlying error")."
    case let .audioAppendFailed(url, status, error):
      return "Could not append audio from \(url.lastPathComponent), writer status \(status.rawValue): \(error?.localizedDescription ?? "no underlying error")."
    case let .audioReaderFailed(url, error):
      return "Audio stem reader failed for \(url.lastPathComponent): \(error?.localizedDescription ?? "no underlying error")."
    case let .finishFailed(status, error):
      return "ProRes movie finalization failed, writer status \(status.rawValue): \(error?.localizedDescription ?? "no underlying error")."
    }
  }
}
