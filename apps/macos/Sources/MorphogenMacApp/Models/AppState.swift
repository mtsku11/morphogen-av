import AppKit
import Combine
import Dispatch
import Foundation
import Metal

final class AppState: ObservableObject {
  @Published var sourceAPath = "No modulator selected"
  @Published var sourceBPath = "No carrier selected"
  @Published var sourceAProbeSummary = "Probe not run"
  @Published var sourceBProbeSummary = "Probe not run"
  @Published var sourceAPreviewSummary = "Preview not run"
  @Published var sourceBPreviewSummary = "Preview not run"
  @Published var sourceAPreviewImage: NSImage?
  @Published var sourceBPreviewImage: NSImage?
  @Published var renderQuality: RenderQualityOption = .highQualityOffline
  @Published var exportFormat: ExportFormatOption = .pngSequence
  @Published var projectPath = "No project loaded"
  @Published var projectSummary = "Project schema idle"
  @Published var renderQueueSummary = "No queue output bundle yet"
  @Published var proResPlanSummary = VideoToolboxProResExportPlanner.defaultPlanSummary()
  @Published var proResExportSummary = "No ProRes movie exported"
  @Published var previewProbeSummary = "No preview frame decoded"
  @Published var statusMessage = "Analysis cache idle. Offline queue empty."

  private var sourceAURL: URL?
  private var sourceBURL: URL?
  private var projectURL: URL?
  private var lastRenderQueueBundleURL: URL?

  func setSource(_ role: SourceRole, url: URL) {
    switch role {
    case .modulator:
      sourceAURL = url
      sourceAPath = url.path
      sourceAProbeSummary = "Probe not run"
      sourceAPreviewSummary = "Preview not run"
      sourceAPreviewImage = nil
    case .carrier:
      sourceBURL = url
      sourceBPath = url.path
      sourceBProbeSummary = "Probe not run"
      sourceBPreviewSummary = "Preview not run"
      sourceBPreviewImage = nil
    }

    statusMessage = "\(role.rawValue) source selected: \(url.lastPathComponent)"
  }

  func probeSelectedSources() {
    let selectedSources = [
      (SourceRole.modulator, sourceAURL),
      (SourceRole.carrier, sourceBURL)
    ].compactMap { role, url -> (SourceRole, URL)? in
      guard let url else {
        return nil
      }
      return (role, url)
    }

    guard !selectedSources.isEmpty else {
      statusMessage = "Select Source A or Source B before probing media."
      return
    }

    statusMessage = "Probing selected media through morphogen-cli..."

    Task {
      var results: [(SourceRole, String)] = []
      for (role, url) in selectedSources {
        let summary: String
        do {
          let appleProbe = try await AppleMediaProbe.probeMedia(mediaURL: url)
          summary = appleProbe.compactSummary
        } catch {
          summary = Self.fallbackProbeSummary(mediaURL: url, appleError: error)
        }
        results.append((role, summary))
      }

      let probeResults = results
      await MainActor.run {
        for result in probeResults {
          switch result.0 {
          case .modulator:
            self.sourceAProbeSummary = result.1
          case .carrier:
            self.sourceBProbeSummary = result.1
          }
        }

        self.statusMessage = "Media probe complete."
      }
    }
  }

  func probePreviewFrames() {
    let selectedSources = [
      (SourceRole.modulator, sourceAURL),
      (SourceRole.carrier, sourceBURL)
    ].compactMap { role, url -> (SourceRole, URL)? in
      guard let url else {
        return nil
      }
      return (role, url)
    }

    guard !selectedSources.isEmpty else {
      statusMessage = "Select Source A or Source B before probing preview frames."
      return
    }

    guard let device = MTLCreateSystemDefaultDevice() else {
      previewProbeSummary = "No Metal device available for source preview."
      statusMessage = "Preview probe failed: no Metal device available."
      return
    }

    statusMessage = "Decoding first source frame into a Metal texture..."

    Task {
      var results: [(SourceRole, String, NSImage?)] = []
      for (role, url) in selectedSources {
        let summary: String
        let previewImage: NSImage?
        do {
          let result = try await SourcePreviewFrameProbe.decodeFirstVideoFrame(
            mediaURL: url,
            device: device
          )
          summary = result.compactSummary
          previewImage = result.previewImage
        } catch {
          summary = "Preview failed: \(error.localizedDescription)"
          previewImage = nil
        }
        results.append((role, summary, previewImage))
      }

      let previewResults = results
      await MainActor.run {
        for result in previewResults {
          switch result.0 {
          case .modulator:
            self.sourceAPreviewSummary = result.1
            self.sourceAPreviewImage = result.2
          case .carrier:
            self.sourceBPreviewSummary = result.1
            self.sourceBPreviewImage = result.2
          }
        }

        self.previewProbeSummary = previewResults
          .map { "\($0.0.rawValue): \($0.1)" }
          .joined(separator: " | ")
        self.statusMessage = "Preview frame probe complete."
      }
    }
  }

