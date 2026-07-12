import SwiftUI

/// Controlled Datamosh, Bitstream Datamosh, Cascade Collage, Trail Cascade.
///
/// Datamosh and Bitstream Datamosh merge former WorkflowPanelView +
/// RenderPanelView duplicates. Note RenderPanelView's Datamosh section was
/// missing "Reuse flow cache" (`state.datamoshReuseFlowCache`) — present
/// only in WorkflowPanelView despite RenderPanelView otherwise being the
/// fuller copy — so the merge here is a genuine union of both, not just
/// "take RenderPanelView's version." Cascade Collage only ever existed in
/// RenderPanelView; Trail Cascade only ever existed in WorkflowPanelView
/// (the milestone doc's "8 overlapping effects" list includes Trail Cascade,
/// but RenderPanelView has no `cascadeFieldType`/`cascadeTileSize` section —
/// verified by grep, not just the doc's claim) — both are straight ports.

struct DatamoshDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .datamosh)
        .help("Source A's per-frame optical flow repeatedly advects Source B's previous output — the \"bloom/melt\" look. Keyframes snap back to Source B.")

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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
        .frame(width: 260)
        .help("FFglitch-style motion-vector remix on the block-MV grid (needs Macroblock Size >= 2). Sort pools motion by magnitude; Shuffle permutes it by the seed. None = off.")

        Toggle("Reuse flow cache", isOn: $state.datamoshReuseFlowCache)
          .toggleStyle(.checkbox)
          .help("Cache Source A's optical flow and reuse it across renders. Changing datamosh knobs (block, amount, preset) then skips recomputing the flow — the slowest per-frame step. Turn off if Source A's content changes without re-extracting.")
      }

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        Stepper(value: $state.datamoshBlockSize, in: 1...64, step: 1) {
          Text("Macroblock Size \(state.datamoshBlockSize)")
        }
        .frame(width: 230, alignment: .leading)
        .help("1 = smooth per-pixel bloom; N >= 2 quantizes A's flow to NxN blocks so whole macroblocks slide (the chunky codec-simulated look).")

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        Stepper(value: $state.datamoshBlockRefreshThreshold, in: 0...8, step: 0.25) {
          Text("Block Refresh \(state.datamoshBlockRefreshThreshold, specifier: "%.2f")")
        }
        .frame(width: 230, alignment: .leading)
        .help("Per-block keep/drop: macroblocks whose mean motion is below this snap back to the carrier (intra-block refresh) while busier blocks rot. 0 = no per-block refresh; needs Macroblock Size >= 2.")

        if state.datamoshVectorRemix == .shuffle {
          Stepper(value: $state.datamoshRemixSeed, in: 0...9999, step: 1) {
            Text("Remix Seed \(state.datamoshRemixSeed)")
          }
          .frame(width: 180, alignment: .leading)
          .help("Deterministic permutation seed for Shuffle.")
        }

        Picker("Backend", selection: $state.datamoshBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 200)
        .help("Metal is gated per-frame against the CPU reference.")

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
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
        }
      }

      Button {
        state.runDatamoshRender()
      } label: {
        Label("Run Datamosh", systemImage: EffectListing.datamosh.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.datamoshSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

    }
  }
}

struct BitstreamDatamoshDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .bitstreamDatamosh)
        .help("Real AVI bitstream surgery: P-frame duplication, keyframe removal, or motion transfer via ffmpeg. Non-deterministic by design.")

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Button {
            state.chooseBitstreamInputVideo()
          } label: {
            Label("Input Video", systemImage: "film")
          }
          .help("Overrides the global Source A video. Falls back to Source A when unset.")

          Button {
            state.chooseBitstreamCarrierVideo()
          } label: {
            Label("Carrier Video", systemImage: "film")
          }
          .disabled(state.bitstreamOperation != .motionTransfer)
          .help("Overrides the global Source B video (Motion Transfer only). Falls back to Source B when unset.")

          Button {
            state.chooseBitstreamOutputDirectory()
          } label: {
            Label("Output Dir", systemImage: "folder")
          }
        }
      }

      Button {
        state.runBitstreamDatamoshRender()
      } label: {
        Label("Run Bitstream Datamosh", systemImage: EffectListing.bitstreamDatamosh.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.bitstreamSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }
}

