import SwiftUI

/// Colour-group dispersion blend (two-source, deterministic). Source A owns tiles
/// via descriptor coagulation; those tiles then scatter and drift, blending both
/// sources. Modulate coagulation_strength / bias / scatter_amount / damping.
struct DispersionBlendPanelView: View {
  @ObservedObject var state: AppState

  var modulatorNames: [String] { state.disperseDeclaredModulatorNames }

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      Text("Dispersion Blend")
        .font(.headline)
      Text("Two-source: tiles are claimed by descriptor coagulation then scatter/drift. Uses shared Source A/B slots. Modulate coagulation_strength / bias / scatter_amount / damping.")
        .font(.caption)
        .foregroundStyle(.secondary)

      HStack {
        Button {
          state.chooseDispersionBlendOutputDirectory()
        } label: {
          Label("Output Folder", systemImage: "folder.badge.plus")
        }
      }
      Text(state.disperseOutputPath)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)

      Grid(alignment: .leading, horizontalSpacing: 20, verticalSpacing: 6) {
        GridRow {
          Stepper(value: $state.disperseBlockSize, in: 1...128) {
            Text("Block \(state.disperseBlockSize)")
          }
          Stepper(value: $state.disperseCoagulationStrength, in: 0...64, step: 1) {
            Text("Coagulate \(state.disperseCoagulationStrength, specifier: "%.0f")")
          }
          Stepper(value: $state.disperseBias, in: -8...8, step: 0.5) {
            Text("Bias \(state.disperseBias, specifier: "%.1f")")
          }
        }
        GridRow {
          Stepper(value: $state.disperseScatterAmount, in: 0...32, step: 0.5) {
            Text("Scatter \(state.disperseScatterAmount, specifier: "%.1f")")
          }
          Stepper(value: $state.disperseDamping, in: 0...0.999, step: 0.05) {
            Text("Damping \(state.disperseDamping, specifier: "%.2f")")
          }
          Stepper(value: $state.disperseDispersionRamp, in: 0...120) {
            Text("Ramp \(state.disperseDispersionRamp)")
          }
        }
        GridRow {
          Stepper(value: $state.disperseOwnershipRefresh, in: 0...1, step: 0.1) {
            Text("Refresh \(state.disperseOwnershipRefresh, specifier: "%.1f")")
          }
          Stepper(value: $state.disperseSmear, in: 0...1, step: 0.05) {
            Text("Smear \(state.disperseSmear, specifier: "%.2f")")
          }
          Stepper(value: $state.disperseMaxFrames, in: 1...600) {
            Text("Max Frames \(state.disperseMaxFrames)")
          }
        }
      }

      Divider()
      Text("Modulation")
        .font(.subheadline)
      ModulationSlotRow(
        label: "coagulation_strength",
        source: $state.disperseModStrengthSource,
        scale: $state.disperseModStrengthScale,
        offset: $state.disperseModStrengthOffset,
        samplingOverride: $state.disperseModStrengthSamplingOverride,
        scaleRange: 0...64,
        offsetRange: 0...64,
        modulator: $state.disperseModStrengthModulator,
        modulatorNames: modulatorNames
      )
      ModulationSlotRow(
        label: "bias",
        source: $state.disperseModBiasSource,
        scale: $state.disperseModBiasScale,
        offset: $state.disperseModBiasOffset,
        samplingOverride: $state.disperseModBiasSamplingOverride,
        scaleRange: -8...8,
        modulator: $state.disperseModBiasModulator,
        modulatorNames: modulatorNames
      )
      ModulationSlotRow(
        label: "scatter_amount",
        source: $state.disperseModScatterSource,
        scale: $state.disperseModScatterScale,
        offset: $state.disperseModScatterOffset,
        samplingOverride: $state.disperseModScatterSamplingOverride,
        scaleRange: -32...32,
        modulator: $state.disperseModScatterModulator,
        modulatorNames: modulatorNames
      )
      ModulationSlotRow(
        label: "damping",
        source: $state.disperseModDampingSource,
        scale: $state.disperseModDampingScale,
        offset: $state.disperseModDampingOffset,
        samplingOverride: $state.disperseModDampingSamplingOverride,
        scaleRange: -0.5...0.5, scaleStep: 0.01,
        modulator: $state.disperseModDampingModulator,
        modulatorNames: modulatorNames
      )
      ModulationMediaRow(
        sources: [
          state.disperseModStrengthSource, state.disperseModBiasSource,
          state.disperseModScatterSource, state.disperseModDampingSource
        ],
        audioURL: state.disperseModulatorAudioURL,
        framesURL: state.disperseModulatorFramesURL,
        sampling: $state.disperseModSampling,
        chooseAudio: { state.chooseDisperseModulatorWAV() },
        chooseFrames: { state.chooseDisperseModulatorFrames() }
      )

      HStack {
        Button {
          state.runDispersionBlendRender()
        } label: {
          Label("Render Dispersion Blend", systemImage: "flame")
        }
      }
      Text(state.disperseSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}
