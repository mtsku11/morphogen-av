//! Spectral audio cross-synthesis (time-domain MVP).
//!
//! Impose Source A's analysis envelope onto Source B's audio: B stays the
//! material, A only shapes B's amplitude (`gain`) or spectral brightness
//! (`filter`) over time. The output follows Source B (sample rate, channels,
//! length); A's descriptor is resolved by time-based hold-last lookup, so A and
//! B stay independent in rate and length. `amount = 0` is an exact B
//! passthrough in both modes. See `docs/SPECTRAL_CROSS_SYNTH_MILESTONE.md`.
//!
//! Our STFT is magnitude-only with no inverse, so true phase-vocoder spectral
//! resynthesis is out of scope here (the roadmap's high-quality tier).

use crate::{
    rms::rms_envelope,
    spectral::spectral_centroid_from_magnitudes,
    stft::stft_magnitude_cache,
    stft_complex::{istft_complex_mono, stft_complex_mono, validate_complex_stft_config},
    AudioBufferF32, AudioError, StftConfig,
};

/// `gain` mode render id.
pub const RMS_GAIN_CROSS_SYNTH_ALGORITHM: &str = "rms_gain_cross_synth_cpu_v1";
/// `filter` mode render id.
pub const CENTROID_FILTER_CROSS_SYNTH_ALGORITHM: &str = "centroid_filter_cross_synth_cpu_v1";
/// `vocode` mode render id.
pub const PHASE_VOCODER_CROSS_SYNTH_ALGORITHM: &str = "phase_vocoder_cross_synth_cpu_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    Lowpass,
    Highpass,
}

fn validate_amount(amount: f32) -> Result<(), AudioError> {
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(AudioError::InvalidSettings(
            "amount must be finite and within [0, 1]".to_string(),
        ));
    }
    Ok(())
}

/// Advance `cursor` to the latest entry whose time is `<= t` (times ascending,
/// queried in non-decreasing `t` order — a hold-last lookup).
fn hold_last(times: &[f64], cursor: &mut usize, t: f64) {
    while *cursor + 1 < times.len() && times[*cursor + 1] <= t {
        *cursor += 1;
    }
}

/// `gain` mode: A's peak-normalized RMS envelope modulates B's amplitude.
pub fn rms_gain_cross_synth(
    modulator: &AudioBufferF32,
    carrier: &AudioBufferF32,
    rms_window: usize,
    rms_hop: usize,
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }

    let env = rms_envelope(modulator, rms_window, rms_hop)?;
    if env.is_empty() {
        return Err(AudioError::InvalidSettings(
            "modulator produced no RMS frames (too short for the window)".to_string(),
        ));
    }
    let peak = env.iter().map(|f| f.rms).fold(0.0_f32, f32::max);
    let times: Vec<f64> = env.iter().map(|f| f.time_seconds).collect();
    let norm: Vec<f32> = env
        .iter()
        .map(|f| if peak > 0.0 { f.rms / peak } else { 0.0 })
        .collect();

    let channels = carrier.channels;
    let mut samples = vec![0.0_f32; carrier.samples.len()];
    let mut cursor = 0_usize;
    for frame in 0..carrier.frames {
        let t = frame as f64 / carrier.sample_rate as f64;
        hold_last(&times, &mut cursor, t);
        // out = B * lerp(1.0, a_norm, amount)
        let gain = 1.0 + (norm[cursor] - 1.0) * amount;
        for channel in 0..channels {
            let idx = frame * channels + channel;
            samples[idx] = carrier.samples[idx] * gain;
        }
    }

    AudioBufferF32::new(channels, carrier.sample_rate, samples)
}