struct CascadeCollageDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .cascadeCollage)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
          ModulationSlotRow(
            label: "Cc Scrib",
            source: $state.collageModScribSource,
            scale: $state.collageModScribScale,
            offset: $state.collageModScribOffset,
            samplingOverride: $state.collageModScribSamplingOverride,
            modulator: $state.collageModScribModulator,
            modulatorNames: state.cascadeCollageDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Cc Morph",
            source: $state.collageModMorphSource,
            scale: $state.collageModMorphScale,
            offset: $state.collageModMorphOffset,
            samplingOverride: $state.collageModMorphSamplingOverride,
            scaleRange: -0.5...0.5, scaleStep: 0.01,
            modulator: $state.collageModMorphModulator,
            modulatorNames: state.cascadeCollageDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Cc Edge",
            source: $state.collageModEdgeSource,
            scale: $state.collageModEdgeScale,
            offset: $state.collageModEdgeOffset,
            samplingOverride: $state.collageModEdgeSamplingOverride,
            scaleRange: -1...1, scaleStep: 0.05,
            modulator: $state.collageModEdgeModulator,
            modulatorNames: state.cascadeCollageDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Cc Face",
            source: $state.collageModFaceSource,
            scale: $state.collageModFaceScale,
            offset: $state.collageModFaceOffset,
            samplingOverride: $state.collageModFaceSamplingOverride,
            scaleRange: -1...1, scaleStep: 0.05,
            modulator: $state.collageModFaceModulator,
            modulatorNames: state.cascadeCollageDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [
              state.collageModScribSource, state.collageModMorphSource,
              state.collageModEdgeSource, state.collageModFaceSource
            ],
            audioURL: state.cascadeCollageModulatorAudioURL,
            framesURL: state.cascadeCollageModulatorFramesURL,
            sampling: $state.cascadeCollageModSampling,
            chooseAudio: { state.chooseCascadeCollageModulatorWAV() },
            chooseFrames: { state.chooseCascadeCollageModulatorFrames() }
          )
        }
      }

      Button {
        state.runCascadeCollageSequenceRender()
      } label: {
        Label("Run Cascade Collage", systemImage: EffectListing.cascadeCollage.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.cascadeCollageSummary)
        .font(.caption)
        .foregroundStyle(.secondary)

    }
  }
}

