//! Deterministic video oscillator bank — source-less pattern generators (a synth's
//! oscillators, for video). Each preset is a **pure, stateless** function:
//! `(settings, frame_index) -> ImageBufferF32`. A frame depends only on
//! `(settings, frame)` — no prior-frame state, no checkpoint, same discipline as
//! [`crate::cascade_collage`].
//!
//! **The phase law** (the core invariant, shared by every preset):
//! `phase(frame) = phase0 + rate * frame as f64`, computed entirely in **f64** —
//! recompute-from-index, never an accumulator, so there is no per-frame drift and
//! the phase-drift anchor (frame `k` at `(rate, phase0)` ≡ frame `0` at
//! `(rate, phase0 + rate*k)`) holds exactly. `rate == 0` ⇒ every frame is
//! byte-identical to frame 0.
//!
//! All hash randomness (the plasma noise lattice) is splitmix64 over integer
//! lattice coordinates seeded by `seed`, the same hash family as
//! [`crate::block_collage`]/[`crate::fluid_advect`]/[`crate::cascade_collage`];
//! `vnoise2` is [`crate::cascade_collage`]'s 1-D smoothstep value noise extended to
//! a 2-D lattice. See `docs/OSCILLATOR_BANK_MILESTONE.md` for the contract.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

/// Algorithm identifiers — one per preset (a preset IS the algorithm; changing its
/// formula bumps that preset's id).
pub const OSCILLATOR_SCAN_BARS_ALGORITHM: &str = "oscillator_scan_bars_cpu_v1";
pub const OSCILLATOR_RADIAL_ALGORITHM: &str = "oscillator_radial_cpu_v1";
pub const OSCILLATOR_PLASMA_ALGORITHM: &str = "oscillator_plasma_cpu_v1";
pub const OSCILLATOR_GRADIENT_ALGORITHM: &str = "oscillator_gradient_cpu_v1";

/// Which oscillator preset to render.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneratorPreset {
    /// Vertical bars scrolling horizontally.
    #[default]
    ScanBars,
    /// Concentric rings breathing outward from centre.
    Radial,
    /// Two-layer drifting interference + hash-noise shimmer, colourized by hue.
    Plasma,
    /// A linear gradient sweeping its angle.
    Gradient,
}

impl GeneratorPreset {
    /// The algorithm identifier recorded in `manifest.json` for this preset.
    pub fn algorithm_id(self) -> &'static str {
        match self {
            Self::ScanBars => OSCILLATOR_SCAN_BARS_ALGORITHM,
            Self::Radial => OSCILLATOR_RADIAL_ALGORITHM,
            Self::Plasma => OSCILLATOR_PLASMA_ALGORITHM,
            Self::Gradient => OSCILLATOR_GRADIENT_ALGORITHM,
        }
    }
}

/// Shared per-frame knobs for every oscillator preset.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeneratorSettings {
    pub width: u32,
    pub height: u32,
    /// Phase advance per frame (the phase-law `rate`). `0` ⇒ static frames.
    pub rate: f32,
    /// Initial phase (the phase-law `phase0`).
    pub phase: f32,
    /// Spatial frequency / pattern density (bar count, ring density, plasma cell
    /// size). Accepted but unused by `gradient` (a uniform knob surface).
    pub scale: f32,
    /// Plasma noise lattice key. Ignored by the other presets (documented, not an
    /// error to set it).
    pub seed: u64,
}

impl Default for GeneratorSettings {
    fn default() -> Self {
        Self {
            width: 640,
            height: 360,
            rate: 0.02,
            phase: 0.0,
            scale: 4.0,
            seed: 71,
        }
    }
}

impl GeneratorSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.width == 0 || self.height == 0 {
            return Err(RenderError::InvalidGeneratorSettings(
                "width and height must be >= 1".into(),
            ));
        }
        if !self.rate.is_finite() || !self.phase.is_finite() || !self.scale.is_finite() {
            return Err(RenderError::InvalidGeneratorSettings(
                "rate, phase, and scale must be finite".into(),
            ));
        }
        Ok(())
    }
}

/// The phase law: recompute-from-index in f64, never an accumulator.
fn phase_at(settings: &GeneratorSettings, frame: u32) -> f64 {
    settings.phase as f64 + settings.rate as f64 * frame as f64
}

// ─── Deterministic 2-D value noise (plasma only) ────────────────────────────────

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Hash a 2-D integer lattice coordinate + seed to `[0, 1)` — `cascade_collage`'s
/// 1-D `hash1` extended with a second axis.
fn hash2(seed: u64, ix: i64, iy: i64) -> f64 {
    let h = splitmix(
        seed ^ (ix as u64).wrapping_mul(0x9e3779b1) ^ (iy as u64).wrapping_mul(0x85ebca6b),
    );
    (h & 0xffff) as f64 / 65535.0
}

