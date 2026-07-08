//! Gray-Scott reaction-diffusion field sim — the morphogenesis engine (Tier
//! "Morphogenesis" S1: `docs/MORPHOGENESIS_MILESTONE.md`).
//!
//! Two chemical fields `U`, `V` (f32 grids, row-major, at a sim resolution
//! downscaled from the carrier by `--sim-scale`) evolve under:
//!
//! ```text
//! dU = Du*∇²U - U*V² + f*(1-U)
//! dV = Dv*∇²V + U*V² - (f+k)*V
//! ```
//!
//! with a 5-point Laplacian, **clamped edges** (declared; not toroidal —
//! footage has a frame, so the stencil replicates the edge value instead of
//! wrapping), and `U,V` clamped to `[0,1]` after every substep (Gray-Scott is
//! stiff; the clamp is part of the algorithm, not a safety net). Substeps read
//! only the previous buffer (double-buffered, gather-only) so update order
//! never matters — deterministic by construction and Metal-friendly (a future
//! S5 port is a straight per-pixel gather kernel).
//!
//! This module is a **stateful temporal node** exactly like
//! [`crate::cpu_reference::flow_feedback_frame_cpu`]: frame-zero seeding is
//! declared ([`seed_morphogenesis_field`]), the exact prior-frame state
//! consumed is the whole `(U,V)` field, and the checkpoint representation is
//! unquantized RGBA32F (see `morphogen-cli`'s `render-morphogenesis-field`,
//! which reuses [`crate::feedback_state`]'s generic RGBA32F codec — the magic
//! bytes there say "feedback" for historical reasons, but the wire format is
//! algorithm-agnostic; the JSON checkpoint contract is what discriminates).
//!
//! Frame zero (the seed, S1's only footage coupling so far): `U = 1`
//! everywhere; `V` is seeded where the carrier's (Source B) frame-0 luma
//! crosses `--seed-threshold`, plus a sparse splitmix64 speckle keyed by
//! `--seed` so growth isn't perfectly symmetric even on a flat carrier. B→(f,k)
//! per-frame parameter maps and the A-driven mod-matrix targets are S3 — out of
//! scope here.
//!
//! Off/identity anchors proven by the tests below:
//! - **A2 (frozen field):** `substeps == 0` ⇒ the field stays exactly the
//!   frame-zero seed forever.
//! - **A3 (dead chemistry):** a dead `(feed, kill)` pair ⇒ field variance
//!   decays toward uniform (falsifiable, not asserted).
//! - **Aliveness (acceptance 2):** every named preset's field variance grows
//!   from the seed and stays in a nontrivial band over 60 frames at default
//!   knobs (the "most of (f,k) space is dead" trap, made falsifiable).

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

/// Algorithm identifier — bump when the stencil, seeding, or clamp semantics
/// change so stale checkpoints invalidate.
pub const MORPHOGENESIS_ALGORITHM: &str = "morphogenesis_cpu_v1";

/// Fraction of sim-resolution pixels that get a stochastic extra seed
/// regardless of the luma threshold — a sparse dusting so growth fronts don't
/// start perfectly symmetric even on a flat/dark carrier.
const SPECKLE_DENSITY: f32 = 0.002;

/// `V` value written into a seeded pixel (threshold-crossed or speckle-hit).
/// `U` stays at 1 everywhere per the frame-zero contract; the reaction still
/// nucleates because `U*V²` dominates locally once `V` is non-zero.
const SEED_ACTIVE_V: f32 = 1.0;

/// Gray-Scott parameters + sim-resolution + seeding knobs. `f`/`k` are named
/// `feed`/`kill` to match the S3-contracted modulation target names.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MorphogenesisSettings {
    /// `U` diffusion rate.
    pub du: f32,
    /// `V` diffusion rate.
    pub dv: f32,
    /// Feed rate (`f` in the reaction-diffusion literature).
    pub feed: f32,
    /// Kill rate (`k`).
    pub kill: f32,
    /// Per-substep integration step.
    pub dt: f32,
    /// Gray-Scott substeps run per output frame. `0` ⇒ the field is frozen
    /// (anchor A2).
    pub substeps: u32,
    /// Sim resolution divisor relative to the carrier frame (`1` = full res;
    /// `2` = half). The field is `ceil(carrier_dim / sim_scale)` per axis.
    pub sim_scale: u32,
    /// Frame-zero seed threshold: carrier luma `>=` this value seeds `V`.
    pub seed_threshold: f32,
    /// Deterministic seed for the frame-zero speckle (splitmix64 keyed).
    pub seed: u64,
}

