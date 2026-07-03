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
          Label("Fluid / Advection", systemImage: "wind")
          Text(state.fluidAdvectionSummary)
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

            LabeledContent("Feedback Passes") {
              Text("1")
                .foregroundStyle(.secondary)
            }
            .frame(width: 130)

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

          ModulationSlotRow(
            label: "Carrier",
            source: $state.feedbackModCarrierAmountSource,
            scale: $state.feedbackModCarrierAmountScale,
            offset: $state.feedbackModCarrierAmountOffset,
            samplingOverride: $state.feedbackModCarrierAmountSamplingOverride,
            scaleRange: -16...16, scaleStep: 0.25, offsetRange: -16...16, offsetStep: 0.25,
            modulator: $state.feedbackModCarrierAmountModulator,
            modulatorNames: state.feedbackDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Feedback",
            source: $state.feedbackModAmountSource,
            scale: $state.feedbackModAmountScale,
            offset: $state.feedbackModAmountOffset,
            samplingOverride: $state.feedbackModAmountSamplingOverride,
            scaleRange: -16...16, scaleStep: 0.25, offsetRange: -16...16, offsetStep: 0.25,
            modulator: $state.feedbackModAmountModulator,
            modulatorNames: state.feedbackDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Mix",
            source: $state.feedbackModMixSource,
            scale: $state.feedbackModMixScale,
            offset: $state.feedbackModMixOffset,
            samplingOverride: $state.feedbackModMixSamplingOverride,
            modulator: $state.feedbackModMixModulator,
            modulatorNames: state.feedbackDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Decay",
            source: $state.feedbackModDecaySource,
            scale: $state.feedbackModDecayScale,
            offset: $state.feedbackModDecayOffset,
            samplingOverride: $state.feedbackModDecaySamplingOverride,
            modulator: $state.feedbackModDecayModulator,
            modulatorNames: state.feedbackDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Structure",
            source: $state.feedbackModStructureMixSource,
            scale: $state.feedbackModStructureMixScale,
            offset: $state.feedbackModStructureMixOffset,
            samplingOverride: $state.feedbackModStructureMixSamplingOverride,
            modulator: $state.feedbackModStructureMixModulator,
            modulatorNames: state.feedbackDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [
              state.feedbackModCarrierAmountSource, state.feedbackModAmountSource,
              state.feedbackModMixSource, state.feedbackModDecaySource,
              state.feedbackModStructureMixSource
            ],
            audioURL: state.feedbackModulatorAudioURL,
            framesURL: state.feedbackModulatorFramesURL,
            sampling: $state.feedbackModSampling,
            chooseAudio: { state.chooseFeedbackModulatorWAV() },
            chooseFrames: { state.chooseFeedbackModulatorFrames() }
          )

          NamedModulatorsSection(
            modulators: $state.feedbackNamedModulators,
            onAdd: { state.addFeedbackNamedModulator() },
            onRemove: { state.removeFeedbackNamedModulator(id: $0) },
            chooseAudio: { state.chooseFeedbackNamedModulatorWAV(id: $0) },
            chooseFrames: { state.chooseFeedbackNamedModulatorFrames(id: $0) }
          )

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
          Text("Fluid / Advection Queue")
            .font(.subheadline.weight(.semibold))

          HStack(spacing: 16) {
            Picker("Backend", selection: $state.fluidBackend) {
              ForEach(FeedbackRenderBackendOption.allCases) { backend in
                Text(backend.rawValue).tag(backend)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)

            Stepper(value: $state.fluidSeed, in: 0...9999, step: 1) {
              Text("Seed \(state.fluidSeed)")
            }
            .frame(width: 130, alignment: .leading)

            Stepper(value: $state.fluidReinject, in: 0...1, step: 0.01) {
              Text("Reinject \(state.fluidReinject, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.fluidMotionAdvect, in: 0...8, step: 0.25) {
              Text("Flow Advect \(state.fluidMotionAdvect, specifier: "%.2f")")
            }
            .frame(width: 185, alignment: .leading)

            Stepper(value: $state.fluidProceduralAdvect, in: 0...48, step: 1) {
              Text("Procedural Advect \(state.fluidProceduralAdvect, specifier: "%.0f")")
            }
            .frame(width: 225, alignment: .leading)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.fluidTurbulenceScale, in: 0...0.05, step: 0.001) {
              Text("Turb Scale \(state.fluidTurbulenceScale, specifier: "%.3f")")
            }
            .frame(width: 185, alignment: .leading)

            Stepper(value: $state.fluidTurbulenceSpeed, in: 0...0.5, step: 0.01) {
              Text("Turb Speed \(state.fluidTurbulenceSpeed, specifier: "%.2f")")
            }
            .frame(width: 185, alignment: .leading)

            Stepper(value: $state.fluidDetail, in: 0...1, step: 0.05) {
              Text("Detail \(state.fluidDetail, specifier: "%.2f")")
            }
            .frame(width: 140, alignment: .leading)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.fieldParticleSpacing, in: 1...64, step: 1) {
              Text("Spacing \(state.fieldParticleSpacing)")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.fieldParticleSize, in: 1...64, step: 1) {
              Text("Particle \(state.fieldParticleSize)")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.fieldParticleAdvect, in: 0...48, step: 1) {
              Text("Particle Advect \(state.fieldParticleAdvect, specifier: "%.0f")")
            }
            .frame(width: 200, alignment: .leading)

            Toggle("Live Colour", isOn: $state.fieldParticleLiveColour)
              .toggleStyle(.checkbox)
          }

          // Procedural Fluid consumes all six slots; A-to-B Fluid and
          // Self-Flow consume only Flow Advect + Reinject (their commands
          // have no turbulence targets). Particles has no routes yet.
          ModulationSlotRow(
            label: "Proc Advect",
            source: $state.fluidModProceduralAdvectSource,
            scale: $state.fluidModProceduralAdvectScale,
            offset: $state.fluidModProceduralAdvectOffset,
            samplingOverride: $state.fluidModProceduralAdvectSamplingOverride,
            scaleRange: -48...48, scaleStep: 1, offsetRange: -48...48, offsetStep: 1,
            modulator: $state.fluidModProceduralAdvectModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Flow Advect",
            source: $state.fluidModMotionAdvectSource,
            scale: $state.fluidModMotionAdvectScale,
            offset: $state.fluidModMotionAdvectOffset,
            samplingOverride: $state.fluidModMotionAdvectSamplingOverride,
            scaleRange: -8...8, scaleStep: 0.25, offsetRange: -8...8, offsetStep: 0.25,
            modulator: $state.fluidModMotionAdvectModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Turb Scale",
            source: $state.fluidModTurbulenceScaleSource,
            scale: $state.fluidModTurbulenceScaleScale,
            offset: $state.fluidModTurbulenceScaleOffset,
            samplingOverride: $state.fluidModTurbulenceScaleSamplingOverride,
            scaleRange: -0.05...0.05, scaleStep: 0.002, offsetRange: -0.05...0.05, offsetStep: 0.002,
            modulator: $state.fluidModTurbulenceScaleModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Turb Speed",
            source: $state.fluidModTurbulenceSpeedSource,
            scale: $state.fluidModTurbulenceSpeedScale,
            offset: $state.fluidModTurbulenceSpeedOffset,
            samplingOverride: $state.fluidModTurbulenceSpeedSamplingOverride,
            scaleRange: -0.5...0.5, scaleStep: 0.01, offsetRange: -0.5...0.5, offsetStep: 0.01,
            modulator: $state.fluidModTurbulenceSpeedModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Detail",
            source: $state.fluidModDetailSource,
            scale: $state.fluidModDetailScale,
            offset: $state.fluidModDetailOffset,
            samplingOverride: $state.fluidModDetailSamplingOverride,
            modulator: $state.fluidModDetailModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Reinject",
            source: $state.fluidModReinjectSource,
            scale: $state.fluidModReinjectScale,
            offset: $state.fluidModReinjectOffset,
            samplingOverride: $state.fluidModReinjectSamplingOverride,
            modulator: $state.fluidModReinjectModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [
              state.fluidModProceduralAdvectSource, state.fluidModMotionAdvectSource,
              state.fluidModTurbulenceScaleSource, state.fluidModTurbulenceSpeedSource,
              state.fluidModDetailSource, state.fluidModReinjectSource
            ],
            audioURL: state.fluidModulatorAudioURL,
            framesURL: state.fluidModulatorFramesURL,
            sampling: $state.fluidModSampling,
            chooseAudio: { state.chooseFluidModulatorWAV() },
            chooseFrames: { state.chooseFluidModulatorFrames() }
          )

          NamedModulatorsSection(
            modulators: $state.fluidNamedModulators,
            onAdd: { state.addFluidNamedModulator() },
            onRemove: { state.removeFluidNamedModulator(id: $0) },
            chooseAudio: { state.chooseFluidNamedModulatorWAV(id: $0) },
            chooseFrames: { state.chooseFluidNamedModulatorFrames(id: $0) }
          )

          Text(state.fluidAdvectionSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          HStack(spacing: 16) {
            Button {
              state.runProceduralFluidAdvectSequenceRender()
            } label: {
              Label("Run Procedural Fluid", systemImage: "wind")
            }

            Button {
              state.runTwoSourceFluidAdvectSequenceRender()
            } label: {
              Label("Run A-to-B Fluid", systemImage: "arrow.triangle.merge")
            }

            Button {
              state.runOpticalFlowAdvectSequenceRender()
            } label: {
              Label("Run Self-Flow", systemImage: "point.topleft.down.curvedto.point.bottomright.up")
            }

            Button {
              state.runFieldParticlesSequenceRender()
            } label: {
              Label("Run Particles", systemImage: "sparkles")
            }
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Cascade Collage — Scribbled-Edge Tile Cascade")
            .font(.subheadline.weight(.semibold))

          HStack(spacing: 16) {
            Stepper(value: $state.cascadeCollageTileScale, in: 0.3...2.0, step: 0.05) {
              Text("Tile Scale \(state.cascadeCollageTileScale, specifier: "%.2f")")
            }
            .frame(width: 175, alignment: .leading)

            Stepper(value: $state.cascadeCollageDetailTiles, in: 0...4, step: 1) {
              Text("Detail Tiles \(state.cascadeCollageDetailTiles)")
            }
            .frame(width: 160, alignment: .leading)

            Stepper(value: $state.cascadeCollageHueRotate, in: 0...1, step: 0.02) {
              Text("Hue Rotate \(state.cascadeCollageHueRotate, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)

            Stepper(value: $state.cascadeCollageSeed, in: 0...9999, step: 1) {
              Text("Seed \(state.cascadeCollageSeed)")
            }
            .frame(width: 130, alignment: .leading)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.cascadeCollageScribAmpScale, in: 0...2, step: 0.05) {
              Text("Scribble \(state.cascadeCollageScribAmpScale, specifier: "%.2f")")
            }
            .frame(width: 160, alignment: .leading)

            Stepper(value: $state.cascadeCollageEdgeStrength, in: 0...1, step: 0.05) {
              Text("Edge Strength \(state.cascadeCollageEdgeStrength, specifier: "%.2f")")
            }
            .frame(width: 190, alignment: .leading)

            Stepper(value: $state.cascadeCollageFaceStrength, in: 0...1, step: 0.05) {
              Text("Face Strength \(state.cascadeCollageFaceStrength, specifier: "%.2f")")
            }
            .frame(width: 190, alignment: .leading)

            Stepper(value: $state.cascadeCollageEdgeDetect, in: 0...2, step: 0.05) {
              Text("Edge Detect \(state.cascadeCollageEdgeDetect, specifier: "%.2f")")
            }
            .frame(width: 175, alignment: .leading)
          }

          HStack(spacing: 16) {
            Picker("Block Blend", selection: $state.cascadeCollageBlockBlend) {
              ForEach(CascadeCollageBlendOption.allCases) { mode in
                Text(mode.rawValue).tag(mode)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 320)

            Stepper(value: $state.cascadeCollageBlockOpacity, in: 0...1, step: 0.05) {
              Text("Block Opacity \(state.cascadeCollageBlockOpacity, specifier: "%.2f")")
            }
            .frame(width: 195, alignment: .leading)
          }

          Text(state.cascadeCollageSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runCascadeCollageSequenceRender()
          } label: {
            Label("Run Cascade Collage", systemImage: "square.stack.3d.up")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Retro Static — Scanline-Filter Misread Glitch")
            .font(.subheadline.weight(.semibold))

          HStack(spacing: 16) {
            Picker("Backend", selection: $state.retroStaticBackend) {
              ForEach(FeedbackRenderBackendOption.allCases) { backend in
                Text(backend.rawValue).tag(backend)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)

            Picker("Filter", selection: $state.retroStaticFilter) {
              ForEach(RetroStaticFilterOption.allCases) { filter in
                Text(filter.rawValue).tag(filter)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 300)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.retroStaticRealBpp, in: 1...8, step: 1) {
              Text("Real BPP \(state.retroStaticRealBpp)")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.retroStaticAssumedBpp, in: 1...8, step: 1) {
              Text("Assumed BPP \(state.retroStaticAssumedBpp)")
            }
            .frame(width: 170, alignment: .leading)

            Stepper(value: $state.retroStaticStrength, in: 0...1, step: 0.05) {
              Text("Strength \(state.retroStaticStrength, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
          }

          ModulationSlotRow(
            label: "Strength",
            source: $state.retroStaticModStrengthSource,
            scale: $state.retroStaticModStrengthScale,
            offset: $state.retroStaticModStrengthOffset,
            samplingOverride: $state.retroStaticModStrengthSamplingOverride,
            modulator: $state.retroStaticModStrengthModulator,
            modulatorNames: state.retroStaticDeclaredModulatorNames
          )

          EnumModulationSlotRow(
            label: "Filter",
            source: $state.retroStaticModFilterSource,
            from: $state.retroStaticModFilterFrom,
            to: $state.retroStaticModFilterTo,
            samplingOverride: $state.retroStaticModFilterSamplingOverride,
            modulator: $state.retroStaticModFilterModulator,
            modulatorNames: state.retroStaticDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [state.retroStaticModStrengthSource, state.retroStaticModFilterSource],
            audioURL: state.retroStaticModulatorAudioURL,
            framesURL: state.retroStaticModulatorFramesURL,
            sampling: $state.retroStaticModSampling,
            chooseAudio: { state.chooseRetroStaticModulatorWAV() },
            chooseFrames: { state.chooseRetroStaticModulatorFrames() }
          )

          NamedModulatorsSection(
            modulators: $state.retroStaticNamedModulators,
            onAdd: { state.addRetroStaticNamedModulator() },
            onRemove: { state.removeRetroStaticNamedModulator(id: $0) },
            chooseAudio: { state.chooseRetroStaticNamedModulatorWAV(id: $0) },
            chooseFrames: { state.chooseRetroStaticNamedModulatorFrames(id: $0) }
          )

          Text(state.retroStaticSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runRetroStaticSequenceRender()
          } label: {
            Label("Run Retro Static", systemImage: "tv")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Channel Shift — RGB Split (+ A-Flow Rows)")
            .font(.subheadline.weight(.semibold))

          HStack(spacing: 16) {
            Picker("Backend", selection: $state.channelShiftBackend) {
              ForEach(FeedbackRenderBackendOption.allCases) { backend in
                Text(backend.rawValue).tag(backend)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)
            .help("Metal covers constant offsets and is parity-gated. Flow-driven mode (Flow Gain ≠ 0) is CPU-only.")
          }

          HStack(spacing: 16) {
            Stepper(value: $state.channelShiftRX, in: -64...64, step: 1) {
              Text("R X \(state.channelShiftRX, specifier: "%.0f")px")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.channelShiftGX, in: -64...64, step: 1) {
              Text("G X \(state.channelShiftGX, specifier: "%.0f")px")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.channelShiftBX, in: -64...64, step: 1) {
              Text("B X \(state.channelShiftBX, specifier: "%.0f")px")
            }
            .frame(width: 150, alignment: .leading)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.channelShiftRY, in: -64...64, step: 1) {
              Text("R Y \(state.channelShiftRY, specifier: "%.0f")px")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.channelShiftGY, in: -64...64, step: 1) {
              Text("G Y \(state.channelShiftGY, specifier: "%.0f")px")
            }
            .frame(width: 150, alignment: .leading)

            Stepper(value: $state.channelShiftBY, in: -64...64, step: 1) {
              Text("B Y \(state.channelShiftBY, specifier: "%.0f")px")
            }
            .frame(width: 150, alignment: .leading)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.channelShiftFlowGain, in: -16...16, step: 0.5) {
              Text("Flow Gain \(state.channelShiftFlowGain, specifier: "%.1f")")
            }
            .frame(width: 170, alignment: .leading)
            .help("A-flow per-row X shift gain; 0 keeps constant-offset mode. Needs Source A frames and the CPU backend.")

            if state.channelShiftFlowGain != 0 {
              Stepper(value: $state.channelShiftFlowRadius, in: 1...8, step: 1) {
                Text("Flow Radius \(state.channelShiftFlowRadius)")
              }
              .frame(width: 180, alignment: .leading)
              .help("Lucas-Kanade window half-radius for the A-flow rows.")
            }
          }

          ModulationSlotRow(
            label: "R X",
            source: $state.channelShiftModRXSource,
            scale: $state.channelShiftModRXScale,
            offset: $state.channelShiftModRXOffset,
            samplingOverride: $state.channelShiftModRXSamplingOverride,
            scaleRange: -64...64, scaleStep: 1, offsetRange: -64...64, offsetStep: 1,
            modulator: $state.channelShiftModRXModulator,
            modulatorNames: state.channelShiftDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "R Y",
            source: $state.channelShiftModRYSource,
            scale: $state.channelShiftModRYScale,
            offset: $state.channelShiftModRYOffset,
            samplingOverride: $state.channelShiftModRYSamplingOverride,
            scaleRange: -64...64, scaleStep: 1, offsetRange: -64...64, offsetStep: 1,
            modulator: $state.channelShiftModRYModulator,
            modulatorNames: state.channelShiftDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "G X",
            source: $state.channelShiftModGXSource,
            scale: $state.channelShiftModGXScale,
            offset: $state.channelShiftModGXOffset,
            samplingOverride: $state.channelShiftModGXSamplingOverride,
            scaleRange: -64...64, scaleStep: 1, offsetRange: -64...64, offsetStep: 1,
            modulator: $state.channelShiftModGXModulator,
            modulatorNames: state.channelShiftDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "G Y",
            source: $state.channelShiftModGYSource,
            scale: $state.channelShiftModGYScale,
            offset: $state.channelShiftModGYOffset,
            samplingOverride: $state.channelShiftModGYSamplingOverride,
            scaleRange: -64...64, scaleStep: 1, offsetRange: -64...64, offsetStep: 1,
            modulator: $state.channelShiftModGYModulator,
            modulatorNames: state.channelShiftDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "B X",
            source: $state.channelShiftModBXSource,
            scale: $state.channelShiftModBXScale,
            offset: $state.channelShiftModBXOffset,
            samplingOverride: $state.channelShiftModBXSamplingOverride,
            scaleRange: -64...64, scaleStep: 1, offsetRange: -64...64, offsetStep: 1,
            modulator: $state.channelShiftModBXModulator,
            modulatorNames: state.channelShiftDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "B Y",
            source: $state.channelShiftModBYSource,
            scale: $state.channelShiftModBYScale,
            offset: $state.channelShiftModBYOffset,
            samplingOverride: $state.channelShiftModBYSamplingOverride,
            scaleRange: -64...64, scaleStep: 1, offsetRange: -64...64, offsetStep: 1,
            modulator: $state.channelShiftModBYModulator,
            modulatorNames: state.channelShiftDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [
              state.channelShiftModRXSource, state.channelShiftModRYSource,
              state.channelShiftModGXSource, state.channelShiftModGYSource,
              state.channelShiftModBXSource, state.channelShiftModBYSource
            ],
            audioURL: state.channelShiftModulatorAudioURL,
            framesURL: state.channelShiftModulatorFramesURL,
            sampling: $state.channelShiftModSampling,
            chooseAudio: { state.chooseChannelShiftModulatorWAV() },
            chooseFrames: { state.chooseChannelShiftModulatorFrames() }
          )

          NamedModulatorsSection(
            modulators: $state.channelShiftNamedModulators,
            onAdd: { state.addChannelShiftNamedModulator() },
            onRemove: { state.removeChannelShiftNamedModulator(id: $0) },
            chooseAudio: { state.chooseChannelShiftNamedModulatorWAV(id: $0) },
            chooseFrames: { state.chooseChannelShiftNamedModulatorFrames(id: $0) }
          )

          Text(state.channelShiftSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runChannelShiftSequenceRender()
          } label: {
            Label("Run Channel Shift", systemImage: "rectangle.split.3x1")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Palette Quantize — Posterize / Neon Palette")
            .font(.subheadline.weight(.semibold))

          HStack(spacing: 16) {
            Picker("Backend", selection: $state.paletteQuantizeBackend) {
              ForEach(FeedbackRenderBackendOption.allCases) { backend in
                Text(backend.rawValue).tag(backend)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)
            .help("Metal covers both modes and is parity-gated against the CPU reference per frame.")

            Picker("Mode", selection: $state.paletteQuantizeMode) {
              ForEach(PaletteQuantizeModeOption.allCases) { mode in
                Text(mode.rawValue).tag(mode)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 240)
            .help("Posterize snaps each channel to uniform steps; Neon Palette maps to the built-in magenta/orange/teal/black set.")

            if state.paletteQuantizeMode == .posterize {
              Stepper(value: $state.paletteQuantizeLevels, in: 2...256, step: 1) {
                Text("Levels \(state.paletteQuantizeLevels)")
              }
              .frame(width: 150, alignment: .leading)
              .help("Discrete steps per channel; 2 is the harshest collapse, 256 is the byte-identical passthrough.")
            }
          }

          ModulationSlotRow(
            label: "Levels",
            source: $state.paletteQuantizeModLevelsSource,
            scale: $state.paletteQuantizeModLevelsScale,
            offset: $state.paletteQuantizeModLevelsOffset,
            samplingOverride: $state.paletteQuantizeModLevelsSamplingOverride,
            scaleRange: -254...254, scaleStep: 8, offsetRange: -256...256, offsetStep: 8,
            modulator: $state.paletteQuantizeModLevelsModulator,
            modulatorNames: state.paletteQuantizeDeclaredModulatorNames
          )

          EnumModulationSlotRow(
            label: "Mode",
            source: $state.paletteQuantizeModModeSource,
            from: $state.paletteQuantizeModModeFrom,
            to: $state.paletteQuantizeModModeTo,
            samplingOverride: $state.paletteQuantizeModModeSamplingOverride,
            modulator: $state.paletteQuantizeModModeModulator,
            modulatorNames: state.paletteQuantizeDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [state.paletteQuantizeModLevelsSource, state.paletteQuantizeModModeSource],
            audioURL: state.paletteQuantizeModulatorAudioURL,
            framesURL: state.paletteQuantizeModulatorFramesURL,
            sampling: $state.paletteQuantizeModSampling,
            chooseAudio: { state.choosePaletteQuantizeModulatorWAV() },
            chooseFrames: { state.choosePaletteQuantizeModulatorFrames() }
          )

          NamedModulatorsSection(
            modulators: $state.paletteQuantizeNamedModulators,
            onAdd: { state.addPaletteQuantizeNamedModulator() },
            onRemove: { state.removePaletteQuantizeNamedModulator(id: $0) },
            chooseAudio: { state.choosePaletteQuantizeNamedModulatorWAV(id: $0) },
            chooseFrames: { state.choosePaletteQuantizeNamedModulatorFrames(id: $0) }
          )

          Text(state.paletteQuantizeSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runPaletteQuantizeSequenceRender()
          } label: {
            Label("Run Palette Quantize", systemImage: "paintpalette")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Rutt-Etra — Luma-Displaced Scanlines (CPU)")
            .font(.subheadline.weight(.semibold))

          HStack(spacing: 16) {
            Stepper(value: $state.ruttEtraLinePitch, in: 1...256, step: 1) {
              Text("Pitch \(state.ruttEtraLinePitch)px")
            }
            .frame(width: 140, alignment: .leading)
            .help("Rows between scanlines; smaller = denser wireframe.")

            Stepper(value: $state.ruttEtraDisplacementDepth, in: -512...512, step: 8) {
              Text("Depth \(state.ruttEtraDisplacementDepth, specifier: "%.0f")px")
            }
            .frame(width: 150, alignment: .leading)
            .help("Vertical push at full brightness; 0 = flat scanlines (off case), negative pushes down.")

            Stepper(value: $state.ruttEtraLineThickness, in: 1...64, step: 1) {
              Text("Thickness \(state.ruttEtraLineThickness)px")
            }
            .frame(width: 160, alignment: .leading)
            .help("Each line extends downward by this many pixels.")

            Toggle("Mono", isOn: $state.ruttEtraMono)
              .toggleStyle(.checkbox)
              .help("White lines instead of source colour — the classic monochrome CRT look.")
          }

          ModulationSlotRow(
            label: "Depth",
            source: $state.ruttEtraModDepthSource,
            scale: $state.ruttEtraModDepthScale,
            offset: $state.ruttEtraModDepthOffset,
            samplingOverride: $state.ruttEtraModDepthSamplingOverride,
            scaleRange: -256...256, scaleStep: 8, offsetRange: -256...256, offsetStep: 8,
            modulator: $state.ruttEtraModDepthModulator,
            modulatorNames: state.ruttEtraDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Pitch",
            source: $state.ruttEtraModPitchSource,
            scale: $state.ruttEtraModPitchScale,
            offset: $state.ruttEtraModPitchOffset,
            samplingOverride: $state.ruttEtraModPitchSamplingOverride,
            scaleRange: -255...255, scaleStep: 1, offsetRange: -256...256, offsetStep: 1,
            modulator: $state.ruttEtraModPitchModulator,
            modulatorNames: state.ruttEtraDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Thickness",
            source: $state.ruttEtraModThicknessSource,
            scale: $state.ruttEtraModThicknessScale,
            offset: $state.ruttEtraModThicknessOffset,
            samplingOverride: $state.ruttEtraModThicknessSamplingOverride,
            scaleRange: -63...63, scaleStep: 1, offsetRange: -64...64, offsetStep: 1,
            modulator: $state.ruttEtraModThicknessModulator,
            modulatorNames: state.ruttEtraDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [
              state.ruttEtraModDepthSource, state.ruttEtraModPitchSource,
              state.ruttEtraModThicknessSource,
            ],
            audioURL: state.ruttEtraModulatorAudioURL,
            framesURL: state.ruttEtraModulatorFramesURL,
            sampling: $state.ruttEtraModSampling,
            chooseAudio: { state.chooseRuttEtraModulatorWAV() },
            chooseFrames: { state.chooseRuttEtraModulatorFrames() }
          )

          NamedModulatorsSection(
            modulators: $state.ruttEtraNamedModulators,
            onAdd: { state.addRuttEtraNamedModulator() },
            onRemove: { state.removeRuttEtraNamedModulator(id: $0) },
            chooseAudio: { state.chooseRuttEtraNamedModulatorWAV(id: $0) },
            chooseFrames: { state.chooseRuttEtraNamedModulatorFrames(id: $0) }
          )

          Text(state.ruttEtraSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runRuttEtraSequenceRender()
          } label: {
            Label("Run Rutt-Etra", systemImage: "waveform.path")
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

            Toggle("Per-channel IR", isOn: $state.impulseConvPerChannel)
              .help("True-stereo: convolve each carrier channel with its own IR from Source A instead of one mono downmix.")
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
          Text("Controlled Datamosh")
            .font(.subheadline.weight(.semibold))
            .help("Source A's per-frame optical flow repeatedly advects Source B's previous output — the \"bloom/melt\" look. Keyframes snap back to Source B.")

          HStack(spacing: 16) {
            Button {
              state.chooseDatamoshModulatorDirectory()
            } label: {
              Label("Source A Frames", systemImage: "photo.on.rectangle")
            }
            Button {
              state.chooseDatamoshCarrierDirectory()
            } label: {
              Label("Source B Frames", systemImage: "photo.on.rectangle")
            }
            Button {
              state.chooseDatamoshOutputDirectory()
            } label: {
              Label("Output Dir", systemImage: "folder")
            }
          }

          HStack(spacing: 16) {
            Stepper(value: $state.datamoshKeyframeInterval, in: 0...120, step: 1) {
              Text("Keyframe Interval \(state.datamoshKeyframeInterval)")
            }
            .frame(width: 230, alignment: .leading)
            .help("1 = Source B passthrough; 0 = full melt from B[0]; N = snap to B every N frames.")

            Stepper(value: $state.datamoshAmount, in: 0...4, step: 0.1) {
              Text("Amount \(state.datamoshAmount, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
            .help("Per-step scale on A's flow; 0 freezes the held frame.")
          }

          HStack(spacing: 16) {
            Stepper(value: $state.datamoshBlockSize, in: 1...64, step: 1) {
              Text("Macroblock Size \(state.datamoshBlockSize)")
            }
            .frame(width: 230, alignment: .leading)
            .help("1 = smooth per-pixel bloom; N >= 2 quantizes A's flow to NxN blocks so whole macroblocks slide (the chunky codec-simulated look).")
          }

          HStack(spacing: 16) {
            Stepper(value: $state.datamoshResidualGain, in: 0...4, step: 0.1) {
              Text("Residual Gain \(state.datamoshResidualGain, specifier: "%.2f")")
            }
            .frame(width: 230, alignment: .leading)
            .help("Re-inject the intra-block motion discarded by quantization (a fine-motion haze atop the macroblock slide). 0 = block path; needs Macroblock Size >= 2.")
            Stepper(value: $state.datamoshResidualDecay, in: 0...1, step: 0.05) {
              Text("Residual Decay \(state.datamoshResidualDecay, specifier: "%.2f")")
            }
            .frame(width: 230, alignment: .leading)
            .help("How long discarded motion lingers in the accumulator: 0 = one-frame kick, ->1 = long-lived drift.")
          }

          HStack(spacing: 16) {
            Stepper(value: $state.datamoshBlockRefreshThreshold, in: 0...8, step: 0.25) {
              Text("Block Refresh \(state.datamoshBlockRefreshThreshold, specifier: "%.2f")")
            }
            .frame(width: 230, alignment: .leading)
            .help("Per-block keep/drop: macroblocks whose mean motion is below this snap back to the carrier (intra-block refresh) while busier blocks rot. 0 = no per-block refresh; needs Macroblock Size >= 2.")
          }

          HStack(spacing: 16) {
            Picker("Preset", selection: $state.datamoshPreset) {
              ForEach(DatamoshPresetOption.allCases) { preset in
                Text(preset.rawValue).tag(preset)
              }
            }
            .frame(width: 220)
            .help("Curated destructive recipes override the detailed datamosh knobs at render time. Custom uses the controls below.")

            Picker("Vector Remix", selection: $state.datamoshVectorRemix) {
              ForEach(DatamoshVectorRemixOption.allCases) { mode in
                Text(mode.rawValue).tag(mode)
              }
            }
            .frame(width: 280)
            .help("FFglitch-style motion-vector remix on the block-MV grid (needs Macroblock Size >= 2). Sort pools motion by magnitude; Shuffle permutes it by the seed. None = off.")

            if state.datamoshVectorRemix == .shuffle {
              Stepper(value: $state.datamoshRemixSeed, in: 0...9999, step: 1) {
                Text("Remix Seed \(state.datamoshRemixSeed)")
              }
              .frame(width: 180, alignment: .leading)
              .help("Deterministic permutation seed for Shuffle.")
            }
          }

          Picker("Backend", selection: $state.datamoshBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { backend in
              Text(backend.rawValue).tag(backend)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 200)
          .help("Metal is gated per-frame against the CPU reference.")

          ModulationSlotRow(
            label: "Amount",
            source: $state.datamoshModAmountSource,
            scale: $state.datamoshModAmountScale,
            offset: $state.datamoshModAmountOffset,
            samplingOverride: $state.datamoshModAmountSamplingOverride,
            modulator: $state.datamoshModAmountModulator,
            modulatorNames: state.datamoshDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Res Gain",
            source: $state.datamoshModResidualGainSource,
            scale: $state.datamoshModResidualGainScale,
            offset: $state.datamoshModResidualGainOffset,
            samplingOverride: $state.datamoshModResidualGainSamplingOverride,
            modulator: $state.datamoshModResidualGainModulator,
            modulatorNames: state.datamoshDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Res Decay",
            source: $state.datamoshModResidualDecaySource,
            scale: $state.datamoshModResidualDecayScale,
            offset: $state.datamoshModResidualDecayOffset,
            samplingOverride: $state.datamoshModResidualDecaySamplingOverride,
            modulator: $state.datamoshModResidualDecayModulator,
            modulatorNames: state.datamoshDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Refresh",
            source: $state.datamoshModRefreshThresholdSource,
            scale: $state.datamoshModRefreshThresholdScale,
            offset: $state.datamoshModRefreshThresholdOffset,
            samplingOverride: $state.datamoshModRefreshThresholdSamplingOverride,
            scaleRange: -8...8, scaleStep: 0.25, offsetRange: -8...8, offsetStep: 0.25,
            modulator: $state.datamoshModRefreshThresholdModulator,
            modulatorNames: state.datamoshDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [
              state.datamoshModAmountSource, state.datamoshModResidualGainSource,
              state.datamoshModResidualDecaySource, state.datamoshModRefreshThresholdSource
            ],
            audioURL: state.datamoshModulatorAudioURL,
            framesURL: state.datamoshModulatorFramesURL,
            sampling: $state.datamoshModSampling,
            chooseAudio: { state.chooseDatamoshModulatorWAV() },
            chooseFrames: { state.chooseDatamoshModulatorFrames() }
          )

          NamedModulatorsSection(
            modulators: $state.datamoshNamedModulators,
            onAdd: { state.addDatamoshNamedModulator() },
            onRemove: { state.removeDatamoshNamedModulator(id: $0) },
            chooseAudio: { state.chooseDatamoshNamedModulatorWAV(id: $0) },
            chooseFrames: { state.chooseDatamoshNamedModulatorFrames(id: $0) }
          )

          Text(state.datamoshSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runDatamoshRender()
          } label: {
            Label("Run Datamosh", systemImage: "slider.horizontal.3")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Bitstream Datamosh")
            .font(.subheadline.weight(.semibold))
            .help("Real AVI bitstream surgery: P-frame duplication, keyframe removal, or motion transfer via ffmpeg. Non-deterministic by design.")

          HStack(spacing: 16) {
            Button {
              state.chooseBitstreamInputVideo()
            } label: {
              Label("Input Video", systemImage: "film")
            }
            Button {
              state.chooseBitstreamCarrierVideo()
            } label: {
              Label("Carrier Video", systemImage: "film")
            }
            .disabled(state.bitstreamOperation != .motionTransfer)
            Button {
              state.chooseBitstreamOutputDirectory()
            } label: {
              Label("Output Dir", systemImage: "folder")
            }
          }

          HStack(spacing: 16) {
            Picker("Operation", selection: $state.bitstreamOperation) {
              ForEach(BitstreamOperationOption.allCases) { op in
                Text(op.rawValue).tag(op)
              }
            }
            .frame(width: 220)
            .help("P-Frame Bloom duplicates a P-frame. Void Mosh removes the keyframe. Motion Transfer splices modulator motion onto carrier content.")

            Picker("Preset", selection: $state.bitstreamPreset) {
              ForEach(BitstreamPresetOption.allCases) { preset in
                Text(preset.rawValue).tag(preset)
              }
            }
            .frame(width: 200)
            .help("Named presets override the operation and knobs. Custom uses the explicit controls.")
          }

          HStack(spacing: 16) {
            Stepper(value: $state.bitstreamFps, in: 1...120, step: 1) {
              Text("FPS \(state.bitstreamFps, specifier: "%.0f")")
            }
            .frame(width: 150, alignment: .leading)

            if state.bitstreamOperation == .pframeDuplicate {
              Stepper(value: $state.bitstreamPFrameIndex, in: 0...999, step: 1) {
                Text("P-Frame \(state.bitstreamPFrameIndex)")
              }
              .frame(width: 160, alignment: .leading)
              .help("0-based P-frame index to bloom.")

              Stepper(value: $state.bitstreamDuplicateCount, in: 0...300, step: 1) {
                Text("Copies \(state.bitstreamDuplicateCount)")
              }
              .frame(width: 160, alignment: .leading)
              .help("Extra copies of the target P-frame to insert.")
            }

            if state.bitstreamOperation == .motionTransfer {
              Stepper(value: $state.bitstreamCarrierKeyframes, in: 1...60, step: 1) {
                Text("Carrier Frames \(state.bitstreamCarrierKeyframes)")
              }
              .frame(width: 200, alignment: .leading)
              .help("Leading carrier frames to keep before the modulator's motion takes over.")
            }
          }

          Text(state.bitstreamSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runBitstreamDatamoshRender()
          } label: {
            Label("Run Bitstream Datamosh", systemImage: "waveform.path.ecg")
          }
        }

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Video-to-Audio Descriptor Routing")
            .font(.subheadline.weight(.semibold))
            .help("A Source A visual descriptor (luma or motion) drives Source B's audio: gain (descriptor → amplitude) or pan (descriptor → equal-power stereo position).")

          Picker("Descriptor", selection: $state.videoAudioRouteDescriptor) {
            ForEach(VideoAudioRouteDescriptorOption.allCases) { descriptor in
              Text(descriptor.rawValue).tag(descriptor)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 360)
          .help("Luma: per-frame mean brightness. Flow: per-frame mean optical-flow magnitude (motion).")

          Picker("Mode", selection: $state.videoAudioRouteMode) {
            ForEach(VideoAudioRouteModeOption.allCases) { mode in
              Text(mode.rawValue).tag(mode)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 360)
          .help("Gain: a strong descriptor keeps B, a weak one attenuates it. Pan: weak steers left, strong steers right. Filter: the descriptor sweeps a one-pole cutoff.")

          if state.videoAudioRouteMode == .filter {
            Picker("Filter", selection: $state.videoAudioRouteFilterType) {
              ForEach(VideoAudioRouteFilterTypeOption.allCases) { filter in
                Text(filter.rawValue).tag(filter)
              }
            }
            .pickerStyle(.segmented)
            .frame(width: 240)
            .help("Lowpass: a strong descriptor opens the cutoff toward Nyquist. Highpass: a strong descriptor lifts the high-pass corner.")
          }

          Picker("Envelope", selection: $state.videoAudioRouteSampling) {
            ForEach(VideoAudioRouteSamplingOption.allCases) { sampling in
              Text(sampling.rawValue).tag(sampling)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 360)
          .help("Hold: the descriptor steps at each frame. Smooth: it linearly interpolates between frames (a continuous curve, no zipper stepping).")

          HStack(spacing: 16) {
            Button {
              state.chooseVideoAudioRouteModulatorDirectory()
            } label: {
              Label("Source A Frames", systemImage: "photo.on.rectangle")
            }
            Button {
              state.chooseVideoAudioRouteCarrierWAV()
            } label: {
              Label("Source B WAV", systemImage: "waveform")
            }
            Button {
              state.chooseVideoAudioRouteOutputDirectory()
            } label: {
              Label("Output Dir", systemImage: "folder")
            }
          }

          HStack(spacing: 16) {
            Stepper(value: $state.videoAudioRouteAmount, in: 0...1, step: 0.05) {
              Text("Amount \(state.videoAudioRouteAmount, specifier: "%.2f")")
            }
            .frame(width: 170, alignment: .leading)
            .help("0 = Source B passthrough; 1 = full routing.")

            Stepper(value: $state.videoAudioRouteFPS, in: 1...120, step: 1) {
              Text("FPS \(state.videoAudioRouteFPS, specifier: "%.0f")")
            }
            .frame(width: 150, alignment: .leading)
            .help("Frame rate mapping A's frame index to time for the luma lookup.")
          }

          Text(state.videoAudioRouteSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runVideoAudioRouteRender()
          } label: {
            Label("Run Video→Audio Route", systemImage: "slider.horizontal.3")
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

          Toggle("Colour kernels (per R/G/B)", isOn: $state.convBlendColorMode)
            .help("Extract a separate kernel from each of Source A's R/G/B channels instead of one luma kernel.")

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

        Divider()

        VStack(alignment: .leading, spacing: 8) {
          Text("Pixel Sort")
            .font(.subheadline.weight(.semibold))
            .help("Threshold-bounded pixel sorting. A drives the sortability mask in cross-synth modes; B provides the sorted content.")

          HStack(spacing: 16) {
            Button {
              state.choosePixelSortModulatorDirectory()
            } label: {
              Label("Source A Frames", systemImage: "photo.on.rectangle")
            }
            Button {
              state.choosePixelSortCarrierDirectory()
            } label: {
              Label("Source B Frames", systemImage: "photo.on.rectangle.angled")
            }
            Button {
              state.choosePixelSortOutputDirectory()
            } label: {
              Label("Output Dir", systemImage: "folder")
            }
          }

          HStack(spacing: 16) {
            Picker("Axis", selection: $state.pixelSortAxis) {
              ForEach(PixelSortAxisOption.allCases) { opt in
                Text(opt.rawValue).tag(opt)
              }
            }
            .frame(width: 130)
            .help("Row = horizontal streaks, Col = vertical.")

            Picker("Key", selection: $state.pixelSortKey) {
              ForEach(PixelSortKeyOption.allCases) { opt in
                Text(opt.rawValue).tag(opt)
              }
            }
            .frame(width: 130)
            .help("Sort key used to order pixels within each span.")

            Picker("Dir", selection: $state.pixelSortDirection) {
              ForEach(PixelSortDirectionOption.allCases) { opt in
                Text(opt.rawValue).tag(opt)
              }
            }
            .frame(width: 100)
          }

          HStack(spacing: 16) {
            Stepper(value: $state.pixelSortThresholdLow, in: 0...1, step: 0.05) {
              Text("Low \(state.pixelSortThresholdLow, specifier: "%.2f")")
            }
            .frame(width: 150, alignment: .leading)
            .help("Lower bound of sortable key range [0, 1].")

            Stepper(value: $state.pixelSortThresholdHigh, in: 0...1, step: 0.05) {
              Text("High \(state.pixelSortThresholdHigh, specifier: "%.2f")")
            }
            .frame(width: 150, alignment: .leading)
            .help("Upper bound of sortable key range [0, 1].")

            Stepper(value: $state.pixelSortMaxSpan, in: 0...2048, step: 16) {
              Text(state.pixelSortMaxSpan == 0
                ? "Span: unlimited"
                : "Span \(state.pixelSortMaxSpan)px")
            }
            .frame(width: 180, alignment: .leading)
            .help("Maximum streak length in pixels; 0 = unbounded.")
          }

          Picker("Mask Source", selection: $state.pixelSortMaskSource) {
            ForEach(PixelSortMaskSourceOption.allCases) { opt in
              Text(opt.rawValue).tag(opt)
            }
          }
          .pickerStyle(.segmented)
          .help("Self = B masks itself (classic). A Luma/Edge/Flow = cross-synth: A defines where sorting happens.")

          if state.pixelSortMaskSource == .aFlow {
            Stepper(value: $state.pixelSortFlowRadius, in: 1...8, step: 1) {
              Text("Flow Radius \(state.pixelSortFlowRadius)")
            }
            .frame(width: 180, alignment: .leading)
            .help("Lucas-Kanade window half-radius for A-flow mask.")
          }

          Picker("Backend", selection: $state.pixelSortBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { backend in
              Text(backend.rawValue).tag(backend)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 200)
          .help("Metal is self-mask only and gated per-frame against CPU. Cross-synth modes are CPU-only.")

          ModulationSlotRow(
            label: "Low",
            source: $state.pixelSortModLowSource,
            scale: $state.pixelSortModLowScale,
            offset: $state.pixelSortModLowOffset,
            samplingOverride: $state.pixelSortModLowSamplingOverride,
            modulator: $state.pixelSortModLowModulator,
            modulatorNames: state.pixelSortDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "High",
            source: $state.pixelSortModHighSource,
            scale: $state.pixelSortModHighScale,
            offset: $state.pixelSortModHighOffset,
            samplingOverride: $state.pixelSortModHighSamplingOverride,
            modulator: $state.pixelSortModHighModulator,
            modulatorNames: state.pixelSortDeclaredModulatorNames
          )

          EnumModulationSlotRow(
            label: "Direction",
            source: $state.pixelSortModDirectionSource,
            from: $state.pixelSortModDirectionFrom,
            to: $state.pixelSortModDirectionTo,
            samplingOverride: $state.pixelSortModDirectionSamplingOverride,
            modulator: $state.pixelSortModDirectionModulator,
            modulatorNames: state.pixelSortDeclaredModulatorNames
          )

          EnumModulationSlotRow(
            label: "Axis",
            source: $state.pixelSortModAxisSource,
            from: $state.pixelSortModAxisFrom,
            to: $state.pixelSortModAxisTo,
            samplingOverride: $state.pixelSortModAxisSamplingOverride,
            modulator: $state.pixelSortModAxisModulator,
            modulatorNames: state.pixelSortDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [
              state.pixelSortModLowSource, state.pixelSortModHighSource,
              state.pixelSortModDirectionSource, state.pixelSortModAxisSource,
            ],
            audioURL: state.pixelSortModulatorAudioURL,
            framesURL: state.pixelSortModulatorFramesURL,
            sampling: $state.pixelSortModSampling,
            chooseAudio: { state.choosePixelSortModulatorWAV() },
            chooseFrames: { state.choosePixelSortModulatorFrames() }
          )

          NamedModulatorsSection(
            modulators: $state.pixelSortNamedModulators,
            onAdd: { state.addPixelSortNamedModulator() },
            onRemove: { state.removePixelSortNamedModulator(id: $0) },
            chooseAudio: { state.choosePixelSortNamedModulatorWAV(id: $0) },
            chooseFrames: { state.choosePixelSortNamedModulatorFrames(id: $0) }
          )

          Text(state.pixelSortSummary)
            .font(.caption)
            .foregroundStyle(.secondary)

          Button {
            state.runPixelSortRender()
          } label: {
            Label("Run Pixel Sort", systemImage: "arrow.left.arrow.right")
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

/// One knob's modulation slot: source picker (Off = no route) plus the affine
/// scale/offset mapping, shown only when a source is chosen.
private struct ModulationSlotRow: View {
  let label: String
  @Binding var source: ModulationSourceOption
  @Binding var scale: Double
  @Binding var offset: Double
  @Binding var samplingOverride: ModulationSamplingOverrideOption
  // Defaults suit [0, 1] knobs; pixel-unit targets (channel-shift offsets)
  // pass wider ranges so the envelope can span a visible shift.
  var scaleRange: ClosedRange<Double> = -8...8
  var scaleStep = 0.1
  var offsetRange: ClosedRange<Double> = -1...1
  var offsetStep = 0.05
  // Named-modulator binding; nil (the default) hides the picker so call sites
  // predating named modulators are unchanged. The picker only shows once at
  // least one named modulator is declared (`modulatorNames` non-empty).
  var modulator: Binding<String>? = nil
  var modulatorNames: [String] = []

  var body: some View {
    HStack(spacing: 16) {
      Picker("Mod \(label)", selection: $source) {
        ForEach(ModulationSourceOption.allCases) { option in
          Text(option.rawValue).tag(option)
        }
      }
      .frame(width: 280)
      .help("Analysis envelope routed onto this knob; Off keeps the knob constant.")

      if source != .off {
        if let modulator, !modulatorNames.isEmpty {
          Picker("Modulator", selection: modulator) {
            Text("Default").tag("")
            ForEach(modulatorNames, id: \.self) { name in
              Text(name).tag(name)
            }
          }
          .frame(width: 180)
          .help("Which modulator media this route reads; Default uses the panel's Modulator WAV/Frames.")
        }

        Stepper(value: $scale, in: scaleRange, step: scaleStep) {
          Text("Scale \(scale, specifier: "%.2f")")
        }
        .frame(width: 150, alignment: .leading)
        .help("knob = clamp(envelope × scale + offset)")

        Stepper(value: $offset, in: offsetRange, step: offsetStep) {
          Text("Offset \(offset, specifier: "%.2f")")
        }
        .frame(width: 160, alignment: .leading)

        Picker("Sampling", selection: $samplingOverride) {
          ForEach(ModulationSamplingOverrideOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .frame(width: 180)
        .help("Overrides this route's sampling; Default inherits the panel Sampling picker.")
      }
    }
  }
}

/// Mod slot for an enum knob: instead of opaque scale/offset steppers, two
/// variant pickers — envelope 0 selects **From**, envelope 1 selects **To**
/// (`enumModulationMapping` emits the equivalent affine route). From == To is
/// legal and holds the knob at that variant (the continuity identity).
private struct EnumModulationSlotRow<Option>: View
where
  Option: CaseIterable & Identifiable & Hashable & RawRepresentable,
  Option.RawValue == String,
  Option.AllCases: RandomAccessCollection
{
  let label: String
  @Binding var source: ModulationSourceOption
  @Binding var from: Option
  @Binding var to: Option
  @Binding var samplingOverride: ModulationSamplingOverrideOption
  // Named-modulator binding; nil (the default) hides the picker so call sites
  // predating named modulators are unchanged. Mirrors `ModulationSlotRow`.
  var modulator: Binding<String>? = nil
  var modulatorNames: [String] = []

  var body: some View {
    HStack(spacing: 16) {
      Picker("Mod \(label)", selection: $source) {
        ForEach(ModulationSourceOption.allCases) { option in
          Text(option.rawValue).tag(option)
        }
      }
      .frame(width: 280)
      .help("Analysis envelope routed onto this knob; Off keeps the knob constant.")

      if source != .off {
        if let modulator, !modulatorNames.isEmpty {
          Picker("Modulator", selection: modulator) {
            Text("Default").tag("")
            ForEach(modulatorNames, id: \.self) { name in
              Text(name).tag(name)
            }
          }
          .frame(width: 180)
          .help("Which modulator media this route reads; Default uses the panel's Modulator WAV/Frames.")
        }

        Picker("From", selection: $from) {
          ForEach(Option.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .frame(width: 170)
        .help("Variant selected when the envelope is at 0.")

        Text("→")
          .foregroundStyle(.secondary)

        Picker("To", selection: $to) {
          ForEach(Option.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .frame(width: 150)
        .help("Variant selected when the envelope is at 1; in between, the envelope steps through the variants From→To.")

        Picker("Sampling", selection: $samplingOverride) {
          ForEach(ModulationSamplingOverrideOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .frame(width: 180)
        .help("Overrides this route's sampling; Default inherits the panel Sampling picker.")
      }
    }
  }
}

/// The modulator media + sampling controls shared by an effect's mod slots;
/// hidden until any slot picks a source.
private struct ModulationMediaRow: View {
  let sources: [ModulationSourceOption]
  let audioURL: URL?
  let framesURL: URL?
  @Binding var sampling: ModulationSamplingOption
  let chooseAudio: () -> Void
  let chooseFrames: () -> Void

  var body: some View {
    if sources.contains(where: { $0 != .off }) {
      HStack(spacing: 16) {
        if sources.contains(where: \.needsAudio) {
          Button("Modulator WAV…", action: chooseAudio)
          Text(audioURL?.lastPathComponent ?? "none selected")
            .font(.caption)
            .foregroundStyle(.secondary)
        }

        if sources.contains(where: \.needsFrames) {
          Button("Modulator Frames…", action: chooseFrames)
          Text(framesURL?.lastPathComponent ?? "none selected")
            .font(.caption)
            .foregroundStyle(.secondary)
        }

        Picker("Sampling", selection: $sampling) {
          ForEach(ModulationSamplingOption.allCases) { option in
            Text(option.rawValue).tag(option)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 200)
        .help("Hold steps between envelope samples; Smooth interpolates linearly.")
      }
    }
  }
}

/// Declares extra named modulators for a panel: a name field plus WAV/Frames
/// pickers per row, and an Add button. A mod slot's Modulator picker binds to
/// one of these by name; the panel's default `ModulationMediaRow` still covers
/// unnamed slots.
private struct NamedModulatorsSection: View {
  @Binding var modulators: [NamedModulatorEntry]
  let onAdd: () -> Void
  let onRemove: (UUID) -> Void
  let chooseAudio: (UUID) -> Void
  let chooseFrames: (UUID) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      HStack {
        Text("Named Modulators")
          .font(.caption.weight(.semibold))
          .foregroundStyle(.secondary)
        Button("Add", action: onAdd)
          .help("Declare another modulator so different slots can read different media.")
      }

      ForEach($modulators) { $entry in
        HStack(spacing: 12) {
          TextField("Name", text: $entry.name)
            .frame(width: 120)
            .help("Route grammar: target=name.source. Must be non-empty and unique.")

          Button("WAV…") { chooseAudio(entry.id) }
          Text(entry.audioURL?.lastPathComponent ?? "—")
            .font(.caption)
            .foregroundStyle(.secondary)

          Button("Frames…") { chooseFrames(entry.id) }
          Text(entry.framesURL?.lastPathComponent ?? "—")
            .font(.caption)
            .foregroundStyle(.secondary)

          Button(role: .destructive) { onRemove(entry.id) } label: {
            Image(systemName: "trash")
          }
          .help("Remove this modulator; slots bound to it reset to Default.")
        }
      }
    }
  }
}
