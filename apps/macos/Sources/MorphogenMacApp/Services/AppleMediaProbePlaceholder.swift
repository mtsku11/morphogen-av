import AVFoundation
import CoreMedia
import Foundation

enum AppleMediaProbe {
  static func probeMedia(mediaURL: URL) async throws -> AppleMediaProbeResult {
    let asset = AVURLAsset(url: mediaURL)
    let duration = try await asset.load(.duration)
    let tracks = try await asset.load(.tracks)
    var trackSummaries: [AppleMediaTrackSummary] = []

    for (index, track) in tracks.enumerated() {
      trackSummaries.append(try await summarizeTrack(track, index: index))
    }

    return AppleMediaProbeResult(
      url: mediaURL,
      durationSeconds: finiteSeconds(duration),
      tracks: trackSummaries
    )
  }

  private static func summarizeTrack(
    _ track: AVAssetTrack,
    index: Int
  ) async throws -> AppleMediaTrackSummary {
    switch track.mediaType {
    case .video:
      let naturalSize = try await track.load(.naturalSize)
      let transform = try await track.load(.preferredTransform)
      let nominalFrameRate = try await track.load(.nominalFrameRate)
      let displaySize = naturalSize.applying(transform)
      return AppleMediaTrackSummary(
        index: index,
        mediaType: "video",
        dimensions: PixelDimensions(
          width: Int(abs(displaySize.width).rounded()),
          height: Int(abs(displaySize.height).rounded())
        ),
        nominalFrameRate: nominalFrameRate > 0 ? Double(nominalFrameRate) : nil,
        sampleRate: nil,
        channelCount: nil
      )

    case .audio:
      let formatDescriptions = try await track.load(.formatDescriptions)
      let audioFormat = firstAudioFormatDescription(formatDescriptions)
      return AppleMediaTrackSummary(
        index: index,
        mediaType: "audio",
        dimensions: nil,
        nominalFrameRate: nil,
        sampleRate: audioFormat?.sampleRate,
        channelCount: audioFormat?.channelCount
      )

    default:
      return AppleMediaTrackSummary(
        index: index,
        mediaType: track.mediaType.rawValue,
        dimensions: nil,
        nominalFrameRate: nil,
        sampleRate: nil,
        channelCount: nil
      )
    }
  }

  private static func firstAudioFormatDescription(
    _ formatDescriptions: [CMFormatDescription]
  ) -> AudioFormatSummary? {
    for formatDescription in formatDescriptions {
      guard let streamDescription =
        CMAudioFormatDescriptionGetStreamBasicDescription(formatDescription)?.pointee
      else {
        continue
      }

      return AudioFormatSummary(
        sampleRate: streamDescription.mSampleRate,
        channelCount: Int(streamDescription.mChannelsPerFrame)
      )
    }

    return nil
  }

  private static func finiteSeconds(_ time: CMTime) -> Double? {
    let seconds = time.seconds
    guard seconds.isFinite && seconds >= 0 else {
      return nil
    }
    return seconds
  }
}

struct AppleMediaProbeResult {
  let url: URL
  let durationSeconds: Double?
  let tracks: [AppleMediaTrackSummary]

  var compactSummary: String {
    var parts = ["AVFoundation: \(url.lastPathComponent)"]
    if let durationSeconds {
      parts.append(String(format: "duration %.3fs", durationSeconds))
    }
    parts.append(contentsOf: tracks.prefix(4).map(\.compactSummary))
    return parts.joined(separator: " | ")
  }
}

struct AppleMediaTrackSummary {
  let index: Int
  let mediaType: String
  let dimensions: PixelDimensions?
  let nominalFrameRate: Double?
  let sampleRate: Double?
  let channelCount: Int?

  var compactSummary: String {
    var parts = ["stream \(index): \(mediaType)"]
    if let dimensions {
      parts.append("\(dimensions.width)x\(dimensions.height)")
    }
    if let nominalFrameRate {
      parts.append(String(format: "%.3f fps", nominalFrameRate))
    }
    if let sampleRate {
      parts.append(String(format: "%.0f Hz", sampleRate))
    }
    if let channelCount {
      parts.append("\(channelCount) ch")
    }
    return parts.joined(separator: " ")
  }
}

struct PixelDimensions {
  let width: Int
  let height: Int
}

private struct AudioFormatSummary {
  let sampleRate: Double
  let channelCount: Int
}