  func runCpuReferenceRender() {
    let outputURL = RustBridgePlaceholder.defaultRenderOutputURL()
    statusMessage = "Running CPU reference render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let commandResult = try RustBridgePlaceholder.runRenderTest(outputURL: outputURL)
        DispatchQueue.main.async {
          self.statusMessage = "Rendered \(outputURL.path). \(commandResult.summary)"
        }
      } catch {
        DispatchQueue.main.async {
          self.statusMessage = "Render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runQueuedTestRender() {
    let projectURL = self.projectURL
    statusMessage = "Running deterministic queued test render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let commandResult = try RustBridgePlaceholder.runFreshQueuedTestRender(projectURL: projectURL)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: commandResult.bundleURL)
        DispatchQueue.main.async {
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.statusMessage = "Queued render output ready: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.renderQueueSummary = "Queued render failed: \(error.localizedDescription)"
          self.statusMessage = "Queued render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func checkProResExportPlan() {
    statusMessage = "Checking ProRes export support through VideoToolbox..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let plan = try VideoToolboxProResExportPlanner.makePlan(
          width: 1920,
          height: 1080,
          frameRate: 24.0,
          profile: .proRes422HQ
        )
        let support = VideoToolboxProResExportPlanner.probeSupport(for: plan)
        let proResEncoderCount = try VideoToolboxProResExportPlanner.availableProResEncoderSummaries().count
        let encoderSummary = proResEncoderCount == 1
          ? "1 ProRes encoder listed"
          : "\(proResEncoderCount) ProRes encoders listed"

        DispatchQueue.main.async {
          self.proResPlanSummary = "\(plan.compactSummary) | \(support.compactSummary) | \(encoderSummary)"
          self.statusMessage = support.isSupported
            ? "ProRes VideoToolbox support check complete."
            : "ProRes support check returned status \(support.status)."
        }
      } catch {
        DispatchQueue.main.async {
          self.proResPlanSummary = "ProRes plan unavailable: \(error.localizedDescription)"
          self.statusMessage = "ProRes check failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func exportProResMovie() {
    guard let frameDirectory = ImageSequenceExportPanel.chooseFrameDirectory() else {
      statusMessage = "ProRes export cancelled."
      return
    }
    guard let outputURL = ImageSequenceExportPanel.chooseMovieSaveLocation() else {
      statusMessage = "ProRes export cancelled."
      return
    }

    statusMessage = "Exporting PNG sequence to ProRes MOV..."

    DispatchQueue.global(qos: .userInitiated).async {
      Task {
        do {
          let result = try await ProResImageSequenceExporter.exportPNGSequence(
            request: ProResImageSequenceExportRequest(
              frameDirectory: frameDirectory,
              outputURL: outputURL,
              frameRate: 24.0,
              profile: .proRes422HQ,
              requiresHardwareEncoder: false
            )
          )
          await MainActor.run {
            self.proResExportSummary = result.compactSummary
            self.statusMessage = "ProRes export complete: \(outputURL.path)"
          }
        } catch {
          await MainActor.run {
            self.proResExportSummary = "ProRes export failed: \(error.localizedDescription)"
            self.statusMessage = "ProRes export failed: \(error.localizedDescription)"
          }
        }
      }
    }
  }

  func exportRenderQueueProResMovie() {
    let defaultBundleURL = RustBridgePlaceholder.defaultQueuedTestRenderBundleURL()
    let bundleURL = lastRenderQueueBundleURL ?? defaultBundleURL

    guard FileManager.default.fileExists(atPath: bundleURL.path) else {
      statusMessage = "Run queued test render before exporting its ProRes movie."
      renderQueueSummary = "No queue output bundle found at \(defaultBundleURL.path)"
      return
    }

    let defaultMovieName = "\(bundleURL.lastPathComponent)-prores.mov"
    guard let outputURL = ImageSequenceExportPanel.chooseMovieSaveLocation(defaultName: defaultMovieName) else {
      statusMessage = "ProRes export cancelled."
      return
    }

    statusMessage = "Exporting render queue image sequence to ProRes MOV..."

    DispatchQueue.global(qos: .userInitiated).async {
      Task {
        do {
          let result = try await ProResImageSequenceExporter.exportRenderQueueBundle(
            request: ProResRenderQueueBundleExportRequest(
              bundleURL: bundleURL,
              outputURL: outputURL,
              frameRate: 24.0,
              profile: .proRes422HQ,
              requiresHardwareEncoder: false
            )
          )
          await MainActor.run {
            self.lastRenderQueueBundleURL = result.bundle.bundleURL
            self.renderQueueSummary = "\(result.bundle.compactSummary) at \(result.bundle.bundleURL.path)"
            self.proResExportSummary = result.compactSummary
            self.statusMessage = "Render queue ProRes export complete: \(outputURL.path)"
          }
        } catch {
          await MainActor.run {
            self.proResExportSummary = "Render queue ProRes export failed: \(error.localizedDescription)"
            self.statusMessage = "Render queue ProRes export failed: \(error.localizedDescription)"
          }
        }
      }
    }
  }

  func createTestProject() {
    guard let outputURL = ProjectFilePanel.chooseProjectSaveLocation() else {
      statusMessage = "Create test project cancelled."
      return
    }

    statusMessage = "Creating test project through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        _ = try RustBridgePlaceholder.createExampleProject(outputURL: outputURL)
        let inspectResult = try RustBridgePlaceholder.inspectProject(projectURL: outputURL)
        DispatchQueue.main.async {
          self.projectURL = outputURL
          self.projectPath = outputURL.path
          self.projectSummary = Self.compactProjectSummary(inspectResult.summary)
          self.statusMessage = "Created project \(outputURL.lastPathComponent)."
        }
      } catch {
        DispatchQueue.main.async {
          self.statusMessage = "Project creation failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func openProject() {
    guard let url = ProjectFilePanel.chooseProjectFile() else {
      statusMessage = "Open project cancelled."
      return
    }

    statusMessage = "Validating project through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let inspectResult = try RustBridgePlaceholder.inspectProject(projectURL: url)
        DispatchQueue.main.async {
          self.projectURL = url
          self.projectPath = url.path
          self.projectSummary = Self.compactProjectSummary(inspectResult.summary)
          self.statusMessage = "Loaded project \(url.lastPathComponent)."
        }
      } catch {
        DispatchQueue.main.async {
          self.statusMessage = "Project load failed: \(error.localizedDescription)"
        }
      }
    }
  }

  private static func compactProbeSummary(_ text: String) -> String {
    let lines = text
      .split(whereSeparator: \.isNewline)
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }

    guard !lines.isEmpty else {
      return "Probe completed."
    }

    return lines.prefix(5).joined(separator: " | ")
  }

  private static func fallbackProbeSummary(mediaURL: URL, appleError: Error) -> String {
    do {
      let commandResult = try RustBridgePlaceholder.probeMedia(mediaURL: mediaURL)
      return "FFprobe fallback: \(compactProbeSummary(commandResult.summary))"
    } catch {
      return "Probe failed: AVFoundation \(appleError.localizedDescription); FFprobe \(error.localizedDescription)"
    }
  }

  private static func compactProjectSummary(_ text: String) -> String {
    let lines = text
      .split(whereSeparator: \.isNewline)
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }

    guard !lines.isEmpty else {
      return "Project validated."
    }

    return lines.prefix(8).joined(separator: " | ")
  }
}

enum RenderQualityOption: String, CaseIterable, Identifiable {
  case draftPreview = "Draft Preview"
  case highQualityOffline = "High Quality Offline"
  case floatMaster = "Float Master"

  var id: String { rawValue }
}

enum ExportFormatOption: String, CaseIterable, Identifiable {
  case pngSequence = "PNG Sequence"
  case exrSequence = "EXR Sequence"
  case proRes = "ProRes"
  case wavStems = "WAV Stems"

  var id: String { rawValue }
}
