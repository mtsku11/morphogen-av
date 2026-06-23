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

use crate::{AudioBufferF32, AudioError};

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
        let peak = samples.iter().map(|(_, value)| *value).fold(0.0_f32, f32::max);
        let times = samples.iter().map(|(time, _)| *time).collect();
        let norm = samples
            .iter()
            .map(|(_, value)| if peak > 0.0 { (value / peak).clamp(0.0, 1.0) } else { 0.0 })
            .collect();
        Self { times, norm }
    }

    fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// Advance `cursor` to the latest frame whose time is `<= t` (times
    /// ascending, queried in non-decreasing `t` order) and return its normalized
    /// value. Holds the first frame for `t` before it. Caller must ensure the
    /// envelope is non-empty.
    fn value_at(&self, cursor: &mut usize, t: f64) -> f32 {
        while *cursor + 1 < self.times.len() && self.times[*cursor + 1] <= t {
            *cursor += 1;
        }
        self.norm[*cursor]
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

    let channels = carrier.channels;
    let mut samples_out = vec![0.0_f32; carrier.samples.len()];
    let mut cursor = 0_usize;
    for frame in 0..carrier.frames {
        let t = frame as f64 / carrier.sample_rate as f64;
        let value = env.value_at(&mut cursor, t);
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

    let channels = carrier.channels;
    let mut samples_out = vec![0.0_f32; carrier.frames * 2];
    let mut cursor = 0_usize;
    for frame in 0..carrier.frames {
        let t = frame as f64 / carrier.sample_rate as f64;
        let value = env.value_at(&mut cursor, t);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(channels: usize, sr: u32, samples: Vec<f32>) -> AudioBufferF32 {
        AudioBufferF32::new(channels, sr, samples).expect("valid test buffer")
    }

    #[test]
    fn envelope_peak_normalizes_and_holds_last() {
        let env = DescriptorEnvelope::from_samples(&[(0.0, 0.25), (1.0, 0.5), (2.0, 0.0)]);
        let mut cursor = 0;
        assert_eq!(env.value_at(&mut cursor, -0.1), 0.5); // hold-first (0.25/0.5)
        assert_eq!(env.value_at(&mut cursor, 0.0), 0.5);
        assert_eq!(env.value_at(&mut cursor, 0.5), 0.5); // hold-last from t=0
        assert_eq!(env.value_at(&mut cursor, 1.5), 1.0); // hold-last from the strong frame
        assert_eq!(env.value_at(&mut cursor, 2.5), 0.0); // hold-last from the zero frame
    }

    #[test]
    fn silent_envelope_is_all_zero() {
        let env = DescriptorEnvelope::from_samples(&[(0.0, 0.0), (1.0, 0.0)]);
        let mut cursor = 0;
        assert_eq!(env.value_at(&mut cursor, 0.0), 0.0);
        assert_eq!(env.value_at(&mut cursor, 0.9), 0.0);
    }

    #[test]
    fn gain_amount_zero_is_byte_identical_passthrough() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_gain_route(&carrier, &env, 0.0).expect("gain");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.channels, 1);
    }

    #[test]
    fn gain_transfers_descriptor_envelope() {
        // A: weak first half (value 0), strong second half (value 1). B: steady 0.5.
        // Frame time = sample/sr; sr=4 ⇒ samples 0..3 at t<1 (weak), 4..7 at t>=1.
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_gain_route(&carrier, &env, 1.0).expect("gain");
        for &s in &out.samples[0..4] {
            assert!(s.abs() < 1e-6, "expected silence where A is weak, got {s}");
        }
        for &s in &out.samples[4..8] {
            assert!((s - 0.5).abs() < 1e-6, "expected carrier where A is strong, got {s}");
        }
    }

    #[test]
    fn pan_amount_zero_is_byte_identical_passthrough() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_pan_route(&carrier, &env, 0.0).expect("pan");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.channels, 1); // mono B stays mono when off
    }

    #[test]
    fn pan_steers_weak_left_strong_right() {
        // sr=4: samples 0..3 weak (t<1), 4..7 strong (t>=1). Mono carrier 0.5.
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 0.0), (1.0, 1.0)];
        let out = descriptor_pan_route(&carrier, &env, 1.0).expect("pan");
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
            assert!(r > 0.4 && l.abs() < 1e-6, "strong frame {frame}: L {l} R {r}");
        }
    }

    #[test]
    fn pan_center_is_equal_power() {
        // A uniform mid value peak-normalizes to 1.0 (strongest=only value),
        // which pans hard right — so to land at center, use a two-value envelope
        // whose normalized mid is 0.5: value 0.5 over peak 1.0.
        let carrier = buf(1, 4, vec![1.0; 4]);
        let env = [(0.0, 0.5), (10.0, 1.0)]; // t<10 ⇒ norm 0.5 ⇒ pan 0 ⇒ center
        let out = descriptor_pan_route(&carrier, &env, 1.0).expect("pan");
        let expected = std::f32::consts::FRAC_1_SQRT_2; // cos(pi/4)
        for frame in 0..4 {
            let l = out.samples[frame * 2];
            let r = out.samples[frame * 2 + 1];
            assert!((l - expected).abs() < 1e-6 && (r - expected).abs() < 1e-6, "L {l} R {r}");
        }
    }

    #[test]
    fn stereo_carrier_is_mono_mixed_for_pan() {
        // Stereo B [L=1, R=0] ⇒ mono mix 0.5; strong A ⇒ hard right.
        let carrier = buf(2, 4, vec![1.0, 0.0, 1.0, 0.0]);
        let env = [(0.0, 1.0)];
        let out = descriptor_pan_route(&carrier, &env, 1.0).expect("pan");
        assert_eq!(out.channels, 2);
        assert_eq!(out.frames, 2);
        assert!(out.samples[0].abs() < 1e-6, "left should be ~0 (hard right)");
        assert!((out.samples[1] - 0.5).abs() < 1e-6, "right should carry mono mix 0.5");
    }

    #[test]
    fn rejects_out_of_range_amount() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let env = [(0.0, 1.0)];
        assert!(descriptor_gain_route(&carrier, &env, 1.5).is_err());
        assert!(descriptor_gain_route(&carrier, &env, f32::NAN).is_err());
        assert!(descriptor_pan_route(&carrier, &env, -0.1).is_err());
    }

    #[test]
    fn empty_descriptor_is_error_when_on() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        assert!(descriptor_gain_route(&carrier, &[], 1.0).is_err());
        assert!(descriptor_pan_route(&carrier, &[], 1.0).is_err());
    }
}
