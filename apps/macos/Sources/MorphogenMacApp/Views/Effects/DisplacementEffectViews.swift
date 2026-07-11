import SwiftUI

/// Flow Displace, Flow Feedback, and Rutt-Etra — the Displacement category.
/// Flow Displace and Flow Feedback merge what were previously duplicate
/// WorkflowPanelView "quick" controls and RenderPanelView "advanced" controls
/// bound to the same AppState properties; Rutt-Etra had no Workflow-side
/// duplicate so it's ported from RenderPanelView as-is.

struct FlowDisplaceDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .flowDisplace)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Stepper(value: $state.frameSequenceAmount, in: 0...64, step: 1) {
          Text("Displacement \(state.frameSequenceAmount, specifier: "%.0f")")
        }
        .frame(width: 190, alignment: .leading)

        Toggle("Reuse flow cache", isOn: $state.frameSequenceWritesFlowCache)
          .toggleStyle(.checkbox)
      }

      MoreKnobs {
        Stepper(value: $state.frameSequenceMaxFrames, in: 1...600, step: 1) {
          Text("Max frames \(state.frameSequenceMaxFrames)")
        }
        .frame(width: 180, alignment: .leading)
      }

      Button {
        state.runTwoSourceFrameSequenceRender()
      } label: {
        Label("Run Flow Displace", systemImage: EffectListing.flowDisplace.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.frameSequenceSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

      QuickPreviewBand(state: state, requiresModulator: true) {
        state.runTwoSourceFrameSequenceRender()
      }
    }
  }
}

struct FlowFeedbackDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .flowFeedback)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Stepper(value: $state.feedbackCarrierAmount, in: 0...8, step: 0.25) {
            Text("Carrier \(state.feedbackCarrierAmount, specifier: "%.2f")")
          }
          .frame(width: 155, alignment: .leading)

          Stepper(value: $state.feedbackDecay, in: 0...1, step: 0.001) {
            Text("Decay \(state.feedbackDecay, specifier: "%.3f")")
          }
          .frame(width: 145, alignment: .leading)

          Stepper(value: $state.feedbackStructureMix, in: 0...2, step: 0.05) {
            Text("Structure \(state.feedbackStructureMix, specifier: "%.2f")")
          }
          .frame(width: 165, alignment: .leading)
        }

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
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
        }

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Toggle("Write Flow Cache", isOn: $state.feedbackWritesFlowCache)
            .toggleStyle(.checkbox)

          Toggle("Reset", isOn: $state.feedbackResetEnabled)
            .toggleStyle(.checkbox)

          Stepper(value: $state.feedbackResetAtFrame, in: 0...max(0, state.frameSequenceMaxFrames - 1)) {
            Text("Reset frame \(state.feedbackResetAtFrame)")
          }
          .disabled(!state.feedbackResetEnabled)
          .frame(width: 165, alignment: .leading)
        }
      }

      Button {
        state.runFlowFeedbackSequenceRender()
      } label: {
        Label("Run Flow Feedback", systemImage: EffectListing.flowFeedback.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.feedbackSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

      QuickPreviewBand(state: state, requiresModulator: true) {
        state.runFlowFeedbackSequenceRender()
      }
    }
  }
}

