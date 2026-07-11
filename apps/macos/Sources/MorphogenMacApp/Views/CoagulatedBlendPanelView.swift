import SwiftUI

/// Descriptor-coagulated flow blend (Tier 1.1 modulation). Two-source: Source A
/// (the intruder) and Source B (the carrier) come from the shared Source slots.
/// Modulation drives coagulation_strength / edge_hardness / bias — coagulated
/// has no checkpoint path, so routes are provenance-only. Runs through the CLI
/// bridge (queue add→run) and loads the result into the preview.
struct CoagulatedBlendPanelView: View {
  @ObservedObject var state: AppState

  var modulatorNames: [String] { state.coagNamedModulators.map { $0.name } }

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      Text("Coagulated Flow Blend")
        .font(.headline)
      Text("Two-source: Source A intrudes into Source B in coagulated patches. Uses the shared Source A/B frame slots. Modulate coagulation_strength / edge_hardness / bias.")
        .font(.caption)
        .foregroundStyle(.secondary)

      HStack {
        Button {
          state.chooseCoagulatedOutputDirectory()
        } label: {
          Label("Output Folder", systemImage: "folder.badge.plus")
        }
        Button {
          state.chooseCoagModulatorAudio()
        } label: {
          Label("Modulator WAV", systemImage: "waveform")
        }
        Picker("Advect", selection: $state.coagAdvectSource) {
          ForEach(CoagulationFlowSourceOption.allCases) { option in
            Text(option.label).tag(option)
          }
        }
        .frame(width: 190)
      }
      Text(state.coagOutputPath)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)

      Grid(alignment: .leading, horizontalSpacing: 20, verticalSpacing: 6) {
        GridRow {
          Stepper(value: $state.coagPatchSize, in: 1...128) {
            Text("Patch \(state.coagPatchSize)")
          }
          Stepper(value: $state.coagCoagulationStrength, in: 0...64, step: 1) {
            Text("Coagulate \(state.coagCoagulationStrength, specifier: "%.0f")")
          }
          Stepper(value: $state.coagEdgeHardness, in: 0...1, step: 0.1) {
            Text("Edge \(state.coagEdgeHardness, specifier: "%.1f")")
          }
        }
        GridRow {
          Stepper(value: $state.coagBias, in: -8...8, step: 0.5) {
            Text("Bias \(state.coagBias, specifier: "%.1f")")
          }
          Stepper(value: $state.coagCoherenceStrength, in: 0...1, step: 0.1) {
            Text("Cohere \(state.coagCoherenceStrength, specifier: "%.1f")")
          }
          Stepper(value: $state.coagRandomness, in: 0...8, step: 0.25) {
            Text("Random \(state.coagRandomness, specifier: "%.2f")")
          }
        }
        GridRow {
          Stepper(value: $state.coagAdvectAmount, in: 0...16, step: 0.5) {
            Text("Advect \(state.coagAdvectAmount, specifier: "%.1f")")
          }
          Stepper(value: $state.coagRefresh, in: 0...1, step: 0.1) {
            Text("Refresh \(state.coagRefresh, specifier: "%.1f")")
          }
          Stepper(value: $state.coagSmear, in: 0...1, step: 0.05) {
            Text("Smear \(state.coagSmear, specifier: "%.2f")")
          }
        }
        GridRow {
          Stepper(value: $state.coagMaxFrames, in: 1...600) {
            Text("Max Frames \(state.coagMaxFrames)")
          }
          Picker("Backend", selection: $state.coagBackend) {
            ForEach(FeedbackRenderBackendOption.allCases) { option in
              Text(option.rawValue).tag(option)
            }
          }
          .frame(width: 170)
        }
      }

      Divider()
      Text("Modulation")
        .font(.subheadline)
      ModulationSlotRow(
        label: "coagulation_strength",
        source: $state.coagStrengthModSource,
        scale: $state.coagStrengthModScale,
        offset: $state.coagStrengthModOffset,
        samplingOverride: $state.coagStrengthModSamplingOverride,
        scaleRange: 0...64,
        offsetRange: 0...64,
        modulator: $state.coagStrengthModModulator,
        modulatorNames: modulatorNames
      )
      ModulationSlotRow(
        label: "edge_hardness",
        source: $state.coagEdgeModSource,
        scale: $state.coagEdgeModScale,
        offset: $state.coagEdgeModOffset,
        samplingOverride: $state.coagEdgeModSamplingOverride,
        modulator: $state.coagEdgeModModulator,
        modulatorNames: modulatorNames
      )
      ModulationSlotRow(
        label: "bias",
        source: $state.coagBiasModSource,
        scale: $state.coagBiasModScale,
        offset: $state.coagBiasModOffset,
        samplingOverride: $state.coagBiasModSamplingOverride,
        scaleRange: -8...8,
        modulator: $state.coagBiasModModulator,
        modulatorNames: modulatorNames
      )

      HStack {
        Button {
          state.runCoagulatedBlendSequenceRender()
        } label: {
          Label("Render Coagulated Blend", systemImage: "drop.triangle")
        }
      }
      Text(state.coagSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)

      QuickPreviewBand(state: state, requiresModulator: true) {
        state.runCoagulatedBlendSequenceRender()
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}
