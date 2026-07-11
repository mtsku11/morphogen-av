import SwiftUI

/// Video Vocoder, Spectral Cross-Synthesis, Audio Impulse Convolution,
/// Audio-to-Video Route, Video-to-Audio Route. Video Vocoder is the one
/// former WorkflowPanelView/RenderPanelView overlap in this category, and
/// WorkflowPanelView's controls (Mode/Bands/Amount primary, Backend
/// advanced) were a strict subset of RenderPanelView's — no union needed.
/// The other four never had a Workflow duplicate.

struct VideoVocoderDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .videoVocoder)

      Picker("Mode", selection: $state.vocoderMode) {
        ForEach(VideoVocoderModeOption.allCases) { mode in
          Text(mode.rawValue).tag(mode)
        }
      }
      .pickerStyle(.segmented)
      .frame(width: 360)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        Picker("Backend", selection: $state.vocoderBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 220)
        .disabled(state.vocoderMode == .gain)
        .help("Metal is parity-gated and available in Match mode; Gain mode renders on the CPU.")
      }

      Button {
        state.runVideoVocoderSequenceRender()
      } label: {
        Label("Run Vocoder", systemImage: EffectListing.videoVocoder.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.vocoderSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }
}

struct SpectralCrossSynthDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .spectralCrossSynth)
        .help("Gain: A's RMS envelope drives B's amplitude. Filter: A's spectral centroid sweeps a one-pole cutoff on B. Vocode: A's log-band spectral envelope reweights B's spectrum through a real inverse STFT (B keeps its own phase).")

      Picker("Mode", selection: $state.crossSynthMode) {
        ForEach(CrossSynthModeOption.allCases) { mode in
          Text(mode.rawValue).tag(mode)
        }
      }
      .pickerStyle(.segmented)
      .frame(width: 560)

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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
        .disabled(state.crossSynthMode != .filter)
        .help("One-pole response (Filter mode only).")

        if state.crossSynthMode == .vocode {
          Stepper(value: $state.crossSynthVocodeBands, in: 1...512, step: 1) {
            Text("Bands \(state.crossSynthVocodeBands)")
          }
          .frame(width: 140, alignment: .leading)
          .help("Log-spaced spectral-envelope bands (must be at most half the FFT size).")
        }
      }

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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
      }

      Button {
        state.runSpectralCrossSynthRender()
      } label: {
        Label("Run Cross-Synth", systemImage: EffectListing.spectralCrossSynth.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.crossSynthSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }
}

struct AudioImpulseConvolutionDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .audioImpulseConvolution)
        .help("Convolve Source B (carrier) with Source A's L1-normalized impulse response. amount 0 = passthrough; the wet tail extends the output.")

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
          Toggle("FFT method (HQ)", isOn: $state.impulseConvUseFFT)
            .help("Frequency-domain convolution for long IRs; gated against the direct path.")

          Toggle("Resample IR", isOn: $state.impulseConvResample)
            .help("Resample A's IR to B's sample rate (Lanczos) instead of erroring on a mismatch.")

          Toggle("Per-channel IR", isOn: $state.impulseConvPerChannel)
            .help("True-stereo: convolve each carrier channel with its own IR from Source A instead of one mono downmix.")
        }
      }

      Button {
        state.runAudioImpulseConvolutionRender()
      } label: {
        Label("Run Impulse Convolution", systemImage: EffectListing.audioImpulseConvolution.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.impulseConvSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }
}

struct AudioVideoRouteDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .audioVideoRoute)
        .help("Source A's RMS envelope drives the per-frame displacement amount applied to Source B's frames.")

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

        Picker("Backend", selection: $state.audioRouteBackend) {
          ForEach(FeedbackRenderBackendOption.allCases) { backend in
            Text(backend.rawValue).tag(backend)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 200)
        .help("Metal is gated per-frame against the CPU reference.")
      }

      Button {
        state.runAudioVideoRouteRender()
      } label: {
        Label("Run Audio→Video Route", systemImage: EffectListing.audioVideoRoute.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.audioRouteSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }
}

struct VideoAudioRouteDetailView: View {
  @ObservedObject var state: AppState

  var body: some View {
    VStack(alignment: .leading, spacing: EffectDetailLayout.sectionSpacing) {
      EffectTitleView(listing: .videoAudioRoute)
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

      HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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

      MoreKnobs {
        Picker("Envelope", selection: $state.videoAudioRouteSampling) {
          ForEach(VideoAudioRouteSamplingOption.allCases) { sampling in
            Text(sampling.rawValue).tag(sampling)
          }
        }
        .pickerStyle(.segmented)
        .frame(width: 360)
        .help("Hold: the descriptor steps at each frame. Smooth: it linearly interpolates between frames (a continuous curve, no zipper stepping).")

        HStack(spacing: EffectDetailLayout.controlRowSpacing) {
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
      }

      Button {
        state.runVideoAudioRouteRender()
      } label: {
        Label("Run Video→Audio Route", systemImage: EffectListing.videoAudioRoute.systemImage)
      }
      .buttonStyle(.borderedProminent)

      Text(state.videoAudioRouteSummary)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }
}
