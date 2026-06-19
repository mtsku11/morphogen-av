import Dispatch
import Foundation

enum RustBridgePlaceholder {
  static let intendedBridgeOptions = [
    "C ABI/staticlib for a narrow stable engine boundary",
    "UniFFI once the Rust API shape settles",
    "Swift calling the local CLI during early development",
    "Later direct engine binding for render jobs and preview"
  ]

  static func currentStatus() -> String {
    "Rust is not directly linked into the SwiftUI shell yet. The dev bridge invokes morphogen-cli."
  }

  static func defaultRenderOutputURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("morphogen-test.png")
  }

  static func defaultRenderQueueURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent("morphogen-render-queue.json")
  }

  static func defaultRenderQueueOutputRootURL() -> URL {
    FileManager.default.temporaryDirectory.appendingPathComponent(
      "morphogen-render-output",
      isDirectory: true
    )
  }

  static func defaultQueuedTestRenderBundleURL() -> URL {
    defaultRenderQueueOutputRootURL().appendingPathComponent("job-0001", isDirectory: true)
  }

  static func runRenderTest(outputURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "morphogen-cli",
      "--",
      "render-test",
      outputURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func runFrameSequenceRender(
    request: FrameSequenceRenderCommandRequest
  ) throws -> FrameSequenceRenderCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = try renderFrameSequenceArguments(request: request)
    _ = try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
    return FrameSequenceRenderCommandResult(
      modulatorDirectoryURL: request.modulatorDirectoryURL,
      carrierDirectoryURL: request.carrierDirectoryURL,
      outputDirectoryURL: request.outputDirectoryURL,
      flowCacheDirectoryURL: request.flowCacheDirectoryURL
    )
  }

  static func renderFrameSequenceArguments(
    request: FrameSequenceRenderCommandRequest
  ) throws -> [String] {
    guard request.amount.isFinite else {
      throw RustBridgeError.invalidFrameSequenceRequest("amount must be finite")
    }
    guard request.frameRate.isFinite && request.frameRate > 0 else {
      throw RustBridgeError.invalidFrameSequenceRequest("frame rate must be positive and finite")
    }
    if let maxFrames = request.maxFrames, maxFrames <= 0 {
      throw RustBridgeError.invalidFrameSequenceRequest("max frame count must be greater than zero")
    }

    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "morphogen-cli",
      "--",
      "render-frame-sequence",
      request.modulatorDirectoryURL.path,
      request.carrierDirectoryURL.path,
      request.outputDirectoryURL.path,
      "--amount",
      cliNumber(request.amount),
      "--frame-rate",
      cliNumber(request.frameRate)
    ]

    if let flowCacheDirectoryURL = request.flowCacheDirectoryURL {
      arguments.append("--flow-cache-dir")
      arguments.append(flowCacheDirectoryURL.path)
    }

    if let maxFrames = request.maxFrames {
      arguments.append("--max-frames")
      arguments.append(String(maxFrames))
    }

    return arguments
  }

  static func runFreshQueuedTestRender(projectURL: URL?) throws -> QueuedRenderCommandResult {
    let queueURL = defaultRenderQueueURL()
    let outputRootURL = defaultRenderQueueOutputRootURL()
    let bundleURL = defaultQueuedTestRenderBundleURL()
    let initResult = try queueInit(queueURL: queueURL)
    let addResult = try queueAddTest(queueURL: queueURL, projectURL: projectURL)
    let runResult = try queueRunTest(queueURL: queueURL, outputRootURL: outputRootURL)

    return QueuedRenderCommandResult(
      queueURL: queueURL,
      outputRootURL: outputRootURL,
      bundleURL: bundleURL,
      commandSummary: [
        initResult.summary,
        addResult.summary,
        runResult.summary
      ].joined(separator: " ")
    )
  }

  static func queueInit(queueURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "morphogen-cli",
      "--",
      "queue-init",
      queueURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func queueAddTest(queueURL: URL, projectURL: URL?) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    var arguments = [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "morphogen-cli",
      "--",
      "queue-add-test",
      queueURL.path
    ]
    if let projectURL {
      arguments.append("--project-path")
      arguments.append(projectURL.path)
    }
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func queueRunTest(queueURL: URL, outputRootURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "morphogen-cli",
      "--",
      "queue-run-test",
      queueURL.path,
      outputRootURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func probeMedia(mediaURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "morphogen-cli",
      "--",
      "probe",
      mediaURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func createExampleProject(outputURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "morphogen-cli",
      "--",
      "init-example",
      outputURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  static func inspectProject(projectURL: URL) throws -> RustCommandResult {
    let repoRoot = try resolveRepoRoot()
    let arguments = [
      "cargo",
      "run",
      "--quiet",
      "-p",
      "morphogen-cli",
      "--",
      "inspect-project",
      projectURL.path
    ]
    return try runCommand(arguments: arguments, currentDirectoryURL: repoRoot)
  }

  private static func resolveRepoRoot() throws -> URL {
    var candidate = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)

    for _ in 0..<8 {
      if FileManager.default.fileExists(atPath: candidate.appendingPathComponent("Cargo.toml").path),
         FileManager.default.fileExists(atPath: candidate.appendingPathComponent("Package.swift").path) {
        return candidate
      }

      let parent = candidate.deletingLastPathComponent()
      if parent.path == candidate.path {
        break
      }
      candidate = parent
    }

    throw RustBridgeError.repoRootNotFound
  }

  private static func cliNumber(_ value: Double) -> String {
    String(format: "%.6g", locale: Locale(identifier: "en_US_POSIX"), value)
  }

  private static func runCommand(
    arguments: [String],
    currentDirectoryURL: URL
  ) throws -> RustCommandResult {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
    process.arguments = arguments
    process.currentDirectoryURL = currentDirectoryURL

    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr

    let stdoutDrain = PipeDrain(pipe: stdout)
    let stderrDrain = PipeDrain(pipe: stderr)
    let outputGroup = DispatchGroup()
    let outputQueue = DispatchQueue(
      label: "dev.morphogen-av.rust-bridge-output",
      qos: .userInitiated,
      attributes: .concurrent
    )

    try process.run()
    stdoutDrain.start(on: outputQueue, group: outputGroup)
    stderrDrain.start(on: outputQueue, group: outputGroup)
    process.waitUntilExit()
    outputGroup.wait()

    let stdoutText = stdoutDrain.text()
    let stderrText = stderrDrain.text()
    let result = RustCommandResult(
      command: arguments.joined(separator: " "),
      exitCode: process.terminationStatus,
      stdout: stdoutText,
      stderr: stderrText
    )

    guard process.terminationStatus == 0 else {
      throw RustBridgeError.commandFailed(result)
    }

    return result
  }
}

struct FrameSequenceRenderCommandRequest {
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputDirectoryURL: URL
  let amount: Double
  let maxFrames: Int?
  let frameRate: Double
  let flowCacheDirectoryURL: URL?
}

struct FrameSequenceRenderCommandResult {
  let modulatorDirectoryURL: URL
  let carrierDirectoryURL: URL
  let outputDirectoryURL: URL
  let flowCacheDirectoryURL: URL?
}

struct QueuedRenderCommandResult {
  let queueURL: URL
  let outputRootURL: URL
  let bundleURL: URL
  let commandSummary: String
}

private final class PipeDrain: @unchecked Sendable {
  private let handle: FileHandle
  private let lock = NSLock()
  private var output = Data()

  init(pipe: Pipe) {
    self.handle = pipe.fileHandleForReading
  }

  func start(on queue: DispatchQueue, group: DispatchGroup) {
    group.enter()
    queue.async {
      let data = self.handle.readDataToEndOfFile()
      self.lock.lock()
      self.output = data
      self.lock.unlock()
      group.leave()
    }
  }

  func text() -> String {
    lock.lock()
    let data = output
    lock.unlock()
    return String(data: data, encoding: .utf8) ?? ""
  }
}

struct RustCommandResult {
  let command: String
  let exitCode: Int32
  let stdout: String
  let stderr: String

  var summary: String {
    let combined = [stdout, stderr]
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }
      .joined(separator: " ")

    if combined.isEmpty {
      return "Command completed."
    }

    return combined
  }
}

enum RustBridgeError: LocalizedError {
  case repoRootNotFound
  case commandFailed(RustCommandResult)
  case invalidFrameSequenceRequest(String)

  var errorDescription: String? {
    switch self {
    case .repoRootNotFound:
      return "Could not find the repository root containing Cargo.toml and Package.swift."
    case .commandFailed(let result):
      let detail = result.summary
      if detail.isEmpty {
        return "\(result.command) exited with status \(result.exitCode)."
      }
      return "\(result.command) exited with status \(result.exitCode): \(detail)"
    case .invalidFrameSequenceRequest(let message):
      return "Invalid frame-sequence render request: \(message)."
    }
  }
}
