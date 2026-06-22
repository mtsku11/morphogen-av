//! Direct time-domain audio convolution.
//!
//! `convolve_mono` is the low-level full linear convolution. On top of it,
//! `impulse_convolution_blend` is the roadmap MVP's audio half of Convolutional
//! AV Blending: Source A is an impulse response (IR), Source B is the carrier,
//! and the output is B convolved with A's IR (convolution-reverb-style), blended
//! wet/dry by `amount`. CPU-only — no Metal path to parity-gate. See
//! `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

use crate::{AudioBufferF32, AudioError};

/// Audio-impulse convolution-blend render id.
pub const IMPULSE_CONVOLUTION_BLEND_ALGORITHM: &str =
    "impulse_response_convolution_blend_cpu_v1";

pub fn convolve_mono(input: &[f32], impulse: &[f32]) -> Result<Vec<f32>, AudioError> {
    if impulse.is_empty() {
        return Err(AudioError::InvalidSettings(
            "impulse response must contain at least one sample".to_string(),
        ));
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let output_len = input
        .len()
        .checked_add(impulse.len())
        .and_then(|len| len.checked_sub(1))
        .ok_or_else(|| {
            AudioError::InvalidSettings("convolution output is too large".to_string())
        })?;
    let mut output = vec![0.0; output_len];
    for (input_index, input_sample) in input.iter().enumerate() {
        for (impulse_index, impulse_sample) in impulse.iter().enumerate() {
            output[input_index + impulse_index] += input_sample * impulse_sample;
        }
    }

    Ok(output)
}

fn validate_amount(amount: f32) -> Result<(), AudioError> {
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(AudioError::InvalidSettings(
            "amount must be finite and within [0, 1]".to_string(),
        ));
    }
    Ok(())
}

/// Downmix `buffer` to a single mono channel (mean across channels per frame).
fn downmix_mono(buffer: &AudioBufferF32) -> Vec<f32> {
    if buffer.channels == 1 {
        return buffer.samples.clone();
    }
    let channels = buffer.channels as f32;
    (0..buffer.frames)
        .map(|frame| {
            let base = frame * buffer.channels;
            let sum: f32 = buffer.samples[base..base + buffer.channels].iter().sum();
            sum / channels
        })
        .collect()
}

/// Build the normalized mono impulse from Source A: optional head truncation,
/// then L1 normalization (so `Σ|tap| = 1`, bounding the wet path). A silent A
/// (`Σ|tap| ≈ 0`) falls back to a unit impulse `[1.0]` (identity ⇒ wet = B).
fn normalized_impulse(modulator: &AudioBufferF32, max_samples: Option<usize>) -> Vec<f32> {
    let mut taps = downmix_mono(modulator);
    if let Some(limit) = max_samples {
        taps.truncate(limit);
    }
    let l1: f32 = taps.iter().map(|t| t.abs()).sum();
    if l1 > 0.0 {
        for tap in &mut taps {
            *tap /= l1;
        }
        taps
    } else {
        // Silent A (or NaN-free zero) ⇒ identity impulse.
        vec![1.0]
    }
}

