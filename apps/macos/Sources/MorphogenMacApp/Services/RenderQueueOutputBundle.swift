import Foundation

struct RenderQueueOutputBundle {
  let bundleURL: URL
  let frameDirectory: URL
  let frameCount: Int
  let audioStemURLs: [URL]
  let status: String?
  let timing: RenderQueueTimingMetadata?

  var audioStemCount: Int {
    audioStemURLs.count
  }

  var compactSummary: String {
    let statusText = status.map { ", status \($0)" } ?? ""
    let timingText = timing.map { ", \($0.compactSummary)" } ?? ""
    return "\(bundleURL.lastPathComponent): \(frameCount) PNG frame(s), \(audioStemCount) audio stem(s)\(timingText)\(statusText)"
  }
}

struct RenderQueueTimingMetadata: Equatable {
  let frameRate: Double
  let frameCount: Int?
  let startSeconds: Double?
  let durationSeconds: Double?
  let sampleRate: Int?
  let audioSampleCount: Int?

  var compactSummary: String {
    var parts = [String(format: "%.3f fps", frameRate)]
    if let durationSeconds {
      parts.append(String(format: "%.3fs", durationSeconds))
    }
    if let sampleRate {
      parts.append("\(sampleRate) Hz")
    }
    return parts.joined(separator: ", ")
  }
}

struct ProResRenderQueueBundleExportRequest {
  let bundleURL: URL
  let outputURL: URL
  let frameRate: Double
  let profile: ProResExportProfile
  let requiresHardwareEncoder: Bool
}

struct ProResRenderQueueBundleExportResult {
  let bundle: RenderQueueOutputBundle
  let movie: ProResImageSequenceExportResult

  var compactSummary: String {
    "\(movie.compactSummary) from render bundle \(bundle.bundleURL.lastPathComponent)"
  }
}

enum RenderQueueOutputBundleResolver {
  static func inspect(bundleURL: URL) throws -> RenderQueueOutputBundle {
    var isDirectory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: bundleURL.path, isDirectory: &isDirectory),
          isDirectory.boolValue
    else {
      throw RenderQueueOutputBundleError.bundleDirectoryMissing(bundleURL)
    }

    let frameDirectory = bundleURL.appendingPathComponent("frames", isDirectory: true)
    let frameURLs = try ProResImageSequenceExporter.collectPNGFrameURLs(in: frameDirectory)
    guard !frameURLs.isEmpty else {
      throw RenderQueueOutputBundleError.noFramesFound(frameDirectory)
    }

