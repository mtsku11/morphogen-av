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
  @Published var proResFrameRate: ProResFrameRateOption = .fps24 {
    didSet {
      refreshProResPlanPreview()
    }
  }
  @Published var proResProfile: ProResExportProfile = .proRes422HQ {
    didSet {
      refreshProResPlanPreview()
    }
  }
  @Published var projectPath = "No project loaded"
  @Published var projectSummary = "Project schema idle"
  @Published var renderQueueSummary = "No queue output bundle yet"
  @Published var proResPlanSummary = VideoToolboxProResExportPlanner.defaultPlanSummary()
  @Published var proResExportSummary = "No ProRes movie exported"
  @Published var previewProbeSummary = "No preview frame decoded"
  @Published var frameSequenceModulatorPath = "No modulator frame directory selected"
  @Published var frameSequenceCarrierPath = "No carrier frame directory selected"
  @Published var frameSequenceOutputPath = "No frame sequence output selected"
  @Published var frameSequenceSummary = "No two-source frame sequence rendered"
  @Published var frameSequenceAmount = 16.0
  @Published var frameSequenceMaxFrames = 120
  @Published var frameSequenceWritesFlowCache = true
  @Published var feedbackPreset: FeedbackPresetOption = .stableTrails {
    didSet {
      applyFeedbackPreset(feedbackPreset)
    }
  }
  @Published var feedbackCarrierAmount = 1.0
  @Published var feedbackAmount = 1.5
  @Published var feedbackMix = 0.68
  @Published var feedbackDecay = 0.99
  @Published var feedbackIterations = 1
  @Published var feedbackStructureMix = 0.0
  @Published var feedbackOutputBitDepth: FeedbackOutputBitDepthOption = .png16
  @Published var feedbackTemporalSupersampling = 1
  @Published var feedbackFlowSource: FeedbackFlowSourceOption = .opticalFlow
  @Published var feedbackBackend: FeedbackRenderBackendOption = .metal
  @Published var feedbackWritesFlowCache = true
  @Published var feedbackResetEnabled = false
  @Published var feedbackResetAtFrame = 48
  @Published var feedbackSummary = "No temporal flow-feedback sequence rendered"
  @Published var granularPoolGrainSize = 32
  @Published var granularPoolRearrangement = 1.0
  @Published var granularPoolVariation = 0.25
  @Published var granularPoolSeed = 0
  @Published var granularPoolAudioWeight = 1.0
  @Published var granularPoolTextureWeight = 0.0
  @Published var granularPoolAudioWeighted = true
  @Published var granularPoolCentroidEnabled = false
  @Published var granularPoolWindow = 0
  @Published var granularPoolAntiRepeatWeight = 0.0
  @Published var granularPoolAntiRepeatCooldown = 8
  @Published var granularPoolCoherenceWeight = 0.0
  @Published var granularPoolCoherenceReach = 8
  @Published var granularPoolSpatialCoherenceWeight = 0.0
  @Published var granularPoolBackend: FeedbackRenderBackendOption = .cpu
  @Published var granularPoolSummary = "No temporal grain pool sequence rendered"
  @Published var vocoderMode: VideoVocoderModeOption = .match
  @Published var vocoderBands = 8
  @Published var vocoderAmount = 1.0
  @Published var vocoderBackend: FeedbackRenderBackendOption = .cpu
  @Published var vocoderSummary = "No video vocoder sequence rendered"

  @Published var crossSynthModulatorURL: URL?
  @Published var crossSynthCarrierURL: URL?
  @Published var crossSynthOutputURL: URL?
  @Published var crossSynthMode: CrossSynthModeOption = .gain
  @Published var crossSynthAmount = 1.0
  @Published var crossSynthFilterType: CrossSynthFilterTypeOption = .lowpass
  @Published var crossSynthRmsWindow = 2048
  @Published var crossSynthRmsHop = 512
  @Published var crossSynthFFTSize = 1024
  @Published var crossSynthSTFTHop = 256
  @Published var crossSynthWindow: CrossSynthWindowOption = .hann
  @Published var crossSynthSummary = "No spectral cross-synth rendered"

  @Published var audioRouteModulatorURL: URL?
  @Published var audioRouteCarrierURL: URL?
  @Published var audioRouteOutputURL: URL?
  @Published var audioRouteAmount = 1.0
  @Published var audioRouteShiftX = 8.0
  @Published var audioRouteShiftY = 0.0
  @Published var audioRouteRmsWindow = 2048
  @Published var audioRouteRmsHop = 512
  @Published var audioRouteFrameRate = 30.0
  @Published var audioRouteBackend: FeedbackRenderBackendOption = .cpu
  @Published var audioRouteSummary = "No audio→video route rendered"
  @Published var mediaProxyOutputPath = RustBridgePlaceholder.defaultMediaProxyRootURL().path
  @Published var mediaProxySummary = "No source proxies extracted"
  @Published var mediaProxyFrameRate = 12.0
  @Published var mediaProxyMaxFrames = 120
  @Published var statusMessage = "Analysis cache idle. Offline queue empty."

  private var sourceAURL: URL?
  private var sourceBURL: URL?
  private var projectURL: URL?
  private var lastRenderQueueBundleURL: URL?
  private var frameSequenceModulatorURL: URL?
  private var frameSequenceCarrierURL: URL?
  private var frameSequenceOutputURL: URL?
  private var lastFrameSequenceOutputURL: URL?
  private var sourceARMSCacheURL: URL?
  private var sourceBRMSCacheURL: URL?
  private var sourceASTFTCacheURL: URL?
  private var sourceBSTFTCacheURL: URL?
  private var mediaProxyOutputURL = RustBridgePlaceholder.defaultMediaProxyRootURL()

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
          self.applyRenderQueueTimingDefaults(bundle)
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

  func chooseFrameSequenceModulatorDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source A Frames",
      message: "Select modulator PNG frames."
    ) else {
      statusMessage = "Source A frame selection cancelled."
      return
    }

    frameSequenceModulatorURL = url
    frameSequenceModulatorPath = url.path
    statusMessage = "Source A frame directory selected: \(url.lastPathComponent)"
  }

  func chooseFrameSequenceCarrierDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source B Frames",
      message: "Select carrier PNG frames."
    ) else {
      statusMessage = "Source B frame selection cancelled."
      return
    }

    frameSequenceCarrierURL = url
    frameSequenceCarrierPath = url.path
    statusMessage = "Source B frame directory selected: \(url.lastPathComponent)"
  }

  func chooseFrameSequenceOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Frame sequence output selection cancelled."
      return
    }

    frameSequenceOutputURL = url
    frameSequenceOutputPath = url.path
    statusMessage = "Frame sequence output selected: \(url.lastPathComponent)"
  }

  func chooseCrossSynthModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source A WAV",
      message: "Select the modulator audio (analysis source)."
    ) else {
      statusMessage = "Source A WAV selection cancelled."
      return
    }

    crossSynthModulatorURL = url
    statusMessage = "Cross-synth Source A WAV selected: \(url.lastPathComponent)"
  }

  func chooseCrossSynthCarrierWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source B WAV",
      message: "Select the carrier audio (material to reshape)."
    ) else {
      statusMessage = "Source B WAV selection cancelled."
      return
    }

    crossSynthCarrierURL = url
    statusMessage = "Cross-synth Source B WAV selected: \(url.lastPathComponent)"
  }

  func chooseCrossSynthOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Cross-synth output selection cancelled."
      return
    }

    crossSynthOutputURL = url
    statusMessage = "Cross-synth output selected: \(url.lastPathComponent)"
  }

  func chooseAudioRouteModulatorWAV() {
    guard let url = MediaFilePicker.chooseWAVFile(
      title: "Choose Source A WAV",
      message: "Select the modulator audio whose RMS drives displacement."
    ) else {
      statusMessage = "Source A WAV selection cancelled."
      return
    }

    audioRouteModulatorURL = url
    statusMessage = "Audio-route Source A WAV selected: \(url.lastPathComponent)"
  }

  func chooseAudioRouteCarrierDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameDirectory(
      title: "Choose Source B Frames",
      message: "Select the carrier PNG frames to displace."
    ) else {
      statusMessage = "Source B frame selection cancelled."
      return
    }

    audioRouteCarrierURL = url
    statusMessage = "Audio-route Source B frame directory selected: \(url.lastPathComponent)"
  }

  func chooseAudioRouteOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseFrameSequenceOutputDirectory() else {
      statusMessage = "Audio-route output selection cancelled."
      return
    }

    audioRouteOutputURL = url
    statusMessage = "Audio-route output selected: \(url.lastPathComponent)"
  }

  func chooseMediaProxyOutputDirectory() {
    guard let url = ImageSequenceExportPanel.chooseMediaProxyOutputDirectory() else {
      statusMessage = "Media proxy output selection cancelled."
      return
    }

    mediaProxyOutputURL = url
    mediaProxyOutputPath = url.path
    statusMessage = "Media proxy output selected: \(url.lastPathComponent)"
  }

  func extractSelectedSourceProxies() {
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
      statusMessage = "Select Source A or Source B before extracting proxies."
      return
    }

    let outputRootURL = mediaProxyOutputURL
    let frameRate = mediaProxyFrameRate
    let maxFrames = mediaProxyMaxFrames
    let selectedProjectURL = projectURL
    statusMessage = "Extracting PNG and WAV source proxies through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        var results: [(SourceRole, MediaProxyExtractionCommandResult)] = []
        for (role, sourceURL) in selectedSources {
          let proxyName: String
          switch role {
          case .modulator:
            proxyName = "source-a"
          case .carrier:
            proxyName = "source-b"
          }
          let result = try RustBridgePlaceholder.extractMediaProxies(
            request: MediaProxyExtractionCommandRequest(
              sourceURL: sourceURL,
              proxyDirectoryURL: outputRootURL.appendingPathComponent(proxyName, isDirectory: true),
              framesPerSecond: frameRate,
              maxFrames: maxFrames,
              sampleRate: 48_000
            )
          )
          results.append((role, result))
        }

        var projectSummary: String?
        if let selectedProjectURL {
          for (role, result) in results {
            _ = try RustBridgePlaceholder.registerProjectSourceProxy(
              projectURL: selectedProjectURL,
              sourceRole: role,
              proxy: result
            )
          }
          let inspectResult = try RustBridgePlaceholder.inspectProject(projectURL: selectedProjectURL)
          projectSummary = Self.compactProjectSummary(inspectResult.summary)
        }

        DispatchQueue.main.async {
          for (role, result) in results {
            switch role {
            case .modulator:
              self.frameSequenceModulatorURL = result.frameDirectoryURL
              self.frameSequenceModulatorPath = result.frameDirectoryURL.path
              self.sourceARMSCacheURL = result.rmsCacheURL
              self.sourceASTFTCacheURL = result.stftCacheURL
            case .carrier:
              self.frameSequenceCarrierURL = result.frameDirectoryURL
              self.frameSequenceCarrierPath = result.frameDirectoryURL.path
              self.sourceBRMSCacheURL = result.rmsCacheURL
              self.sourceBSTFTCacheURL = result.stftCacheURL
            }
          }
          if let projectSummary {
            self.projectSummary = projectSummary
          }
          let projectText = selectedProjectURL == nil ? "" : " and recorded in the project"
          self.mediaProxySummary = "\(results.count) source proxy set(s) with RMS + STFT analysis caches at \(outputRootURL.path)\(projectText)"
          self.statusMessage = "Source proxy extraction and analysis caching complete\(projectText)."
        }
      } catch {
        DispatchQueue.main.async {
          self.mediaProxySummary = "Media proxy extraction failed: \(error.localizedDescription)"
          self.statusMessage = "Media proxy extraction failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runTwoSourceFrameSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering."
      return
    }
    guard let outputURL = frameSequenceOutputURL else {
      statusMessage = "Choose a frame sequence output directory before rendering."
      return
    }

    let request = FrameSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultFrameSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      amount: frameSequenceAmount,
      maxFrames: frameSequenceMaxFrames,
      frameRate: proResFrameRate.framesPerSecond,
      writesFlowCache: frameSequenceWritesFlowCache,
      projectURL: projectURL
    )

    statusMessage = "Queueing two-source frame sequence render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedFrameSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let cacheText = request.writesFlowCache ? ", flow cache persisted" : ""
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.frameSequenceSummary = "\(bundle.frameCount) PNG frame(s) at \(bundle.frameDirectory.path)\(cacheText)"
          self.proResExportSummary = "Queued frame sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Two-source frame sequence render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.frameSequenceSummary = "Two-source frame sequence render failed: \(error.localizedDescription)"
          self.statusMessage = "Two-source frame sequence render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runFlowFeedbackSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering flow feedback."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering flow feedback."
      return
    }
    guard let outputURL = frameSequenceOutputURL else {
      statusMessage = "Choose a frame sequence output directory before rendering flow feedback."
      return
    }

    let request = FeedbackSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultFeedbackSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      carrierAmount: feedbackCarrierAmount,
      feedbackAmount: feedbackAmount,
      feedbackMix: feedbackMix,
      decay: feedbackDecay,
      iterations: feedbackIterations,
      structureMix: feedbackStructureMix,
      outputBitDepth: feedbackOutputBitDepth,
      temporalSupersampling: feedbackTemporalSupersampling,
      maxFrames: frameSequenceMaxFrames,
      resetAtFrame: feedbackResetEnabled ? feedbackResetAtFrame : nil,
      frameRate: proResFrameRate.framesPerSecond,
      writesFlowCache: feedbackWritesFlowCache,
      backend: feedbackBackend,
      flowSource: feedbackFlowSource,
      projectURL: projectURL
    )

    statusMessage = "Queueing temporal flow-feedback render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedFeedbackSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let resetText = request.resetAtFrame.map { ", reset at frame \($0)" } ?? ""
        let cacheText = request.writesFlowCache ? ", flow cache persisted" : ""
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.feedbackSummary = "\(bundle.frameCount) \(request.outputBitDepth.rawValue) feedback frame(s), \(request.temporalSupersampling)x temporal samples at \(bundle.frameDirectory.path)\(cacheText)\(resetText)"
          self.proResExportSummary = "Queued flow-feedback sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Temporal flow-feedback render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.feedbackSummary = "Temporal flow-feedback render failed: \(error.localizedDescription)"
          self.statusMessage = "Temporal flow-feedback render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runGranularMosaicPoolSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering the grain pool."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering the grain pool."
      return
    }
    guard let outputURL = frameSequenceOutputURL else {
      statusMessage = "Choose a frame sequence output directory before rendering the grain pool."
      return
    }

    let audioWeighted = granularPoolAudioWeighted
    let modulatorRMSCacheURL = audioWeighted ? sourceARMSCacheURL : nil
    let carrierRMSCacheURL = audioWeighted ? sourceBRMSCacheURL : nil
    if audioWeighted && (modulatorRMSCacheURL == nil || carrierRMSCacheURL == nil) {
      statusMessage = "Extract source proxies first to generate the RMS caches audio matching needs, or turn off Audio-Weighted."
      return
    }

    let centroidEnabled = granularPoolCentroidEnabled
    let modulatorCentroidCacheURL = centroidEnabled ? sourceASTFTCacheURL : nil
    let carrierCentroidCacheURL = centroidEnabled ? sourceBSTFTCacheURL : nil
    if centroidEnabled && (modulatorCentroidCacheURL == nil || carrierCentroidCacheURL == nil) {
      statusMessage = "Extract source proxies first to generate the STFT caches spectral-centroid matching needs, or turn off Spectral Centroid."
      return
    }

    let request = GranularMosaicPoolSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultGranularMosaicPoolSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      grainSize: granularPoolGrainSize,
      rearrangement: granularPoolRearrangement,
      variation: granularPoolVariation,
      seed: UInt64(max(0, granularPoolSeed)),
      audioWeight: granularPoolAudioWeight,
      textureWeight: granularPoolTextureWeight,
      modulatorRMSCacheURL: modulatorRMSCacheURL,
      carrierRMSCacheURL: carrierRMSCacheURL,
      modulatorCentroidCacheURL: modulatorCentroidCacheURL,
      carrierCentroidCacheURL: carrierCentroidCacheURL,
      poolWindow: max(0, granularPoolWindow),
      antiRepeatWeight: granularPoolAntiRepeatWeight,
      antiRepeatCooldown: max(0, granularPoolAntiRepeatCooldown),
      coherenceWeight: granularPoolCoherenceWeight,
      coherenceReach: max(0, granularPoolCoherenceReach),
      spatialCoherenceWeight: granularPoolSpatialCoherenceWeight,
      maxFrames: frameSequenceMaxFrames,
      frameRate: proResFrameRate.framesPerSecond,
      backend: granularPoolBackend,
      projectURL: projectURL
    )

    statusMessage = "Queueing temporal grain pool (joint-AV) render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedGranularMosaicPoolSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let audioText = audioWeighted ? ", audio-weighted (RMS)" : ", color-only"
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.granularPoolSummary = "\(bundle.frameCount) grain-pool frame(s)\(audioText) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued grain pool sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Temporal grain pool render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.granularPoolSummary = "Temporal grain pool render failed: \(error.localizedDescription)"
          self.statusMessage = "Temporal grain pool render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runVideoVocoderSequenceRender() {
    guard let modulatorURL = frameSequenceModulatorURL else {
      statusMessage = "Select Source A frame directory before rendering the video vocoder."
      return
    }
    guard let carrierURL = frameSequenceCarrierURL else {
      statusMessage = "Select Source B frame directory before rendering the video vocoder."
      return
    }
    guard let outputURL = frameSequenceOutputURL else {
      statusMessage = "Choose a frame sequence output directory before rendering the video vocoder."
      return
    }

    let request = VideoVocoderSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultVideoVocoderSequenceRenderQueueURL(),
      modulatorDirectoryURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      bands: vocoderBands,
      amount: vocoderAmount,
      mode: vocoderMode,
      maxFrames: frameSequenceMaxFrames,
      frameRate: proResFrameRate.framesPerSecond,
      backend: vocoderBackend,
      projectURL: projectURL
    )

    statusMessage = "Queueing video vocoder render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedVideoVocoderSequenceRender(request: request)
        let bundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: result.bundleURL)
        let modeText = self.vocoderMode == .match ? "tonal-match" : "band-gain"
        DispatchQueue.main.async {
          self.applyRenderQueueTimingDefaults(bundle)
          self.lastFrameSequenceOutputURL = bundle.frameDirectory
          self.lastRenderQueueBundleURL = bundle.bundleURL
          self.renderQueueSummary = "\(bundle.compactSummary) at \(bundle.bundleURL.path)"
          self.vocoderSummary = "\(bundle.frameCount) vocoder frame(s) (\(modeText)) at \(bundle.frameDirectory.path)"
          self.proResExportSummary = "Queued video vocoder sequence ready for ProRes export: \(bundle.bundleURL.path)"
          self.statusMessage = "Video vocoder render complete: \(bundle.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.vocoderSummary = "Video vocoder render failed: \(error.localizedDescription)"
          self.statusMessage = "Video vocoder render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runSpectralCrossSynthRender() {
    guard let modulatorURL = crossSynthModulatorURL else {
      statusMessage = "Select a Source A WAV before rendering the spectral cross-synth."
      return
    }
    guard let carrierURL = crossSynthCarrierURL else {
      statusMessage = "Select a Source B WAV before rendering the spectral cross-synth."
      return
    }
    guard let outputURL = crossSynthOutputURL else {
      statusMessage = "Choose an output directory before rendering the spectral cross-synth."
      return
    }

    let request = SpectralCrossSynthRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultSpectralCrossSynthRenderQueueURL(),
      modulatorWAVURL: modulatorURL,
      carrierWAVURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      mode: crossSynthMode,
      amount: crossSynthAmount,
      filterType: crossSynthFilterType,
      rmsWindow: crossSynthRmsWindow,
      rmsHop: crossSynthRmsHop,
      fftSize: crossSynthFFTSize,
      stftHop: crossSynthSTFTHop,
      window: crossSynthWindow,
      projectURL: projectURL
    )

    statusMessage = "Queueing spectral cross-synth render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedSpectralCrossSynthRender(request: request)
        let modeText = self.crossSynthMode == .gain ? "RMS-gain" : "centroid-filter"
        DispatchQueue.main.async {
          self.crossSynthSummary = "Cross-synth (\(modeText)) bundle at \(result.bundleURL.path)"
          self.statusMessage = "Spectral cross-synth render complete: \(result.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.crossSynthSummary = "Spectral cross-synth render failed: \(error.localizedDescription)"
          self.statusMessage = "Spectral cross-synth render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func runAudioVideoRouteRender() {
    guard let modulatorURL = audioRouteModulatorURL else {
      statusMessage = "Select a Source A WAV before rendering the audio→video route."
      return
    }
    guard let carrierURL = audioRouteCarrierURL else {
      statusMessage = "Select a Source B frame directory before rendering the audio→video route."
      return
    }
    guard let outputURL = audioRouteOutputURL else {
      statusMessage = "Choose an output directory before rendering the audio→video route."
      return
    }

    let request = AudioVideoRouteSequenceRenderQueueCommandRequest(
      queueURL: RustBridgePlaceholder.defaultAudioVideoRouteSequenceRenderQueueURL(),
      modulatorWAVURL: modulatorURL,
      carrierDirectoryURL: carrierURL,
      outputRootDirectoryURL: outputURL,
      amount: audioRouteAmount,
      shiftX: audioRouteShiftX,
      shiftY: audioRouteShiftY,
      rmsWindow: audioRouteRmsWindow,
      rmsHop: audioRouteRmsHop,
      frameRate: audioRouteFrameRate,
      maxFrames: nil,
      backend: audioRouteBackend,
      projectURL: projectURL
    )

    statusMessage = "Queueing audio→video route render through morphogen-cli..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let result = try RustBridgePlaceholder.runQueuedAudioVideoRouteSequenceRender(
          request: request
        )
        DispatchQueue.main.async {
          self.audioRouteSummary = "Audio→video route bundle at \(result.bundleURL.path)"
          self.statusMessage = "Audio→video route render complete: \(result.bundleURL.path)"
        }
      } catch {
        DispatchQueue.main.async {
          self.audioRouteSummary = "Audio→video route render failed: \(error.localizedDescription)"
          self.statusMessage = "Audio→video route render failed: \(error.localizedDescription)"
        }
      }
    }
  }

  func applyFeedbackPreset(_ preset: FeedbackPresetOption) {
    guard let settings = preset.settings else {
      return
    }

    feedbackCarrierAmount = settings.carrierAmount
    feedbackAmount = settings.feedbackAmount
    feedbackMix = settings.feedbackMix
    feedbackDecay = settings.decay
    feedbackStructureMix = settings.structureMix
    feedbackFlowSource = settings.flowSource
    feedbackBackend = settings.backend
    feedbackWritesFlowCache = settings.writesFlowCache
    feedbackResetEnabled = settings.resetAtFrame != nil
    if let resetAtFrame = settings.resetAtFrame {
      feedbackResetAtFrame = min(resetAtFrame, frameSequenceMaxFrames - 1)
    }
    statusMessage = "Applied flow-feedback preset: \(preset.rawValue)."
  }

  func checkProResExportPlan() {
    let selectedFrameRate = proResFrameRate.framesPerSecond
    let selectedProfile = proResProfile
    statusMessage = "Checking ProRes export support through VideoToolbox..."

    DispatchQueue.global(qos: .userInitiated).async {
      do {
        let plan = try VideoToolboxProResExportPlanner.makePlan(
          width: 1920,
          height: 1080,
          frameRate: selectedFrameRate,
          profile: selectedProfile
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
    let selectedFrameRate = proResFrameRate.framesPerSecond
    let selectedProfile = proResProfile
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
              frameRate: selectedFrameRate,
              profile: selectedProfile,
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

  func exportLastFrameSequenceProResMovie() {
    guard let frameDirectory = lastFrameSequenceOutputURL ?? frameSequenceOutputURL else {
      statusMessage = "Run a two-source frame sequence before exporting its ProRes movie."
      return
    }

    let selectedFrameRate = proResFrameRate.framesPerSecond
    let selectedProfile = proResProfile
    let defaultMovieName = "\(frameDirectory.lastPathComponent)-prores.mov"
    guard let outputURL = ImageSequenceExportPanel.chooseMovieSaveLocation(defaultName: defaultMovieName) else {
      statusMessage = "ProRes export cancelled."
      return
    }

    statusMessage = "Exporting two-source frame sequence to ProRes MOV..."

    DispatchQueue.global(qos: .userInitiated).async {
      Task {
        do {
          let result = try await ProResImageSequenceExporter.exportPNGSequence(
            request: ProResImageSequenceExportRequest(
              frameDirectory: frameDirectory,
              outputURL: outputURL,
              frameRate: selectedFrameRate,
              profile: selectedProfile,
              requiresHardwareEncoder: false
            )
          )
          await MainActor.run {
            self.proResExportSummary = result.compactSummary
            self.statusMessage = "Two-source ProRes export complete: \(outputURL.path)"
          }
        } catch {
          await MainActor.run {
            self.proResExportSummary = "Two-source ProRes export failed: \(error.localizedDescription)"
            self.statusMessage = "Two-source ProRes export failed: \(error.localizedDescription)"
          }
        }
      }
    }
  }

  func exportRenderQueueProResMovie() {
    let defaultBundleURL = RustBridgePlaceholder.defaultQueuedTestRenderBundleURL()
    let bundleURL = lastRenderQueueBundleURL ?? defaultBundleURL

    let inspectedBundle: RenderQueueOutputBundle
    do {
      inspectedBundle = try RenderQueueOutputBundleResolver.inspect(bundleURL: bundleURL)
      applyRenderQueueTimingDefaults(inspectedBundle)
      renderQueueSummary = "\(inspectedBundle.compactSummary) at \(inspectedBundle.bundleURL.path)"
    } catch {
      statusMessage = "Run queued test render before exporting its ProRes movie."
      renderQueueSummary = "No queue output bundle found at \(defaultBundleURL.path): \(error.localizedDescription)"
      return
    }

    let selectedFrameRate = proResFrameRate.framesPerSecond
    let selectedProfile = proResProfile
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
              frameRate: selectedFrameRate,
              profile: selectedProfile,
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

  private func refreshProResPlanPreview() {
    proResPlanSummary = VideoToolboxProResExportPlanner.defaultPlanSummary(
      frameRate: proResFrameRate.framesPerSecond,
      profile: proResProfile
    )
  }

  private func applyRenderQueueTimingDefaults(_ bundle: RenderQueueOutputBundle) {
    guard let timing = bundle.timing,
          let frameRateOption = ProResFrameRateOption.matching(timing.frameRate)
    else {
      return
    }
    proResFrameRate = frameRateOption
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

enum FeedbackFlowSourceOption: String, CaseIterable, Identifiable {
  case opticalFlow = "Optical Flow"
  case luminance = "Luminance Gradient"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .opticalFlow:
      return "optical-flow"
    case .luminance:
      return "luminance"
    }
  }
}

enum FeedbackRenderBackendOption: String, CaseIterable, Identifiable {
  case cpu = "CPU Reference"
  case metal = "Metal"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .cpu:
      return "cpu"
    case .metal:
      return "metal"
    }
  }
}

enum VideoVocoderModeOption: String, CaseIterable, Identifiable {
  case match = "Match (tonal transfer)"
  case gain = "Gain (band routing)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .match:
      return "match"
    case .gain:
      return "gain"
    }
  }
}