impl MorphogenesisSettings {
    /// The default, known-alive "coral growth" band from the contract.
    pub fn coral() -> Self {
        Self {
            du: 0.16,
            dv: 0.08,
            feed: 0.037,
            kill: 0.060,
            dt: 1.0,
            substeps: 12,
            sim_scale: 2,
            seed_threshold: 0.5,
            seed: 71,
        }
    }

    /// Mitosis-like splitting spots (contract-pinned `f`/`k`).
    pub fn mitosis() -> Self {
        Self {
            feed: 0.0367,
            kill: 0.0649,
            ..Self::coral()
        }
    }

    /// Wandering worm/maze-like stripes (standard Gray-Scott parameter atlas —
    /// the spots/worms boundary region, `f≈k≈0.06`).
    pub fn worms() -> Self {
        Self {
            feed: 0.062,
            kill: 0.061,
            ..Self::coral()
        }
    }

    /// Stable, non-splitting spots (standard Gray-Scott parameter atlas).
    pub fn spots() -> Self {
        Self {
            feed: 0.030,
            kill: 0.062,
            ..Self::coral()
        }
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        if !(self.du.is_finite() && self.du >= 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "du must be finite and >= 0".into(),
            ));
        }
        if !(self.dv.is_finite() && self.dv >= 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "dv must be finite and >= 0".into(),
            ));
        }
        if !(self.feed.is_finite() && self.feed >= 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "feed must be finite and >= 0".into(),
            ));
        }
        if !(self.kill.is_finite() && self.kill >= 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "kill must be finite and >= 0".into(),
            ));
        }
        if !(self.dt.is_finite() && self.dt > 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "dt must be finite and > 0".into(),
            ));
        }
        if self.sim_scale == 0 {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "sim_scale must be >= 1".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.seed_threshold) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "seed_threshold must be in [0, 1]".into(),
            ));
        }
        Ok(())
    }
}

impl Default for MorphogenesisSettings {
    fn default() -> Self {
        Self::coral()
    }
}

/// Named parameter-atlas presets so users don't have to prospect raw `(f,k)`
/// numbers (most of that space is dead / uniform grey).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MorphogenesisPreset {
    #[default]
    Coral,
    Mitosis,
    Worms,
    Spots,
}

impl MorphogenesisPreset {
    pub fn settings(self) -> MorphogenesisSettings {
        match self {
            Self::Coral => MorphogenesisSettings::coral(),
            Self::Mitosis => MorphogenesisSettings::mitosis(),
            Self::Worms => MorphogenesisSettings::worms(),
            Self::Spots => MorphogenesisSettings::spots(),
        }
    }
}

/// The `(U,V)` field at sim resolution. Row-major, same indexing convention
/// as [`ImageBufferF32`].
#[derive(Debug, Clone, PartialEq)]
pub struct MorphogenesisField {
    pub width: u32,
    pub height: u32,
    pub u: Vec<f32>,
    pub v: Vec<f32>,
}

impl MorphogenesisField {
    pub fn new(width: u32, height: u32, u: Vec<f32>, v: Vec<f32>) -> Result<Self, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::InvalidMorphogenesisField(
                "width and height must be greater than zero".into(),
            ));
        }
        let expected = (width as usize) * (height as usize);
        if u.len() != expected || v.len() != expected {
            return Err(RenderError::InvalidMorphogenesisField(format!(
                "expected {expected} U/V samples, got {}/{}",
                u.len(),
                v.len()
            )));
        }
        Ok(Self {
            width,
            height,
            u,
            v,
        })
    }

    /// Population variance of the `V` channel — the field's "aliveness"
    /// signal: uniform (dead) fields converge to ~0 variance.
    pub fn v_variance(&self) -> f32 {
        let n = self.v.len() as f32;
        if n == 0.0 {
            return 0.0;
        }
        let mean = self.v.iter().sum::<f32>() / n;
        self.v
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f32>()
            / n
    }
}