fn smoothstep(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// 2-D smoothstep value noise on a splitmix64 lattice, in `[0, 1)`.
fn vnoise2(seed: u64, x: f64, y: f64) -> f64 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = smoothstep(x - ix);
    let fy = smoothstep(y - iy);
    let ix = ix as i64;
    let iy = iy as i64;
    let c00 = hash2(seed, ix, iy);
    let c10 = hash2(seed, ix + 1, iy);
    let c01 = hash2(seed, ix, iy + 1);
    let c11 = hash2(seed, ix + 1, iy + 1);
    lerp(lerp(c00, c10, fx), lerp(c01, c11, fx), fy)
}

/// HSV (h in turns, s/v in `[0,1]`) → RGB in `[0,1]` — the same convention as
/// `cascade_collage::hsv_to_rgb`.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    if s <= 0.0 {
        return (v, v, v);
    }
    let h6 = (h - h.floor()) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

// ─── Presets ─────────────────────────────────────────────────────────────────

/// Vertical bars scrolling horizontally (greyscale).
pub fn render_scan_bars_frame(
    settings: &GeneratorSettings,
    frame: u32,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let phase = phase_at(settings, frame);
    let scale = settings.scale as f64;
    let width = settings.width as f64;
    ImageBufferF32::from_fn(settings.width, settings.height, |x, _y| {
        let x_norm = (x as f64 + 0.5) / width;
        let v = 0.5 + 0.5 * (TAU * (x_norm * scale + phase)).sin();
        let v = v as f32;
        [v, v, v, 1.0]
    })
}

/// Concentric rings breathing outward from centre (greyscale).
pub fn render_radial_frame(
    settings: &GeneratorSettings,
    frame: u32,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let phase = phase_at(settings, frame);
    let scale = settings.scale as f64;
    let (cx, cy, half_min) = radial_geometry(settings);
    ImageBufferF32::from_fn(settings.width, settings.height, |x, y| {
        let d = radial_distance(x, y, cx, cy, half_min);
        let v = 0.5 + 0.5 * (TAU * (d * scale - phase)).sin();
        let v = v as f32;
        [v, v, v, 1.0]
    })
}

/// Classic two-layer drifting interference + hash-noise shimmer, colourized by hue.
pub fn render_plasma_frame(
    settings: &GeneratorSettings,
    frame: u32,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let phase = phase_at(settings, frame);
    let scale = settings.scale as f64;
    let seed = settings.seed;
    let (cx, cy, half_min) = radial_geometry(settings);
    let width = settings.width as f64;
    let height = settings.height as f64;
    ImageBufferF32::from_fn(settings.width, settings.height, |x, y| {
        let x_norm = (x as f64 + 0.5) / width;
        let y_norm = (y as f64 + 0.5) / height;
        let d_center = radial_distance(x, y, cx, cy, half_min);
        let n = vnoise2(
            seed,
            x_norm * scale * 4.0 + phase * 0.7,
            y_norm * scale * 4.0 - phase * 0.9,
        );
        let raw = ((TAU * (x_norm * scale + phase)).sin()
            + (TAU * (y_norm * scale * 0.83 - phase * 1.13)).sin()
            + (TAU * (d_center * scale * 1.31 + phase * 0.57)).sin()
            + (2.0 * n - 1.0) * 0.8)
            / 3.8
            * 0.5
            + 0.5;
        let v = raw.clamp(0.0, 1.0);
        let (r, g, b) = hsv_to_rgb(v, 0.7, 0.35 + 0.65 * v);
        [r as f32, g as f32, b as f32, 1.0]
    })
}

/// A linear gradient sweeping its angle (greyscale). `scale` is accepted but
/// unused (a uniform knob surface across all four presets).
pub fn render_gradient_frame(
    settings: &GeneratorSettings,
    frame: u32,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let phase = phase_at(settings, frame);
    let width = settings.width as f64;
    let height = settings.height as f64;
    let angle = TAU * phase;
    let (cos_a, sin_a) = (angle.cos(), angle.sin());
    ImageBufferF32::from_fn(settings.width, settings.height, |x, y| {
        let x_norm = (x as f64 + 0.5) / width;
        let y_norm = (y as f64 + 0.5) / height;
        let t = 0.5 + (x_norm - 0.5) * cos_a + (y_norm - 0.5) * sin_a;
        let v = t.clamp(0.0, 1.0) as f32;
        [v, v, v, 1.0]
    })
}

/// Render one frame of the given preset.
pub fn render_generator_frame(
    preset: GeneratorPreset,
    settings: &GeneratorSettings,
    frame: u32,
) -> Result<ImageBufferF32, RenderError> {
    match preset {
        GeneratorPreset::ScanBars => render_scan_bars_frame(settings, frame),
        GeneratorPreset::Radial => render_radial_frame(settings, frame),
        GeneratorPreset::Plasma => render_plasma_frame(settings, frame),
        GeneratorPreset::Gradient => render_gradient_frame(settings, frame),
    }
}