enum CrossSynthModeOption: String, CaseIterable, Identifiable {
  case gain = "Gain (RMS → amplitude)"
  case filter = "Filter (centroid → cutoff)"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .gain:
      return "gain"
    case .filter:
      return "filter"
    }
  }
}

enum CrossSynthFilterTypeOption: String, CaseIterable, Identifiable {
  case lowpass = "Lowpass"
  case highpass = "Highpass"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .lowpass:
      return "lowpass"
    case .highpass:
      return "highpass"
    }
  }
}

enum CrossSynthWindowOption: String, CaseIterable, Identifiable {
  case hann = "Hann"
  case hamming = "Hamming"
  case rectangular = "Rectangular"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .hann:
      return "hann"
    case .hamming:
      return "hamming"
    case .rectangular:
      return "rectangular"
    }
  }
}

enum FeedbackOutputBitDepthOption: String, CaseIterable, Identifiable {
  case png8 = "PNG 8-bit"
  case png16 = "PNG 16-bit"

  var id: String { rawValue }

  var cliValue: String {
    switch self {
    case .png8:
      return "8"
    case .png16:
      return "16"
    }
  }
}

struct FeedbackPresetSettings: Equatable {
  let carrierAmount: Double
  let feedbackAmount: Double
  let feedbackMix: Double
  let decay: Double
  let structureMix: Double
  let flowSource: FeedbackFlowSourceOption
  let backend: FeedbackRenderBackendOption
  let writesFlowCache: Bool
  let resetAtFrame: Int?
}

