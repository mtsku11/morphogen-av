import AVFoundation
import CoreGraphics
import Foundation
import ImageIO
@testable import MorphogenMacApp
import UniformTypeIdentifiers
import XCTest

final class ProResImageSequenceExporterTests: XCTestCase {
  func testRenderQueueBundleResolverUsesFramesDirectoryAndManifest() throws {
    let rootDirectory = try makeTemporaryDirectory()
    let bundleDirectory = rootDirectory.appendingPathComponent("job-0001", isDirectory: true)
    let frameDirectory = bundleDirectory.appendingPathComponent("frames", isDirectory: true)
    let audioDirectory = bundleDirectory.appendingPathComponent("audio", isDirectory: true)
    try FileManager.default.createDirectory(at: frameDirectory, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: audioDirectory, withIntermediateDirectories: true)
    try writePNGFrame(frameDirectory.appendingPathComponent("frame_000000.png"), seed: 0)
    try Data([0, 1, 2, 3]).write(to: audioDirectory.appendingPathComponent("main.wav"))
    try """
    {
      "job_id": "job-0001",
      "status": "complete",
      "frames": ["frames/frame_000000.png"],
      "audio_stems": ["audio/main.wav"],
      "deterministic": true
    }
    """.write(
      to: bundleDirectory.appendingPathComponent("manifest.json"),
      atomically: true,
      encoding: .utf8
    )

    let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: bundleDirectory)

