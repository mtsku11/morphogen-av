//! Video-to-Audio Descriptor Routing: a per-frame Source A **visual descriptor**
//! envelope drives Source B's audio amplitude (`gain`) or stereo position
//! (`pan`).
//!
//! The only new deterministic logic lives here — turning a peak-normalized
//! descriptor envelope into a per-output-sample gain or equal-power pan. The
//! descriptor itself (mean luma, optical-flow magnitude, ...) is computed by the
//! CLI (which owns image decoding) and handed in as raw `(time_seconds, value)`
//! samples, keeping this crate decoupled from the image crate (the symmetric
//! decoupling `audio_route.rs` keeps from audio). These routes are descriptor-
//! neutral: which visual signal produced the samples is recorded by the caller
//! (the algorithm id is composed in `morphogen-core` from descriptor + mode).
//!
//! CPU-only — audio is not a GPU target here, so there is no Metal path to
//! parity-gate. See `docs/VIDEO_AUDIO_ROUTE_MILESTONE.md` for the contract.

use crate::cross_synth::{one_pole_filter_sweep, FilterType};
use crate::{AudioBufferF32, AudioError};

/// How the sparse per-frame descriptor envelope is resampled onto B's
/// per-output-sample grid (the roadmap's "time-resampled descriptor curves").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeSampling {
    /// Step: hold the latest frame's value until the next frame (default).
    Hold,
    /// Linearly interpolate between adjacent frames (a smooth curve, no zipper
    /// stepping); holds the first/last frame outside the envelope's time span.
    Smooth,
}

fn validate_amount(amount: f32) -> Result<(), AudioError> {
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(AudioError::InvalidSettings(
            "amount must be finite and within [0, 1]".to_string(),
        ));
    }
    Ok(())
}

/// A peak-normalized per-frame descriptor envelope, sampled by output time with
/// a hold-last lookup. Built from raw `(time_seconds, value)` samples
/// (time-ascending); the strongest input frame maps to a normalized value of
/// `1.0`, zero to `0.0`.
struct DescriptorEnvelope {
    times: Vec<f64>,
    norm: Vec<f32>,
}

