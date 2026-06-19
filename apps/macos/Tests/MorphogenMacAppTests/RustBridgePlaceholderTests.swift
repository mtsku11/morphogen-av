import Foundation
@testable import MorphogenMacApp
import XCTest

final class RustBridgePlaceholderTests: XCTestCase {
  func testQueuedFrameSequenceArgumentsIncludeSelectedInputsAndOptions() throws {
    let request = FrameSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/frame-sequence-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: 12.5,
      maxFrames: 48,
      frameRate: 23.976,
      writesFlowCache: false,
      projectURL: URL(fileURLWithPath: "/tmp/project.morphogen.json")
    )

    let arguments = try RustBridgePlaceholder.queueAddFrameSequenceArguments(request: request)

    XCTAssertEqual(arguments.prefix(7), ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "queue-add-frame-sequence"])
    XCTAssertTrue(arguments.contains("/tmp/frame-sequence-queue.json"))
    XCTAssertTrue(arguments.contains("/tmp/source-a-frames"))
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/output-root"))
    XCTAssertTrue(arguments.contains("--amount"))
    XCTAssertTrue(arguments.contains("12.5"))
    XCTAssertTrue(arguments.contains("--frame-rate"))
    XCTAssertTrue(arguments.contains("23.976"))
    XCTAssertTrue(arguments.contains("--no-flow-cache"))
    XCTAssertTrue(arguments.contains("--max-frames"))
    XCTAssertTrue(arguments.contains("48"))
    XCTAssertTrue(arguments.contains("--project-path"))
    XCTAssertTrue(arguments.contains("/tmp/project.morphogen.json"))
  }

  func testQueuedFrameSequenceArgumentsRejectInvalidValues() {
    let request = FrameSequenceRenderQueueCommandRequest(
      queueURL: URL(fileURLWithPath: "/tmp/frame-sequence-queue.json"),
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputRootDirectoryURL: URL(fileURLWithPath: "/tmp/output-root", isDirectory: true),
      amount: .nan,
      maxFrames: 48,
      frameRate: 24.0,
      writesFlowCache: true,
      projectURL: nil
    )

    XCTAssertThrowsError(try RustBridgePlaceholder.queueAddFrameSequenceArguments(request: request))
  }

  func testMediaProxyArgumentsIncludeFrameAndAudioExtraction() throws {
    let request = MediaProxyExtractionCommandRequest(
      sourceURL: URL(fileURLWithPath: "/tmp/source.mov"),
      proxyDirectoryURL: URL(fileURLWithPath: "/tmp/proxy/source-a", isDirectory: true),
      framesPerSecond: 12.0,
      maxFrames: 120,
      sampleRate: 48_000
    )

    let arguments = try RustBridgePlaceholder.mediaProxyExtractionArguments(request: request)

    XCTAssertEqual(arguments.frameExtraction.prefix(7), ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "extract-frames"])
    XCTAssertTrue(arguments.frameExtraction.contains("/tmp/proxy/source-a/frames"))
    XCTAssertTrue(arguments.frameExtraction.contains("--fps"))
    XCTAssertTrue(arguments.frameExtraction.contains("12"))
    XCTAssertTrue(arguments.audioExtraction.contains("extract-audio"))
    XCTAssertTrue(arguments.audioExtraction.contains("/tmp/proxy/source-a/audio.wav"))
    XCTAssertTrue(arguments.audioExtraction.contains("--sample-rate"))
    XCTAssertTrue(arguments.audioExtraction.contains("48000"))
  }

  func testMediaProxyArgumentsGenerateRMSAndSTFTAnalysisCaches() throws {
    let request = MediaProxyExtractionCommandRequest(
      sourceURL: URL(fileURLWithPath: "/tmp/source.mov"),
      proxyDirectoryURL: URL(fileURLWithPath: "/tmp/proxy/source-a", isDirectory: true),
      framesPerSecond: 12.0,
      maxFrames: nil,
      sampleRate: 48_000
    )

    let arguments = try RustBridgePlaceholder.mediaProxyExtractionArguments(request: request)

    XCTAssertTrue(arguments.rmsCacheGeneration.contains("cache-rms"))
    XCTAssertTrue(arguments.rmsCacheGeneration.contains("/tmp/proxy/source-a/audio.wav"))
    XCTAssertTrue(arguments.rmsCacheGeneration.contains("/tmp/proxy/source-a/analysis/rms.json"))
    XCTAssertTrue(arguments.rmsCacheGeneration.contains("--window-size"))

    XCTAssertTrue(arguments.stftCacheGeneration.contains("cache-stft"))
    XCTAssertTrue(arguments.stftCacheGeneration.contains("/tmp/proxy/source-a/audio.wav"))
    XCTAssertTrue(arguments.stftCacheGeneration.contains("/tmp/proxy/source-a/analysis/stft.json"))
    XCTAssertTrue(arguments.stftCacheGeneration.contains("--fft-size"))
  }
}