    XCTAssertEqual(bundle.bundleURL, bundleDirectory)
    XCTAssertEqual(bundle.frameDirectory, frameDirectory)
    XCTAssertEqual(bundle.frameCount, 1)
    XCTAssertEqual(bundle.audioStemCount, 1)
    XCTAssertEqual(bundle.audioStemURLs.map(\.lastPathComponent), ["main.wav"])
    XCTAssertEqual(bundle.status, "complete")
    XCTAssertTrue(bundle.compactSummary.contains("job-0001"))
  }

  func testCollectPNGFrameURLsUsesNaturalSortAndIgnoresNonPNGFiles() throws {
    let directory = try makeTemporaryDirectory()
    try writePNGFrame(directory.appendingPathComponent("frame_10.png"), seed: 10)
    try writePNGFrame(directory.appendingPathComponent("frame_2.png"), seed: 2)
    try "not a frame".write(
      to: directory.appendingPathComponent("notes.txt"),
      atomically: true,
      encoding: .utf8
    )

    let frames = try ProResImageSequenceExporter.collectPNGFrameURLs(in: directory)

    XCTAssertEqual(frames.map(\.lastPathComponent), ["frame_2.png", "frame_10.png"])
  }

  func testExportPNGSequenceWritesTinyProResMovieWhenEncoderIsAvailable() async throws {
    let directory = try makeTemporaryDirectory()
    try writePNGFrame(directory.appendingPathComponent("frame_000000.png"), seed: 0)
    try writePNGFrame(directory.appendingPathComponent("frame_000001.png"), seed: 64)
    let outputURL = directory.appendingPathComponent("test-prores.mov")

    let plan = try VideoToolboxProResExportPlanner.makePlan(
      width: 16,
      height: 16,
      frameRate: 24.0,
      profile: .proRes422HQ
    )
    let support = VideoToolboxProResExportPlanner.probeSupport(for: plan)
    try XCTSkipUnless(support.isSupported, support.compactSummary)

    let result = try await ProResImageSequenceExporter.exportPNGSequence(
      request: ProResImageSequenceExportRequest(
        frameDirectory: directory,
        outputURL: outputURL,
        frameRate: 24.0,
        profile: .proRes422HQ,
        requiresHardwareEncoder: false
      )
    )

    XCTAssertEqual(result.frameCount, 2)
    XCTAssertEqual(result.dimensions.width, 16)
    XCTAssertEqual(result.dimensions.height, 16)

    let attributes = try FileManager.default.attributesOfItem(atPath: outputURL.path)
    let byteCount = (attributes[.size] as? NSNumber)?.intValue ?? 0
    XCTAssertGreaterThan(byteCount, 0)

    let asset = AVURLAsset(url: outputURL)
    let tracks = try await asset.load(.tracks)
    XCTAssertEqual(tracks.filter { $0.mediaType == .video }.count, 1)
    XCTAssertEqual(tracks.filter { $0.mediaType == .audio }.count, 0)
  }

  func testExportRenderQueueBundleMuxesFirstWAVStemWhenEncoderIsAvailable() async throws {
    let rootDirectory = try makeTemporaryDirectory()
    let bundleDirectory = rootDirectory.appendingPathComponent("job-0001", isDirectory: true)
    let frameDirectory = bundleDirectory.appendingPathComponent("frames", isDirectory: true)
    let audioDirectory = bundleDirectory.appendingPathComponent("audio", isDirectory: true)
    try FileManager.default.createDirectory(at: frameDirectory, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: audioDirectory, withIntermediateDirectories: true)
    try writePNGFrame(frameDirectory.appendingPathComponent("frame_000000.png"), seed: 0)
    try writePNGFrame(frameDirectory.appendingPathComponent("frame_000001.png"), seed: 64)
    try writeWAVStem(audioDirectory.appendingPathComponent("main.wav"))
    try """
    {
      "job_id": "job-0001",
      "status": "complete",
      "frames": ["frames/frame_000000.png", "frames/frame_000001.png"],
      "audio_stems": ["audio/main.wav"],
      "deterministic": true
    }
    """.write(
      to: bundleDirectory.appendingPathComponent("manifest.json"),
      atomically: true,
      encoding: .utf8
    )

    let outputURL = rootDirectory.appendingPathComponent("queue-prores-audio.mov")
    let plan = try VideoToolboxProResExportPlanner.makePlan(
      width: 16,
      height: 16,
      frameRate: 24.0,
      profile: .proRes422HQ
    )
    let support = VideoToolboxProResExportPlanner.probeSupport(for: plan)
    try XCTSkipUnless(support.isSupported, support.compactSummary)

    let result = try await ProResImageSequenceExporter.exportRenderQueueBundle(
      request: ProResRenderQueueBundleExportRequest(
        bundleURL: bundleDirectory,
        outputURL: outputURL,
        frameRate: 24.0,
        profile: .proRes422HQ,
        requiresHardwareEncoder: false
      )
    )

    XCTAssertEqual(result.bundle.audioStemCount, 1)
    XCTAssertEqual(result.movie.frameCount, 2)
    XCTAssertEqual(result.movie.audioTrackCount, 1)
    XCTAssertGreaterThan(result.movie.audioSampleBufferCount, 0)

    let asset = AVURLAsset(url: outputURL)
    let tracks = try await asset.load(.tracks)
    XCTAssertEqual(tracks.filter { $0.mediaType == .video }.count, 1)
    XCTAssertEqual(tracks.filter { $0.mediaType == .audio }.count, 1)
  }

  private func makeTemporaryDirectory() throws -> URL {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-swift-tests-\(UUID().uuidString)",
      isDirectory: true
    )
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    return directory
  }

  private func writePNGFrame(_ url: URL, seed: UInt8) throws {
    let width = 16
    let height = 16
    let bytesPerPixel = 4
    let bytesPerRow = width * bytesPerPixel
    var pixels = [UInt8](repeating: 0, count: width * height * bytesPerPixel)

    for y in 0..<height {
      for x in 0..<width {
        let offset = (y * bytesPerRow) + (x * bytesPerPixel)
        pixels[offset] = UInt8((x * 13 + Int(seed)) % 255)
        pixels[offset + 1] = UInt8((y * 17 + Int(seed)) % 255)
        pixels[offset + 2] = UInt8((x * 7 + y * 5 + Int(seed)) % 255)
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

  private func writeWAVStem(_ url: URL) throws {
    let frameCount = 4_800
    let channelCount = 2
    let sampleRate = 48_000
    let bitsPerSample = 16
    let blockAlign = channelCount * bitsPerSample / 8
    let byteRate = sampleRate * blockAlign
    let dataByteCount = frameCount * blockAlign

    var data = Data()
    appendASCII("RIFF", to: &data)
    appendUInt32LE(UInt32(36 + dataByteCount), to: &data)
    appendASCII("WAVE", to: &data)
    appendASCII("fmt ", to: &data)
    appendUInt32LE(16, to: &data)
    appendUInt16LE(1, to: &data)
    appendUInt16LE(UInt16(channelCount), to: &data)
    appendUInt32LE(UInt32(sampleRate), to: &data)
    appendUInt32LE(UInt32(byteRate), to: &data)
    appendUInt16LE(UInt16(blockAlign), to: &data)
    appendUInt16LE(UInt16(bitsPerSample), to: &data)
    appendASCII("data", to: &data)
    appendUInt32LE(UInt32(dataByteCount), to: &data)

    for frame in 0..<frameCount {
      for channel in 0..<channelCount {
        let phase = Float((frame + channel * 17) % 96) / 96.0
        let value = Int16((phase * 2.0 - 1.0) * 4_000.0)
        appendInt16LE(value, to: &data)
      }
    }

    try data.write(to: url)
  }

  private func appendASCII(_ string: String, to data: inout Data) {
    data.append(contentsOf: string.utf8)
  }

  private func appendUInt16LE(_ value: UInt16, to data: inout Data) {
    data.append(contentsOf: withUnsafeBytes(of: value.littleEndian, Array.init))
  }

  private func appendUInt32LE(_ value: UInt32, to data: inout Data) {
    data.append(contentsOf: withUnsafeBytes(of: value.littleEndian, Array.init))
  }

  private func appendInt16LE(_ value: Int16, to data: inout Data) {
    data.append(contentsOf: withUnsafeBytes(of: value.littleEndian, Array.init))
  }
}