impl DescriptorEnvelope {
    /// Peak-normalize raw `(time, value)` samples by their maximum. Yields
    /// all-zero values when the peak is ~0 (a flat/dark modulator ⇒ no effect).
    fn from_samples(samples: &[(f64, f32)]) -> Self {
        let peak = samples
            .iter()
            .map(|(_, value)| *value)
            .fold(0.0_f32, f32::max);
        let times = samples.iter().map(|(time, _)| *time).collect();
        let norm = samples
            .iter()
            .map(|(_, value)| {
                if peak > 0.0 {
                    (value / peak).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect();
        Self { times, norm }
    }

    fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// Resample the sparse per-frame envelope onto a dense per-output-frame curve
    /// (`frames` values at `i / sample_rate`). `Hold` steps (the latest frame at
    /// or before each output time); `Smooth` linearly interpolates between the
    /// bracketing frames, holding the first/last value outside the time span.
    /// Caller must ensure the envelope is non-empty.
    fn resample(&self, frames: usize, sample_rate: u32, sampling: EnvelopeSampling) -> Vec<f32> {
        let sr = sample_rate as f64;
        let mut out = Vec::with_capacity(frames);
        let mut cursor = 0_usize;
        for frame in 0..frames {
            let t = frame as f64 / sr;
            while cursor + 1 < self.times.len() && self.times[cursor + 1] <= t {
                cursor += 1;
            }
            let value = match sampling {
                EnvelopeSampling::Hold => self.norm[cursor],
                EnvelopeSampling::Smooth => {
                    if cursor + 1 < self.times.len() {
                        let t0 = self.times[cursor];
                        let t1 = self.times[cursor + 1];
                        if t <= t0 || t1 <= t0 {
                            self.norm[cursor]
                        } else {
                            let frac = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32;
                            self.norm[cursor] + (self.norm[cursor + 1] - self.norm[cursor]) * frac
                        }
                    } else {
                        self.norm[cursor]
                    }
                }
            };
            out.push(value);
        }
        out
    }
}

/// Equal-power stereo gains for a pan position `pan` ∈ `[-1, 1]` (`-1` hard
/// left, `+1` hard right, `0` center). Returns `(left_gain, right_gain)`.
fn equal_power_gains(pan: f32) -> (f32, f32) {
    let theta = (pan.clamp(-1.0, 1.0) + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
    (theta.cos(), theta.sin())
}

/// `gain` mode: A's peak-normalized per-frame descriptor envelope modulates B's
/// amplitude. `amount = 0` returns Source B unchanged (byte-identical).
pub fn descriptor_gain_route(
    carrier: &AudioBufferF32,
    samples: &[(f64, f32)],
    sampling: EnvelopeSampling,
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }

    let env = DescriptorEnvelope::from_samples(samples);
    if env.is_empty() {
        return Err(AudioError::InvalidSettings(
            "modulator produced no descriptor frames".to_string(),
        ));
    }
    let curve = env.resample(carrier.frames, carrier.sample_rate, sampling);

    let channels = carrier.channels;
    let mut samples_out = vec![0.0_f32; carrier.samples.len()];
    for (frame, &value) in curve.iter().enumerate() {
        // out = B * lerp(1.0, value, amount): strong A keeps B, weak A silences it.
        let gain = 1.0 + (value - 1.0) * amount;
        for channel in 0..channels {
            let idx = frame * channels + channel;
            samples_out[idx] = carrier.samples[idx] * gain;
        }
    }

    AudioBufferF32::new(channels, carrier.sample_rate, samples_out)
}

/// `pan` mode: A's peak-normalized per-frame descriptor drives an equal-power
/// stereo pan of B (mono-mixed). A weak frame steers energy left, a strong frame
/// right. Output is always 2-channel. `amount = 0` returns Source B unchanged.
pub fn descriptor_pan_route(
    carrier: &AudioBufferF32,
    samples: &[(f64, f32)],
    sampling: EnvelopeSampling,
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }

    let env = DescriptorEnvelope::from_samples(samples);
    if env.is_empty() {
        return Err(AudioError::InvalidSettings(
            "modulator produced no descriptor frames".to_string(),
        ));
    }
    let curve = env.resample(carrier.frames, carrier.sample_rate, sampling);

    let channels = carrier.channels;
    let mut samples_out = vec![0.0_f32; carrier.frames * 2];
    for (frame, &value) in curve.iter().enumerate() {
        let pan = (2.0 * value - 1.0) * amount;
        let (left_gain, right_gain) = equal_power_gains(pan);
        let mut mono = 0.0_f32;
        for channel in 0..channels {
            mono += carrier.samples[frame * channels + channel];
        }
        mono /= channels.max(1) as f32;
        samples_out[frame * 2] = mono * left_gain;
        samples_out[frame * 2 + 1] = mono * right_gain;
    }

    AudioBufferF32::new(2, carrier.sample_rate, samples_out)
}

/// `filter` mode: A's peak-normalized per-frame descriptor sweeps a one-pole
/// LP/HP filter cutoff on B (a strong descriptor opens the cutoff toward
/// Nyquist, a weak one closes it). Output follows B's channels. `amount = 0`
/// returns Source B unchanged (byte-identical).
pub fn descriptor_filter_route(
    carrier: &AudioBufferF32,
    samples: &[(f64, f32)],
    filter_type: FilterType,
    sampling: EnvelopeSampling,
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }

    let env = DescriptorEnvelope::from_samples(samples);
    if env.is_empty() {
        return Err(AudioError::InvalidSettings(
            "modulator produced no descriptor frames".to_string(),
        ));
    }
    // Resample the descriptor to a dense per-output-sample cutoff curve, then
    // sweep with a per-sample time grid so the one-pole filter reads it directly.
    let cutoff: Vec<f64> = env
        .resample(carrier.frames, carrier.sample_rate, sampling)
        .iter()
        .map(|&v| v as f64)
        .collect();
    let times: Vec<f64> = (0..carrier.frames)
        .map(|i| i as f64 / carrier.sample_rate as f64)
        .collect();
    one_pole_filter_sweep(carrier, &times, &cutoff, filter_type, amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(channels: usize, sr: u32, samples: Vec<f32>) -> AudioBufferF32 {
        AudioBufferF32::new(channels, sr, samples).expect("valid test buffer")
    }

    #[test]
    fn envelope_hold_resample_peak_normalizes_and_steps() {
        // Two frames at t=0 (0.5) and t=1 (1.0), peak-normalized to 0.5 / 1.0.
        // sr=4 ⇒ frames 0..3 at t<1 hold 0.5, frame 4 at t=1 steps to 1.0.
        let env = DescriptorEnvelope::from_samples(&[(0.0, 0.5), (1.0, 1.0)]);
        let curve = env.resample(5, 4, EnvelopeSampling::Hold);
        assert_eq!(curve, vec![0.5, 0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn envelope_smooth_resample_interpolates_between_frames() {
        // Same frames; Smooth linearly ramps 0.5→1.0 across t∈[0,1] (sr=4 ⇒
        // 0.25/0.5/0.75 fractions), then holds 1.0 at/after the last frame.
        let env = DescriptorEnvelope::from_samples(&[(0.0, 0.5), (1.0, 1.0)]);
        let curve = env.resample(5, 4, EnvelopeSampling::Smooth);
        let expected = [0.5, 0.625, 0.75, 0.875, 1.0];
        for (got, want) in curve.iter().zip(expected) {
            assert!((got - want).abs() < 1e-6, "got {got}, want {want}");
        }
    }

    #[test]
    fn silent_envelope_resamples_all_zero() {
        let env = DescriptorEnvelope::from_samples(&[(0.0, 0.0), (1.0, 0.0)]);
        assert_eq!(env.resample(4, 4, EnvelopeSampling::Hold), vec![0.0; 4]);
        assert_eq!(env.resample(4, 4, EnvelopeSampling::Smooth), vec![0.0; 4]);
    }

    #[test]
    fn gain_amount_zero_is_byte_identical_passthrough() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_gain_route(&carrier, &env, EnvelopeSampling::Hold, 0.0).expect("gain");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.channels, 1);
    }

    #[test]
    fn gain_transfers_descriptor_envelope() {
        // A: weak first half (value 0), strong second half (value 1). B: steady 0.5.
        // Frame time = sample/sr; sr=4 ⇒ samples 0..3 at t<1 (weak), 4..7 at t>=1.
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_gain_route(&carrier, &env, EnvelopeSampling::Hold, 1.0).expect("gain");
        for &s in &out.samples[0..4] {
            assert!(s.abs() < 1e-6, "expected silence where A is weak, got {s}");
        }
        for &s in &out.samples[4..8] {
            assert!(
                (s - 0.5).abs() < 1e-6,
                "expected carrier where A is strong, got {s}"
            );
        }
    }

    #[test]
    fn pan_amount_zero_is_byte_identical_passthrough() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_pan_route(&carrier, &env, EnvelopeSampling::Hold, 0.0).expect("pan");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.channels, 1); // mono B stays mono when off
    }

    #[test]
    fn pan_steers_weak_left_strong_right() {
        // sr=4: samples 0..3 weak (t<1), 4..7 strong (t>=1). Mono carrier 0.5.
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_pan_route(&carrier, &env, EnvelopeSampling::Hold, 1.0).expect("pan");
        assert_eq!(out.channels, 2);
        // Weak frames (pan -1): all energy left, right ~0.
        for frame in 0..4 {
            let l = out.samples[frame * 2];
            let r = out.samples[frame * 2 + 1];
            assert!(l > 0.4 && r.abs() < 1e-6, "weak frame {frame}: L {l} R {r}");
        }
        // Strong frames (pan +1): all energy right, left ~0.
        for frame in 4..8 {
            let l = out.samples[frame * 2];
            let r = out.samples[frame * 2 + 1];
            assert!(
                r > 0.4 && l.abs() < 1e-6,
                "strong frame {frame}: L {l} R {r}"
            );
        }
    }

    #[test]
    fn pan_center_is_equal_power() {
        // A uniform mid value peak-normalizes to 1.0 (strongest=only value),
        // which pans hard right — so to land at center, use a two-value envelope
        // whose normalized mid is 0.5: value 0.5 over peak 1.0.
        let carrier = buf(1, 4, vec![1.0; 4]);
        let env = [(0.0, 0.5), (10.0, 1.0)]; // t<10 ⇒ norm 0.5 ⇒ pan 0 ⇒ center
        let out = descriptor_pan_route(&carrier, &env, EnvelopeSampling::Hold, 1.0).expect("pan");
        let expected = std::f32::consts::FRAC_1_SQRT_2; // cos(pi/4)
        for frame in 0..4 {
            let l = out.samples[frame * 2];
            let r = out.samples[frame * 2 + 1];
            assert!(
                (l - expected).abs() < 1e-6 && (r - expected).abs() < 1e-6,
                "L {l} R {r}"
            );
        }
    }

    #[test]
    fn stereo_carrier_is_mono_mixed_for_pan() {
        // Stereo B [L=1, R=0] ⇒ mono mix 0.5; strong A ⇒ hard right.
        let carrier = buf(2, 4, vec![1.0, 0.0, 1.0, 0.0]);
        let env = [(0.0, 1.0)];
        let out = descriptor_pan_route(&carrier, &env, EnvelopeSampling::Hold, 1.0).expect("pan");
        assert_eq!(out.channels, 2);
        assert_eq!(out.frames, 2);
        assert!(
            out.samples[0].abs() < 1e-6,
            "left should be ~0 (hard right)"
        );
        assert!(
            (out.samples[1] - 0.5).abs() < 1e-6,
            "right should carry mono mix 0.5"
        );
    }

    #[test]
    fn rejects_out_of_range_amount() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 1.0)];
        assert!(descriptor_gain_route(&carrier, &env, EnvelopeSampling::Hold, 1.5).is_err());
        assert!(descriptor_gain_route(&carrier, &env, EnvelopeSampling::Hold, f32::NAN).is_err());
        assert!(descriptor_pan_route(&carrier, &env, EnvelopeSampling::Hold, -0.1).is_err());
    }

    #[test]
    fn empty_descriptor_is_error_when_on() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        assert!(descriptor_gain_route(&carrier, &[], EnvelopeSampling::Hold, 1.0).is_err());
        assert!(descriptor_pan_route(&carrier, &[], EnvelopeSampling::Hold, 1.0).is_err());
        assert!(descriptor_filter_route(
            &carrier,
            &[],
            FilterType::Lowpass,
            EnvelopeSampling::Hold,
            1.0
        )
        .is_err());
    }

    #[test]
    fn filter_amount_zero_is_byte_identical_passthrough() {
        let carrier = buf(1, 8, vec![1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_filter_route(
            &carrier,
            &env,
            FilterType::Lowpass,
            EnvelopeSampling::Hold,
            0.0,
        )
        .expect("filter");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.channels, 1);
    }

    #[test]
    fn filter_descriptor_opens_lowpass_cutoff() {
        // A Nyquist-rate carrier (alternating ±1) through a lowpass: a weak (dark)
        // descriptor closes the cutoff ⇒ heavy attenuation; a strong (bright) one
        // opens it ⇒ the alternation survives. sr=8 ⇒ samples 0..7 weak (t<1),
        // 8..15 strong (t>=1).
        let carrier = buf(
            1,
            8,
            (0..16)
                .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
                .collect(),
        );
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_filter_route(
            &carrier,
            &env,
            FilterType::Lowpass,
            EnvelopeSampling::Hold,
            1.0,
        )
        .expect("filter");
        let weak_energy: f32 = out.samples[0..8].iter().map(|s| s * s).sum();
        let strong_energy: f32 = out.samples[8..16].iter().map(|s| s * s).sum();
        assert!(
            strong_energy > weak_energy * 4.0,
            "strong descriptor must pass far more HF energy: weak {weak_energy}, strong {strong_energy}"
        );
    }

    #[test]
    fn filter_lowpass_and_highpass_differ() {
        let carrier = buf(
            1,
            8,
            (0..16)
                .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
                .collect(),
        );
        let env = [(0.0, 0.3), (10.0, 1.0)]; // mid cutoff over the whole carrier
        let lp = descriptor_filter_route(
            &carrier,
            &env,
            FilterType::Lowpass,
            EnvelopeSampling::Hold,
            1.0,
        )
        .expect("lp");
        let hp = descriptor_filter_route(
            &carrier,
            &env,
            FilterType::Highpass,
            EnvelopeSampling::Hold,
            1.0,
        )
        .expect("hp");
        assert_ne!(lp.samples, hp.samples, "lowpass and highpass must differ");
    }
}
