import SwiftUI

/// Fluid / Advection — one sidebar row, four run functions. The mode picker
/// (from the former WorkflowPanelView) selects which of
/// `runTwoSourceFluidAdvectSequenceRender` / `runOpticalFlowAdvectSequenceRender`
/// / `runProceduralFluidAdvectSequenceRender` / `runFieldParticlesSequenceRender`
/// the single Run button calls; the knob set is RenderPanelView's fuller one
/// (seed, turbulence speed/detail, particle spacing/size, modulation slots).

private enum FluidAdvectionMode: String, CaseIterable, Identifiable {
  case twoSource = "A to B"
  case selfFlow = "Self-Flow"
  case procedural = "Field"
  case particles = "Particles"

  var id: String { rawValue }
}

struct FluidAdvectionDetailView: View {
  @ObservedObject var state: AppState

  @State private var mode: FluidAdvectionMode = .twoSource

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .fluidAdvection)

      Picker("Mode", selection: $mode) {
        ForEach(FluidAdvectionMode.allCases) { mode in
          Text(mode.rawValue).tag(mode)
        }
      }
      .pickerStyle(.segmented)
      .frame(width: 420)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Stepper(value: $state.fluidReinject, in: 0...1, step: 0.01) {
          Text("Reinject \(state.fluidReinject, specifier: "%.2f")")
        }
        .frame(width: 165, alignment: .leading)

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
          .disabled(mode != .particles)
      }

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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
        }

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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
        }

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
          ModulationSlotRow(
            label: "Prt Advect",
            source: $state.particleModAdvectSource,
            scale: $state.particleModAdvectScale,
            offset: $state.particleModAdvectOffset,
            samplingOverride: $state.particleModAdvectSamplingOverride,
            scaleRange: -48...48, scaleStep: 1, offsetRange: -48...48, offsetStep: 1,
            modulator: $state.particleModAdvectModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Prt TurbSc",
            source: $state.particleModTurbScaleSource,
            scale: $state.particleModTurbScaleScale,
            offset: $state.particleModTurbScaleOffset,
            samplingOverride: $state.particleModTurbScaleSamplingOverride,
            scaleRange: -0.05...0.05, scaleStep: 0.002, offsetRange: -0.05...0.05, offsetStep: 0.002,
            modulator: $state.particleModTurbScaleModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Prt TurbSp",
            source: $state.particleModTurbSpeedSource,
            scale: $state.particleModTurbSpeedScale,
            offset: $state.particleModTurbSpeedOffset,
            samplingOverride: $state.particleModTurbSpeedSamplingOverride,
            scaleRange: -0.5...0.5, scaleStep: 0.01, offsetRange: -0.5...0.5, offsetStep: 0.01,
            modulator: $state.particleModTurbSpeedModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Prt Detail",
            source: $state.particleModDetailSource,
            scale: $state.particleModDetailScale,
            offset: $state.particleModDetailOffset,
            samplingOverride: $state.particleModDetailSamplingOverride,
            modulator: $state.particleModDetailModulator,
            modulatorNames: state.fluidDeclaredModulatorNames
          )

          // Procedural Fluid consumes all six slots; A-to-B Fluid and
          // Self-Flow consume only Flow Advect + Reinject (their commands
          // have no turbulence targets).
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
              state.particleModAdvectSource, state.particleModTurbScaleSource,
              state.particleModTurbSpeedSource, state.particleModDetailSource,
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
        }
      }

      Button {
        runSelectedMode()
      } label: {
        Label("Run \(mode.rawValue) Fluid", systemImage: EffectListing.fluidAdvection.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.fluidAdvectionSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }

  private func runSelectedMode() {
    switch mode {
    case .twoSource:
      state.runTwoSourceFluidAdvectSequenceRender()
    case .selfFlow:
      state.runOpticalFlowAdvectSequenceRender()
    case .procedural:
      state.runProceduralFluidAdvectSequenceRender()
    case .particles:
      state.runFieldParticlesSequenceRender()
    }
  }
}
