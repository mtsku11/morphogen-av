import Foundation
@testable import MorphogenMacApp
import XCTest

final class RustBridgePlaceholderTests: XCTestCase {
  func testRenderFrameSequenceArgumentsIncludeSelectedInputsAndOptions() throws {
    let request = FrameSequenceRenderCommandRequest(
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputDirectoryURL: URL(fileURLWithPath: "/tmp/output-frames", isDirectory: true),
      amount: 12.5,
      maxFrames: 48,
      frameRate: 23.976,
      flowCacheDirectoryURL: URL(fileURLWithPath: "/tmp/flow-cache", isDirectory: true)
    )

    let arguments = try RustBridgePlaceholder.renderFrameSequenceArguments(request: request)

    XCTAssertEqual(arguments.prefix(7), ["cargo", "run", "--quiet", "-p", "morphogen-cli", "--", "render-frame-sequence"])
    XCTAssertTrue(arguments.contains("/tmp/source-a-frames"))
    XCTAssertTrue(arguments.contains("/tmp/source-b-frames"))
    XCTAssertTrue(arguments.contains("/tmp/output-frames"))
    XCTAssertTrue(arguments.contains("--amount"))
    XCTAssertTrue(arguments.contains("12.5"))
    XCTAssertTrue(arguments.contains("--frame-rate"))
    XCTAssertTrue(arguments.contains("23.976"))
    XCTAssertTrue(arguments.contains("--flow-cache-dir"))
    XCTAssertTrue(arguments.contains("/tmp/flow-cache"))
    XCTAssertTrue(arguments.contains("--max-frames"))
    XCTAssertTrue(arguments.contains("48"))
  }

  func testRenderFrameSequenceArgumentsRejectInvalidValues() {
    let request = FrameSequenceRenderCommandRequest(
      modulatorDirectoryURL: URL(fileURLWithPath: "/tmp/source-a-frames", isDirectory: true),
      carrierDirectoryURL: URL(fileURLWithPath: "/tmp/source-b-frames", isDirectory: true),
      outputDirectoryURL: URL(fileURLWithPath: "/tmp/output-frames", isDirectory: true),
      amount: .nan,
      maxFrames: 48,
      frameRate: 24.0,
      flowCacheDirectoryURL: nil
    )

    XCTAssertThrowsError(try RustBridgePlaceholder.renderFrameSequenceArguments(request: request))
  }
}
