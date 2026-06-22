//! Direct time-domain audio convolution.
//!
//! `convolve_mono` is the low-level full linear convolution. On top of it,
//! `impulse_convolution_blend` is the roadmap MVP's audio half of Convolutional
//! AV Blending: Source A is an impulse response (IR), Source B is the carrier,
//! and the output is B convolved with A's IR (convolution-reverb-style), blended
//! wet/dry by `amount`. CPU-only — no Metal path to parity-gate. See
//! `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.
//!
//! Two HQ-tier knobs sit on top of the MVP: `ConvolutionMethod::Fft` swaps the
//! direct `O(B·L)` loop for a frequency-domain multiply (the same result up to
//! rounding, gated against `Direct` within [`FFT_DIRECT_PARITY_EPSILON`]); and
//! `resample_impulse` opt-in resamples A's IR to B's sample rate (deterministic
//! Lanczos) instead of erroring on a rate mismatch.

use crate::fft::convolve_via_fft;
use crate::{AudioBufferF32, AudioError};

/// Audio-impulse convolution-blend render id.
pub const IMPULSE_CONVOLUTION_BLEND_ALGORITHM: &str = "impulse_response_convolution_blend_cpu_v1";

/// Tolerance the FFT method is gated against the direct reference within. The
/// two paths are the same convolution; this bounds their floating-point drift
/// (analogous to the Metal/CPU parity epsilon for GPU kernels).
pub const FFT_DIRECT_PARITY_EPSILON: f32 = 1e-4;

/// Selects how each Source B channel is convolved with the impulse response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConvolutionMethod {
    /// Direct time-domain convolution (`O(B·L)`) — the reference path.
    #[default]
    Direct,
    /// Frequency-domain convolution via FFT (`O(N log N)`) — the HQ tier for long
    /// impulse responses. Gated against [`ConvolutionMethod::Direct`] within
    /// [`FFT_DIRECT_PARITY_EPSILON`].
    Fft,
}

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

/// Normalized-sinc Lanczos kernel (`a` lobes), the resampling interpolation
/// weight at fractional offset `x`.
fn lanczos_kernel(x: f64, a: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    if x.abs() >= a {
        return 0.0;
    }
    let px = std::f64::consts::PI * x;
    a * px.sin() * (px / a).sin() / (px * px)
}

/// Deterministically resample `taps` from `source_rate` to `target_rate` with a
/// 3-lobe Lanczos kernel. Equal rates short-circuit to an exact copy. When
/// downsampling the kernel widens (`filter_scale = ratio`) so it low-passes,
/// suppressing aliasing; weights are sum-normalized to preserve DC.
fn resample_lanczos(taps: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if taps.is_empty() || source_rate == target_rate {
        return taps.to_vec();
    }
    let ratio = target_rate as f64 / source_rate as f64;
    let out_len = (((taps.len() as f64) * ratio).round() as usize).max(1);
    let a = 3.0_f64;
    let filter_scale = ratio.min(1.0);
    let support = (a / filter_scale).ceil() as isize;

    (0..out_len)
        .map(|i| {
            let src_pos = i as f64 / ratio;
            let center = src_pos.round() as isize;
            let mut acc = 0.0_f64;
            let mut weight_sum = 0.0_f64;
            for k in (center - support)..=(center + support) {
                if k < 0 || k as usize >= taps.len() {
                    continue;
                }
                let weight = lanczos_kernel((src_pos - k as f64) * filter_scale, a);
                acc += taps[k as usize] as f64 * weight;
                weight_sum += weight;
            }
            if weight_sum != 0.0 {
                acc /= weight_sum;
            }
            acc as f32
        })
        .collect()
}

