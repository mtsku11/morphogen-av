import SwiftUI

/// Morphogenesis and Granular Mosaic. Morphogenesis never had a
/// WorkflowPanelView duplicate (it's a straight, if very large, port from
/// RenderPanelView). Granular Mosaic's WorkflowPanelView "quick" controls
/// were a strict subset of RenderPanelView's — RenderPanelView additionally
/// had Seed, Audio Weight, Pool Window, Anti-Repeat Cooldown, Coherence
/// Reach, and Spatial Coherence knobs — so no union is needed, just
/// RenderPanelView's fuller set with WorkflowPanelView's primary/advanced
/// split preference.

struct MorphogenesisDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .morphogenesis)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Picker("Preset", selection: $state.morphogenesisPreset) {
          ForEach(MorphogenesisPresetOption.allCases) { preset in
            Text(preset.rawValue).tag(preset)
          }
        }
        .frame(width: 160)
        .help("Named (feed, kill) atlas points — most of that space is dead (uniform grey).")

        Picker("Output", selection: $state.morphogenesisOutputView) {
          ForEach(MorphogenesisOutputViewOption.allCases) { view in
            Text(view.rawValue).tag(view)
          }
        }
        .frame(width: 160)
        .help(
          "Composite: the pattern-mix/displace look. Field: the raw V field, greyscale — "
            + "the composite knobs below stay legal but inert in this view."
        )

        Stepper(value: $state.morphogenesisPatternMix, in: 0...1, step: 0.05) {
          Text("Pattern Mix \(state.morphogenesisPatternMix, specifier: "%.2f")")
        }
        .frame(width: 180, alignment: .leading)
        .help("V-weighted colourize tint strength; 0 = the carrier passes through unmodified.")

        Stepper(value: $state.morphogenesisDisplace, in: -64...64, step: 1) {
          Text("Displace \(state.morphogenesisDisplace, specifier: "%.0f")px")
        }
        .frame(width: 150, alignment: .leading)
        .help("Pixel displacement pushing the carrier sample along the growth gradient.")
      }

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Picker("Model", selection: $state.morphogenesisModel) {
          ForEach(MorphogenesisModelOption.allCases) { model in
            Text(model.rawValue).tag(model)
          }
        }
        .frame(width: 190)
        .help(
          "Gray-Scott: patterns that grow and settle. FitzHugh-Nagumo: an excitable medium — "
            + "travelling pulse waves, never settles. FHN knobs below stay legal but inert "
            + "in Gray-Scott."
        )

        Picker("FHN Preset", selection: $state.morphogenesisFhnPreset) {
          ForEach(FhnPresetOption.allCases) { preset in
            Text(preset.rawValue).tag(preset)
          }
        }
        .frame(width: 170)
        .disabled(state.morphogenesisModel != .fitzhughNagumo)
        .help("Pulse: fires and dies out. Spiral: self-sustaining rotors. Labyrinth: standing structure.")
      }

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Stepper(value: $state.morphogenesisPatternHue, in: 0...1, step: 0.02) {
            Text("Hue \(state.morphogenesisPatternHue, specifier: "%.2f")")
          }
          .frame(width: 130, alignment: .leading)
          .disabled(state.morphogenesisPatternColorMode == .inherit)
          .help("Fixed tint hue (turns); ignored in Inherit mode.")

          Picker("Colour", selection: $state.morphogenesisPatternColorMode) {
            ForEach(MorphogenesisColorModeOption.allCases) { mode in
              Text(mode.rawValue).tag(mode)
            }
          }
          .frame(width: 160)
          .help("Hue tints toward a fixed colour; Inherit tints toward the sample's own hue.")

          Stepper(value: $state.morphogenesisParamMapStrength, in: 0...4, step: 0.1) {
            Text("Param Map \(state.morphogenesisParamMapStrength, specifier: "%.2f")")
          }
          .frame(width: 170, alignment: .leading)
          .help("Strength of the carrier-luma-driven (feed, kill) shift; 0 = uniform chemistry.")
        }

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Stepper(value: $state.morphogenesisSeedThreshold, in: 0...1, step: 0.05) {
            Text("Seed Threshold \(state.morphogenesisSeedThreshold, specifier: "%.2f")")
          }
          .frame(width: 190, alignment: .leading)
          .help("Frame-zero carrier luma at/above this seeds the growth field.")

          Stepper(value: $state.morphogenesisSimScale, in: 1...8, step: 1) {
            Text("Sim Scale \(state.morphogenesisSimScale)")
          }
          .frame(width: 150, alignment: .leading)
          .help("Sim resolution divisor relative to the carrier frame; 1 = full res.")

          Stepper(value: $state.morphogenesisSubsteps, in: 0...64, step: 1) {
            Text("Substeps \(state.morphogenesisSubsteps)")
          }
          .frame(width: 150, alignment: .leading)
          .help("Gray-Scott substeps per output frame; 0 freezes the field at its seed.")
        }

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Stepper(value: $state.morphogenesisInject, in: 0...1, step: 0.01) {
            Text("Inject \(state.morphogenesisInject, specifier: "%.2f")")
          }
          .frame(width: 150, alignment: .leading)
          .help("Per-frame V source strength; 0 = off (the pre-Live-Coupling behaviour).")

          Stepper(value: $state.morphogenesisErode, in: 0...1, step: 0.01) {
            Text("Erode \(state.morphogenesisErode, specifier: "%.2f")")
          }
          .frame(width: 150, alignment: .leading)
          .help("Per-frame V sink strength (the same weight field as Inject); 0 = off.")

          if state.morphogenesisInject > 0 {
            Picker("Inject Source", selection: $state.morphogenesisInjectSource) {
              ForEach(MorphogenesisInjectSourceOption.allCases) { source in
                Text(source.rawValue).tag(source)
              }
            }
            .frame(width: 190)
            .help("Which weight field Inject/Erode read: bright regions (Luma) or motion.")
          }

          Stepper(value: $state.morphogenesisCoverageTarget, in: 0...1, step: 0.05) {
            Text("Coverage Target \(state.morphogenesisCoverageTarget, specifier: "%.2f")")
          }
          .frame(width: 200, alignment: .leading)
          .help("Homeostat target for mean(V); 0 = off (no coverage feedback).")
        }

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Stepper(value: $state.morphogenesisShade, in: 0...1, step: 0.05) {
            Text("Shade \(state.morphogenesisShade, specifier: "%.2f")")
          }
          .frame(width: 150, alignment: .leading)
          .help("Relief-shading blend strength; 0 = off (the pre-shading tint/field look).")

          Stepper(value: $state.morphogenesisShadeHeight, in: 0...32, step: 0.5) {
            Text("Shade Height \(state.morphogenesisShadeHeight, specifier: "%.1f")")
          }
          .frame(width: 180, alignment: .leading)
          .help("Gradient→normal scale for the relief-shading surface.")

          Stepper(value: $state.morphogenesisShadeAzimuth, in: 0...1, step: 0.02) {
            Text("Shade Azimuth \(state.morphogenesisShadeAzimuth, specifier: "%.2f")")
          }
          .frame(width: 190, alignment: .leading)
          .help("Light azimuth, turns (wraps).")
        }

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Stepper(value: $state.morphogenesisShadeElevation, in: 0...0.25, step: 0.01) {
            Text("Shade Elevation \(state.morphogenesisShadeElevation, specifier: "%.2f")")
          }
          .frame(width: 200, alignment: .leading)
          .help("Light elevation above the horizon, turns.")

          Stepper(value: $state.morphogenesisShadeSpecular, in: 0...1, step: 0.05) {
            Text("Shade Specular \(state.morphogenesisShadeSpecular, specifier: "%.2f")")
          }
          .frame(width: 190, alignment: .leading)
          .help("Specular highlight strength; 0 = purely diffuse.")

          Stepper(value: $state.morphogenesisShadeShininess, in: 1...128, step: 1) {
            Text("Shininess \(state.morphogenesisShadeShininess, specifier: "%.0f")")
          }
          .frame(width: 150, alignment: .leading)
          .help("Specular exponent (Phong shininess).")
        }

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Stepper(value: $state.morphogenesisFhnEpsilon, in: 0.01...0.5, step: 0.01) {
            Text("Epsilon \(state.morphogenesisFhnEpsilon, specifier: "%.2f")")
          }
          .frame(width: 150, alignment: .leading)
          .disabled(state.morphogenesisModel != .fitzhughNagumo)
          .help("Recovery time-scale separation; small = slower recovery = longer pulses.")

          Stepper(value: $state.morphogenesisFhnA, in: -2...2, step: 0.05) {
            Text("A \(state.morphogenesisFhnA, specifier: "%.2f")")
          }
          .frame(width: 110, alignment: .leading)
          .disabled(state.morphogenesisModel != .fitzhughNagumo)
          .help("FHN nullcline shape parameter a.")

          Stepper(value: $state.morphogenesisFhnB, in: 0.1...2, step: 0.05) {
            Text("B \(state.morphogenesisFhnB, specifier: "%.2f")")
          }
          .frame(width: 110, alignment: .leading)
          .disabled(state.morphogenesisModel != .fitzhughNagumo)
          .help("FHN nullcline shape parameter b.")

          Stepper(value: $state.morphogenesisFhnStimulus, in: 0.5...6, step: 0.1) {
            Text("Stimulus \(state.morphogenesisFhnStimulus, specifier: "%.1f")")
          }
          .frame(width: 150, alignment: .leading)
          .disabled(state.morphogenesisModel != .fitzhughNagumo)
          .help("How far above resting u a seeded/injected cell is pushed.")
        }

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
          ModulationSlotRow(
            label: "Feed",
            source: $state.morphogenesisModFeedSource,
            scale: $state.morphogenesisModFeedScale,
            offset: $state.morphogenesisModFeedOffset,
            samplingOverride: $state.morphogenesisModFeedSamplingOverride,
            scaleRange: -0.12...0.12, scaleStep: 0.005, offsetRange: -0.12...0.12, offsetStep: 0.005,
            modulator: $state.morphogenesisModFeedModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModFeedLfoShape,
            lfoRate: $state.morphogenesisModFeedLfoRate,
            lfoPhase: $state.morphogenesisModFeedLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModFeedMidiCc
          )

          ModulationSlotRow(
            label: "Kill",
            source: $state.morphogenesisModKillSource,
            scale: $state.morphogenesisModKillScale,
            offset: $state.morphogenesisModKillOffset,
            samplingOverride: $state.morphogenesisModKillSamplingOverride,
            scaleRange: -0.12...0.12, scaleStep: 0.005, offsetRange: -0.12...0.12, offsetStep: 0.005,
            modulator: $state.morphogenesisModKillModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModKillLfoShape,
            lfoRate: $state.morphogenesisModKillLfoRate,
            lfoPhase: $state.morphogenesisModKillLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModKillMidiCc
          )

          ModulationSlotRow(
            label: "Param Map",
            source: $state.morphogenesisModParamMapStrengthSource,
            scale: $state.morphogenesisModParamMapStrengthScale,
            offset: $state.morphogenesisModParamMapStrengthOffset,
            samplingOverride: $state.morphogenesisModParamMapStrengthSamplingOverride,
            modulator: $state.morphogenesisModParamMapStrengthModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModParamMapStrengthLfoShape,
            lfoRate: $state.morphogenesisModParamMapStrengthLfoRate,
            lfoPhase: $state.morphogenesisModParamMapStrengthLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModParamMapStrengthMidiCc
          )

          ModulationSlotRow(
            label: "Pattern Mix",
            source: $state.morphogenesisModPatternMixSource,
            scale: $state.morphogenesisModPatternMixScale,
            offset: $state.morphogenesisModPatternMixOffset,
            samplingOverride: $state.morphogenesisModPatternMixSamplingOverride,
            modulator: $state.morphogenesisModPatternMixModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModPatternMixLfoShape,
            lfoRate: $state.morphogenesisModPatternMixLfoRate,
            lfoPhase: $state.morphogenesisModPatternMixLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModPatternMixMidiCc
          )

          ModulationSlotRow(
            label: "Displace",
            source: $state.morphogenesisModDisplaceSource,
            scale: $state.morphogenesisModDisplaceScale,
            offset: $state.morphogenesisModDisplaceOffset,
            samplingOverride: $state.morphogenesisModDisplaceSamplingOverride,
            scaleRange: -256...256, scaleStep: 8, offsetRange: -256...256, offsetStep: 8,
            modulator: $state.morphogenesisModDisplaceModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModDisplaceLfoShape,
            lfoRate: $state.morphogenesisModDisplaceLfoRate,
            lfoPhase: $state.morphogenesisModDisplaceLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModDisplaceMidiCc
          )

          ModulationSlotRow(
            label: "Inject",
            source: $state.morphogenesisModInjectSource,
            scale: $state.morphogenesisModInjectScale,
            offset: $state.morphogenesisModInjectOffset,
            samplingOverride: $state.morphogenesisModInjectSamplingOverride,
            modulator: $state.morphogenesisModInjectModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModInjectLfoShape,
            lfoRate: $state.morphogenesisModInjectLfoRate,
            lfoPhase: $state.morphogenesisModInjectLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModInjectMidiCc
          )

          ModulationSlotRow(
            label: "Erode",
            source: $state.morphogenesisModErodeSource,
            scale: $state.morphogenesisModErodeScale,
            offset: $state.morphogenesisModErodeOffset,
            samplingOverride: $state.morphogenesisModErodeSamplingOverride,
            modulator: $state.morphogenesisModErodeModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModErodeLfoShape,
            lfoRate: $state.morphogenesisModErodeLfoRate,
            lfoPhase: $state.morphogenesisModErodeLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModErodeMidiCc
          )

          ModulationSlotRow(
            label: "Coverage Target",
            source: $state.morphogenesisModCoverageTargetSource,
            scale: $state.morphogenesisModCoverageTargetScale,
            offset: $state.morphogenesisModCoverageTargetOffset,
            samplingOverride: $state.morphogenesisModCoverageTargetSamplingOverride,
            modulator: $state.morphogenesisModCoverageTargetModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModCoverageTargetLfoShape,
            lfoRate: $state.morphogenesisModCoverageTargetLfoRate,
            lfoPhase: $state.morphogenesisModCoverageTargetLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModCoverageTargetMidiCc
          )

          ModulationSlotRow(
            label: "Shade",
            source: $state.morphogenesisModShadeSource,
            scale: $state.morphogenesisModShadeScale,
            offset: $state.morphogenesisModShadeOffset,
            samplingOverride: $state.morphogenesisModShadeSamplingOverride,
            modulator: $state.morphogenesisModShadeModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModShadeLfoShape,
            lfoRate: $state.morphogenesisModShadeLfoRate,
            lfoPhase: $state.morphogenesisModShadeLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModShadeMidiCc
          )

          ModulationSlotRow(
            label: "Shade Azimuth",
            source: $state.morphogenesisModShadeAzimuthSource,
            scale: $state.morphogenesisModShadeAzimuthScale,
            offset: $state.morphogenesisModShadeAzimuthOffset,
            samplingOverride: $state.morphogenesisModShadeAzimuthSamplingOverride,
            scaleRange: 0...1, scaleStep: 0.05, offsetRange: 0...1, offsetStep: 0.05,
            modulator: $state.morphogenesisModShadeAzimuthModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModShadeAzimuthLfoShape,
            lfoRate: $state.morphogenesisModShadeAzimuthLfoRate,
            lfoPhase: $state.morphogenesisModShadeAzimuthLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModShadeAzimuthMidiCc
          )

          ModulationSlotRow(
            label: "Shade Height",
            source: $state.morphogenesisModShadeHeightSource,
            scale: $state.morphogenesisModShadeHeightScale,
            offset: $state.morphogenesisModShadeHeightOffset,
            samplingOverride: $state.morphogenesisModShadeHeightSamplingOverride,
            scaleRange: 0...64, scaleStep: 0.5, offsetRange: 0...64, offsetStep: 0.5,
            modulator: $state.morphogenesisModShadeHeightModulator,
            modulatorNames: state.morphogenesisDeclaredModulatorNames,
            lfoShape: $state.morphogenesisModShadeHeightLfoShape,
            lfoRate: $state.morphogenesisModShadeHeightLfoRate,
            lfoPhase: $state.morphogenesisModShadeHeightLfoPhase,
            midiAvailable: true,
            midiCcNumber: $state.morphogenesisModShadeHeightMidiCc
          )

          ModulationMediaRow(
            sources: [
              state.morphogenesisModFeedSource, state.morphogenesisModKillSource,
              state.morphogenesisModParamMapStrengthSource, state.morphogenesisModPatternMixSource,
              state.morphogenesisModDisplaceSource,
              state.morphogenesisModInjectSource, state.morphogenesisModErodeSource,
              state.morphogenesisModCoverageTargetSource,
              state.morphogenesisModShadeSource, state.morphogenesisModShadeAzimuthSource,
              state.morphogenesisModShadeHeightSource,
            ],
            audioURL: state.morphogenesisModulatorAudioURL,
            framesURL: state.morphogenesisModulatorFramesURL,
            sampling: $state.morphogenesisModSampling,
            chooseAudio: { state.chooseMorphogenesisModulatorWAV() },
            chooseFrames: { state.chooseMorphogenesisModulatorFrames() },
            midiURL: state.morphogenesisModulatorMidiURL,
            chooseMidi: { state.chooseMorphogenesisModulatorMIDI() }
          )

          NamedModulatorsSection(
            modulators: $state.morphogenesisNamedModulators,
            onAdd: { state.addMorphogenesisNamedModulator() },
            onRemove: { state.removeMorphogenesisNamedModulator(id: $0) },
            chooseAudio: { state.chooseMorphogenesisNamedModulatorWAV(id: $0) },
            chooseFrames: { state.chooseMorphogenesisNamedModulatorFrames(id: $0) },
            chooseMidi: { state.chooseMorphogenesisNamedModulatorMIDI(id: $0) }
          )
        }
      }

      Button {
        state.runMorphogenesisSequenceRender()
      } label: {
        Label("Run Morphogenesis", systemImage: EffectListing.morphogenesis.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.morphogenesisSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

    }
  }
}

