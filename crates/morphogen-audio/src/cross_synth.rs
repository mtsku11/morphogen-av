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
    rms::rms_envelope, spectral::spectral_centroid_from_magnitudes, stft::stft_magnitude_cache,
    AudioBufferF32, AudioError, StftConfig,
};

/// `gain` mode render id.
pub const RMS_GAIN_CROSS_SYNTH_ALGORITHM: &str = "rms_gain_cross_synth_cpu_v1";
/// `filter` mode render id.
pub const CENTROID_FILTER_CROSS_SYNTH_ALGORITHM: &str = "centroid_filter_cross_synth_cpu_v1";

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
        let centroid =
            spectral_centroid_from_magnitudes(&frame.magnitudes, stft_config.fft_size, modulator.sample_rate)?;
        times.push(frame.time_seconds);
        cnorm.push((centroid as f64 / nyquist_a).clamp(0.0, 1.0));
    }

    let channels = carrier.channels;
    let sr_b = carrier.sample_rate as f64;
    let nyquist_b = sr_b / 2.0;
    let two_pi = std::f64::consts::TAU;
    let mut samples = vec![0.0_f32; carrier.samples.len()];
    let mut lp = vec![0.0_f64; channels];
    let mut cursor = 0_usize;
    for frame in 0..carrier.frames {
        let t = frame as f64 / sr_b;
        hold_last(&times, &mut cursor, t);
        let fc = cnorm[cursor] * nyquist_b;
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
            assert!(s.abs() < 1e-6, "expected silence where A is silent, got {s}");
        }
        for &s in &out.samples[4..8] {
            assert!((s - 0.5).abs() < 1e-6, "expected carrier where A is loud, got {s}");
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
        let dark_out =
            centroid_filter_cross_synth(&dark, &carrier, cfg, FilterType::Lowpass, 1.0).expect("dark");
        // Bright A: Nyquist alternation ⇒ centroid ≈ Nyquist ⇒ cutoff high ⇒ B kept.
        let bright = buf(1, 4, vec![1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0]);
        let bright_out = centroid_filter_cross_synth(&bright, &carrier, cfg, FilterType::Lowpass, 1.0)
            .expect("bright");

        let dark_energy: f32 = dark_out.samples.iter().map(|s| s.abs()).sum();
        let bright_carrier_err: f32 = bright_out
            .samples
            .iter()
            .zip(&carrier.samples)
            .map(|(o, c)| (o - c).abs())
            .sum();
        assert!(dark_energy < 1e-6, "dark A should silence B, got {dark_energy}");
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
}
