//! Video-to-Audio Descriptor Routing (MVP): Source A's per-frame luma envelope
//! drives Source B's audio amplitude (`gain`) or stereo position (`pan`).
//!
//! The only new deterministic logic lives here — turning a peak-normalized luma
//! envelope into a per-output-sample gain or equal-power pan. The luma itself is
//! computed by the CLI (which owns image decoding) and handed in as raw
//! `(time_seconds, luma)` samples, keeping this crate decoupled from the image
//! crate (the symmetric decoupling `audio_route.rs` keeps from audio).
//!
//! CPU-only — audio is not a GPU target here, so there is no Metal path to
//! parity-gate. See `docs/VIDEO_AUDIO_ROUTE_MILESTONE.md` for the contract.

use crate::{AudioBufferF32, AudioError};

/// `gain` mode render id (per-frame luma → B amplitude).
pub const LUMA_GAIN_ROUTE_ALGORITHM: &str = "luma_gain_route_cpu_v1";
/// `pan` mode render id (per-frame luma → equal-power stereo position).
pub const LUMA_PAN_ROUTE_ALGORITHM: &str = "luma_pan_route_cpu_v1";

fn validate_amount(amount: f32) -> Result<(), AudioError> {
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(AudioError::InvalidSettings(
            "amount must be finite and within [0, 1]".to_string(),
        ));
    }
    Ok(())
}

/// A peak-normalized per-frame luma envelope, sampled by output time with a
/// hold-last lookup. Built from raw `(time_seconds, luma)` samples
/// (time-ascending); the brightest input frame maps to a normalized value of
/// `1.0`, black to `0.0`.
struct LumaEnvelope {
    times: Vec<f64>,
    norm: Vec<f32>,
}

impl LumaEnvelope {
    /// Peak-normalize raw `(time, luma)` samples by their maximum luma. Yields
    /// all-zero values when the peak is ~0 (a fully dark modulator ⇒ no effect).
    fn from_luma_samples(samples: &[(f64, f32)]) -> Self {
        let peak = samples.iter().map(|(_, luma)| *luma).fold(0.0_f32, f32::max);
        let times = samples.iter().map(|(time, _)| *time).collect();
        let norm = samples
            .iter()
            .map(|(_, luma)| if peak > 0.0 { (luma / peak).clamp(0.0, 1.0) } else { 0.0 })
            .collect();
        Self { times, norm }
    }

    fn is_empty(&self) -> bool {
        self.times.is_empty()
    }