enum FeedbackPresetOption: String, CaseIterable, Identifiable {
  case stableTrails = "Stable Trails"
  case aggressiveDegradation = "Aggressive Degradation"
  case resetDrivenCuts = "Reset-Driven Cuts"
  case structuredMorph = "Structured Morph"
  case custom = "Custom"

  var id: String { rawValue }

  var settings: FeedbackPresetSettings? {
    switch self {
    case .stableTrails:
      return FeedbackPresetSettings(
        carrierAmount: 1.0,
        feedbackAmount: 1.5,
        feedbackMix: 0.68,
        decay: 0.99,
        structureMix: 0.0,
        flowSource: .opticalFlow,
        backend: .metal,
        writesFlowCache: true,
        resetAtFrame: nil
      )
    case .aggressiveDegradation:
      return FeedbackPresetSettings(
        carrierAmount: 2.5,
        feedbackAmount: 7.0,
        feedbackMix: 0.92,
        decay: 0.998,
        structureMix: 0.0,
        flowSource: .opticalFlow,
        backend: .metal,
        writesFlowCache: true,
        resetAtFrame: nil
      )
    case .resetDrivenCuts:
      return FeedbackPresetSettings(
        carrierAmount: 1.25,
        feedbackAmount: 3.5,
        feedbackMix: 0.84,
        decay: 0.99,
        structureMix: 0.0,
        flowSource: .opticalFlow,
        backend: .metal,
        writesFlowCache: true,
        resetAtFrame: 48
      )
    case .structuredMorph:
      // "Beyond recognition" as a structured morph: high feedback-mix so the
      // carrier stops re-asserting its composition, but structure re-injection
      // keeps regenerating high-frequency detail instead of washing to fog.
      // Settings follow the empirical lever sweep (mix ~0.97, decay ~0.97).
      return FeedbackPresetSettings(
        carrierAmount: 2.5,
        feedbackAmount: 7.0,
        feedbackMix: 0.97,
        decay: 0.97,
        structureMix: 0.6,
        flowSource: .opticalFlow,
        backend: .metal,
        writesFlowCache: true,
        resetAtFrame: nil
      )
    case .custom:
      return nil
    }
  }
}

enum ProResFrameRateOption: String, CaseIterable, Identifiable {
  case fps12 = "12 fps"
  case fps23976 = "23.976 fps"
  case fps24 = "24 fps"
  case fps25 = "25 fps"
  case fps2997 = "29.97 fps"
  case fps30 = "30 fps"
  case fps60 = "60 fps"

  var id: String { rawValue }

  var framesPerSecond: Double {
    switch self {
    case .fps12:
      return 12.0
    case .fps23976:
      return 24_000.0 / 1_001.0
    case .fps24:
      return 24.0
    case .fps25:
      return 25.0
    case .fps2997:
      return 30_000.0 / 1_001.0
    case .fps30:
      return 30.0
    case .fps60:
      return 60.0
    }
  }

  static func matching(_ frameRate: Double) -> ProResFrameRateOption? {
    guard frameRate.isFinite && frameRate > 0 else {
      return nil
    }
    return allCases.first { option in
      abs(option.framesPerSecond - frameRate) < 0.0005
    }
  }
}