/// Sweep a per-sample one-pole LP/HP filter on `carrier`, its cutoff driven by a
/// normalized envelope (`cutoff_norm[i]` ∈ `[0,1]` of B's Nyquist) resolved by
/// output time with a hold-last lookup over `times`. Output follows B (channels,
/// length); `amount` blends dry→wet. Shared by spectral cross-synth (centroid
/// envelope) and video-to-audio routing (a visual-descriptor envelope); callers
/// short-circuit `amount = 0` to a passthrough before calling. `times` and
/// `cutoff_norm` must be non-empty and equal length.
pub(crate) fn one_pole_filter_sweep(
    carrier: &AudioBufferF32,
    times: &[f64],
    cutoff_norm: &[f64],
    filter_type: FilterType,
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    let channels = carrier.channels;
    let sr_b = carrier.sample_rate as f64;
    let nyquist_b = sr_b / 2.0;
    let two_pi = std::f64::consts::TAU;
    let mut samples = vec![0.0_f32; carrier.samples.len()];
    let mut lp = vec![0.0_f64; channels];
    let mut cursor = 0_usize;
    for frame in 0..carrier.frames {
        let t = frame as f64 / sr_b;
        hold_last(times, &mut cursor, t);
        let fc = cutoff_norm[cursor] * nyquist_b;
        let alpha = (1.0 - (-two_pi * fc / sr_b).exp()).clamp(0.0, 1.0);
        for (channel, lp_ch) in lp.iter_mut().enumerate() {
            let idx = frame * channels + channel;
            let x = carrier.samples[idx] as f64;
            *lp_ch += alpha * (x - *lp_ch);
            let filtered = match filter_type {
                FilterType::Lowpass => *lp_ch,
                FilterType::Highpass => x - *lp_ch,
            };
            samples[idx] = (x + (filtered - x) * amount as f64) as f32;
        }
    }

    AudioBufferF32::new(channels, carrier.sample_rate, samples)
}

/// `filter` mode: A's spectral-centroid envelope sweeps a per-sample one-pole
/// filter cutoff on B.
pub fn centroid_filter_cross_synth(
    modulator: &AudioBufferF32,
    carrier: &AudioBufferF32,
    stft_config: StftConfig,
    filter_type: FilterType,
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }

    let cache = stft_magnitude_cache(modulator, stft_config)?;
    if cache.frames.is_empty() {
        return Err(AudioError::InvalidSettings(
            "modulator produced no STFT frames (too short for the FFT size)".to_string(),
        ));
    }
    let nyquist_a = modulator.sample_rate as f64 / 2.0;
    let mut times = Vec::with_capacity(cache.frames.len());
    let mut cnorm = Vec::with_capacity(cache.frames.len());
    for frame in &cache.frames {
        let centroid = spectral_centroid_from_magnitudes(
            &frame.magnitudes,
            stft_config.fft_size,
            modulator.sample_rate,
        )?;
        times.push(frame.time_seconds);
        cnorm.push((centroid as f64 / nyquist_a).clamp(0.0, 1.0));
    }

    one_pole_filter_sweep(carrier, &times, &cnorm, filter_type, amount)
}

/// Downmix an interleaved multi-channel buffer to mono `f64` samples (mean of
/// channels per frame — the same `mean_channels` convention as
/// `stft_magnitude_cache`).
fn mono_mix_f64(buffer: &AudioBufferF32) -> Vec<f64> {
    let channels = buffer.channels.max(1);
    (0..buffer.frames)
        .map(|frame| {
            let start = frame * channels;
            let sum: f64 = buffer.samples[start..start + channels]
                .iter()
                .map(|&s| s as f64)
                .sum();
            sum / channels as f64
        })
        .collect()
}