    /// Advance `cursor` to the latest frame whose time is `<= t` (times
    /// ascending, queried in non-decreasing `t` order) and return its normalized
    /// luma. Holds the first frame for `t` before it. Caller must ensure the
    /// envelope is non-empty.
    fn luma_at(&self, cursor: &mut usize, t: f64) -> f32 {
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

/// `gain` mode: A's peak-normalized per-frame luma envelope modulates B's
/// amplitude. `amount = 0` returns Source B unchanged (byte-identical).
pub fn luma_gain_route(
    carrier: &AudioBufferF32,
    luma_samples: &[(f64, f32)],
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }

    let env = LumaEnvelope::from_luma_samples(luma_samples);
    if env.is_empty() {
        return Err(AudioError::InvalidSettings(
            "modulator produced no luma frames".to_string(),
        ));
    }

    let channels = carrier.channels;
    let mut samples = vec![0.0_f32; carrier.samples.len()];
    let mut cursor = 0_usize;
    for frame in 0..carrier.frames {
        let t = frame as f64 / carrier.sample_rate as f64;
        let luma = env.luma_at(&mut cursor, t);
        // out = B * lerp(1.0, luma, amount): bright A keeps B, dark A silences it.
        let gain = 1.0 + (luma - 1.0) * amount;
        for channel in 0..channels {
            let idx = frame * channels + channel;
            samples[idx] = carrier.samples[idx] * gain;
        }
    }

    AudioBufferF32::new(channels, carrier.sample_rate, samples)
}

/// `pan` mode: A's peak-normalized per-frame luma drives an equal-power stereo
/// pan of B (mono-mixed). A dark frame steers energy left, a bright frame right.
/// Output is always 2-channel. `amount = 0` returns Source B unchanged.
pub fn luma_pan_route(
    carrier: &AudioBufferF32,
    luma_samples: &[(f64, f32)],
    amount: f32,
) -> Result<AudioBufferF32, AudioError> {
    validate_amount(amount)?;
    if amount == 0.0 {
        return Ok(carrier.clone());
    }

    let env = LumaEnvelope::from_luma_samples(luma_samples);
    if env.is_empty() {
        return Err(AudioError::InvalidSettings(
            "modulator produced no luma frames".to_string(),
        ));
    }

    let channels = carrier.channels;
    let mut samples = vec![0.0_f32; carrier.frames * 2];
    let mut cursor = 0_usize;
    for frame in 0..carrier.frames {
        let t = frame as f64 / carrier.sample_rate as f64;
        let luma = env.luma_at(&mut cursor, t);
        let pan = (2.0 * luma - 1.0) * amount;
        let (left_gain, right_gain) = equal_power_gains(pan);
        let mut mono = 0.0_f32;
        for channel in 0..channels {
            mono += carrier.samples[frame * channels + channel];
        }
        mono /= channels.max(1) as f32;
        samples[frame * 2] = mono * left_gain;
        samples[frame * 2 + 1] = mono * right_gain;
    }

    AudioBufferF32::new(2, carrier.sample_rate, samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(channels: usize, sr: u32, samples: Vec<f32>) -> AudioBufferF32 {
        AudioBufferF32::new(channels, sr, samples).expect("valid test buffer")
    }

    #[test]
    fn envelope_peak_normalizes_and_holds_last() {
        let env = LumaEnvelope::from_luma_samples(&[(0.0, 0.25), (1.0, 0.5), (2.0, 0.0)]);
        let mut cursor = 0;
        assert_eq!(env.luma_at(&mut cursor, -0.1), 0.5); // hold-first (0.25/0.5)
        assert_eq!(env.luma_at(&mut cursor, 0.0), 0.5);
        assert_eq!(env.luma_at(&mut cursor, 0.5), 0.5); // hold-last from t=0
        assert_eq!(env.luma_at(&mut cursor, 1.5), 1.0); // hold-last from the bright frame
        assert_eq!(env.luma_at(&mut cursor, 2.5), 0.0); // hold-last from the black frame
    }

    #[test]
    fn silent_envelope_is_all_zero() {
        let env = LumaEnvelope::from_luma_samples(&[(0.0, 0.0), (1.0, 0.0)]);
        let mut cursor = 0;
        assert_eq!(env.luma_at(&mut cursor, 0.0), 0.0);
        assert_eq!(env.luma_at(&mut cursor, 0.9), 0.0);
    }

    #[test]
    fn gain_amount_zero_is_byte_identical_passthrough() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let luma = [(0.0, 0.0), (1.0, 1.0)];
        let out = luma_gain_route(&carrier, &luma, 0.0).expect("gain");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.channels, 1);
    }

    #[test]
    fn gain_transfers_luma_envelope() {
        // A: dark first half (luma 0), bright second half (luma 1). B: steady 0.5.
        // Frame time = sample/sr; sr=4 ⇒ samples 0..3 at t<1 (dark), 4..7 at t>=1.
        let carrier = buf(1, 4, vec![0.5; 8]);
        let luma = [(0.0, 0.0), (1.0, 1.0)];
        let out = luma_gain_route(&carrier, &luma, 1.0).expect("gain");
        for &s in &out.samples[0..4] {
            assert!(s.abs() < 1e-6, "expected silence where A is dark, got {s}");
        }
        for &s in &out.samples[4..8] {
            assert!((s - 0.5).abs() < 1e-6, "expected carrier where A is bright, got {s}");
        }
    }