/// Build the normalized mono impulse from Source A: downmix, optional head
/// truncation (in A's sample domain), optional Lanczos resampling to
/// `target_rate`, then L1 normalization (so `Σ|tap| = 1`, bounding the wet path —
/// applied *after* resampling so the bound survives). A silent A (`Σ|tap| ≈ 0`)
/// falls back to a unit impulse `[1.0]` (identity ⇒ wet = B).
fn normalized_impulse(
    modulator: &AudioBufferF32,
    target_rate: u32,
    max_samples: Option<usize>,
    resample: bool,
) -> Vec<f32> {
    let mut taps = downmix_mono(modulator);
    if let Some(limit) = max_samples {
        taps.truncate(limit);
    }
    if resample {
        taps = resample_lanczos(&taps, modulator.sample_rate, target_rate);
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
    method: ConvolutionMethod,
    resample_impulse: bool,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }
    if modulator.sample_rate != carrier.sample_rate && !resample_impulse {
        return Err(AudioError::InvalidSettings(format!(
            "impulse ({} Hz) and carrier ({} Hz) sample rates must match unless --resample-impulse is set",
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

    let impulse = normalized_impulse(
        modulator,
        carrier.sample_rate,
        max_impulse_samples,
        resample_impulse,
    );
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
        let wet = match method {
            ConvolutionMethod::Direct => convolve_mono(&dry, &impulse)?,
            ConvolutionMethod::Fft => convolve_via_fft(&dry, &impulse)?,
        };
        for (frame, &wet_sample) in wet.iter().enumerate() {
            let dry_sample = dry.get(frame).copied().unwrap_or(0.0);
            samples[frame * channels + channel] = dry_sample + (wet_sample - dry_sample) * amount;
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

    /// The MVP path: direct method, no impulse resampling.
    fn blend(
        modulator: &AudioBufferF32,
        carrier: &AudioBufferF32,
        amount: f32,
        max: Option<usize>,
    ) -> Result<AudioBufferF32, AudioError> {
        impulse_convolution_blend(
            modulator,
            carrier,
            amount,
            max,
            ConvolutionMethod::Direct,
            false,
        )
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
        let out = blend(&modulator, &carrier, 0.0, None).expect("blend");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.frames, carrier.frames);
    }

    #[test]
    fn unit_impulse_at_full_amount_is_passthrough() {
        // A single positive tap L1-normalizes to [1.0] ⇒ wet == B, no tail.
        let modulator = buf(1, 48_000, vec![0.7]);
        let carrier = buf(1, 48_000, vec![0.3, -0.2, 0.4, -0.1]);
        let out = blend(&modulator, &carrier, 1.0, None).expect("blend");
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
        let out = blend(&modulator, &carrier, 1.0, None).expect("blend");
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
        let out = blend(&modulator, &carrier, 1.0, None).expect("blend");
        let peak = out.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(
            peak <= 1.0 + 1e-6,
            "L1 IR should not amplify past carrier peak, got {peak}"
        );
    }

    #[test]
    fn silent_modulator_falls_back_to_identity() {
        let modulator = buf(1, 48_000, vec![0.0, 0.0, 0.0]);
        let carrier = buf(1, 48_000, vec![0.3, -0.2, 0.4, -0.1]);
        let out = blend(&modulator, &carrier, 1.0, None).expect("blend");
        assert_eq!(out.samples, carrier.samples);
    }

    #[test]
    fn output_extends_by_impulse_tail() {
        let modulator = buf(1, 48_000, vec![1.0, 0.5, 0.25, 0.1]); // L=4 ⇒ tail 3
        let carrier = buf(1, 48_000, vec![0.5; 5]);
        let out = blend(&modulator, &carrier, 1.0, None).expect("blend");
        assert_eq!(out.frames, 5 + 4 - 1);
    }

    #[test]
    fn max_impulse_samples_truncates_to_head() {
        // Truncating the IR to 1 sample ⇒ identity ⇒ passthrough length.
        let modulator = buf(1, 48_000, vec![0.5, 0.5, 0.5, 0.5]);
        let carrier = buf(1, 48_000, vec![0.3, -0.2, 0.4]);
        let out = blend(&modulator, &carrier, 1.0, Some(1)).expect("blend");
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
        let out = blend(&modulator, &carrier, 1.0, None).expect("blend");
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
        assert!(blend(&modulator, &carrier, 1.0, None).is_err());
        let matched = buf(1, 48_000, vec![1.0, 0.5]);
        assert!(blend(&matched, &carrier, 1.5, None).is_err());
        assert!(blend(&matched, &carrier, f32::NAN, None).is_err());
        assert!(blend(&matched, &carrier, 1.0, Some(0)).is_err());
    }

    #[test]
    fn fft_method_matches_direct_within_epsilon() {
        // A non-trivial IR + carrier whose lengths don't sum to a power of two.
        let modulator = buf(
            1,
            48_000,
            (0..23).map(|i| ((i * 5 % 7) as f32 / 3.0) - 1.0).collect(),
        );
        let carrier = buf(
            1,
            48_000,
            (0..40).map(|i| ((i * 3 % 11) as f32 / 5.0) - 1.0).collect(),
        );
        let direct = impulse_convolution_blend(
            &modulator,
            &carrier,
            1.0,
            None,
            ConvolutionMethod::Direct,
            false,
        )
        .expect("direct blend");
        let fft = impulse_convolution_blend(
            &modulator,
            &carrier,
            1.0,
            None,
            ConvolutionMethod::Fft,
            false,
        )
        .expect("fft blend");
        assert_eq!(direct.frames, fft.frames);
        assert_eq!(direct.channels, fft.channels);
        for (d, f) in direct.samples.iter().zip(&fft.samples) {
            assert!(
                (d - f).abs() <= FFT_DIRECT_PARITY_EPSILON,
                "fft {f} vs direct {d} exceeds parity epsilon"
            );
        }
    }

    #[test]
    fn resample_equal_rates_is_exact_copy() {
        let taps = vec![0.1_f32, -0.2, 0.3, -0.4];
        assert_eq!(resample_lanczos(&taps, 48_000, 48_000), taps);
    }

    #[test]
    fn resample_scales_length_by_ratio() {
        let taps = vec![0.0_f32, 0.5, 1.0, 0.5, 0.0, -0.5, -1.0, -0.5];
        let up = resample_lanczos(&taps, 24_000, 48_000);
        assert_eq!(up.len(), taps.len() * 2);
        let down = resample_lanczos(&taps, 48_000, 24_000);
        assert_eq!(down.len(), taps.len() / 2);
    }

    #[test]
    fn resample_preserves_dc() {
        // A constant IR must stay (approximately) constant under resampling — the
        // sum-normalized Lanczos weights preserve DC.
        let taps = vec![0.5_f32; 16];
        let up = resample_lanczos(&taps, 32_000, 48_000);
        for value in &up {
            assert!((value - 0.5).abs() < 1e-4, "DC drifted to {value}");
        }
    }

    #[test]
    fn resample_impulse_enables_mismatched_rates_and_bounds_gain() {
        // 24 kHz IR, 48 kHz carrier: without the flag this errors; with it the IR
        // resamples to 48 kHz and the L1 bound still holds.
        let modulator = buf(1, 24_000, vec![3.0, -2.0, 4.0, 1.0, -1.0]);
        let carrier = buf(1, 48_000, vec![1.0, -1.0, 1.0, -1.0, 1.0, -1.0]);
        assert!(impulse_convolution_blend(
            &modulator,
            &carrier,
            1.0,
            None,
            ConvolutionMethod::Direct,
            false,
        )
        .is_err());
        let out = impulse_convolution_blend(
            &modulator,
            &carrier,
            1.0,
            None,
            ConvolutionMethod::Direct,
            true,
        )
        .expect("resampled blend");
        assert_eq!(out.sample_rate, carrier.sample_rate);
        let peak = out.samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!(
            peak <= 1.0 + 1e-6,
            "resampled L1 IR amplified past carrier peak: {peak}"
        );
    }
}