/// Sim-resolution dimensions for a carrier of `(carrier_width, carrier_height)`
/// at `sim_scale` — `ceil(dim / sim_scale)` per axis (mirrors
/// `box_downscale_dimensions`'s ceil convention: the last block never drops).
pub fn morphogenesis_field_dimensions(
    carrier_width: u32,
    carrier_height: u32,
    sim_scale: u32,
) -> (u32, u32) {
    let scale = sim_scale.max(1);
    (
        carrier_width.div_ceil(scale),
        carrier_height.div_ceil(scale),
    )
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Hash a 2-D sim-lattice coordinate + seed to `[0, 1)`.
fn seed_hash_unit(seed: u64, x: u32, y: u32) -> f32 {
    let lattice = (u64::from(x) << 32) | u64::from(y);
    let h = splitmix64(seed ^ lattice.wrapping_mul(0x9e3779b97f4a7c15));
    (h & 0xffff) as f32 / 65536.0
}

/// Frame-zero seed (declared): `U = 1` everywhere; `V` is seeded (to
/// [`SEED_ACTIVE_V`]) where the carrier's frame-0 luma crosses
/// `settings.seed_threshold`, OR where the deterministic speckle hits
/// (`settings.seed`-keyed, density [`SPECKLE_DENSITY`]) — patterns therefore
/// nucleate on the carrier's bright structure plus a sparse scatter elsewhere.
pub fn seed_morphogenesis_field(
    carrier_frame_zero: &ImageBufferF32,
    settings: &MorphogenesisSettings,
) -> Result<MorphogenesisField, RenderError> {
    settings.validate()?;
    let (width, height) = morphogenesis_field_dimensions(
        carrier_frame_zero.width,
        carrier_frame_zero.height,
        settings.sim_scale,
    );
    let count = (width as usize) * (height as usize);
    let u = vec![1.0_f32; count];
    let mut v = vec![0.0_f32; count];

    for y in 0..height {
        for x in 0..width {
            let src_x = (x * settings.sim_scale).min(carrier_frame_zero.width - 1);
            let src_y = (y * settings.sim_scale).min(carrier_frame_zero.height - 1);
            let pixel = carrier_frame_zero.pixel(src_x, src_y).ok_or_else(|| {
                RenderError::InvalidMorphogenesisField(
                    "carrier sample coordinate out of bounds".into(),
                )
            })?;
            let luma = 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];
            let luma_seeded = luma >= settings.seed_threshold;
            let speckle_seeded = seed_hash_unit(settings.seed, x, y) < SPECKLE_DENSITY;
            if luma_seeded || speckle_seeded {
                let idx = (y as usize) * (width as usize) + (x as usize);
                v[idx] = SEED_ACTIVE_V;
            }
        }
    }

    MorphogenesisField::new(width, height, u, v)
}

fn clamp_prev(index: u32) -> u32 {
    index.saturating_sub(1)
}

fn clamp_next(index: u32, extent: u32) -> u32 {
    (index + 1).min(extent - 1)
}