    #[test]
    fn pan_amount_zero_is_byte_identical_passthrough() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let luma = [(0.0, 0.0), (1.0, 1.0)];
        let out = luma_pan_route(&carrier, &luma, 0.0).expect("pan");
        assert_eq!(out.samples, carrier.samples);
        assert_eq!(out.channels, 1); // mono B stays mono when off
    }

    #[test]
    fn pan_steers_dark_left_bright_right() {
        // sr=4: samples 0..3 dark (t<1), 4..7 bright (t>=1). Mono carrier 0.5.
        let carrier = buf(1, 4, vec![0.5; 8]);
        let luma = [(0.0, 0.0), (1.0, 1.0)];
        let out = luma_pan_route(&carrier, &luma, 1.0).expect("pan");
        assert_eq!(out.channels, 2);
        // Dark frames (pan -1): all energy left, right ~0.
        for frame in 0..4 {
            let l = out.samples[frame * 2];
            let r = out.samples[frame * 2 + 1];
            assert!(l > 0.4 && r.abs() < 1e-6, "dark frame {frame}: L {l} R {r}");
        }
        // Bright frames (pan +1): all energy right, left ~0.
        for frame in 4..8 {
            let l = out.samples[frame * 2];
            let r = out.samples[frame * 2 + 1];
            assert!(r > 0.4 && l.abs() < 1e-6, "bright frame {frame}: L {l} R {r}");
        }
    }

    #[test]
    fn pan_center_is_equal_power() {
        // A uniform mid-luma A peak-normalizes to 1.0 (brightest=only value),
        // which pans hard right — so to land at center, use a two-value envelope
        // whose normalized mid is 0.5: luma 0.5 over peak 1.0.
        let carrier = buf(1, 4, vec![1.0; 4]);
        let luma = [(0.0, 0.5), (10.0, 1.0)]; // t<10 ⇒ norm 0.5 ⇒ pan 0 ⇒ center
        let out = luma_pan_route(&carrier, &luma, 1.0).expect("pan");
        let expected = std::f32::consts::FRAC_1_SQRT_2; // cos(pi/4)
        for frame in 0..4 {
            let l = out.samples[frame * 2];
            let r = out.samples[frame * 2 + 1];
            assert!((l - expected).abs() < 1e-6 && (r - expected).abs() < 1e-6, "L {l} R {r}");
        }
    }

    #[test]
    fn stereo_carrier_is_mono_mixed_for_pan() {
        // Stereo B [L=1, R=0] ⇒ mono mix 0.5; bright A ⇒ hard right.
        let carrier = buf(2, 4, vec![1.0, 0.0, 1.0, 0.0]);
        let luma = [(0.0, 1.0)];
        let out = luma_pan_route(&carrier, &luma, 1.0).expect("pan");
        assert_eq!(out.channels, 2);
        assert_eq!(out.frames, 2);
        assert!(out.samples[0].abs() < 1e-6, "left should be ~0 (hard right)");
        assert!((out.samples[1] - 0.5).abs() < 1e-6, "right should carry mono mix 0.5");
    }

    #[test]
    fn rejects_out_of_range_amount() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        let luma = [(0.0, 1.0)];
        assert!(luma_gain_route(&carrier, &luma, 1.5).is_err());
        assert!(luma_gain_route(&carrier, &luma, f32::NAN).is_err());
        assert!(luma_pan_route(&carrier, &luma, -0.1).is_err());
    }

    #[test]
    fn empty_luma_is_error_when_on() {
        let carrier = buf(1, 4, vec![0.5; 8]);
        assert!(luma_gain_route(&carrier, &[], 1.0).is_err());
        assert!(luma_pan_route(&carrier, &[], 1.0).is_err());
    }
}