struct TrailCascadeDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .trailCascade)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
        Picker("Field", selection: $state.cascadeFieldType) {
          ForEach(CascadeFieldOption.allCases) { f in
            Text(f.rawValue).tag(f)
          }
        }
        .pickerStyle(.menu)
        .frame(width: 130)

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

      MoreKnobs {
        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
          // Row 1: always-visible knobs
          HStack(spacing: EffectDetailLayout.controlRowSpacing) {
            if state.cascadeFieldType == .vortex {
              Stepper(value: $state.cascadeTurbulenceScale, in: 0.002...0.05, step: 0.001) {
                Text("Vortex \(state.cascadeTurbulenceScale, specifier: "%.3f")")
              }
              .frame(width: 170, alignment: .leading)
              .help("Field scale: smaller = larger, broader vortices.")

              Stepper(value: $state.cascadeDetail, in: 0...1, step: 0.05) {
                Text("Detail \(state.cascadeDetail, specifier: "%.2f")")
              }
              .frame(width: 150, alignment: .leading)
            }

            Stepper(value: $state.cascadeSeed, in: 0...9999, step: 1) {
              Text("Seed \(state.cascadeSeed)")
            }
            .frame(width: 140, alignment: .leading)

            Stepper(value: $state.cascadeDecay, in: 0...0.5, step: 0.01) {
              Text("Decay \(state.cascadeDecay, specifier: "%.2f")")
            }
            .frame(width: 155, alignment: .leading)
            .help("Fade rate per frame. 0 = permanent trails. ~0.08–0.2 = squares fade as new ones appear.")

            if state.cascadeFieldType != .squarePop {
              Toggle("Live refresh", isOn: $state.cascadeLiveRefresh)
                .toggleStyle(.checkbox)
                .help("Re-sample each tile from the current frame so video plays through the trails.")

              Toggle("Temporal", isOn: $state.cascadeTemporalTiles)
                .toggleStyle(.checkbox)
                .help("Each tile carries a different moment of the clip — temporal slit-scan look.")
            }
          }

          // Row 2: field-specific knobs
          if state.cascadeFieldType == .river || state.cascadeFieldType == .riverRoot {
            HStack(spacing: EffectDetailLayout.controlRowSpacing) {
              Stepper(value: $state.cascadeRiverDirection, in: 0...360, step: 15) {
                Text("Dir \(state.cascadeRiverDirection, specifier: "%.0f")°")
              }
              .frame(width: 130, alignment: .leading)
              .help("Flow direction: 0°=right, 90°=down, 180°=left, 270°=up.")

              Stepper(value: $state.cascadeRiverSpeed, in: 0...20, step: 0.5) {
                Text("Speed \(state.cascadeRiverSpeed, specifier: "%.1f")")
              }
              .frame(width: 145, alignment: .leading)
              .help("Base flow speed in pixels per frame.")

              Stepper(value: $state.cascadeRiverTurbulence, in: 0...100, step: 1) {
                Text("Turbulence \(state.cascadeRiverTurbulence, specifier: "%.0f")")
              }
              .frame(width: 165, alignment: .leading)
              .help("Per-tile lateral jitter amplitude (px); 0 = perfectly uniform flow.")
            }
          }

          if state.cascadeFieldType == .centerSplit {
            HStack(spacing: EffectDetailLayout.controlRowSpacing) {
              Stepper(value: $state.cascadeRiverSpeed, in: 0...20, step: 0.5) {
                Text("Speed \(state.cascadeRiverSpeed, specifier: "%.1f")")
              }
              .frame(width: 145, alignment: .leading)
              .help("Outward flow speed — how fast tiles drift left/right from the centre.")

              Stepper(value: $state.cascadeRiverTurbulence, in: 0...200, step: 2) {
                Text("Oscillation \(state.cascadeRiverTurbulence, specifier: "%.0f")px")
              }
              .frame(width: 185, alignment: .leading)
              .help("Root-tile oscillation amplitude in both x and y (px). Above grid spacing = roots visibly cross into neighbours.")
            }
          }

          if state.cascadeFieldType == .oscillate {
            HStack(spacing: EffectDetailLayout.controlRowSpacing) {
              Stepper(value: $state.cascadeRiverTurbulence, in: 0...200, step: 2) {
                Text("Amplitude \(state.cascadeRiverTurbulence, specifier: "%.0f")px")
              }
              .frame(width: 185, alignment: .leading)
              .help("Per-tile oscillation radius in x and y. Above grid spacing = tiles paint into neighbours' territory.")
            }
          }

          if state.cascadeFieldType == .squarePop {
            HStack(spacing: EffectDetailLayout.controlRowSpacing) {
              Stepper(value: $state.cascadeRiverTurbulence, in: 0...500, step: 10) {
                Text("Scatter \(state.cascadeRiverTurbulence, specifier: "%.0f")px")
              }
              .frame(width: 175, alignment: .leading)
              .help("Max distance squares can appear from their home cell. 0 = static grid of outlines.")
            }
          }
        }

        // Cascade trails mod slots
        VStack(alignment: .leading, spacing: EffectDetailLayout.modGroupSpacing) {
          ModulationSlotRow(
            label: "Tr Advect",
            source: $state.trailsModAdvectSource,
            scale: $state.trailsModAdvectScale,
            offset: $state.trailsModAdvectOffset,
            samplingOverride: $state.trailsModAdvectSamplingOverride,
            scaleRange: -48...48, scaleStep: 1, offsetRange: -48...48, offsetStep: 1,
            modulator: $state.trailsModAdvectModulator,
            modulatorNames: state.cascadeTrailsDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Tr TurbSc",
            source: $state.trailsModTurbScaleSource,
            scale: $state.trailsModTurbScaleScale,
            offset: $state.trailsModTurbScaleOffset,
            samplingOverride: $state.trailsModTurbScaleSamplingOverride,
            scaleRange: -0.05...0.05, scaleStep: 0.002,
            modulator: $state.trailsModTurbScaleModulator,
            modulatorNames: state.cascadeTrailsDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Tr Detail",
            source: $state.trailsModDetailSource,
            scale: $state.trailsModDetailScale,
            offset: $state.trailsModDetailOffset,
            samplingOverride: $state.trailsModDetailSamplingOverride,
            modulator: $state.trailsModDetailModulator,
            modulatorNames: state.cascadeTrailsDeclaredModulatorNames
          )

          ModulationSlotRow(
            label: "Tr Decay",
            source: $state.trailsModDecaySource,
            scale: $state.trailsModDecayScale,
            offset: $state.trailsModDecayOffset,
            samplingOverride: $state.trailsModDecaySamplingOverride,
            scaleRange: -0.5...0.5, scaleStep: 0.01,
            modulator: $state.trailsModDecayModulator,
            modulatorNames: state.cascadeTrailsDeclaredModulatorNames
          )

          ModulationMediaRow(
            sources: [
              state.trailsModAdvectSource, state.trailsModTurbScaleSource,
              state.trailsModDetailSource, state.trailsModDecaySource
            ],
            audioURL: state.cascadeTrailsModulatorAudioURL,
            framesURL: state.cascadeTrailsModulatorFramesURL,
            sampling: $state.cascadeTrailsModSampling,
            chooseAudio: { state.chooseCascadeTrailsModulatorWAV() },
            chooseFrames: { state.chooseCascadeTrailsModulatorFrames() }
          )
        }
      }

      Button {
        state.runTrailCascadeSequenceRender()
      } label: {
        Label("Run Trail Cascade", systemImage: EffectListing.trailCascade.systemImage)
      }
      .buttonStyle(.borderedProminent)

    }
  }
}
