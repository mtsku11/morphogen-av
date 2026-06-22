//! Video vocoder — luma-band gain routing (MVP).
//!
//! The visual analog of an audio vocoder: Source A's per-frame luma distribution
//! becomes a per-band gain envelope that reweights Source B's tonal bands. Source
//! B stays the material at every output pixel; A only decides how its luma bands
//! are emphasized or suppressed. Stateless, per-frame, deterministic.
//!
//! See `docs/VIDEO_VOCODER_MILESTONE.md` for the full contract.

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

/// CPU reference render id and luma-band-envelope sidecar algorithm id.
pub const VIDEO_VOCODER_ALGORITHM: &str = "luma_band_gain_vocoder_cpu_v1";

/// Source A's per-frame tonal envelope: the fraction of A pixels whose luma falls
/// in each of `bands.len()` equal luma bands over `[0, 1]` (the entries sum to 1
/// for a non-empty frame). Reusable sidecar data — regenerable from Source A plus
/// the band count and algorithm id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LumaBandEnvelope {
    /// Per-band occupancy fractions, band `b` covering luma `[b/N, (b+1)/N)`.
    pub bands: Vec<f32>,
}

/// Render parameters for the luma-band gain vocoder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoVocoderSettings {
    /// Number of equal luma bands `N` (>= 1).
    pub bands: u32,
    /// Blend from identity (`0` = exact Source B passthrough) to full routing
    /// (`1`). Values above 1 over-drive the envelope. Must be finite and >= 0.
    pub amount: f32,
}

impl VideoVocoderSettings {
    pub fn validate(self) -> Result<(), RenderError> {
        if self.bands == 0 {
            return Err(RenderError::InvalidVideoVocoderSettings(
                "bands must be greater than zero".to_string(),
            ));
        }
        if !self.amount.is_finite() || self.amount < 0.0 {
            return Err(RenderError::InvalidVideoVocoderSettings(
                "amount must be finite and non-negative".to_string(),
            ));
        }
        Ok(())
    }
}