struct GranularMosaicDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .granularMosaic)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Stepper(value: $state.granularPoolGrainSize, in: 4...256, step: 4) {
          Text("Grain \(state.granularPoolGrainSize)px")
        }
        .frame(width: 150, alignment: .leading)

        Stepper(value: $state.granularPoolRearrangement, in: 0...1, step: 0.05) {
          Text("Rearrange \(state.granularPoolRearrangement, specifier: "%.2f")")
        }
        .frame(width: 180, alignment: .leading)

        Toggle("Audio-Weighted (RMS)", isOn: $state.granularPoolAudioWeighted)
          .toggleStyle(.checkbox)
      }

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Stepper(value: $state.granularPoolVariation, in: 0...1, step: 0.05) {
            Text("Variation \(state.granularPoolVariation, specifier: "%.2f")")
          }
          .frame(width: 170, alignment: .leading)

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

          Toggle("Spectral Centroid (k=2)", isOn: $state.granularPoolCentroidEnabled)
            .toggleStyle(.checkbox)
        }

        Stepper(value: $state.granularPoolWindow, in: 0...512, step: 1) {
          Text(state.granularPoolWindow == 0
            ? "Pool Window: whole clip"
            : "Pool Window \(state.granularPoolWindow)")
        }
        .frame(width: 230, alignment: .leading)

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        Stepper(value: $state.granularPoolSpatialCoherenceWeight, in: 0...8, step: 0.1) {
          Text("Spatial \(state.granularPoolSpatialCoherenceWeight, specifier: "%.1f")")
        }
        .frame(width: 190, alignment: .leading)
        .help("Rewards grain-origin continuity within a frame; shares the coherence Reach.")

        Picker("Backend", selection: $state.granularPoolBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 220)
      }

      Button {
        state.runGranularMosaicPoolSequenceRender()
      } label: {
        Label("Run Grain Pool", systemImage: EffectListing.granularMosaic.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.granularPoolSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

    }
  }
}