/// One Gray-Scott substep: a 5-point Laplacian with **clamped edges** (the
/// stencil replicates the border value instead of wrapping — footage has a
/// frame), `U,V` clamped to `[0,1]` afterward. Gather-only from the previous
/// buffer (double-buffered), so raster order never affects the result.
pub fn morphogenesis_substep(
    field: &MorphogenesisField,
    settings: &MorphogenesisSettings,
) -> Result<MorphogenesisField, RenderError> {
    let width = field.width;
    let height = field.height;
    let w = width as usize;
    let mut new_u = vec![0.0_f32; field.u.len()];
    let mut new_v = vec![0.0_f32; field.v.len()];

    for y in 0..height {
        let y_prev = clamp_prev(y) as usize;
        let y_next = clamp_next(y, height) as usize;
        let row = y as usize * w;
        let row_prev = y_prev * w;
        let row_next = y_next * w;
        for x in 0..width {
            let x_prev = clamp_prev(x) as usize;
            let x_next = clamp_next(x, width) as usize;
            let idx = row + x as usize;

            let u_c = field.u[idx];
            let v_c = field.v[idx];
            let lap_u = field.u[row + x_prev]
                + field.u[row + x_next]
                + field.u[row_prev + x as usize]
                + field.u[row_next + x as usize]
                - 4.0 * u_c;
            let lap_v = field.v[row + x_prev]
                + field.v[row + x_next]
                + field.v[row_prev + x as usize]
                + field.v[row_next + x as usize]
                - 4.0 * v_c;

            let reaction = u_c * v_c * v_c;
            let du = settings.du * lap_u - reaction + settings.feed * (1.0 - u_c);
            let dv = settings.dv * lap_v + reaction - (settings.feed + settings.kill) * v_c;

            new_u[idx] = (u_c + settings.dt * du).clamp(0.0, 1.0);
            new_v[idx] = (v_c + settings.dt * dv).clamp(0.0, 1.0);
        }
    }

    MorphogenesisField::new(width, height, new_u, new_v)
}

/// Advance one output frame: `settings.substeps` Gray-Scott substeps.
/// `substeps == 0` ⇒ the field is returned unchanged (anchor A2).
pub fn advance_morphogenesis_frame(
    field: &MorphogenesisField,
    settings: &MorphogenesisSettings,
) -> Result<MorphogenesisField, RenderError> {
    let mut current = field.clone();
    for _ in 0..settings.substeps {
        current = morphogenesis_substep(&current, settings)?;
    }
    Ok(current)
}

/// Pack the field into an unquantized RGBA32F image (`U` in R, `V` in G, `B`/`A`
/// spare) for the checkpoint codec, which is [`crate::feedback_state`]'s
/// generic RGBA32F round-trip (reused as-is; the format itself doesn't know
/// or care which stateful effect wrote it).
pub fn morphogenesis_field_to_rgba32f(
    field: &MorphogenesisField,
) -> Result<ImageBufferF32, RenderError> {
    let pixels = field
        .u
        .iter()
        .zip(&field.v)
        .map(|(u, v)| [*u, *v, 0.0, 1.0])
        .collect();
    ImageBufferF32::new(field.width, field.height, pixels)
}

/// Inverse of [`morphogenesis_field_to_rgba32f`].
pub fn morphogenesis_field_from_rgba32f(
    image: &ImageBufferF32,
) -> Result<MorphogenesisField, RenderError> {
    let mut u = Vec::with_capacity(image.pixels.len());
    let mut v = Vec::with_capacity(image.pixels.len());
    for pixel in &image.pixels {
        u.push(pixel[0]);
        v.push(pixel[1]);
    }
    MorphogenesisField::new(image.width, image.height, u, v)
}