/// Convolution-reverb blend: convolve every Source B channel with the
/// L1-normalized mono IR derived from Source A, blended wet/dry by `amount`. The
/// output extends past B by `L − 1` samples (the reverb tail); the dry signal is
/// B zero-padded to that length. `amount = 0` returns B untouched.
pub fn impulse_convolution_blend(
    modulator: &AudioBufferF32,
    carrier: &AudioBufferF32,
    amount: f32,
    max_impulse_samples: Option<usize>,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }
    if modulator.sample_rate != carrier.sample_rate {
        return Err(AudioError::InvalidSettings(format!(
            "impulse ({} Hz) and carrier ({} Hz) sample rates must match; resampling is not supported",
            modulator.sample_rate, carrier.sample_rate
        )));
    }
    if let Some(0) = max_impulse_samples {
        return Err(AudioError::InvalidSettings(
            "max-impulse-samples must be greater than zero".to_string(),
        ));
    }
    if carrier.frames == 0 {
        return Ok(carrier.clone());
    }

    let impulse = normalized_impulse(modulator, max_impulse_samples);
    let channels = carrier.channels;
    let out_frames = carrier
        .frames
        .checked_add(impulse.len())
        .and_then(|len| len.checked_sub(1))
        .ok_or_else(|| {
            AudioError::InvalidSettings("convolution output is too large".to_string())
        })?;

    let mut samples = vec![0.0_f32; out_frames * channels];
    for channel in 0..channels {
        let dry: Vec<f32> = (0..carrier.frames)
            .map(|frame| carrier.samples[frame * channels + channel])
            .collect();
        let wet = convolve_mono(&dry, &impulse)?;
        for (frame, &wet_sample) in wet.iter().enumerate() {
            let dry_sample = dry.get(frame).copied().unwrap_or(0.0);
            samples[frame * channels + channel] =
                dry_sample + (wet_sample - dry_sample) * amount;
        }
    }

    AudioBufferF32::new(channels, carrier.sample_rate, samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(channels: usize, sr: u32, samples: Vec<f32>) -> AudioBufferF32 {
        AudioBufferF32::new(channels, sr, samples).expect("valid test buffer")
    }

    #[test]
    fn direct_convolution_matches_tiny_fixture() {
        let output = convolve_mono(&[1.0, 2.0], &[0.5, 0.5]).expect("convolve");

        assert_eq!(output, vec![0.5, 1.5, 1.0]);
    }

    #[test]
    fn amount_zero_is_byte_identical_passthrough() {
        let modulator = buf(1, 48_000, vec![1.0, 0.5, 0.25]);
        let carrier = buf(1, 48_000, vec![0.3, -0.2, 0.4, -0.1]);
        let out = impulse_convolution_blend(&modulator, &carrier, 0.0, None).expect("blend");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.frames, carrier.frames);
    }

    #[test]
    fn unit_impulse_at_full_amount_is_passthrough() {
        // A single positive tap L1-normalizes to [1.0] ⇒ wet == B, no tail.
        let modulator = buf(1, 48_000, vec![0.7]);
        let carrier = buf(1, 48_000, vec![0.3, -0.2, 0.4, -0.1]);
        let out = impulse_convolution_blend(&modulator, &carrier, 1.0, None).expect("blend");
        assert_eq!(out.frames, carrier.frames);
        for (o, c) in out.samples.iter().zip(&carrier.samples) {
            assert!((o - c).abs() < 1e-6, "expected passthrough, got {o} vs {c}");
        }
    }

    #[test]
    fn two_tap_averager_smooths_and_normalizes() {
        // IR [1,1] L1-normalizes to [0.5,0.5] — a moving-average lowpass.
        let modulator = buf(1, 48_000, vec![1.0, 1.0]);
        let carrier = buf(1, 48_000, vec![1.0, -1.0, 1.0, -1.0]);
        let out = impulse_convolution_blend(&modulator, &carrier, 1.0, None).expect("blend");
        // wet = conv([1,-1,1,-1],[0.5,0.5]) = [0.5, 0, 0, 0, -0.5]
        let expected = [0.5_f32, 0.0, 0.0, 0.0, -0.5];
        assert_eq!(out.frames, expected.len());
        for (o, e) in out.samples.iter().zip(&expected) {
            assert!((o - e).abs() < 1e-6, "got {o}, expected {e}");
        }
    }

    #[test]
    fn l1_normalization_bounds_wet_peak() {
        // A loud, long IR must not amplify a unit carrier beyond its own peak.
        let modulator = buf(1, 48_000, vec![3.0, -2.0, 4.0, 1.0]);
        let carrier = buf(1, 48_000, vec![1.0, -1.0, 1.0, -1.0, 1.0, -1.0]);
        let out = impulse_convolution_blend(&modulator, &carrier, 1.0, None).expect("blend");
        let peak = out.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(peak <= 1.0 + 1e-6, "L1 IR should not amplify past carrier peak, got {peak}");
    }

    #[test]
    fn silent_modulator_falls_back_to_identity() {
        let modulator = buf(1, 48_000, vec![0.0, 0.0, 0.0]);
        let carrier = buf(1, 48_000, vec![0.3, -0.2, 0.4, -0.1]);
        let out = impulse_convolution_blend(&modulator, &carrier, 1.0, None).expect("blend");
        assert_eq!(out.samples, carrier.samples);
    }

    #[test]
    fn output_extends_by_impulse_tail() {
        let modulator = buf(1, 48_000, vec![1.0, 0.5, 0.25, 0.1]); // L=4 ⇒ tail 3
        let carrier = buf(1, 48_000, vec![0.5; 5]);
        let out = impulse_convolution_blend(&modulator, &carrier, 1.0, None).expect("blend");
        assert_eq!(out.frames, 5 + 4 - 1);
    }

    #[test]
    fn max_impulse_samples_truncates_to_head() {
        // Truncating the IR to 1 sample ⇒ identity ⇒ passthrough length.
        let modulator = buf(1, 48_000, vec![0.5, 0.5, 0.5, 0.5]);
        let carrier = buf(1, 48_000, vec![0.3, -0.2, 0.4]);
        let out =
            impulse_convolution_blend(&modulator, &carrier, 1.0, Some(1)).expect("blend");
        assert_eq!(out.frames, carrier.frames);
        for (o, c) in out.samples.iter().zip(&carrier.samples) {
            assert!((o - c).abs() < 1e-6);
        }
    }

    #[test]
    fn stereo_carrier_convolves_each_channel() {
        let modulator = buf(1, 48_000, vec![1.0, 1.0]); // ⇒ [0.5,0.5]
        // Interleaved L/R: L = [1, 1], R = [-1, -1]
        let carrier = buf(2, 48_000, vec![1.0, -1.0, 1.0, -1.0]);
        let out = impulse_convolution_blend(&modulator, &carrier, 1.0, None).expect("blend");
        assert_eq!(out.channels, 2);
        assert_eq!(out.frames, 3);
        // L wet = conv([1,1],[0.5,0.5]) = [0.5,1.0,0.5]; R = negated.
        let expected = [0.5, -0.5, 1.0, -1.0, 0.5, -0.5];
        for (o, e) in out.samples.iter().zip(&expected) {
            assert!((o - e).abs() < 1e-6, "got {o}, expected {e}");
        }
    }

    #[test]
    fn rejects_sample_rate_mismatch_and_bad_amount() {
        let modulator = buf(1, 44_100, vec![1.0, 0.5]);
        let carrier = buf(1, 48_000, vec![0.3, -0.2]);
        assert!(impulse_convolution_blend(&modulator, &carrier, 1.0, None).is_err());
        let matched = buf(1, 48_000, vec![1.0, 0.5]);
        assert!(impulse_convolution_blend(&matched, &carrier, 1.5, None).is_err());
        assert!(impulse_convolution_blend(&matched, &carrier, f32::NAN, None).is_err());
        assert!(impulse_convolution_blend(&matched, &carrier, 1.0, Some(0)).is_err());
    }
}