    let manifest = try readManifestInfoIfPresent(bundleURL: bundleURL)
    return RenderQueueOutputBundle(
      bundleURL: bundleURL,
      frameDirectory: frameDirectory,
      frameCount: frameURLs.count,
      audioStemURLs: try audioStemURLs(
        for: manifest?.audioStemPaths,
        bundleURL: bundleURL
      ),
      status: manifest?.status,
      timing: manifest?.timing
    )
  }

  private static func readManifestInfoIfPresent(bundleURL: URL) throws -> RenderQueueManifestInfo? {
    let manifestURL = bundleURL.appendingPathComponent("manifest.json")
    guard FileManager.default.fileExists(atPath: manifestURL.path) else {
      return nil
    }

    let data = try Data(contentsOf: manifestURL)
    let value = try JSONSerialization.jsonObject(with: data)
    guard let object = value as? [String: Any] else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }

    let status = object["status"] as? String
    let audioStemPaths = try parseAudioStemPaths(
      object["audio_stems"],
      manifestURL: manifestURL
    )
    let timing = try parseTimingMetadata(object["timing"], manifestURL: manifestURL)
    return RenderQueueManifestInfo(
      status: status,
      audioStemPaths: audioStemPaths,
      timing: timing
    )
  }

  private static func parseTimingMetadata(
    _ value: Any?,
    manifestURL: URL
  ) throws -> RenderQueueTimingMetadata? {
    guard let value else {
      return nil
    }
    guard let object = value as? [String: Any] else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }

    let frameRate = try parsePositiveFiniteDouble(
      object["frame_rate"],
      manifestURL: manifestURL
    )
    return RenderQueueTimingMetadata(
      frameRate: frameRate,
      frameCount: try parseOptionalPositiveInt(object["frame_count"], manifestURL: manifestURL),
      startSeconds: try parseOptionalNonNegativeFiniteDouble(
        object["start_seconds"],
        manifestURL: manifestURL
      ),
      durationSeconds: try parseOptionalNonNegativeFiniteDouble(
        object["duration_seconds"],
        manifestURL: manifestURL
      ),
      sampleRate: try parseOptionalPositiveInt(object["sample_rate"], manifestURL: manifestURL),
      audioSampleCount: try parseOptionalNonNegativeInt(
        object["audio_sample_count"],
        manifestURL: manifestURL
      )
    )
  }

  private static func parsePositiveFiniteDouble(
    _ value: Any?,
    manifestURL: URL
  ) throws -> Double {
    guard let number = value as? NSNumber else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }
    let doubleValue = number.doubleValue
    guard doubleValue.isFinite && doubleValue > 0 else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }
    return doubleValue
  }

  private static func parseOptionalNonNegativeFiniteDouble(
    _ value: Any?,
    manifestURL: URL
  ) throws -> Double? {
    guard let value else {
      return nil
    }
    guard let number = value as? NSNumber else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }
    let doubleValue = number.doubleValue
    guard doubleValue.isFinite && doubleValue >= 0 else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }
    return doubleValue
  }

  private static func parseOptionalPositiveInt(
    _ value: Any?,
    manifestURL: URL
  ) throws -> Int? {
    guard let parsed = try parseOptionalNonNegativeInt(value, manifestURL: manifestURL) else {
      return nil
    }
    guard parsed > 0 else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }
    return parsed
  }

  private static func parseOptionalNonNegativeInt(
    _ value: Any?,
    manifestURL: URL
  ) throws -> Int? {
    guard let value else {
      return nil
    }
    guard let number = value as? NSNumber else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }
    let doubleValue = number.doubleValue
    let intValue = number.intValue
    guard doubleValue.isFinite,
          doubleValue >= 0,
          Double(intValue) == doubleValue
    else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }
    return intValue
  }

  private static func parseAudioStemPaths(
    _ value: Any?,
    manifestURL: URL
  ) throws -> [String] {
    guard let value else {
      return []
    }
    guard let stems = value as? [Any] else {
      throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
    }

    return try stems.map { stem in
      guard let path = stem as? String,
            !path.isEmpty,
            !path.hasPrefix("/"),
            !path.split(separator: "/").contains("..")
      else {
        throw RenderQueueOutputBundleError.malformedManifest(manifestURL)
      }
      return path
    }
  }

  private static func audioStemURLs(
    for manifestPaths: [String]?,
    bundleURL: URL
  ) throws -> [URL] {
    guard let manifestPaths else {
      return try collectAudioStemURLs(in: bundleURL)
    }

    return try manifestPaths.map { path in
      let url = bundleURL.appendingPathComponent(path)
      guard FileManager.default.fileExists(atPath: url.path) else {
        throw RenderQueueOutputBundleError.audioStemMissing(url)
      }
      return url
    }
  }

  private static func collectAudioStemURLs(in bundleURL: URL) throws -> [URL] {
    let audioDirectory = bundleURL.appendingPathComponent("audio", isDirectory: true)
    guard FileManager.default.fileExists(atPath: audioDirectory.path) else {
      return []
    }

    let urls = try FileManager.default.contentsOfDirectory(
      at: audioDirectory,
      includingPropertiesForKeys: [.isRegularFileKey],
      options: [.skipsHiddenFiles]
    )

    return urls.filter { url in
      guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey]) else {
        return false
      }
      return values.isRegularFile == true && url.pathExtension.lowercased() == "wav"
    }
    .sorted { lhs, rhs in
      lhs.lastPathComponent.localizedStandardCompare(rhs.lastPathComponent) == .orderedAscending
    }
  }
}

extension ProResImageSequenceExporter {
  static func exportRenderQueueBundle(
    request: ProResRenderQueueBundleExportRequest
  ) async throws -> ProResRenderQueueBundleExportResult {
    let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: request.bundleURL)
    let movie = try await exportPNGSequence(
      request: ProResImageSequenceExportRequest(
        frameDirectory: bundle.frameDirectory,
        outputURL: request.outputURL,
        frameRate: request.frameRate,
        profile: request.profile,
        requiresHardwareEncoder: request.requiresHardwareEncoder,
        audioStemURL: bundle.audioStemURLs.first
      )
    )

    return ProResRenderQueueBundleExportResult(bundle: bundle, movie: movie)
  }
}

private struct RenderQueueManifestInfo {
  let status: String?
  let audioStemPaths: [String]
  let timing: RenderQueueTimingMetadata?
}

enum RenderQueueOutputBundleError: LocalizedError {
  case bundleDirectoryMissing(URL)
  case noFramesFound(URL)
  case malformedManifest(URL)
  case audioStemMissing(URL)

  var errorDescription: String? {
    switch self {
    case .bundleDirectoryMissing(let url):
      return "Render queue output bundle does not exist: \(url.path)."
    case .noFramesFound(let url):
      return "Render queue output has no PNG frames in \(url.path)."
    case .malformedManifest(let url):
      return "Render queue manifest is not a JSON object: \(url.path)."
    case .audioStemMissing(let url):
      return "Render queue manifest references a missing audio stem: \(url.path)."
    }
  }
}