/// Debug visualization: `V` as a greyscale image (the S1 scaffold's only
/// composite — S2 replaces this with the real pattern-mix/displace output).
pub fn render_v_field_grayscale(field: &MorphogenesisField) -> Result<ImageBufferF32, RenderError> {
    ImageBufferF32::from_fn(field.width, field.height, |x, y| {
        let idx = (y as usize) * (field.width as usize) + (x as usize);
        let v = field.v[idx];
        [v, v, v, 1.0]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_carrier(width: u32, height: u32, luma: f32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |_, _| [luma, luma, luma, 1.0]).unwrap()
    }

    fn structured_carrier(width: u32, height: u32) -> ImageBufferF32 {
        // A bright ring on a dark field — gives the seed both an interior
        // "off" region and a bright nucleation front, like the radial
        // generator fixture used for the visual proof.
        ImageBufferF32::from_fn(width, height, |x, y| {
            let fx = x as f32 / width.max(1) as f32 - 0.5;
            let fy = y as f32 / height.max(1) as f32 - 0.5;
            let r = (fx * fx + fy * fy).sqrt();
            let luma = if (0.2..0.35).contains(&r) { 0.9 } else { 0.1 };
            [luma, luma, luma, 1.0]
        })
        .unwrap()
    }

    /// A single small (3x3) bright nucleus in an otherwise dark field — unlike
    /// [`structured_carrier`]'s ring (already near-maximal contrast at seed
    /// time), this starts with *low* variance and lets the reaction spread
    /// from the nucleus, so growth is directly observable as rising variance.
    fn nucleus_carrier(width: u32, height: u32) -> ImageBufferF32 {
        let patch = 3;
        let x0 = width / 2 - patch / 2;
        let y0 = height / 2 - patch / 2;
        ImageBufferF32::from_fn(width, height, |x, y| {
            let inside = x >= x0 && x < x0 + patch && y >= y0 && y < y0 + patch;
            let luma = if inside { 1.0 } else { 0.0 };
            [luma, luma, luma, 1.0]
        })
        .unwrap()
    }

    #[test]
    fn seeding_sets_u_to_one_everywhere() {
        let carrier = solid_carrier(8, 8, 0.0);
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            ..MorphogenesisSettings::coral()
        };
        let field = seed_morphogenesis_field(&carrier, &settings).expect("seed");
        assert!(field.u.iter().all(|&u| u == 1.0), "U=1 everywhere at seed");
    }

    #[test]
    fn seeding_thresholds_v_from_carrier_luma() {
        // Large enough that the ~1% speckle density is virtually certain to
        // hit at least one (but nowhere near all) pixels deterministically.
        let bright = solid_carrier(48, 48, 1.0);
        let dark = solid_carrier(48, 48, 0.0);
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            seed_threshold: 0.5,
            seed: 0,
            ..MorphogenesisSettings::coral()
        };

        let bright_field = seed_morphogenesis_field(&bright, &settings).expect("seed bright");
        assert!(
            bright_field.v.iter().all(|&v| v == SEED_ACTIVE_V),
            "every pixel above threshold is seeded"
        );

        let dark_field = seed_morphogenesis_field(&dark, &settings).expect("seed dark");
        let seeded_count = dark_field.v.iter().filter(|&&v| v == SEED_ACTIVE_V).count();
        // Below threshold everywhere: only the sparse speckle seeds anything,
        // and it must not seed everything (density is ~1%).
        assert!(seeded_count > 0, "speckle seeds at least one pixel");
        assert!(
            seeded_count < dark_field.v.len(),
            "speckle must not seed the whole dark field"
        );
    }

    #[test]
    fn seeding_is_deterministic_for_a_fixed_seed() {
        let carrier = structured_carrier(24, 24);
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            ..MorphogenesisSettings::coral()
        };
        let a = seed_morphogenesis_field(&carrier, &settings).expect("seed a");
        let b = seed_morphogenesis_field(&carrier, &settings).expect("seed b");
        assert_eq!(a, b, "identical inputs seed byte-identical fields");
    }

    #[test]
    fn different_seeds_speckle_differently() {
        let carrier = solid_carrier(32, 32, 0.0);
        let settings_a = MorphogenesisSettings {
            sim_scale: 1,
            seed: 1,
            ..MorphogenesisSettings::coral()
        };
        let settings_b = MorphogenesisSettings {
            seed: 2,
            ..settings_a
        };
        let a = seed_morphogenesis_field(&carrier, &settings_a).expect("seed a");
        let b = seed_morphogenesis_field(&carrier, &settings_b).expect("seed b");
        assert_ne!(a.v, b.v, "different seed knobs must speckle differently");
    }

    #[test]
    fn anchor_a2_frozen_field_stays_exactly_the_seed() {
        let carrier = structured_carrier(20, 20);
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            substeps: 0,
            ..MorphogenesisSettings::coral()
        };
        let seed = seed_morphogenesis_field(&carrier, &settings).expect("seed");

        let mut field = seed.clone();
        for _ in 0..10 {
            field = advance_morphogenesis_frame(&field, &settings).expect("advance");
            assert_eq!(
                field, seed,
                "A2: substeps=0 must leave the field exactly the frame-zero seed"
            );
        }
    }

    #[test]
    fn anchor_a3_dead_chemistry_decays_toward_uniform() {
        let carrier = structured_carrier(24, 24);
        // A pair well outside every alive band: feed=0 starves the reaction
        // (no `f*(1-U)` replenishment) and a moderate kill drains any seeded
        // V straight to zero.
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            feed: 0.0,
            kill: 0.08,
            substeps: 12,
            ..MorphogenesisSettings::coral()
        };
        let mut field = seed_morphogenesis_field(&carrier, &settings).expect("seed");
        let initial_variance = field.v_variance();
        assert!(
            initial_variance > 0.0,
            "seed must start with nonzero variance"
        );

        let mut variances = vec![initial_variance];
        for _ in 0..40 {
            field = advance_morphogenesis_frame(&field, &settings).expect("advance");
            variances.push(field.v_variance());
        }

        let final_variance = *variances.last().unwrap();
        assert!(
            final_variance < initial_variance * 0.05,
            "A3: dead chemistry must decay toward uniform (initial={initial_variance}, final={final_variance})"
        );
        // Falsifiable decay, not just a lower final value: the tail must be
        // monotonically non-increasing once the initial transient settles.
        let tail = &variances[variances.len() - 10..];
        for window in tail.windows(2) {
            assert!(
                window[1] <= window[0] + 1e-6,
                "A3: variance must not grow again once decaying: {tail:?}"
            );
        }
    }

    #[test]
    fn determinism_two_runs_are_byte_identical() {
        let carrier = structured_carrier(28, 20);
        let settings = MorphogenesisSettings {
            sim_scale: 2,
            ..MorphogenesisSettings::coral()
        };

        let run = |settings: &MorphogenesisSettings| {
            let mut field = seed_morphogenesis_field(&carrier, settings).expect("seed");
            for _ in 0..8 {
                field = advance_morphogenesis_frame(&field, settings).expect("advance");
            }
            field
        };

        assert_eq!(run(&settings), run(&settings));
    }

    #[test]
    fn anchor_a4_resume_matches_uninterrupted_via_rgba32f_round_trip() {
        let carrier = structured_carrier(24, 24);
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            ..MorphogenesisSettings::coral()
        };

        let mut uninterrupted = seed_morphogenesis_field(&carrier, &settings).expect("seed");
        for _ in 0..5 {
            uninterrupted =
                advance_morphogenesis_frame(&uninterrupted, &settings).expect("advance");
        }

        let mut resumed = seed_morphogenesis_field(&carrier, &settings).expect("seed");
        for _ in 0..2 {
            resumed = advance_morphogenesis_frame(&resumed, &settings).expect("advance");
        }
        // Round-trip through the checkpoint's RGBA32F representation exactly
        // like a resumed CLI render would (write, read back, keep advancing).
        let packed = morphogenesis_field_to_rgba32f(&resumed).expect("pack");
        let mut resumed = morphogenesis_field_from_rgba32f(&packed).expect("unpack");
        for _ in 0..3 {
            resumed = advance_morphogenesis_frame(&resumed, &settings).expect("advance");
        }

        assert_eq!(
            resumed, uninterrupted,
            "A4: resuming from the unquantized RGBA32F state must be byte-identical to an uninterrupted run"
        );
    }

    #[test]
    fn aliveness_every_preset_grows_and_stays_in_a_nontrivial_band() {
        let carrier = nucleus_carrier(64, 64);
        for preset in [
            MorphogenesisPreset::Coral,
            MorphogenesisPreset::Mitosis,
            MorphogenesisPreset::Worms,
            MorphogenesisPreset::Spots,
        ] {
            let settings = MorphogenesisSettings {
                sim_scale: 1,
                ..preset.settings()
            };
            let mut field = seed_morphogenesis_field(&carrier, &settings).expect("seed");
            let initial_variance = field.v_variance();

            let mut max_variance = initial_variance;
            for _ in 0..60 {
                field = advance_morphogenesis_frame(&field, &settings).expect("advance");
                max_variance = max_variance.max(field.v_variance());
            }
            let final_variance = field.v_variance();

            assert!(
                max_variance > initial_variance * 1.5,
                "{preset:?}: variance must grow from the seed (initial={initial_variance}, max={max_variance})"
            );
            assert!(
                final_variance > 0.001 && final_variance < 0.5,
                "{preset:?}: final variance must stay in a nontrivial band (final={final_variance})"
            );
        }
    }
}