struct RuttEtraDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .ruttEtra)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      Toggle("Two-Source (Source A drives displacement)", isOn: $state.ruttEtraUseTwoSource)
        .toggleStyle(.checkbox)
        .help(
          "Cross-synthesis: Source A's luma displaces Source B's scanlines while Source B "
          + "supplies the colour. Off = Source B displaces its own scanlines. Source A is the "
          + "shared modulator frame directory.")

      MoreKnobs {
        Picker("Backend", selection: $state.ruttEtraBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .frame(width: 130)
        .help("Metal runs the gather kernel and is parity-gated per-frame against the CPU reference.")

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
          ModulationSlotRow(
            label: "Depth",
            source: $state.ruttEtraModDepthSource,
            scale: $state.ruttEtraModDepthScale,
            offset: $state.ruttEtraModDepthOffset,
            samplingOverride: $state.ruttEtraModDepthSamplingOverride,
            scaleRange: -256...256, scaleStep: 8, offsetRange: -256...256, offsetStep: 8,
            modulator: $state.ruttEtraModDepthModulator,
            modulatorNames: state.ruttEtraDeclaredModulatorNames,
            lfoShape: $state.ruttEtraModDepthLfoShape,
            lfoRate: $state.ruttEtraModDepthLfoRate,
            lfoPhase: $state.ruttEtraModDepthLfoPhase,
            captureAvailable: true,
            midiAvailable: true,
            midiCcNumber: $state.ruttEtraModDepthMidiCc
          )

          ModulationSlotRow(
            label: "Pitch",
            source: $state.ruttEtraModPitchSource,
            scale: $state.ruttEtraModPitchScale,
            offset: $state.ruttEtraModPitchOffset,
            samplingOverride: $state.ruttEtraModPitchSamplingOverride,
            scaleRange: -255...255, scaleStep: 1, offsetRange: -256...256, offsetStep: 1,
            modulator: $state.ruttEtraModPitchModulator,
            modulatorNames: state.ruttEtraDeclaredModulatorNames,
            lfoShape: $state.ruttEtraModPitchLfoShape,
            lfoRate: $state.ruttEtraModPitchLfoRate,
            lfoPhase: $state.ruttEtraModPitchLfoPhase,
            captureAvailable: true,
            midiAvailable: true,
            midiCcNumber: $state.ruttEtraModPitchMidiCc
          )

          ModulationSlotRow(
            label: "Thickness",
            source: $state.ruttEtraModThicknessSource,
            scale: $state.ruttEtraModThicknessScale,
            offset: $state.ruttEtraModThicknessOffset,
            samplingOverride: $state.ruttEtraModThicknessSamplingOverride,
            scaleRange: -63...63, scaleStep: 1, offsetRange: -64...64, offsetStep: 1,
            modulator: $state.ruttEtraModThicknessModulator,
            modulatorNames: state.ruttEtraDeclaredModulatorNames,
            lfoShape: $state.ruttEtraModThicknessLfoShape,
            lfoRate: $state.ruttEtraModThicknessLfoRate,
            lfoPhase: $state.ruttEtraModThicknessLfoPhase,
            captureAvailable: true,
            midiAvailable: true,
            midiCcNumber: $state.ruttEtraModThicknessMidiCc
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
            chooseFrames: { state.chooseRuttEtraModulatorFrames() },
            midiURL: state.ruttEtraModulatorMidiURL,
            chooseMidi: { state.chooseRuttEtraModulatorMIDI() }
          )

          NamedModulatorsSection(
            modulators: $state.ruttEtraNamedModulators,
            onAdd: { state.addRuttEtraNamedModulator() },
            onRemove: { state.removeRuttEtraNamedModulator(id: $0) },
            chooseAudio: { state.chooseRuttEtraNamedModulatorWAV(id: $0) },
            chooseFrames: { state.chooseRuttEtraNamedModulatorFrames(id: $0) },
            chooseMidi: { state.chooseRuttEtraNamedModulatorMIDI(id: $0) }
          )

          MatteConfigRow(
            source: $state.ruttEtraMatteSource,
            gain: $state.ruttEtraMatteGain,
            framesURL: state.ruttEtraMatteFramesURL,
            chooseFrames: { state.chooseRuttEtraMatteFrames() },
            framesHelp: "Defaults to Source A when Two-Source is on; otherwise pick a matte frame directory."
          )
        }
      }

      Button {
        state.runRuttEtraSequenceRender()
      } label: {
        Label("Run Rutt-Etra", systemImage: EffectListing.ruttEtra.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.ruttEtraSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

      QuickPreviewBand(state: state, requiresModulator: state.ruttEtraUseTwoSource) {
        state.runRuttEtraSequenceRender()
      }
    }
  }
}