/// `bands + 1` strictly increasing bin-index boundaries in `[0, bin_count]`,
/// log-spaced (band `i` spans bins `[boundaries[i], boundaries[i + 1])`). Low
/// bands are narrow (fine resolution near DC), high bands wide — the standard
/// log-frequency banding shape. Degenerate collapses (possible when `bands` is
/// close to `bin_count`) are walked forward to stay strictly increasing;
/// callers must ensure `bands <= bin_count - 1` so the walk cannot overflow.
fn log_band_boundaries(bands: usize, bin_count: usize) -> Vec<usize> {
    let bin_count_f = bin_count as f64;
    let mut raw: Vec<f64> = (0..=bands)
        .map(|i| {
            let t = i as f64 / bands as f64;
            (bin_count_f + 1.0).powf(t) - 1.0
        })
        .collect();
    let scale = bin_count_f / raw[bands];
    for value in raw.iter_mut() {
        *value *= scale;
    }
    let mut boundaries: Vec<usize> = raw.iter().map(|v| v.round() as usize).collect();
    boundaries[0] = 0;
    boundaries[bands] = bin_count;
    for i in 1..=bands {
        if boundaries[i] <= boundaries[i - 1] {
            boundaries[i] = boundaries[i - 1] + 1;
        }
    }
    boundaries
}

/// Map each bin `0..bin_count` to its band index, given `boundaries` from
/// [`log_band_boundaries`].
fn bin_to_band_map(boundaries: &[usize], bin_count: usize) -> Vec<usize> {
    let bands = boundaries.len() - 1;
    let mut map = Vec::with_capacity(bin_count);
    let mut band = 0_usize;
    for bin in 0..bin_count {
        while band + 1 < bands && bin >= boundaries[band + 1] {
            band += 1;
        }
        map.push(band);
    }
    map
}

/// Mean magnitude per band.
fn band_means(magnitudes: &[f64], boundaries: &[usize]) -> Vec<f64> {
    let bands = boundaries.len() - 1;
    (0..bands)
        .map(|band| {
            let start = boundaries[band];
            let end = boundaries[band + 1].min(magnitudes.len());
            if end > start {
                magnitudes[start..end].iter().sum::<f64>() / (end - start) as f64
            } else {
                0.0
            }
        })
        .collect()
}

