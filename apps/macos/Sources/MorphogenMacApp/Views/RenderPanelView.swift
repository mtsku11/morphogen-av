import SwiftUI

struct RenderPanelView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      Text("Offline Render")
        .font(.headline)

      HStack(spacing: 16) {
        Picker("Render Quality", selection: $state.renderQuality) {
          ForEach(RenderQualityOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.segmented)

        Picker("Output Format", selection: $state.exportFormat) {
          ForEach(ExportFormatOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 180)
      }

      HStack(spacing: 16) {
        Picker("ProRes FPS", selection: $state.proResFrameRate) {
          ForEach(ProResFrameRateOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 140)

        Picker("ProRes Profile", selection: $state.proResProfile) {
          ForEach(ProResExportProfile.allCases) { profile in
            Text(profile.displayName).tag(profile)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 260)
      }

      Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 8) {
        GridRow {
          Label("Project", systemImage: "doc.text")
          Text(state.projectPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Schema", systemImage: "checkmark.seal")
          Text(state.projectSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Analysis Cache", systemImage: "externaldrive")
          Text("No cache entries yet")
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Render Queue", systemImage: "list.bullet.rectangle")
          Text(state.renderQueueSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Preview Frame", systemImage: "rectangle.on.rectangle")
          Text(state.previewProbeSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("ProRes Export", systemImage: "film.stack")
          Text(state.proResPlanSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("ProRes Output", systemImage: "film")
          Text(state.proResExportSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Source A Frames", systemImage: "a.square")
          Text(state.frameSequenceModulatorPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Source B Frames", systemImage: "b.square")
          Text(state.frameSequenceCarrierPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Sequence Output Root", systemImage: "rectangle.stack.badge.play")
          Text(state.frameSequenceOutputPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Media Proxy Output", systemImage: "externaldrive.badge.plus")
          Text(state.mediaProxyOutputPath)
            .lineLimit(2)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Media Proxy Ingest", systemImage: "arrow.down.to.line.compact")
          Text(state.mediaProxySummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Two-Source Render", systemImage: "point.3.connected.trianglepath.dotted")
          Text(state.frameSequenceSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Flow Feedback", systemImage: "arrow.triangle.2.circlepath")
          Text(state.feedbackSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
        GridRow {
          Label("Grain Pool", systemImage: "circle.grid.3x3")
          Text(state.granularPoolSummary)
            .lineLimit(3)
            .foregroundStyle(.secondary)
        }
      }

      VStack(alignment: .leading, spacing: 8) {
        HStack {
          Button {
            state.createTestProject()
          } label: {
            Label("Create Test Project", systemImage: "doc.badge.plus")
          }

          Button {
            state.openProject()
          } label: {
            Label("Open Project", systemImage: "folder.badge.gearshape")
          }

          Button {
            state.probeSelectedSources()
          } label: {
            Label("Probe Sources", systemImage: "waveform.path.ecg.rectangle")
          }

          Button {
            state.probePreviewFrames()
          } label: {
            Label("Probe Preview Frames", systemImage: "rectangle.on.rectangle")
          }
        }

        HStack {
          Button {
            state.runCpuReferenceRender()
          } label: {
            Label("Run CPU Reference Render", systemImage: "play.circle")
          }

          Button {
            state.runQueuedTestRender()
          } label: {
            Label("Run Queue Test", systemImage: "list.bullet.rectangle")
          }
        }

        HStack {
          Button {
            state.chooseMediaProxyOutputDirectory()
          } label: {
            Label("Proxy Output", systemImage: "folder.badge.plus")
          }

          Button {
            state.extractSelectedSourceProxies()
          } label: {
            Label("Extract Source Proxies", systemImage: "arrow.down.to.line.compact")
          }
        }

        HStack(spacing: 16) {
          Stepper(value: $state.mediaProxyFrameRate, in: 1...60, step: 1) {
            Text("Proxy FPS \(state.mediaProxyFrameRate, specifier: "%.0f")")
          }
          .frame(width: 140, alignment: .leading)

          Stepper(value: $state.mediaProxyMaxFrames, in: 1...600, step: 1) {
            Text("Proxy Max \(state.mediaProxyMaxFrames)")
          }
          .frame(width: 170, alignment: .leading)
        }

        HStack {
          Button {
            state.chooseFrameSequenceModulatorDirectory()
          } label: {
            Label("Source A Frames", systemImage: "a.square")
          }

          Button {
            state.chooseFrameSequenceCarrierDirectory()
          } label: {
            Label("Source B Frames", systemImage: "b.square")
          }

          Button {
            state.chooseFrameSequenceOutputDirectory()
          } label: {
            Label("Sequence Output", systemImage: "folder.badge.plus")
          }
        }

        HStack(spacing: 16) {
          Stepper(value: $state.frameSequenceAmount, in: 0...64, step: 1) {
            Text("Amount \(state.frameSequenceAmount, specifier: "%.0f")")
          }
          .frame(width: 140, alignment: .leading)

          Stepper(value: $state.frameSequenceMaxFrames, in: 1...600, step: 1) {
            Text("Max Frames \(state.frameSequenceMaxFrames)")
          }
          .frame(width: 170, alignment: .leading)

          Toggle("Flow Cache", isOn: $state.frameSequenceWritesFlowCache)
            .toggleStyle(.checkbox)
            .frame(width: 120, alignment: .leading)
        }

        HStack {
          Button {
            state.runTwoSourceFrameSequenceRender()
          } label: {
            Label("Run Two-Source Sequence", systemImage: "play.rectangle.on.rectangle")
          }

          Button {
            state.exportLastFrameSequenceProResMovie()
          } label: {
            Label("Export Sequence ProRes MOV", systemImage: "film.badge.plus")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Temporal Flow Feedback")
            .font(.subheadline.weight(.semibold))

          HStack(spacing: 16) {
            Picker("Preset", selection: $state.feedbackPreset) {
              ForEach(FeedbackPresetOption.allCases) { preset in
                Text(preset.rawValue).tag(preset)
              }
            }
            .pickerStyle(.menu)
            .frame(width: 190)

            Picker("Flow Source", selection: $state.feedbackFlowSource) {
              ForEach(FeedbackFlowSourceOption.allCases) { source in
                Text(source.rawValue).tag(source)
              }
            }
            .pickerStyle(.menu)
            .frame(width: 180)

            Picker("Backend", selection: $state.feedbackBackend) {
              ForEach(FeedbackRenderBackendOption.allCases) { backend in
                Text(backend.rawValue).tag(backend)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)

            Picker("Iterations", selection: $state.feedbackIterations) {
              Text("1").tag(1)
            }
            .pickerStyle(.menu)
            .frame(width: 100)

            Picker("Output", selection: $state.feedbackOutputBitDepth) {
              ForEach(FeedbackOutputBitDepthOption.allCases) { bitDepth in
                Text(bitDepth.rawValue).tag(bitDepth)
              }
            }
            .pickerStyle(.menu)
            .frame(width: 130)

            Picker("Temporal Samples", selection: $state.feedbackTemporalSupersampling) {
              Text("1x").tag(1)
              Text("2x").tag(2)
              Text("4x").tag(4)
            }
            .pickerStyle(.segmented)
            .frame(width: 170)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.feedbackCarrierAmount, in: 0...8, step: 0.25) {
              Text("Carrier \(state.feedbackCarrierAmount, specifier: "%.2f")")
            }
            .frame(width: 155, alignment: .leading)

            Stepper(value: $state.feedbackAmount, in: 0...12, step: 0.25) {
              Text("Feedback \(state.feedbackAmount, specifier: "%.2f")")
            }
            .frame(width: 165, alignment: .leading)

            Stepper(value: $state.feedbackMix, in: 0...1, step: 0.01) {
              Text("Mix \(state.feedbackMix, specifier: "%.2f")")
            }
            .frame(width: 125, alignment: .leading)

            Stepper(value: $state.feedbackDecay, in: 0...1, step: 0.001) {
              Text("Decay \(state.feedbackDecay, specifier: "%.3f")")
            }
            .frame(width: 145, alignment: .leading)

            Stepper(value: $state.feedbackStructureMix, in: 0...2, step: 0.05) {
              Text("Structure \(state.feedbackStructureMix, specifier: "%.2f")")
            }
            .frame(width: 165, alignment: .leading)
          }

          HStack(spacing: 16) {
            Toggle("Write Flow Cache", isOn: $state.feedbackWritesFlowCache)
              .toggleStyle(.checkbox)

            Toggle("Reset Feedback", isOn: $state.feedbackResetEnabled)
              .toggleStyle(.checkbox)

            Stepper(value: $state.feedbackResetAtFrame, in: 0...state.frameSequenceMaxFrames - 1) {
              Text("Reset Frame \(state.feedbackResetAtFrame)")
            }
            .disabled(!state.feedbackResetEnabled)
            .frame(width: 160, alignment: .leading)

            Button {
              state.runFlowFeedbackSequenceRender()
            } label: {
              Label("Run Flow Feedback", systemImage: "arrow.triangle.2.circlepath")
            }
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Granular Mosaic — Temporal Pool (Joint-AV)")
            .font(.subheadline.weight(.semibold))

          HStack(spacing: 16) {
            Stepper(value: $state.granularPoolGrainSize, in: 4...256, step: 4) {
              Text("Grain \(state.granularPoolGrainSize)px")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.granularPoolRearrangement, in: 0...1, step: 0.05) {
              Text("Rearrange \(state.granularPoolRearrangement, specifier: "%.2f")")
            }
            .frame(width: 180, alignment: .leading)

            Stepper(value: $state.granularPoolVariation, in: 0...1, step: 0.05) {
              Text("Variation \(state.granularPoolVariation, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.granularPoolSeed, in: 0...9999, step: 1) {
              Text("Seed \(state.granularPoolSeed)")
            }
            .frame(width: 140, alignment: .leading)

            Stepper(value: $state.granularPoolAudioWeight, in: 0...8, step: 0.25) {
              Text("Audio Weight \(state.granularPoolAudioWeight, specifier: "%.2f")")
            }
            .frame(width: 200, alignment: .leading)
            .disabled(!state.granularPoolAudioWeighted)

            Stepper(value: $state.granularPoolTextureWeight, in: 0...8, step: 0.1) {
              Text("Texture Weight \(state.granularPoolTextureWeight, specifier: "%.1f")")
            }
            .frame(width: 200, alignment: .leading)

            Toggle("Audio-Weighted (RMS)", isOn: $state.granularPoolAudioWeighted)
              .toggleStyle(.checkbox)

            Toggle("Spectral Centroid (k=2)", isOn: $state.granularPoolCentroidEnabled)
              .toggleStyle(.checkbox)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.granularPoolWindow, in: 0...512, step: 1) {
              Text(state.granularPoolWindow == 0
                ? "Pool Window: whole clip"
                : "Pool Window \(state.granularPoolWindow)")
            }
            .frame(width: 230, alignment: .leading)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.granularPoolAntiRepeatWeight, in: 0...8, step: 0.1) {
              Text("Anti-Repeat \(state.granularPoolAntiRepeatWeight, specifier: "%.1f")")
            }
            .frame(width: 190, alignment: .leading)

            Stepper(value: $state.granularPoolAntiRepeatCooldown, in: 1...64, step: 1) {
              Text("Cooldown \(state.granularPoolAntiRepeatCooldown)")
            }
            .frame(width: 170, alignment: .leading)
            .disabled(state.granularPoolAntiRepeatWeight <= 0)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.granularPoolCoherenceWeight, in: 0...8, step: 0.1) {
              Text("Coherence \(state.granularPoolCoherenceWeight, specifier: "%.1f")")
            }
            .frame(width: 190, alignment: .leading)

            Stepper(value: $state.granularPoolCoherenceReach, in: 1...64, step: 1) {
              Text("Reach \(state.granularPoolCoherenceReach)")
            }
            .frame(width: 150, alignment: .leading)
            .disabled(
              state.granularPoolCoherenceWeight <= 0
                && state.granularPoolSpatialCoherenceWeight <= 0
            )
          }

          HStack(spacing: 16) {
            Stepper(value: $state.granularPoolSpatialCoherenceWeight, in: 0...8, step: 0.1) {
              Text("Spatial \(state.granularPoolSpatialCoherenceWeight, specifier: "%.1f")")
            }
            .frame(width: 190, alignment: .leading)
            .help("Rewards grain-origin continuity within a frame; shares the coherence Reach.")
          }

          HStack(spacing: 16) {
            Picker("Backend", selection: $state.granularPoolBackend) {
              ForEach(FeedbackRenderBackendOption.allCases) { backend in
                Text(backend.rawValue).tag(backend)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)

            Button {
              state.runGranularMosaicPoolSequenceRender()
            } label: {
              Label("Run Grain Pool", systemImage: "circle.grid.3x3.fill")
            }
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Video Vocoder — Tonal Routing")
            .font(.subheadline.weight(.semibold))

          Picker("Mode", selection: $state.vocoderMode) {
            ForEach(VideoVocoderModeOption.allCases) { mode in
              Text(mode.rawValue).tag(mode)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 360)

          HStack(spacing: 16) {
            Stepper(value: $state.vocoderBands, in: 1...64, step: 1) {
              Text("Bands \(state.vocoderBands)")
            }
            .frame(width: 150, alignment: .leading)
            .disabled(state.vocoderMode == .match)
            .help("Luma band count (Gain mode only; Match mode uses a 256-level tone map).")

            Stepper(value: $state.vocoderAmount, in: 0...4, step: 0.05) {
              Text("Amount \(state.vocoderAmount, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
            .help("0 = Source B passthrough; 1 = full routing.")
          }

          Text(state.vocoderSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          HStack(spacing: 16) {
            Picker("Backend", selection: $state.vocoderBackend) {
              ForEach(FeedbackRenderBackendOption.allCases) { backend in
                Text(backend.rawValue).tag(backend)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)
            .disabled(state.vocoderMode == .gain)
            .help("Metal is parity-gated and available in Match mode; Gain mode renders on the CPU.")

            Button {
              state.runVideoVocoderSequenceRender()
            } label: {
              Label("Run Vocoder", systemImage: "slider.horizontal.3")
            }
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Spectral Audio Cross-Synthesis")
            .font(.subheadline.weight(.semibold))

          Picker("Mode", selection: $state.crossSynthMode) {
            ForEach(CrossSynthModeOption.allCases) { mode in
              Text(mode.rawValue).tag(mode)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 360)
          .help("Gain: A's RMS envelope drives B's amplitude. Filter: A's spectral centroid sweeps a one-pole cutoff on B.")

          HStack(spacing: 16) {
            Button {
              state.chooseCrossSynthModulatorWAV()
            } label: {
              Label("Source A WAV", systemImage: "waveform")
            }
            Button {
              state.chooseCrossSynthCarrierWAV()
            } label: {
              Label("Source B WAV", systemImage: "waveform")
            }
            Button {
              state.chooseCrossSynthOutputDirectory()
            } label: {
              Label("Output Dir", systemImage: "folder")
            }
          }

          HStack(spacing: 16) {
            Stepper(value: $state.crossSynthAmount, in: 0...1, step: 0.05) {
              Text("Amount \(state.crossSynthAmount, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
            .help("0 = Source B passthrough; 1 = full shaping.")

            Picker("Filter", selection: $state.crossSynthFilterType) {
              ForEach(CrossSynthFilterTypeOption.allCases) { type in
                Text(type.rawValue).tag(type)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 200)
            .disabled(state.crossSynthMode == .gain)
            .help("One-pole response (Filter mode only).")
          }

          Text(state.crossSynthSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runSpectralCrossSynthRender()
          } label: {
            Label("Run Cross-Synth", systemImage: "slider.horizontal.3")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Audio Impulse Convolution")
            .font(.subheadline.weight(.semibold))
            .help("Convolve Source B (carrier) with Source A's L1-normalized impulse response. amount 0 = passthrough; the wet tail extends the output.")

          HStack(spacing: 16) {
            Button {
              state.chooseImpulseConvModulatorWAV()
            } label: {
              Label("Source A IR", systemImage: "waveform")
            }
            Button {
              state.chooseImpulseConvCarrierWAV()
            } label: {
              Label("Source B WAV", systemImage: "waveform")
            }
            Button {
              state.chooseImpulseConvOutputDirectory()
            } label: {
              Label("Output Dir", systemImage: "folder")
            }
          }

          HStack(spacing: 16) {
            Stepper(value: $state.impulseConvAmount, in: 0...1, step: 0.05) {
              Text("Amount \(state.impulseConvAmount, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
            .help("0 = Source B passthrough; 1 = full wet convolution.")

            Stepper(value: $state.impulseConvMaxSamples, in: 0...192_000, step: 1024) {
              Text("Max IR \(state.impulseConvMaxSamples == 0 ? "full" : String(state.impulseConvMaxSamples))")
            }
            .frame(width: 200, alignment: .leading)
            .help("Truncate the impulse response to its head (samples); 0 = use the whole IR.")
          }

          HStack(spacing: 16) {
            Toggle("FFT method (HQ)", isOn: $state.impulseConvUseFFT)
              .help("Frequency-domain convolution for long IRs; gated against the direct path.")

            Toggle("Resample IR", isOn: $state.impulseConvResample)
              .help("Resample A's IR to B's sample rate (Lanczos) instead of erroring on a mismatch.")
          }

          Text(state.impulseConvSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runAudioImpulseConvolutionRender()
          } label: {
            Label("Run Impulse Convolution", systemImage: "slider.horizontal.3")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Audio-to-Video Descriptor Routing")
            .font(.subheadline.weight(.semibold))
            .help("Source A's RMS envelope drives the per-frame displacement amount applied to Source B's frames.")

          HStack(spacing: 16) {
            Button {
              state.chooseAudioRouteModulatorWAV()
            } label: {
              Label("Source A WAV", systemImage: "waveform")
            }
            Button {
              state.chooseAudioRouteCarrierDirectory()
            } label: {
              Label("Source B Frames", systemImage: "photo.on.rectangle")
            }
            Button {
              state.chooseAudioRouteOutputDirectory()
            } label: {
              Label("Output Dir", systemImage: "folder")
            }
          }

          HStack(spacing: 16) {
            Stepper(value: $state.audioRouteAmount, in: 0...4, step: 0.1) {
              Text("Amount \(state.audioRouteAmount, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
            .help("0 = Source B passthrough; scales the loudest-frame displacement.")

            Stepper(value: $state.audioRouteShiftX, in: -128...128, step: 1) {
              Text("Shift X \(state.audioRouteShiftX, specifier: "%.0f")")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.audioRouteShiftY, in: -128...128, step: 1) {
              Text("Shift Y \(state.audioRouteShiftY, specifier: "%.0f")")
            }
            .frame(width: 150, alignment: .leading)
          }

          Picker("Backend", selection: $state.audioRouteBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { backend in
              Text(backend.rawValue).tag(backend)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 200)
          .help("Metal is gated per-frame against the CPU reference.")

          Text(state.audioRouteSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runAudioVideoRouteRender()
          } label: {
            Label("Run Audio→Video Route", systemImage: "slider.horizontal.3")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Convolutional AV Blending")
            .font(.subheadline.weight(.semibold))
            .help("Each Source A frame supplies a normalized KxK luma kernel that Source B's frame is convolved with.")

          HStack(spacing: 16) {
            Button {
              state.chooseConvBlendModulatorDirectory()
            } label: {
              Label("Source A Frames", systemImage: "photo.on.rectangle")
            }
            Button {
              state.chooseConvBlendCarrierDirectory()
            } label: {
              Label("Source B Frames", systemImage: "photo.on.rectangle.angled")
            }
            Button {
              state.chooseConvBlendOutputDirectory()
            } label: {
              Label("Output Dir", systemImage: "folder")
            }
          }

          HStack(spacing: 16) {
            Stepper(value: $state.convBlendKernelSize, in: 1...15, step: 2) {
              Text("Kernel \(state.convBlendKernelSize)×\(state.convBlendKernelSize)")
            }
            .frame(width: 170, alignment: .leading)
            .help("Odd kernel edge length; larger spreads the blend wider.")

            Stepper(value: $state.convBlendAmount, in: 0...1, step: 0.05) {
              Text("Amount \(state.convBlendAmount, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
            .help("0 = Source B passthrough; 1 = fully convolved.")
          }

          Picker("Backend", selection: $state.convBlendBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { backend in
              Text(backend.rawValue).tag(backend)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 200)
          .help("Metal is gated per-frame against the CPU reference.")

          Text(state.convBlendSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runConvolutionalBlendRender()
          } label: {
            Label("Run Convolution Blend", systemImage: "square.grid.3x3.fill")
          }
        }

        HStack {
          Button {
            state.checkProResExportPlan()
          } label: {
            Label("Check ProRes", systemImage: "film")
          }

          Button {
            state.exportRenderQueueProResMovie()
          } label: {
            Label("Export Queue ProRes MOV", systemImage: "film.badge.plus")
          }

          Button {
            state.exportProResMovie()
          } label: {
            Label("Export Frame Directory MOV", systemImage: "folder.badge.plus")
          }
        }
      }

      Text(state.statusMessage)
        .font(.caption)
        .foregroundStyle(.secondary)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 8))
  }
}
