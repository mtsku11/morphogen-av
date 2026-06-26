import SwiftUI

struct WorkflowPanelView: View {
  @ObservedObject var state: AppState

  @State private var sourceMode: WorkflowSourceMode = .twoSource
  @State private var analysisSignal: WorkflowAnalysisSignal = .opticalFlow
  @State private var modulationTarget: WorkflowModulationTarget = .displacement
  @State private var selectedEffect: WorkflowEffect = .flowFeedback
  @State private var fluidMode: WorkflowFluidMode = .twoSource
  @State private var showsAdvancedEffectControls = false

  var body: some View {
    VStack(alignment: .leading, spacing: 18) {
      setupBand
      routingBand
      effectBrowser
      selectedEffectControls
      renderBand
      previewBand
      statusBand
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .onChange(of: sourceMode) { _, newMode in
      applySourceModeDefaults(newMode)
    }
  }

  private var setupBand: some View {
    workflowBand {
      VStack(alignment: .leading, spacing: 12) {
        workflowHeading("1. Sources and Proxies", systemImage: "tray.and.arrow.down")

        HStack(spacing: 12) {
          Button {
            state.probeSelectedSources()
          } label: {
            Label("Probe Sources", systemImage: "waveform.path.ecg.rectangle")
          }

          Button {
            state.probePreviewFrames()
          } label: {
            Label("Decode Preview Frames", systemImage: "rectangle.on.rectangle")
          }

          Button {
            state.chooseMediaProxyOutputDirectory()
          } label: {
            Label("Proxy Output", systemImage: "folder.badge.plus")
          }

          Button {
            state.extractSelectedSourceProxies()
          } label: {
            Label("Extract Proxies", systemImage: "square.stack.3d.down.forward")
          }
          .buttonStyle(.borderedProminent)
        }

        HStack(spacing: 16) {
          Picker("Source Mode", selection: $sourceMode) {
            ForEach(WorkflowSourceMode.allCases) { mode in
              Text(mode.rawValue).tag(mode)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 320)

          Stepper(value: $state.mediaProxyFrameRate, in: 1...60, step: 1) {
            Text("Proxy \(state.mediaProxyFrameRate, specifier: "%.0f") fps")
          }
          .frame(width: 140, alignment: .leading)

          Stepper(value: $state.mediaProxyMaxFrames, in: 1...600, step: 1) {
            Text("Limit \(state.mediaProxyMaxFrames) frames")
          }
          .frame(width: 170, alignment: .leading)
        }

        pathGrid([
          ("A Frames", state.frameSequenceModulatorPath),
          ("B Frames", state.frameSequenceCarrierPath),
          ("Proxy Root", state.mediaProxyOutputPath)
        ])
      }
    }
  }

  private var routingBand: some View {
    workflowBand {
      VStack(alignment: .leading, spacing: 12) {
        workflowHeading("2. Modulation Routing", systemImage: "point.3.connected.trianglepath.dotted")

        HStack(spacing: 16) {
          Picker("Analysis Signal", selection: $analysisSignal) {
            ForEach(WorkflowAnalysisSignal.allCases) { signal in
              Text(signal.rawValue).tag(signal)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 240)

          Picker("Controls", selection: $modulationTarget) {
            ForEach(WorkflowModulationTarget.allCases) { target in
              Text(target.rawValue).tag(target)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 250)
        }

        HStack(spacing: 10) {
          routeNode(sourceMode.routeSource, systemImage: sourceMode == .twoSource ? "a.square" : "b.square")
          routeArrow
          routeNode(analysisSignal.shortLabel, systemImage: analysisSignal.systemImage)
          routeArrow
          routeNode(modulationTarget.shortLabel, systemImage: modulationTarget.systemImage)
          routeArrow
          routeNode(selectedEffect.routeOutputLabel, systemImage: selectedEffect.systemImage)
        }
        .frame(maxWidth: .infinity, alignment: .leading)

        Text(selectedEffect.routeDescription)
          .font(.caption)
          .foregroundStyle(.secondary)

        if sourceMode == .selfModulated {
          Text("One-video mode currently routes Source B through the self-flow, procedural field, or particle advection renderers.")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }
    }
  }

  private var effectBrowser: some View {
    VStack(alignment: .leading, spacing: 12) {
      workflowHeading("3. Effects", systemImage: "slider.horizontal.below.rectangle")

      LazyVGrid(
        columns: [GridItem(.adaptive(minimum: 210, maximum: 280), spacing: 12)],
        alignment: .leading,
        spacing: 12
      ) {
        ForEach(WorkflowEffect.allCases) { effect in
          Button {
            selectedEffect = effect
            effect.applyRoutingDefaults(
              analysisSignal: $analysisSignal,
              modulationTarget: $modulationTarget
            )
          } label: {
            effectCard(effect)
          }
          .buttonStyle(.plain)
        }
      }
    }
  }

  private var selectedEffectControls: some View {
    workflowBand {
      VStack(alignment: .leading, spacing: 12) {
        workflowHeading("4. \(selectedEffect.rawValue)", systemImage: selectedEffect.systemImage)

        selectedPrimaryControls

        DisclosureGroup("Advanced knobs", isExpanded: $showsAdvancedEffectControls) {
          selectedAdvancedControls
            .padding(.top, 8)
        }
      }
    }
  }

  @ViewBuilder
  private var selectedPrimaryControls: some View {
    switch selectedEffect {
    case .flowDisplace:
      HStack(spacing: 16) {
        Stepper(value: $state.frameSequenceAmount, in: 0...64, step: 1) {
          Text("Displacement \(state.frameSequenceAmount, specifier: "%.0f")")
        }
        .frame(width: 190, alignment: .leading)

        Toggle("Reuse flow cache", isOn: $state.frameSequenceWritesFlowCache)
          .toggleStyle(.checkbox)
      }

    case .flowFeedback:
      HStack(spacing: 16) {
        Picker("Preset", selection: $state.feedbackPreset) {
          ForEach(FeedbackPresetOption.allCases) { preset in
            Text(preset.rawValue).tag(preset)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 210)

        Stepper(value: $state.feedbackAmount, in: 0...12, step: 0.25) {
          Text("Feedback \(state.feedbackAmount, specifier: "%.2f")")
        }
        .frame(width: 170, alignment: .leading)

        Stepper(value: $state.feedbackMix, in: 0...1, step: 0.01) {
          Text("Mix \(state.feedbackMix, specifier: "%.2f")")
        }
        .frame(width: 125, alignment: .leading)
      }

    case .fluidAdvection:
      HStack(spacing: 16) {
        Picker("Mode", selection: $fluidMode) {
          ForEach(WorkflowFluidMode.allCases) { mode in
            Text(mode.rawValue).tag(mode)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 520)

        Stepper(value: $state.fluidReinject, in: 0...1, step: 0.01) {
          Text("Reinject \(state.fluidReinject, specifier: "%.2f")")
        }
        .frame(width: 165, alignment: .leading)
      }

      HStack(spacing: 16) {
        Stepper(value: $state.fluidMotionAdvect, in: 0...8, step: 0.25) {
          Text("Motion \(state.fluidMotionAdvect, specifier: "%.2f")")
        }
        .frame(width: 160, alignment: .leading)

        Stepper(value: $state.fluidProceduralAdvect, in: 0...48, step: 1) {
          Text("Field \(state.fluidProceduralAdvect, specifier: "%.0f")")
        }
        .frame(width: 140, alignment: .leading)

        Toggle("Live particle colour", isOn: $state.fieldParticleLiveColour)
          .toggleStyle(.checkbox)
          .disabled(fluidMode != .particles)
      }

    case .granularMosaic:
      HStack(spacing: 16) {
        Stepper(value: $state.granularPoolGrainSize, in: 4...256, step: 4) {
          Text("Grain \(state.granularPoolGrainSize)px")
        }
        .frame(width: 150, alignment: .leading)

        Stepper(value: $state.granularPoolRearrangement, in: 0...1, step: 0.05) {
          Text("Rearrange \(state.granularPoolRearrangement, specifier: "%.2f")")
        }
        .frame(width: 190, alignment: .leading)

        Toggle("Audio weighted", isOn: $state.granularPoolAudioWeighted)
          .toggleStyle(.checkbox)
      }

    case .datamosh:
      HStack(spacing: 16) {
        Picker("Preset", selection: $state.datamoshPreset) {
          ForEach(DatamoshPresetOption.allCases) { preset in
            Text(preset.rawValue).tag(preset)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 220)

        Picker("Vector Remix", selection: $state.datamoshVectorRemix) {
          ForEach(DatamoshVectorRemixOption.allCases) { mode in
            Text(mode.rawValue).tag(mode)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 260)
      }

    case .videoVocoder:
      HStack(spacing: 16) {
        Picker("Mode", selection: $state.vocoderMode) {
          ForEach(VideoVocoderModeOption.allCases) { mode in
            Text(mode.rawValue).tag(mode)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 360)

        Stepper(value: $state.vocoderAmount, in: 0...4, step: 0.05) {
          Text("Amount \(state.vocoderAmount, specifier: "%.2f")")
        }
        .frame(width: 170, alignment: .leading)
      }

    case .trailCascade:
      HStack(spacing: 16) {
        Stepper(value: $state.cascadeTileSize, in: 4...256, step: 4) {
          Text("Tile \(state.cascadeTileSize)px")
        }
        .frame(width: 150, alignment: .leading)

        Stepper(value: $state.cascadeGridSpacing, in: 4...256, step: 4) {
          Text("Spacing \(state.cascadeGridSpacing)px")
        }
        .frame(width: 165, alignment: .leading)
        .help("> Tile = sparse ribbons on black; = Tile smears the whole image.")

        Stepper(value: $state.cascadeAdvect, in: 0...8, step: 0.1) {
          Text("Flow \(state.cascadeAdvect, specifier: "%.1f")")
        }
        .frame(width: 150, alignment: .leading)
        .help("0 = static grid (no trails); higher = longer ribbons.")
      }
    }
  }

  @ViewBuilder
  private var selectedAdvancedControls: some View {
    switch selectedEffect {
    case .flowDisplace:
      Stepper(value: $state.frameSequenceMaxFrames, in: 1...600, step: 1) {
        Text("Max frames \(state.frameSequenceMaxFrames)")
      }
      .frame(width: 180, alignment: .leading)

    case .flowFeedback:
      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 16) {
          Picker("Flow Source", selection: $state.feedbackFlowSource) {
            ForEach(FeedbackFlowSourceOption.allCases) { source in
              Text(source.rawValue).tag(source)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 190)

          Picker("Backend", selection: $state.feedbackBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { backend in
              Text(backend.rawValue).tag(backend)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 220)

          Stepper(value: $state.feedbackCarrierAmount, in: 0...8, step: 0.25) {
            Text("Carrier \(state.feedbackCarrierAmount, specifier: "%.2f")")
          }
          .frame(width: 155, alignment: .leading)
        }

        HStack(spacing: 16) {
          Stepper(value: $state.feedbackDecay, in: 0...1, step: 0.001) {
            Text("Decay \(state.feedbackDecay, specifier: "%.3f")")
          }
          .frame(width: 145, alignment: .leading)

          Stepper(value: $state.feedbackStructureMix, in: 0...2, step: 0.05) {
            Text("Structure \(state.feedbackStructureMix, specifier: "%.2f")")
          }
          .frame(width: 165, alignment: .leading)

          Toggle("Reset", isOn: $state.feedbackResetEnabled)
            .toggleStyle(.checkbox)

          Stepper(value: $state.feedbackResetAtFrame, in: 0...max(0, state.frameSequenceMaxFrames - 1)) {
            Text("Reset frame \(state.feedbackResetAtFrame)")
          }
          .disabled(!state.feedbackResetEnabled)
          .frame(width: 165, alignment: .leading)
        }
      }

    case .fluidAdvection:
      HStack(spacing: 16) {
        Picker("Backend", selection: $state.fluidBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 220)

        Stepper(value: $state.fluidTurbulenceScale, in: 0...0.05, step: 0.001) {
          Text("Scale \(state.fluidTurbulenceScale, specifier: "%.3f")")
        }
        .frame(width: 150, alignment: .leading)

        Stepper(value: $state.fieldParticleSpacing, in: 1...64, step: 1) {
          Text("Spacing \(state.fieldParticleSpacing)")
        }
        .frame(width: 150, alignment: .leading)

        Stepper(value: $state.fieldParticleSize, in: 1...64, step: 1) {
          Text("Particle \(state.fieldParticleSize)")
        }
        .frame(width: 150, alignment: .leading)
      }

    case .granularMosaic:
      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 16) {
          Stepper(value: $state.granularPoolVariation, in: 0...1, step: 0.05) {
            Text("Variation \(state.granularPoolVariation, specifier: "%.2f")")
          }
          .frame(width: 170, alignment: .leading)

          Stepper(value: $state.granularPoolTextureWeight, in: 0...8, step: 0.1) {
            Text("Texture \(state.granularPoolTextureWeight, specifier: "%.1f")")
          }
          .frame(width: 160, alignment: .leading)

          Toggle("Centroid", isOn: $state.granularPoolCentroidEnabled)
            .toggleStyle(.checkbox)

          Picker("Backend", selection: $state.granularPoolBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { backend in
              Text(backend.rawValue).tag(backend)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 220)
        }

        HStack(spacing: 16) {
          Stepper(value: $state.granularPoolAntiRepeatWeight, in: 0...8, step: 0.1) {
            Text("Anti-repeat \(state.granularPoolAntiRepeatWeight, specifier: "%.1f")")
          }
          .frame(width: 180, alignment: .leading)

          Stepper(value: $state.granularPoolCoherenceWeight, in: 0...8, step: 0.1) {
            Text("Coherence \(state.granularPoolCoherenceWeight, specifier: "%.1f")")
          }
          .frame(width: 180, alignment: .leading)
        }
      }

    case .datamosh:
      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 16) {
          Stepper(value: $state.datamoshAmount, in: 0...4, step: 0.1) {
            Text("Amount \(state.datamoshAmount, specifier: "%.2f")")
          }
          .frame(width: 165, alignment: .leading)

          Stepper(value: $state.datamoshBlockSize, in: 1...64, step: 1) {
            Text("Block \(state.datamoshBlockSize)")
          }
          .frame(width: 140, alignment: .leading)

          Stepper(value: $state.datamoshKeyframeInterval, in: 0...120, step: 1) {
            Text("Keyframe \(state.datamoshKeyframeInterval)")
          }
          .frame(width: 170, alignment: .leading)

          Picker("Backend", selection: $state.datamoshBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { backend in
              Text(backend.rawValue).tag(backend)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 200)
        }

        HStack(spacing: 16) {
          Stepper(value: $state.datamoshResidualGain, in: 0...4, step: 0.1) {
            Text("Residual \(state.datamoshResidualGain, specifier: "%.2f")")
          }
          .frame(width: 175, alignment: .leading)

          Stepper(value: $state.datamoshBlockRefreshThreshold, in: 0...8, step: 0.25) {
            Text("Refresh \(state.datamoshBlockRefreshThreshold, specifier: "%.2f")")
          }
          .frame(width: 175, alignment: .leading)

          if state.datamoshVectorRemix == .shuffle {
            Stepper(value: $state.datamoshRemixSeed, in: 0...9999, step: 1) {
              Text("Seed \(state.datamoshRemixSeed)")
            }
            .frame(width: 130, alignment: .leading)
          }
        }
      }

    case .videoVocoder:
      HStack(spacing: 16) {
        Stepper(value: $state.vocoderBands, in: 1...64, step: 1) {
          Text("Bands \(state.vocoderBands)")
        }
        .frame(width: 150, alignment: .leading)
        .disabled(state.vocoderMode == .match)

        Picker("Backend", selection: $state.vocoderBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 220)
        .disabled(state.vocoderMode == .gain)
      }

    case .trailCascade:
      HStack(spacing: 16) {
        Stepper(value: $state.cascadeTurbulenceScale, in: 0.002...0.05, step: 0.001) {
          Text("Vortex \(state.cascadeTurbulenceScale, specifier: "%.3f")")
        }
        .frame(width: 170, alignment: .leading)
        .help("Field scale: smaller = larger, broader vortices.")

        Stepper(value: $state.cascadeDetail, in: 0...1, step: 0.05) {
          Text("Detail \(state.cascadeDetail, specifier: "%.2f")")
        }
        .frame(width: 150, alignment: .leading)

        Stepper(value: $state.cascadeSeed, in: 0...9999, step: 1) {
          Text("Seed \(state.cascadeSeed)")
        }
        .frame(width: 140, alignment: .leading)

        Toggle("Live refresh", isOn: $state.cascadeLiveRefresh)
          .toggleStyle(.checkbox)
          .help("Re-sample each tile from the current frame so the video plays through the trails.")
      }
    }
  }

  private var renderBand: some View {
    workflowBand {
      VStack(alignment: .leading, spacing: 12) {
        workflowHeading("5. Render", systemImage: "play.rectangle.on.rectangle")

        HStack(spacing: 16) {
          Picker("Quality", selection: $state.renderQuality) {
            ForEach(RenderQualityOption.allCases) { option in
              Text(option.rawValue).tag(option)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 420)

          Picker("Format", selection: $state.exportFormat) {
            ForEach(ExportFormatOption.allCases) { option in
              Text(option.rawValue).tag(option)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 170)

          Picker("FPS", selection: $state.proResFrameRate) {
            ForEach(ProResFrameRateOption.allCases) { option in
              Text(option.rawValue).tag(option)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 130)
        }

        HStack(spacing: 12) {
          Button {
            state.chooseFrameSequenceOutputDirectory()
          } label: {
            Label("Choose Output", systemImage: "folder.badge.plus")
          }

          Picker("Preview", selection: $state.showcaseIntensity) {
            ForEach(ShowcaseIntensityOption.allCases) { intensity in
              Text(intensity.rawValue).tag(intensity)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 220)

          Button {
            state.runShowcasePreviewRender()
          } label: {
            Label("Showcase Preview", systemImage: "sparkles")
          }
          .buttonStyle(.bordered)

          Button {
            runSelectedEffectPreview()
          } label: {
            Label("Quick Preview", systemImage: "eye")
          }
          .buttonStyle(.bordered)
          .disabled(state.isRenderingPreview)

          Button {
            runSelectedEffect()
          } label: {
            Label("Render \(selectedEffect.shortActionLabel)", systemImage: selectedEffect.systemImage)
          }
          .buttonStyle(.borderedProminent)

          Button {
            state.exportLastFrameSequenceProResMovie()
          } label: {
            Label("Export ProRes", systemImage: "film.badge.plus")
          }
        }

        pathGrid([
          ("Output Root", state.frameSequenceOutputPath),
          ("Showcase", state.showcaseSummary),
          ("Queue Bundle", state.renderQueueSummary),
          ("ProRes", state.proResExportSummary)
        ])
      }
    }
  }

  private var previewBand: some View {
    workflowBand {
      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 8) {
          workflowHeading("Preview", systemImage: "eye")
          if state.isRenderingPreview {
            ProgressView()
              .controlSize(.small)
          }
          Spacer()
        }

        if state.previewFrames.isEmpty {
          Text(state.isRenderingPreview
            ? state.previewSummary
            : "Quick Preview renders the first \(state.previewFrameCount) frames of the selected effect on your loaded sources — a fast look before committing to the full clip.")
            .font(.caption)
            .foregroundStyle(.secondary)
        } else {
          ScrollView(.horizontal, showsIndicators: true) {
            HStack(spacing: 8) {
              ForEach(Array(state.previewFrames.enumerated()), id: \.offset) { index, image in
                VStack(spacing: 4) {
                  Image(nsImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(height: 140)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                  Text("frame \(index)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                }
              }
            }
            .padding(.vertical, 2)
          }

          Text(state.previewSummary)
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }
    }
  }

  private var statusBand: some View {
    HStack(spacing: 8) {
      Image(systemName: "info.circle")
        .foregroundStyle(.secondary)
      Text(state.statusMessage)
        .font(.caption)
        .foregroundStyle(.secondary)
        .lineLimit(3)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  /// Render a short, few-frame preview of the selected effect into a temp
  /// directory (no output directory required) and show the frames inline.
  private func runSelectedEffectPreview() {
    // Single-source effects (procedural/self/particle fluid, trail cascade) need only Source B.
    let singleSource = selectedEffect == .trailCascade
      || (selectedEffect == .fluidAdvection && fluidMode != .twoSource)
    guard state.beginEffectPreview(requiresModulator: !singleSource) else {
      return
    }
    runSelectedEffect()
  }

  private func runSelectedEffect() {
    switch selectedEffect {
    case .flowDisplace:
      state.runTwoSourceFrameSequenceRender()
    case .flowFeedback:
      state.runFlowFeedbackSequenceRender()
    case .fluidAdvection:
      switch fluidMode {
      case .twoSource:
        state.runTwoSourceFluidAdvectSequenceRender()
      case .selfFlow:
        state.runOpticalFlowAdvectSequenceRender()
      case .procedural:
        state.runProceduralFluidAdvectSequenceRender()
      case .particles:
        state.runFieldParticlesSequenceRender()
      }
    case .granularMosaic:
      state.runGranularMosaicPoolSequenceRender()
    case .datamosh:
      state.runDatamoshRender()
    case .videoVocoder:
      state.runVideoVocoderSequenceRender()
    case .trailCascade:
      state.runTrailCascadeSequenceRender()
    }
  }

  private func applySourceModeDefaults(_ mode: WorkflowSourceMode) {
    switch mode {
    case .twoSource:
      if selectedEffect == .fluidAdvection && fluidMode == .selfFlow {
        fluidMode = .twoSource
      }
    case .selfModulated:
      selectedEffect = .fluidAdvection
      fluidMode = .selfFlow
      analysisSignal = .opticalFlow
      modulationTarget = .feedback
    }
  }

  private func effectCard(_ effect: WorkflowEffect) -> some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Label(effect.rawValue, systemImage: effect.systemImage)
          .font(.subheadline.weight(.semibold))
        Spacer()
        if selectedEffect == effect {
          Image(systemName: "checkmark.circle.fill")
            .foregroundStyle(.tint)
        }
      }

      Text(effect.summary)
        .font(.caption)
        .foregroundStyle(.secondary)
        .lineLimit(3)
        .fixedSize(horizontal: false, vertical: true)
    }
    .padding(12)
    .frame(maxWidth: .infinity, minHeight: 104, alignment: .topLeading)
    .background(
      RoundedRectangle(cornerRadius: 8)
        .fill(selectedEffect == effect ? Color.accentColor.opacity(0.12) : Color.clear)
    )
    .overlay(
      RoundedRectangle(cornerRadius: 8)
        .stroke(selectedEffect == effect ? Color.accentColor : Color.secondary.opacity(0.22), lineWidth: 1)
    )
  }

  private func pathGrid(_ rows: [(String, String)]) -> some View {
    Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 6) {
      ForEach(rows, id: \.0) { row in
        GridRow {
          Text(row.0)
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
          Text(row.1)
            .font(.system(.caption, design: .monospaced))
            .foregroundStyle(.secondary)
            .lineLimit(2)
        }
      }
    }
  }

  private var routeArrow: some View {
    Image(systemName: "arrow.right")
      .foregroundStyle(.secondary)
  }

  private func routeNode(_ label: String, systemImage: String) -> some View {
    HStack(spacing: 7) {
      Image(systemName: systemImage)
      Text(label)
        .lineLimit(1)
        .minimumScaleFactor(0.8)
    }
    .font(.callout)
    .padding(.horizontal, 10)
    .padding(.vertical, 8)
    .background(.background.opacity(0.75), in: RoundedRectangle(cornerRadius: 8))
  }

  private func workflowHeading(_ title: String, systemImage: String) -> some View {
    Label(title, systemImage: systemImage)
      .font(.headline)
  }

  private func workflowBand<Content: View>(@ViewBuilder content: () -> Content) -> some View {
    content()
      .padding(14)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(.quaternary.opacity(0.28), in: RoundedRectangle(cornerRadius: 8))
  }
}

private enum WorkflowSourceMode: String, CaseIterable, Identifiable {
  case twoSource = "A modulates B"
  case selfModulated = "One-video self modulation"

  var id: String { rawValue }

  var routeSource: String {
    switch self {
    case .twoSource:
      return "Source A"
    case .selfModulated:
      return "Source B"
    }
  }
}

private enum WorkflowAnalysisSignal: String, CaseIterable, Identifiable {
  case opticalFlow = "Optical Flow (motion)"
  case luminance = "Luminance"
  case audioRMS = "Audio RMS"
  case spectralCentroid = "Spectral Centroid"
  case grainDescriptors = "Grain Descriptors"

  var id: String { rawValue }

  var shortLabel: String {
    switch self {
    case .opticalFlow:
      return "Optical Flow"
    case .luminance:
      return "Luminance"
    case .audioRMS:
      return "RMS"
    case .spectralCentroid:
      return "Centroid"
    case .grainDescriptors:
      return "Grains"
    }
  }

  var systemImage: String {
    switch self {
    case .opticalFlow:
      return "point.topleft.down.curvedto.point.bottomright.up"
    case .luminance:
      return "sun.max"
    case .audioRMS:
      return "waveform"
    case .spectralCentroid:
      return "slider.horizontal.3"
    case .grainDescriptors:
      return "circle.grid.3x3"
    }
  }
}

private enum WorkflowModulationTarget: String, CaseIterable, Identifiable {
  case displacement = "B displacement field"
  case feedback = "Feedback direction and persistence"
  case grainSelection = "Carrier grain selection"
  case toneTransfer = "Carrier tone bands"
  case audioShape = "Audio gain, pan, or filter"

  var id: String { rawValue }

  var shortLabel: String {
    switch self {
    case .displacement:
      return "Displacement"
    case .feedback:
      return "Feedback"
    case .grainSelection:
      return "Grains"
    case .toneTransfer:
      return "Tone"
    case .audioShape:
      return "Audio"
    }
  }

  var systemImage: String {
    switch self {
    case .displacement:
      return "arrow.up.left.and.arrow.down.right"
    case .feedback:
      return "arrow.triangle.2.circlepath"
    case .grainSelection:
      return "square.grid.3x3"
    case .toneTransfer:
      return "camera.filters"
    case .audioShape:
      return "speaker.wave.2"
    }
  }
}

private enum WorkflowFluidMode: String, CaseIterable, Identifiable {
  case twoSource = "A to B"
  case selfFlow = "Self-flow"
  case procedural = "Field"
  case particles = "Particles"

  var id: String { rawValue }
}

private enum WorkflowEffect: String, CaseIterable, Identifiable {
  case flowDisplace = "Flow Displace"
  case flowFeedback = "Flow Feedback"
  case fluidAdvection = "Fluid Advection"
  case granularMosaic = "Granular Mosaic"
  case datamosh = "Datamosh"
  case videoVocoder = "Video Vocoder"
  case trailCascade = "Trail Cascade"

  var id: String { rawValue }

  var systemImage: String {
    switch self {
    case .flowDisplace:
      return "arrow.up.left.and.arrow.down.right"
    case .flowFeedback:
      return "arrow.triangle.2.circlepath"
    case .fluidAdvection:
      return "wind"
    case .granularMosaic:
      return "circle.grid.3x3.fill"
    case .datamosh:
      return "rectangle.stack.badge.play"
    case .videoVocoder:
      return "camera.filters"
    case .trailCascade:
      return "scribble.variable"
    }
  }

  var summary: String {
    switch self {
    case .flowDisplace:
      return "A's analysis field pushes B's pixels into a deterministic displaced sequence."
    case .flowFeedback:
      return "A's motion repeatedly steers B through a resumable temporal feedback loop."
    case .fluidAdvection:
      return "Continuous dye, self-flow, procedural fields, or particles carried by motion."
    case .granularMosaic:
      return "A selects and rearranges B's temporal grain pool with optional audio weighting."
    case .datamosh:
      return "Controlled flow reuse, macroblock motion, residual melt, and destructive presets."
    case .videoVocoder:
      return "A's tone structure remaps B's visual bands for video-vocoder style transfer."
    case .trailCascade:
      return "B's tiles flow along a faux-fluid field, stamping persistent trails into ribbons."
    }
  }

  var routeDescription: String {
    switch self {
    case .flowDisplace:
      return "A produces a vector field; B is sampled through that field."
    case .flowFeedback:
      return "A's motion drives the next feedback state while B re-enters as carrier structure."
    case .fluidAdvection:
      return "Motion or a procedural field carries B as dye, self-dye, or particle colour."
    case .granularMosaic:
      return "A's visual and audio descriptors choose grains from B's temporal material pool."
    case .datamosh:
      return "A's temporal motion vectors are reused to drag, rot, and reshuffle B."
    case .videoVocoder:
      return "A's tonal distribution gates or matches B's visual tone bands."
    case .trailCascade:
      return "A steady vortex field carries B's tiles; a never-cleared canvas keeps their trails."
    }
  }

  var routeOutputLabel: String {
    switch self {
    case .flowDisplace:
      return "Displaced B"
    case .flowFeedback:
      return "Feedback B"
    case .fluidAdvection:
      return "Advected B"
    case .granularMosaic:
      return "Grain B"
    case .datamosh:
      return "Mosh B"
    case .videoVocoder:
      return "Vocoder B"
    case .trailCascade:
      return "Cascade B"
    }
  }

  var shortActionLabel: String {
    switch self {
    case .flowDisplace:
      return "Displace"
    case .flowFeedback:
      return "Feedback"
    case .fluidAdvection:
      return "Advection"
    case .granularMosaic:
      return "Mosaic"
    case .datamosh:
      return "Datamosh"
    case .videoVocoder:
      return "Vocoder"
    case .trailCascade:
      return "Cascade"
    }
  }

  func applyRoutingDefaults(
    analysisSignal: Binding<WorkflowAnalysisSignal>,
    modulationTarget: Binding<WorkflowModulationTarget>
  ) {
    switch self {
    case .flowDisplace:
      analysisSignal.wrappedValue = .opticalFlow
      modulationTarget.wrappedValue = .displacement
    case .flowFeedback:
      analysisSignal.wrappedValue = .opticalFlow
      modulationTarget.wrappedValue = .feedback
    case .fluidAdvection:
      analysisSignal.wrappedValue = .opticalFlow
      modulationTarget.wrappedValue = .feedback
    case .granularMosaic:
      analysisSignal.wrappedValue = .grainDescriptors
      modulationTarget.wrappedValue = .grainSelection
    case .datamosh:
      analysisSignal.wrappedValue = .opticalFlow
      modulationTarget.wrappedValue = .feedback
    case .videoVocoder:
      analysisSignal.wrappedValue = .luminance
      modulationTarget.wrappedValue = .toneTransfer
    case .trailCascade:
      analysisSignal.wrappedValue = .opticalFlow
      modulationTarget.wrappedValue = .feedback
    }
  }
}