/// Pixel-centre-sampling geometry shared by `radial` and `plasma`: canvas centre
/// `(w/2, h/2)` and the `min(w, h) / 2` normalizing radius.
fn radial_geometry(settings: &GeneratorSettings) -> (f64, f64, f64) {
    let cx = settings.width as f64 / 2.0;
    let cy = settings.height as f64 / 2.0;
    let half_min = settings.width.min(settings.height) as f64 / 2.0;
    (cx, cy, half_min)
}

/// Normalized pixel-centre distance from `(cx, cy)`, per the contract's `d`.
fn radial_distance(x: u32, y: u32, cx: f64, cy: f64, half_min: f64) -> f64 {
    let px = x as f64 + 0.5;
    let py = y as f64 + 0.5;
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt() / half_min
}

#[cfg(test)]
mod tests {
    use super::*;

    // rate/phase are dyadic rationals (exact in binary floating point) so that
    // storing an f64-computed "equivalent phase" back into the f32 `phase` knob
    // round-trips losslessly — see the phase-drift-equivalence tests below.
    fn settings() -> GeneratorSettings {
        GeneratorSettings {
            width: 64,
            height: 48,
            rate: 0.0625,
            phase: 0.125,
            scale: 4.0,
            seed: 71,
        }
    }

    // ─── Anchor 1: rate-0 identity ──────────────────────────────────────────

    #[test]
    fn rate_zero_scan_bars_is_identical_across_frames() {
        let s = GeneratorSettings {
            rate: 0.0,
            ..settings()
        };
        let f0 = render_scan_bars_frame(&s, 0).unwrap();
        let f9 = render_scan_bars_frame(&s, 9).unwrap();
        assert_eq!(f0, f9, "A1: rate 0 must hold every frame at frame 0");
    }

    #[test]
    fn rate_zero_plasma_is_identical_across_frames() {
        let s = GeneratorSettings {
            rate: 0.0,
            ..settings()
        };
        let f0 = render_plasma_frame(&s, 0).unwrap();
        let f9 = render_plasma_frame(&s, 9).unwrap();
        assert_eq!(f0, f9, "A1: rate 0 must hold every frame at frame 0");
    }

    // ─── Anchor 2: phase-drift equivalence ──────────────────────────────────

    #[test]
    fn phase_drift_equivalence_holds_for_scan_bars() {
        let base = settings();
        let k = 7u32;
        let advanced_phase = (base.phase as f64 + base.rate as f64 * k as f64) as f32;
        let a = render_scan_bars_frame(&base, k).unwrap();
        let equivalent = GeneratorSettings {
            phase: advanced_phase,
            ..base
        };
        let b = render_scan_bars_frame(&equivalent, 0).unwrap();
        assert_eq!(
            a, b,
            "A2: frame k at (rate,phase) == frame 0 at (rate, phase+rate*k)"
        );
    }

    #[test]
    fn phase_drift_equivalence_holds_for_plasma_worst_case_noise_path() {
        let base = settings();
        let k = 11u32;
        let advanced_phase = (base.phase as f64 + base.rate as f64 * k as f64) as f32;
        let a = render_plasma_frame(&base, k).unwrap();
        let equivalent = GeneratorSettings {
            phase: advanced_phase,
            ..base
        };
        let b = render_plasma_frame(&equivalent, 0).unwrap();
        assert_eq!(
            a, b,
            "A2: phase-drift equivalence must hold through the noise path too"
        );
    }

    // ─── Anchor 3: two-run determinism ──────────────────────────────────────

    #[test]
    fn two_runs_are_byte_identical_for_every_preset() {
        let s = settings();
        for preset in [
            GeneratorPreset::ScanBars,
            GeneratorPreset::Radial,
            GeneratorPreset::Plasma,
            GeneratorPreset::Gradient,
        ] {
            let a = render_generator_frame(preset, &s, 3).unwrap();
            let b = render_generator_frame(preset, &s, 3).unwrap();
            assert_eq!(a, b, "A3: {preset:?} must be byte-identical across runs");
        }
    }

    // ─── Anchor 4: plasma seed sensitivity ───────────────────────────────────

    #[test]
    fn plasma_different_seed_differs_same_seed_matches() {
        let s = settings();
        let other_seed = GeneratorSettings { seed: 99, ..s };
        let same_seed = GeneratorSettings { seed: s.seed, ..s };

        let a = render_plasma_frame(&s, 2).unwrap();
        let b = render_plasma_frame(&other_seed, 2).unwrap();
        let c = render_plasma_frame(&same_seed, 2).unwrap();

        assert_ne!(a, b, "A4: different seed must change plasma output");
        assert_eq!(a, c, "A4: same seed must reproduce the identical frame");
    }