/// `vocode` mode: impose A's log-band spectral envelope onto B's complex
/// spectrum, keeping B's phase verbatim, then resynthesize with a real inverse
/// STFT. See `docs/PHASE_VOCODER_MILESTONE.md`.
pub fn phase_vocoder_cross_synth(
    modulator: &AudioBufferF32,
    carrier: &AudioBufferF32,
    stft_config: StftConfig,
    vocode_bands: usize,
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }
    validate_complex_stft_config(&stft_config)?;
    let bin_count = stft_config.fft_size / 2 + 1;
    if vocode_bands == 0 || vocode_bands > stft_config.fft_size / 2 {
        return Err(AudioError::InvalidSettings(format!(
            "vocode_bands must be between 1 and fft_size / 2 ({}), got {vocode_bands}",
            stft_config.fft_size / 2
        )));
    }

    // Analyze A (mixed to mono, matching the existing filter mode's convention)
    // into per-frame log-band envelopes, normalized by the global peak band
    // value across all A frames (silent A => all-zero envelope, not an error).
    let a_mono = mono_mix_f64(modulator);
    let a_frames = stft_complex_mono(&a_mono, stft_config, modulator.sample_rate)?;
    if a_frames.is_empty() {
        return Err(AudioError::InvalidSettings(
            "modulator produced no STFT frames (too short for the FFT size)".to_string(),
        ));
    }
    // Frames whose window reaches past the end of A are zero-padded (needed so
    // ISTFT can cover a full-length B — see `stft_complex`); a hard signal->0
    // truncation edge injects broadband energy that can outweigh a genuine
    // sustained plateau, so it must not be eligible to win the peak-normalization
    // search below. Fall back to the unfiltered set only if A is so short that
    // every frame is zero-padded (nothing else to normalize against).
    let fully_valid_count = a_frames
        .iter()
        .enumerate()
        .filter(|(index, _)| index * stft_config.hop_size + stft_config.fft_size <= a_mono.len())
        .count();
    let envelope_frames: &[_] = if fully_valid_count > 0 {
        &a_frames[..fully_valid_count]
    } else {
        &a_frames
    };
    let boundaries = log_band_boundaries(vocode_bands, bin_count);
    let bin_band = bin_to_band_map(&boundaries, bin_count);

    let mut times = Vec::with_capacity(envelope_frames.len());
    let mut envelopes = Vec::with_capacity(envelope_frames.len());
    let mut global_peak = 0.0_f64;
    for frame in envelope_frames {
        let magnitudes: Vec<f64> = (0..bin_count)
            .map(|bin| {
                (frame.real[bin] * frame.real[bin] + frame.imag[bin] * frame.imag[bin]).sqrt()
            })
            .collect();
        let env = band_means(&magnitudes, &boundaries);
        global_peak = env.iter().cloned().fold(global_peak, f64::max);
        times.push(frame.time_seconds);
        envelopes.push(env);
    }
    if global_peak > 0.0 {
        for env in envelopes.iter_mut() {
            for value in env.iter_mut() {
                *value /= global_peak;
            }
        }
    }

    // Shape each B channel independently through the same A envelope timeline.
    let channels = carrier.channels;
    let mut out_channels: Vec<Vec<f64>> = Vec::with_capacity(channels);
    for channel in 0..channels {
        let b_mono: Vec<f64> = (0..carrier.frames)
            .map(|frame| carrier.samples[frame * channels + channel] as f64)
            .collect();
        let mut b_frames = stft_complex_mono(&b_mono, stft_config, carrier.sample_rate)?;
        let mut cursor = 0_usize;
        for frame in b_frames.iter_mut() {
            hold_last(&times, &mut cursor, frame.time_seconds);
            let env = &envelopes[cursor];
            for bin in 0..bin_count {
                let e = env[bin_band[bin]];
                // out = lerp(B, B * E, amount) = B * (1 + (E - 1) * amount)
                let factor = 1.0 + (e - 1.0) * amount as f64;
                frame.real[bin] *= factor;
                frame.imag[bin] *= factor;
                // Mirror the negative-frequency bin so the scaled spectrum stays
                // conjugate-symmetric (E is real, so this preserves symmetry
                // exactly rather than relying on the forward FFT's own
                // near-symmetry surviving floating-point rounding).
                if bin != 0 && bin * 2 != stft_config.fft_size {
                    let mirror = stft_config.fft_size - bin;
                    frame.real[mirror] = frame.real[bin];
                    frame.imag[mirror] = -frame.imag[bin];
                }
            }
        }
        out_channels.push(istft_complex_mono(&b_frames, stft_config, carrier.frames)?);
    }

    let mut samples = vec![0.0_f32; carrier.samples.len()];
    for frame in 0..carrier.frames {
        for (channel, out) in out_channels.iter().enumerate() {
            samples[frame * channels + channel] = out[frame] as f32;
        }
    }
    AudioBufferF32::new(channels, carrier.sample_rate, samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::WindowFunction;

    fn buf(channels: usize, sr: u32, samples: Vec<f32>) -> AudioBufferF32 {
        AudioBufferF32::new(channels, sr, samples).expect("valid test buffer")
    }

    #[test]
    fn gain_amount_zero_is_byte_identical_passthrough() {
        let modulator = buf(1, 4, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        let carrier = buf(1, 4, vec![0.5; 8]);
        let out = rms_gain_cross_synth(&modulator, &carrier, 4, 4, 0.0).expect("gain");
        assert_eq!(out.samples, carrier.samples);
    }

    #[test]
    fn gain_transfers_modulator_envelope() {
        // A: silent first half, full second half. B: steady 0.5 tone.
        let modulator = buf(1, 4, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
        let carrier = buf(1, 4, vec![0.5; 8]);
        let out = rms_gain_cross_synth(&modulator, &carrier, 4, 4, 1.0).expect("gain");
        // First half (A silent) ⇒ ~0; second half (A peak) ⇒ unchanged 0.5.
        for &s in &out.samples[0..4] {
            assert!(
                s.abs() < 1e-6,
                "expected silence where A is silent, got {s}"
            );
        }
        for &s in &out.samples[4..8] {
            assert!(
                (s - 0.5).abs() < 1e-6,
                "expected carrier where A is loud, got {s}"
            );
        }
    }

    #[test]
    fn filter_amount_zero_is_byte_identical_passthrough() {
        let modulator = buf(1, 4, vec![1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0]);
        let carrier = buf(1, 4, vec![0.3, -0.2, 0.4, -0.1, 0.5, -0.3, 0.2, -0.4]);
        let cfg = StftConfig {
            fft_size: 4,
            hop_size: 4,
            window: WindowFunction::Rectangular,
        };
        let out = centroid_filter_cross_synth(&modulator, &carrier, cfg, FilterType::Lowpass, 0.0)
            .expect("filter");
        assert_eq!(out.samples, carrier.samples);
    }

    #[test]
    fn filter_brightness_controls_lowpass_cutoff() {
        let carrier = buf(1, 4, vec![0.3, -0.2, 0.4, -0.1, 0.5, -0.3, 0.2, -0.4]);
        let cfg = StftConfig {
            fft_size: 4,
            hop_size: 4,
            window: WindowFunction::Rectangular,
        };
        // Dark A: a DC/constant signal ⇒ centroid 0 ⇒ cutoff 0 ⇒ B fully removed.
        let dark = buf(1, 4, vec![1.0; 8]);
        let dark_out = centroid_filter_cross_synth(&dark, &carrier, cfg, FilterType::Lowpass, 1.0)
            .expect("dark");
        // Bright A: Nyquist alternation ⇒ centroid ≈ Nyquist ⇒ cutoff high ⇒ B kept.
        let bright = buf(1, 4, vec![1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0]);
        let bright_out =
            centroid_filter_cross_synth(&bright, &carrier, cfg, FilterType::Lowpass, 1.0)
                .expect("bright");

        let dark_energy: f32 = dark_out.samples.iter().map(|s| s.abs()).sum();
        let bright_carrier_err: f32 = bright_out
            .samples
            .iter()
            .zip(&carrier.samples)
            .map(|(o, c)| (o - c).abs())
            .sum();
        assert!(
            dark_energy < 1e-6,
            "dark A should silence B, got {dark_energy}"
        );
        let carrier_energy: f32 = carrier.samples.iter().map(|s| s.abs()).sum();
        assert!(
            bright_carrier_err < 0.25 * carrier_energy,
            "bright A should largely preserve B (err {bright_carrier_err} vs energy {carrier_energy})"
        );
    }

    #[test]
    fn rejects_out_of_range_amount() {
        let modulator = buf(1, 4, vec![1.0; 8]);
        let carrier = buf(1, 4, vec![0.5; 8]);
        assert!(rms_gain_cross_synth(&modulator, &carrier, 4, 4, 1.5).is_err());
        assert!(rms_gain_cross_synth(&modulator, &carrier, 4, 4, f32::NAN).is_err());
    }

    fn vocode_config() -> StftConfig {
        StftConfig {
            fft_size: 64,
            hop_size: 16,
            window: WindowFunction::Hann,
        }
    }

    /// A deterministic pseudo-noise-like signal, nonzero everywhere — a stand-in
    /// carrier for vocode tests.
    fn synthetic_carrier(len: usize) -> Vec<f32> {
        (0..len)
            .map(|i| {
                let t = i as f64;
                (0.5 * (0.7 * t).sin() + 0.3 * (1.9 * t + 0.3).sin()) as f32
            })
            .collect()
    }

    /// A modulator that is silent for the first half and a steady 300 Hz tone
    /// for the second half (sample rate 8000).
    fn half_silent_half_tone_modulator(len: usize, sample_rate: u32) -> Vec<f32> {
        (0..len)
            .map(|i| {
                if i < len / 2 {
                    0.0
                } else {
                    (std::f64::consts::TAU * 300.0 * i as f64 / sample_rate as f64).sin() as f32
                }
            })
            .collect()
    }

    #[test]
    fn vocode_amount_zero_is_byte_identical_passthrough() {
        let modulator = buf(1, 8000, half_silent_half_tone_modulator(1024, 8000));
        let carrier = buf(1, 8000, synthetic_carrier(1024));
        let out = phase_vocoder_cross_synth(&modulator, &carrier, vocode_config(), 8, 0.0)
            .expect("vocode");
        assert_eq!(out.samples, carrier.samples);
    }

    #[test]
    fn vocode_is_deterministic_across_two_runs() {
        let modulator = buf(1, 8000, half_silent_half_tone_modulator(1024, 8000));
        let carrier = buf(1, 8000, synthetic_carrier(1024));
        let out_a = phase_vocoder_cross_synth(&modulator, &carrier, vocode_config(), 8, 1.0)
            .expect("vocode run 1");
        let out_b = phase_vocoder_cross_synth(&modulator, &carrier, vocode_config(), 8, 1.0)
            .expect("vocode run 2");
        assert_eq!(out_a.samples, out_b.samples);
    }

    #[test]
    fn vocode_silent_modulator_yields_silence() {
        let modulator = buf(1, 8000, vec![0.0; 1024]);
        let carrier = buf(1, 8000, synthetic_carrier(1024));
        let out = phase_vocoder_cross_synth(&modulator, &carrier, vocode_config(), 8, 1.0)
            .expect("vocode");
        let max_abs: f32 = out.samples.iter().map(|s| s.abs()).fold(0.0, f32::max);
        assert!(max_abs < 1e-6, "expected silence, got max abs {max_abs}");
    }

    #[test]
    fn vocode_one_band_behaves_like_broadband_gain_envelope() {
        // A constant (non-varying, no discontinuity) nonzero modulator: with a
        // single band the envelope is one scalar per frame, and since A never
        // varies, every interior frame ties for the global peak and normalizes
        // to exactly 1.0 — i.e. a no-op broadband gain. A discontinuous
        // loud/silent step is deliberately avoided here: the step's edge itself
        // injects broadband energy that can (correctly, given a mean-magnitude
        // envelope) outweigh a sustained plateau, which would make an interior
        // sample of the plateau an unreliable proxy for "envelope ≈ 1".
        let sample_rate = 8000;
        let modulator = buf(1, sample_rate, vec![1.0; 1024]);
        let carrier_samples = synthetic_carrier(1024);
        let carrier = buf(1, sample_rate, carrier_samples.clone());
        let out = phase_vocoder_cross_synth(&modulator, &carrier, vocode_config(), 1, 1.0)
            .expect("vocode");

        // Interior (clear of the zero-padded tail edge): output ≈ carrier.
        let mut max_abs_err = 0.0_f32;
        for (got, want) in out.samples[64..900].iter().zip(&carrier_samples[64..900]) {
            max_abs_err = max_abs_err.max((got - want).abs());
        }
        assert!(
            max_abs_err < 1e-3,
            "expected carrier preserved under a constant 1-band envelope, err {max_abs_err}"
        );
    }

    #[test]
    fn vocode_rejects_bands_above_half_fft_size() {
        let modulator = buf(1, 8000, half_silent_half_tone_modulator(256, 8000));
        let carrier = buf(1, 8000, synthetic_carrier(256));
        let config = StftConfig {
            fft_size: 16,
            hop_size: 4,
            window: WindowFunction::Hann,
        };
        assert!(phase_vocoder_cross_synth(&modulator, &carrier, config, 9, 1.0).is_err());
        assert!(phase_vocoder_cross_synth(&modulator, &carrier, config, 8, 1.0).is_ok());
    }

    #[test]
    fn vocode_rejects_zero_bands() {
        let modulator = buf(1, 8000, half_silent_half_tone_modulator(256, 8000));
        let carrier = buf(1, 8000, synthetic_carrier(256));
        assert!(phase_vocoder_cross_synth(&modulator, &carrier, vocode_config(), 0, 1.0).is_err());
    }
}
