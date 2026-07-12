import SwiftUI

/// Retro Static, Channel Shift, Palette Quantize, Pixel Sort — none of these
/// ever had a WorkflowPanelView "quick" duplicate, so all four are straight
/// ports from RenderPanelView.

struct RetroStaticDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .retroStatic)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Picker("Filter", selection: $state.retroStaticFilter) {
          ForEach(RetroStaticFilterOption.allCases) { filter in
            Text(filter.rawValue).tag(filter)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 300)

        Stepper(value: $state.retroStaticStrength, in: 0...1, step: 0.05) {
          Text("Strength \(state.retroStaticStrength, specifier: "%.2f")")
        }
        .frame(width: 170, alignment: .leading)
      }

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Picker("Backend", selection: $state.retroStaticBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { backend in
              Text(backend.rawValue).tag(backend)
            }
          }
          .pickerStyle(.segmented)
          .frame(width: 220)

          Stepper(value: $state.retroStaticRealBpp, in: 1...8, step: 1) {
            Text("Real BPP \(state.retroStaticRealBpp)")
          }
          .frame(width: 150, alignment: .leading)

          Stepper(value: $state.retroStaticAssumedBpp, in: 1...8, step: 1) {
            Text("Assumed BPP \(state.retroStaticAssumedBpp)")
          }
          .frame(width: 170, alignment: .leading)
        }

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
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
        }
      }

      Button {
        state.runRetroStaticSequenceRender()
      } label: {
        Label("Run Retro Static", systemImage: EffectListing.retroStatic.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.retroStaticSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

    }
  }
}

struct ChannelShiftDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .channelShift)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        Picker("Backend", selection: $state.channelShiftBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 220)
        .help("Metal covers constant offsets and is parity-gated. Flow-driven mode (Flow Gain ≠ 0) is CPU-only.")

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
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

          MatteConfigRow(
            source: $state.channelShiftMatteSource,
            gain: $state.channelShiftMatteGain,
            framesURL: state.channelShiftMatteFramesURL,
            chooseFrames: { state.chooseChannelShiftMatteFrames() },
            framesHelp: "Defaults to Source A when flow-driven mode (Flow Gain ≠ 0) is on."
          )
        }
      }

      Button {
        state.runChannelShiftSequenceRender()
      } label: {
        Label("Run Channel Shift", systemImage: EffectListing.channelShift.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.channelShiftSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

    }
  }
}

struct PaletteQuantizeDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .paletteQuantize)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        Picker("Backend", selection: $state.paletteQuantizeBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 220)
        .help("Metal covers both modes and is parity-gated against the CPU reference per frame.")

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
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

          MatteConfigRow(
            source: $state.paletteQuantizeMatteSource,
            gain: $state.paletteQuantizeMatteGain,
            framesURL: state.paletteQuantizeMatteFramesURL,
            chooseFrames: { state.choosePaletteQuantizeMatteFrames() },
            framesHelp: "Required when a matte source is selected (palette-quantize has no Source A)."
          )
        }
      }

      Button {
        state.runPaletteQuantizeSequenceRender()
      } label: {
        Label("Run Palette Quantize", systemImage: EffectListing.paletteQuantize.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.paletteQuantizeSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

    }
  }
}

struct PixelSortDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .pixelSort)
        .help("Threshold-bounded pixel sorting. A drives the sortability mask in cross-synth modes; B provides the sorted content.")

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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
      }

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        Stepper(value: $state.pixelSortMaxSpan, in: 0...2048, step: 16) {
          Text(state.pixelSortMaxSpan == 0
            ? "Span: unlimited"
            : "Span \(state.pixelSortMaxSpan)px")
        }
        .frame(width: 180, alignment: .leading)
        .help("Maximum streak length in pixels; 0 = unbounded.")

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

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
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
        }
      }

      Button {
        state.runPixelSortRender()
      } label: {
        Label("Run Pixel Sort", systemImage: EffectListing.pixelSort.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.pixelSortSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }
}