/// Rec. 709 luma, matching the convention used by the granular-mosaic path.
fn luminance(pixel: [f32; 4]) -> f32 {
    pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Histogram Source A's luma into `bands` equal bins over `[0, 1]`. Every pixel
/// contributes once (A is consumed at its own resolution; no resampling). Returns
/// occupancy fractions that sum to 1.
pub fn analyze_luma_band_envelope_cpu(
    modulator: &ImageBufferF32,
    bands: u32,
) -> Result<LumaBandEnvelope, RenderError> {
    if bands == 0 {
        return Err(RenderError::InvalidVideoVocoderSettings(
            "bands must be greater than zero".to_string(),
        ));
    }
    let band_count = bands as usize;
    let mut counts = vec![0.0_f32; band_count];
    for &pixel in &modulator.pixels {
        let luma = luminance(pixel).clamp(0.0, 1.0);
        let index = ((luma * bands as f32) as usize).min(band_count - 1);
        counts[index] += 1.0;
    }
    let total: f32 = counts.iter().sum();
    if total > 0.0 {
        for count in &mut counts {
            *count /= total;
        }
    }
    Ok(LumaBandEnvelope { bands: counts })
}

/// Continuous per-band gain at carrier luma `l`, linearly interpolated between the
/// two nearest band centers (band `b` center = `(b + 0.5)/N`), clamped at the end
/// bands so the result is `C0`-continuous in luma (no posterization).
fn soft_gain(l: f32, gains: &[f32]) -> f32 {
    let n = gains.len();
    // n >= 1 guaranteed by the caller (settings/envelope band counts agree).
    if n == 1 {
        return gains[0];
    }
    let pos = (l.clamp(0.0, 1.0) * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
    let lower = pos.floor() as usize;
    let upper = (lower + 1).min(n - 1);
    let frac = pos - lower as f32;
    lerp(gains[lower], gains[upper], frac)
}

/// Apply a precomputed Source A envelope to Source B. Output dimensions follow the
/// carrier; each pixel's RGB is scaled by the soft gain at its luma and clamped to
/// `[0, 1]`; alpha is preserved. `amount = 0` is an exact passthrough.
pub fn video_vocoder_cpu(
    carrier: &ImageBufferF32,
    envelope: &LumaBandEnvelope,
    settings: VideoVocoderSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    if envelope.bands.len() != settings.bands as usize {
        return Err(RenderError::InvalidVideoVocoderSettings(format!(
            "envelope has {} bands but settings request {}",
            envelope.bands.len(),
            settings.bands
        )));
    }

    let band_count = settings.bands as f32;
    let gains: Vec<f32> = envelope
        .bands
        .iter()
        .map(|&fraction| lerp(1.0, band_count * fraction, settings.amount))
        .collect();

    let pixels = carrier
        .pixels
        .iter()
        .map(|&pixel| {
            let gain = soft_gain(luminance(pixel), &gains);
            [
                (pixel[0] * gain).clamp(0.0, 1.0),
                (pixel[1] * gain).clamp(0.0, 1.0),
                (pixel[2] * gain).clamp(0.0, 1.0),
                pixel[3],
            ]
        })
        .collect();

    ImageBufferF32::new(carrier.width, carrier.height, pixels)
}

/// Convenience: analyze Source A then apply to Source B in one call (the
/// still-image path). The sequence/queue paths analyze once into a reusable
/// sidecar instead.
pub fn video_vocoder_from_modulator_cpu(
    modulator: &ImageBufferF32,
    carrier: &ImageBufferF32,
    settings: VideoVocoderSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let envelope = analyze_luma_band_envelope_cpu(modulator, settings.bands)?;
    video_vocoder_cpu(carrier, &envelope, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, color: [f32; 4]) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |_, _| color).expect("buffer")
    }

    #[test]
    fn passthrough_amount_zero_preserves_carrier() {
        // A highlight-heavy modulator that would otherwise reshape the carrier.
        let modulator = solid(4, 4, [1.0, 1.0, 1.0, 1.0]);
        let carrier = ImageBufferF32::from_fn(4, 1, |x, _| {
            let v = x as f32 / 3.0;
            [v, v * 0.5, 0.25, 1.0]
        })
        .expect("carrier");
        let settings = VideoVocoderSettings {
            bands: 8,
            amount: 0.0,
        };
        let envelope = analyze_luma_band_envelope_cpu(&modulator, settings.bands).expect("env");
        let out = video_vocoder_cpu(&carrier, &envelope, settings).expect("render");
        assert_eq!(out.pixels, carrier.pixels);
    }

    #[test]
    fn uniform_modulator_is_neutral_at_full_amount() {
        // A flat full-range modulator spreads occupancy evenly -> every gain ~1.
        let modulator = ImageBufferF32::from_fn(8, 1, |x, _| {
            let v = (x as f32 + 0.5) / 8.0;
            [v, v, v, 1.0]
        })
        .expect("modulator");
        let carrier = ImageBufferF32::from_fn(5, 1, |x, _| {
            let v = x as f32 / 4.0;
            [v, v, v, 1.0]
        })
        .expect("carrier");
        let settings = VideoVocoderSettings {
            bands: 8,
            amount: 1.0,
        };
        let envelope = analyze_luma_band_envelope_cpu(&modulator, settings.bands).expect("env");
        let out = video_vocoder_cpu(&carrier, &envelope, settings).expect("render");
        for (rendered, original) in out.pixels.iter().zip(&carrier.pixels) {
            for channel in 0..4 {
                assert!(
                    (rendered[channel] - original[channel]).abs() <= 1.0 / 255.0,
                    "channel {channel}: {} vs {}",
                    rendered[channel],
                    original[channel]
                );
            }
        }
    }

    #[test]
    fn highlight_modulator_boosts_brights_and_cuts_shadows() {
        // All-white A -> occupancy pinned to the top band -> top gain == N, the
        // rest 0. A dark carrier band is attenuated, a bright one boosted.
        let modulator = solid(4, 4, [1.0, 1.0, 1.0, 1.0]);
        let carrier = ImageBufferF32::from_fn(2, 1, |x, _| {
            let v = if x == 0 { 0.1 } else { 0.95 };
            [v, v, v, 1.0]
        })
        .expect("carrier");
        let settings = VideoVocoderSettings {
            bands: 4,
            amount: 1.0,
        };
        let envelope = analyze_luma_band_envelope_cpu(&modulator, settings.bands).expect("env");
        assert_eq!(envelope.bands, vec![0.0, 0.0, 0.0, 1.0]);
        let out = video_vocoder_cpu(&carrier, &envelope, settings).expect("render");
        // Shadow pixel (luma 0.1, low bands gain 0) is driven toward black.
        assert!(out.pixels[0][0] < carrier.pixels[0][0]);
        // Highlight pixel (luma 0.95, top band gain N) is boosted (clamped at 1).
        assert!(out.pixels[1][0] > carrier.pixels[1][0]);
    }

    #[test]
    fn soft_membership_interpolates_between_band_centers() {
        // Gains [0, 4]: a luma exactly at the band-0 center reads gain 0; exactly
        // at the band-1 center reads gain 4; halfway between reads the average.
        let gains = [0.0_f32, 4.0];
        assert_eq!(soft_gain(0.25, &gains), 0.0); // center of band 0 = 0.25
        assert_eq!(soft_gain(0.75, &gains), 4.0); // center of band 1 = 0.75
        assert_eq!(soft_gain(0.5, &gains), 2.0); // midpoint -> average
        // Below the first / above the last center clamps (no extrapolation).
        assert_eq!(soft_gain(0.0, &gains), 0.0);
        assert_eq!(soft_gain(1.0, &gains), 4.0);
    }

    #[test]
    fn envelope_band_count_must_match_settings() {
        let carrier = solid(2, 2, [0.5, 0.5, 0.5, 1.0]);
        let envelope = LumaBandEnvelope {
            bands: vec![0.5, 0.5],
        };
        let settings = VideoVocoderSettings {
            bands: 4,
            amount: 1.0,
        };
        assert!(video_vocoder_cpu(&carrier, &envelope, settings).is_err());
    }

    #[test]
    fn invalid_settings_are_rejected() {
        assert!(VideoVocoderSettings {
            bands: 0,
            amount: 1.0
        }
        .validate()
        .is_err());
        assert!(VideoVocoderSettings {
            bands: 4,
            amount: f32::NAN
        }
        .validate()
        .is_err());
        assert!(VideoVocoderSettings {
            bands: 4,
            amount: -0.5
        }
        .validate()
        .is_err());
    }
}