    // ─── Formula value pins (exact pixel values) ────────────────────────────

    #[test]
    fn scan_bars_pins_a_known_pixel_value() {
        // width=8, x=2 -> x_norm = 2.5/8 = 0.3125; scale=1, phase=0 ->
        // v = 0.5 + 0.5*sin(TAU*0.3125) = 0.5 + 0.5*sin(1.9635) ≈ 0.85355
        let s = GeneratorSettings {
            width: 8,
            height: 1,
            rate: 0.0,
            phase: 0.0,
            scale: 1.0,
            seed: 0,
        };
        let frame = render_scan_bars_frame(&s, 0).unwrap();
        let expected = 0.5 + 0.5 * (TAU * (2.5 / 8.0)).sin();
        let got = frame.pixel(2, 0).unwrap()[0];
        assert!(
            (got as f64 - expected).abs() < 1e-6,
            "expected {expected}, got {got}"
        );
    }

    #[test]
    fn radial_pins_a_known_pixel_value_at_centre() {
        // 4x4, centre (2,2). Pixel (1,1) centre = (1.5,1.5) -> d = hypot(0.5,0.5)/2
        let s = GeneratorSettings {
            width: 4,
            height: 4,
            rate: 0.0,
            phase: 0.0,
            scale: 1.0,
            seed: 0,
        };
        let frame = render_radial_frame(&s, 0).unwrap();
        let d = (0.5_f64.powi(2) + 0.5_f64.powi(2)).sqrt() / 2.0;
        let expected = 0.5 + 0.5 * (TAU * d).sin();
        let got = frame.pixel(1, 1).unwrap()[0];
        assert!(
            (got as f64 - expected).abs() < 1e-6,
            "expected {expected}, got {got}"
        );
    }

    #[test]
    fn gradient_pins_the_centre_pixel_independent_of_angle() {
        // For any angle, the exact geometric centre (x_norm=y_norm=0.5) has
        // t == 0.5 identically (both offsets are zero).
        let s = GeneratorSettings {
            width: 2,
            height: 2,
            rate: 1.0,
            phase: 0.37,
            scale: 4.0,
            seed: 0,
        };
        // width/height=2 -> pixel-centres are at 0.25/0.75, not exactly 0.5, so
        // instead pin the (0,0) pixel at phase 0 (angle 0 -> cos=1, sin=0).
        let zero_phase = GeneratorSettings {
            phase: 0.0,
            rate: 0.0,
            ..s
        };
        let frame = render_gradient_frame(&zero_phase, 0).unwrap();
        // x_norm=y_norm=0.25 -> t = 0.5 + (0.25-0.5)*1 + (0.25-0.5)*0 = 0.25
        let got = frame.pixel(0, 0).unwrap()[0];
        assert!((got - 0.25).abs() < 1e-6, "expected 0.25, got {got}");
    }

    #[test]
    fn gradient_scale_is_accepted_but_unused() {
        let a = GeneratorSettings {
            scale: 1.0,
            ..settings()
        };
        let b = GeneratorSettings {
            scale: 99.0,
            ..settings()
        };
        let fa = render_gradient_frame(&a, 3).unwrap();
        let fb = render_gradient_frame(&b, 3).unwrap();
        assert_eq!(fa, fb, "gradient must ignore scale entirely");
    }

    // ─── Validation ──────────────────────────────────────────────────────────

    #[test]
    fn validate_rejects_zero_dimensions_and_non_finite_knobs() {
        let zero_w = GeneratorSettings {
            width: 0,
            ..settings()
        };
        assert!(zero_w.validate().is_err());

        let nan_rate = GeneratorSettings {
            rate: f32::NAN,
            ..settings()
        };
        assert!(nan_rate.validate().is_err());

        let inf_scale = GeneratorSettings {
            scale: f32::INFINITY,
            ..settings()
        };
        assert!(inf_scale.validate().is_err());
    }

    #[test]
    fn algorithm_ids_are_one_per_preset() {
        assert_eq!(
            GeneratorPreset::ScanBars.algorithm_id(),
            OSCILLATOR_SCAN_BARS_ALGORITHM
        );
        assert_eq!(
            GeneratorPreset::Radial.algorithm_id(),
            OSCILLATOR_RADIAL_ALGORITHM
        );
        assert_eq!(
            GeneratorPreset::Plasma.algorithm_id(),
            OSCILLATOR_PLASMA_ALGORITHM
        );
        assert_eq!(
            GeneratorPreset::Gradient.algorithm_id(),
            OSCILLATOR_GRADIENT_ALGORITHM
        );
    }
}
