import SwiftUI

/// Fluid colour-sort mosaic (two-source, CPU-only, deterministic).
/// Tiles of both sources flow under a curl field and sort into colour groups.
/// Modulate cohesion / repulsion / fluid_strength / turbulence.
struct FluidMosaicPanelView: View {
  @ObservedObject var state: AppState

  var modulatorNames: [String] { state.mosaicDeclaredModulatorNames() }

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      Text("Fluid Colour-Sort Mosaic")
        .font(.headline)
      Text("Two-source: tiles sort into colour groups and flow under a curl field. Uses shared Source A/B slots. Modulate cohesion / repulsion / fluid_strength / turbulence.")
        .font(.caption)
        .foregroundStyle(.secondary)

      HStack {
        Button { state.chooseMosaicOutputDirectory() } label: {
          Label("Output Folder", systemImage: "folder.badge.plus")
        }
      }
      Text(state.mosaicOutputPath)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)

      Grid(alignment: .leading, horizontalSpacing: 20, verticalSpacing: 6) {
        GridRow {
          Stepper(value: $state.mosaicTileSize, in: 2...64) {
            Text("Tile \(state.mosaicTileSize)")
          }
          Stepper(value: $state.mosaicColorBins, in: 2...16) {
            Text("Bins \(state.mosaicColorBins)")
          }
          Stepper(value: $state.mosaicSettleIterations, in: 0...300, step: 10) {
            Text("Settle \(state.mosaicSettleIterations)")
          }
        }
        GridRow {
          Stepper(value: $state.mosaicCohesion, in: 0...0.5, step: 0.005) {
            Text("Cohesion \(state.mosaicCohesion, specifier: "%.3f")")
          }
          Stepper(value: $state.mosaicRepulsion, in: 0...10, step: 0.2) {
            Text("Repulsion \(state.mosaicRepulsion, specifier: "%.1f")")
          }
          Stepper(value: $state.mosaicFluidStrength, in: 0...5, step: 0.1) {
            Text("Fluid \(state.mosaicFluidStrength, specifier: "%.1f")")
          }
        }
        GridRow {
          Stepper(value: $state.mosaicDamping, in: 0...0.999, step: 0.02) {
            Text("Damp \(state.mosaicDamping, specifier: "%.2f")")
          }
          Stepper(value: $state.mosaicJitter, in: 0...1, step: 0.005) {
            Text("Jitter \(state.mosaicJitter, specifier: "%.3f")")
          }
          Stepper(value: $state.mosaicTurbulence, in: 0...5, step: 0.1) {
            Text("Turbulence \(state.mosaicTurbulence, specifier: "%.1f")")
          }
        }
        GridRow {
          Stepper(value: $state.mosaicFrames, in: 1...600, step: 10) {
            Text("Frames \(state.mosaicFrames)")
          }
        }
      }

      Divider()
      Text("Modulation")
        .font(.subheadline)
      ModulationSlotRow(
        label: "cohesion",
        source: $state.mosaicModCohesionSource,
        scale: $state.mosaicModCohesionScale,
        offset: $state.mosaicModCohesionOffset,
        samplingOverride: $state.mosaicModCohesionSamplingOverride,
        scaleRange: -0.5...0.5, scaleStep: 0.005,
        modulator: $state.mosaicModCohesionModulator,
        modulatorNames: modulatorNames
      )
      ModulationSlotRow(
        label: "repulsion",
        source: $state.mosaicModRepulsionSource,
        scale: $state.mosaicModRepulsionScale,
        offset: $state.mosaicModRepulsionOffset,
        samplingOverride: $state.mosaicModRepulsionSamplingOverride,
        scaleRange: -5...5, scaleStep: 0.2,
        modulator: $state.mosaicModRepulsionModulator,
        modulatorNames: modulatorNames
      )
      ModulationSlotRow(
        label: "fluid_strength",
        source: $state.mosaicModFluidSource,
        scale: $state.mosaicModFluidScale,
        offset: $state.mosaicModFluidOffset,
        samplingOverride: $state.mosaicModFluidSamplingOverride,
        scaleRange: -5...5, scaleStep: 0.2,
        modulator: $state.mosaicModFluidModulator,
        modulatorNames: modulatorNames
      )
      ModulationSlotRow(
        label: "turbulence",
        source: $state.mosaicModTurbulenceSource,
        scale: $state.mosaicModTurbulenceScale,
        offset: $state.mosaicModTurbulenceOffset,
        samplingOverride: $state.mosaicModTurbulenceSamplingOverride,
        scaleRange: -5...5, scaleStep: 0.2,
        modulator: $state.mosaicModTurbulenceModulator,
        modulatorNames: modulatorNames
      )
      ModulationMediaRow(
        sources: [
          state.mosaicModCohesionSource, state.mosaicModRepulsionSource,
          state.mosaicModFluidSource, state.mosaicModTurbulenceSource
        ],
        audioURL: state.mosaicModulatorAudioURL,
        framesURL: state.mosaicModulatorFramesURL,
        sampling: $state.mosaicModSampling,
        chooseAudio: { state.chooseMosaicModulatorWAV() },
        chooseFrames: { state.chooseMosaicModulatorFrames() }
      )

      HStack {
        Button { state.runFluidMosaicRender() } label: {
          Label("Render Fluid Mosaic", systemImage: "square.grid.3x3.fill")
        }
      }
      Text(state.mosaicSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)

      QuickPreviewBand(state: state, requiresModulator: true) {
        state.runFluidMosaicRender()
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}
