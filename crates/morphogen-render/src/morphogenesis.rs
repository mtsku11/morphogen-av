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

use crate::{sample_bilinear_clamped, ImageBufferF32, RenderError};

/// Algorithm identifier — bump when the stencil, seeding, or clamp semantics
/// change so stale checkpoints invalidate.
pub const MORPHOGENESIS_ALGORITHM: &str = "morphogenesis_cpu_v1";

/// Fraction of sim-resolution pixels that get a stochastic extra seed
/// regardless of the luma threshold — a sparse dusting so growth fronts don't
/// start perfectly symmetric even on a flat/dark carrier.
const SPECKLE_DENSITY: f32 = 0.002;

/// S3 (`docs/MORPHOGENESIS_MILESTONE.md`, "B → parameter maps") declared line
/// segment in `(feed, kill)` parameter space: the carrier's per-cell luma
/// (`[0,1]`, centered at `0.5`) shifts that cell's `(feed, kill)` away from
/// `settings`'s own values by `(luma - 0.5) * param_map_strength * DELTA`.
/// The segment's MIDPOINT (`luma == 0.5`) is exactly `settings`'s own
/// `(feed, kill)`, so `param_map_strength == 0` reproduces the uniform sim
/// exactly regardless of the carrier (continuity anchor). `feed` and `kill`
/// shift with the SAME sign (not opposite): an opposite-sign segment was
/// tried first and empirically kills the whole field on real (mostly-dark)
/// footage — Gray-Scott diffuses, so a dark-dominated carrier pushing its
/// (majority) dark cells into a truly-dead `(feed, kill)` pair drags the
/// bright/alive cells down with it over 60 frames, even though a
/// synthetic-carrier unit test (uniform luma, no diffusion pressure from a
/// dead majority) doesn't reveal it. At the coral default (`feed=0.037,
/// kill=0.060`) and `param_map_strength == 1.0`, the full-bright endpoint
/// (`feed≈0.044, kill≈0.064`) and full-dark endpoint (`feed≈0.030,
/// kill≈0.056`) both stay alive on the real-footage acceptance render (V
/// variance ≈0.015 vs ≈0.008 at frame 59) while growing visibly different
/// pattern species — empirically probed via `render-morphogenesis-field` on
/// several candidate segments before landing here.
pub const PARAM_MAP_SEGMENT_DELTA_FEED: f32 = 0.014;
pub const PARAM_MAP_SEGMENT_DELTA_KILL: f32 = 0.008;

/// Declared default `--param-map-strength`: visible bright/dark species
/// divergence without pushing the whole atlas into uniformly-dead territory
/// (see [`PARAM_MAP_SEGMENT_DELTA_FEED`]/[`PARAM_MAP_SEGMENT_DELTA_KILL`]).
pub const PARAM_MAP_STRENGTH_DEFAULT: f32 = 1.0;

fn default_param_map_strength() -> f32 {
    PARAM_MAP_STRENGTH_DEFAULT
}

/// Which weight field `--inject`/`--erode` read (Tier "Morphogenesis Live
/// Coupling" L-S1, `docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md`); only
/// meaningful when `inject > 0` or `erode > 0`. See
/// [`injection_weight_luma`]/[`injection_weight_motion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InjectSource {
    /// `w = max(0, luma - seed_threshold) / (1 - seed_threshold)`, clamped
    /// `[0,1]` — bright regions continuously feed growth.
    Luma,
    /// `w = |luma(frame N) - luma(frame N-1)|` per sim cell, clamped
    /// `[0,1]` — growth chases movement, static regions starve. Frame 0 has
    /// no prior ⇒ `w = 0` everywhere (the matte frame-zero precedent: no
    /// forward peeking, declared). The default when `inject > 0`.
    #[default]
    Motion,
}

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
    /// S3 footage coupling: strength of the per-cell `(feed, kill)` shift
    /// along [`PARAM_MAP_SEGMENT_DELTA_FEED`]/[`PARAM_MAP_SEGMENT_DELTA_KILL`],
    /// driven by the carrier's per-frame luma (see
    /// [`advance_morphogenesis_frame_with_param_map`]). `0` = the exact
    /// uniform-`(feed,kill)` sim (continuity anchor); `>= 0` only (a negative
    /// strength would just mirror the segment, which is not a meaningful
    /// distinct control). `#[serde(default)]` so pre-S3 checkpoints (whose
    /// JSON has no such key) deserialize as the same default a fresh
    /// unmodulated render would use, keeping them resumable.
    #[serde(default = "default_param_map_strength")]
    pub param_map_strength: f32,
    /// Live Coupling L-S1: per-frame source strength. `V += inject * w`
    /// (clamped), `w` chosen by `inject_source`, applied every frame BEFORE
    /// the substeps (declared order: inject → erode → substeps). `0` = off
    /// (anchor L1: byte-identical to the pre-live-coupling build).
    /// `#[serde(default)]` (`0.0`, `f32`'s own `Default`) so pre-milestone
    /// checkpoints deserialize unmodulated and stay resumable.
    #[serde(default)]
    pub inject: f32,
    /// Live Coupling L-S1: per-frame sink strength. `V *= (1 - erode * (1 -
    /// w))` (clamped), the SAME `w` as `inject` (one weight computation per
    /// frame), applied immediately after the inject pass. `0` = off.
    /// `#[serde(default)]`, same compatibility rule as `inject`.
    #[serde(default)]
    pub erode: f32,
    /// Live Coupling L-S1: which weight field `inject`/`erode` read. Only
    /// meaningful when `inject > 0` or `erode > 0`. `#[serde(default)]`
    /// (`InjectSource::Motion`), same compatibility rule as `inject`.
    #[serde(default)]
    pub inject_source: InjectSource,
    /// Live Coupling L-S2: the global negative-feedback homeostat target for
    /// mean(`V`) coverage, `[0, 1]`. Each frame (when `> 0`), the effective
    /// `(feed, kill)` used for THAT frame's substeps is shifted toward
    /// dissolution (kill up, feed down) when mean(`V`) is above target, toward
    /// growth (kill down, feed up) when below — see
    /// [`apply_coverage_homeostat`]. `0` = off: no mean(`V`) computation and no
    /// shift, byte-identical to the pre-L-S2 build (continuity anchor).
    /// `#[serde(default)]`, same pre-milestone-checkpoint compatibility rule
    /// as `inject`/`erode`.
    #[serde(default)]
    pub coverage_target: f32,
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
            param_map_strength: PARAM_MAP_STRENGTH_DEFAULT,
            inject: 0.0,
            erode: 0.0,
            inject_source: InjectSource::Motion,
            coverage_target: 0.0,
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
        if !(self.param_map_strength.is_finite() && self.param_map_strength >= 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "param_map_strength must be finite and >= 0".into(),
            ));
        }
        if !(self.inject.is_finite() && (0.0..=1.0).contains(&self.inject)) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "inject must be finite and in [0, 1]".into(),
            ));
        }
        if !(self.erode.is_finite() && (0.0..=1.0).contains(&self.erode)) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "erode must be finite and in [0, 1]".into(),
            ));
        }
        if !(self.coverage_target.is_finite() && (0.0..=1.0).contains(&self.coverage_target)) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "coverage_target must be finite and in [0, 1]".into(),
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

/// Rec.709 luma of the carrier pixel nearest-sampled at sim-lattice
/// coordinate `(x, y)` — the shared nearest-sample convention used by both
/// frame-zero seeding and the S3 per-frame `(feed, kill)` parameter map:
/// `src = (lattice_coord * sim_scale).min(carrier_dim - 1)`.
fn carrier_luma_at_sim_cell(
    carrier: &ImageBufferF32,
    sim_scale: u32,
    x: u32,
    y: u32,
) -> Result<f32, RenderError> {
    let src_x = (x * sim_scale).min(carrier.width - 1);
    let src_y = (y * sim_scale).min(carrier.height - 1);
    let pixel = carrier.pixel(src_x, src_y).ok_or_else(|| {
        RenderError::InvalidMorphogenesisField("carrier sample coordinate out of bounds".into())
    })?;
    Ok(0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2])
}

/// The carrier's luma at every sim-resolution cell (`sim_width x
/// sim_height`), nearest-sampled per [`carrier_luma_at_sim_cell`]. Used by
/// [`advance_morphogenesis_frame_with_param_map`] to build the per-frame
/// `(feed, kill)` parameter map from Source B's CURRENT frame.
pub fn sample_carrier_luma_at_sim_resolution(
    carrier: &ImageBufferF32,
    sim_width: u32,
    sim_height: u32,
    sim_scale: u32,
) -> Result<Vec<f32>, RenderError> {
    let mut luma = Vec::with_capacity((sim_width as usize) * (sim_height as usize));
    for y in 0..sim_height {
        for x in 0..sim_width {
            luma.push(carrier_luma_at_sim_cell(carrier, sim_scale, x, y)?);
        }
    }
    Ok(luma)
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
            let luma = carrier_luma_at_sim_cell(carrier_frame_zero, settings.sim_scale, x, y)?;
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

/// The per-cell `(feed, kill)` shifted along the S3 declared line segment
/// (see [`PARAM_MAP_SEGMENT_DELTA_FEED`]/[`PARAM_MAP_SEGMENT_DELTA_KILL`]).
/// `luma == 0.5` (the segment midpoint) reproduces `settings`'s own values
/// exactly; `param_map_strength == 0` does too, regardless of `luma`.
fn local_feed_kill(
    settings: &MorphogenesisSettings,
    param_map_strength: f32,
    luma: f32,
) -> (f32, f32) {
    let t = (luma - 0.5) * param_map_strength;
    (
        settings.feed + t * PARAM_MAP_SEGMENT_DELTA_FEED,
        settings.kill + t * PARAM_MAP_SEGMENT_DELTA_KILL,
    )
}

/// [`morphogenesis_substep`], but `(feed, kill)` are shifted per-cell by
/// [`local_feed_kill`] using `cell_luma` (one sample per sim cell, row-major
/// — see [`sample_carrier_luma_at_sim_resolution`]). `du`/`dv` stay global:
/// the contract only shifts the reaction rates, not diffusion.
pub fn morphogenesis_substep_with_param_map(
    field: &MorphogenesisField,
    settings: &MorphogenesisSettings,
    param_map_strength: f32,
    cell_luma: &[f32],
) -> Result<MorphogenesisField, RenderError> {
    let width = field.width;
    let height = field.height;
    let w = width as usize;
    if cell_luma.len() != field.u.len() {
        return Err(RenderError::InvalidMorphogenesisField(format!(
            "param map expected {} luma samples, got {}",
            field.u.len(),
            cell_luma.len()
        )));
    }
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

            let (feed, kill) = local_feed_kill(settings, param_map_strength, cell_luma[idx]);
            let reaction = u_c * v_c * v_c;
            let du = settings.du * lap_u - reaction + feed * (1.0 - u_c);
            let dv = settings.dv * lap_v + reaction - (feed + kill) * v_c;

            new_u[idx] = (u_c + settings.dt * du).clamp(0.0, 1.0);
            new_v[idx] = (v_c + settings.dt * dv).clamp(0.0, 1.0);
        }
    }

    MorphogenesisField::new(width, height, new_u, new_v)
}

/// [`advance_morphogenesis_frame`], but each substep runs
/// [`morphogenesis_substep_with_param_map`] against the SAME `cell_luma`
/// (the carrier's CURRENT output frame, sampled once per frame — not
/// per-substep, matching the contract's "reads the current B frame each
/// output frame"). `param_map_strength == 0.0` delegates to
/// [`advance_morphogenesis_frame`] verbatim, so the continuity anchor
/// (byte-identical to the uniform sim) holds by construction rather than by
/// floating-point coincidence.
pub fn advance_morphogenesis_frame_with_param_map(
    field: &MorphogenesisField,
    settings: &MorphogenesisSettings,
    param_map_strength: f32,
    cell_luma: &[f32],
) -> Result<MorphogenesisField, RenderError> {
    if param_map_strength == 0.0 {
        return advance_morphogenesis_frame(field, settings);
    }
    let mut current = field.clone();
    for _ in 0..settings.substeps {
        current = morphogenesis_substep_with_param_map(
            &current,
            settings,
            param_map_strength,
            cell_luma,
        )?;
    }
    Ok(current)
}

// ─── Live Coupling L-S1: per-frame inject/erode ────────────────────────────
//
// `docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md`: the reaction-diffusion
// field otherwise reads footage exactly once (frame-zero seeding); once the
// pattern fills the sim domain, Gray-Scott settles into a quasi-static
// labyrinth and the field stops responding to new frames. `inject` adds a
// per-frame source (`V += inject * w`) and `erode` a per-frame sink
// (`V *= (1 - erode * (1 - w))`), both reading a weight field `w` derived
// from the carrier's CURRENT frame (see `InjectSource`). Declared pass
// order, applied by the CLI once per output frame, BEFORE the Gray-Scott
// substeps: inject → erode → substeps. `inject == 0.0 && erode == 0.0` is
// anchor L1 — callers should skip computing `w` entirely in that case (both
// for performance and so the off-path never touches carrier motion/luma
// data it doesn't need), though [`apply_inject_erode`] is also a
// mathematical identity at that point regardless.

/// [`InjectSource::Luma`]'s weight field: the carrier's per-cell luma
/// (`carrier_luma`, at sim resolution — see
/// [`sample_carrier_luma_at_sim_resolution`]), rescaled so `seed_threshold`
/// sits at `w = 0` and full-bright (`luma == 1.0`) sits at `w = 1`, clamped
/// `[0,1]`. `seed_threshold == 1.0` would divide by zero; guarded to `0.0`
/// instead (every `luma <= 1.0`, so the numerator is already `<= 0` at that
/// threshold and the mathematically-consistent weight is zero anyway).
pub fn injection_weight_luma(carrier_luma: &[f32], seed_threshold: f32) -> Vec<f32> {
    let denom = (1.0 - seed_threshold).max(1e-6);
    carrier_luma
        .iter()
        .map(|&luma| ((luma - seed_threshold).max(0.0) / denom).clamp(0.0, 1.0))
        .collect()
}

/// [`InjectSource::Motion`]'s weight field: `w = |luma(frame N) - luma(frame
/// N-1)|` per sim cell, clamped `[0,1]`. `previous_luma == None` (frame 0,
/// no prior frame to diff against) ⇒ `w = 0` everywhere — the matte
/// frame-zero precedent, declared: no forward peeking. Mismatched lengths
/// between `current_luma` and `previous_luma` are a caller bug (both must be
/// sampled at the same sim resolution); pairs beyond the shorter slice are
/// silently dropped by `zip` rather than panicking, but callers should never
/// let the lengths diverge in the first place — [`apply_inject_erode`] is
/// the layer that validates `w`'s length against the field.
pub fn injection_weight_motion(current_luma: &[f32], previous_luma: Option<&[f32]>) -> Vec<f32> {
    match previous_luma {
        None => vec![0.0; current_luma.len()],
        Some(previous_luma) => current_luma
            .iter()
            .zip(previous_luma)
            .map(|(&current, &previous)| (current - previous).abs().clamp(0.0, 1.0))
            .collect(),
    }
}

/// Apply the L-S1 inject/erode passes to `V` only, in the declared order —
/// inject, then erode — each clamped to `[0,1]` immediately: `V += inject *
/// w`, then `V *= (1 - erode * (1 - w))`. `U` passes through unchanged. `w`
/// must be sampled at `field`'s own resolution (one sample per cell);
/// mismatched lengths are an error rather than a silent truncation.
pub fn apply_inject_erode(
    field: &MorphogenesisField,
    settings: &MorphogenesisSettings,
    w: &[f32],
) -> Result<MorphogenesisField, RenderError> {
    if w.len() != field.v.len() {
        return Err(RenderError::InvalidMorphogenesisField(format!(
            "inject/erode weight field expected {} samples, got {}",
            field.v.len(),
            w.len()
        )));
    }
    let mut v = field.v.clone();
    for (value, &w) in v.iter_mut().zip(w) {
        *value = (*value + settings.inject * w).clamp(0.0, 1.0);
        *value = (*value * (1.0 - settings.erode * (1.0 - w))).clamp(0.0, 1.0);
    }
    MorphogenesisField::new(field.width, field.height, field.u.clone(), v)
}

// ─── Live Coupling L-S2: coverage-target homeostat ─────────────────────────
//
// `docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md`: `inject` alone (no sink)
// on bright footage saturates V toward the injection ceiling — the field
// still "freezes" (just at a different, brighter equilibrium) instead of
// finding a moving balance. `coverage_target` is a global negative-feedback
// controller: each frame, mean(V) is compared to the target and the
// EFFECTIVE `(feed, kill)` used for that frame's substeps is nudged toward
// dissolution (kill up, feed down — the Gray-Scott "dead" direction) when
// coverage is too high, toward growth (kill down, feed up) when too low.

/// Pinned proportional gain for [`apply_coverage_homeostat`]'s `(feed,
/// kill)` shift, tuned empirically on the L4 anchor (coverage-target 0.3 +
/// pure-luma injection on a bright, textured carrier — see
/// `anchor_l4_homeostat_settles_mean_v_within_band_of_coverage_target`).
/// Swept `0.01..=0.6` on the anchor scenario: `0.01..=0.3` all settle within
/// the ±0.1 band (tail mean rising from ≈0.22 to ≈0.27 as gain increases,
/// tracking closer to the 0.3 target with milder residual oscillation at the
/// low end), but `0.6` COLLAPSES the field toward near-total dissolution
/// (tail settles at ≈0.003 — outside the band): too-aggressive kill increase
/// self-reinforces once the field starts dying, since less V feeds back
/// into less injection weight overlap and the loop never recovers. `0.15`
/// sits mid-band with the least oscillation (tail spread < 0.001) and the
/// most margin from both the low-gain undershoot and the high-gain collapse.
pub const COVERAGE_GAIN: f32 = 0.15;

/// The L-S2 homeostat. Returns a copy of `settings` with `feed`/`kill`
/// shifted by `COVERAGE_GAIN * (mean(V) - coverage_target)` — feed DOWN and
/// kill UP (floored at `0.0`) when mean(V) exceeds the target (dissolution),
/// the opposite when it's under (growth). Declared to run AFTER modulation
/// routes have resolved `settings.feed`/`kill` for this frame (the shift
/// "rides on top" of a routed value — `feed = audio-rms` still drives the
/// base), and its OUTPUT is what the per-cell param map centers its own
/// per-cell divergence on for the frame's substeps (declared order: routes →
/// param map → homeostat describes the knob-resolution precedence; the
/// homeostat's shifted `(feed, kill)` is the "effective" base the param map
/// then divides around). `coverage_target <= 0.0` is the off case: returns
/// `*settings` unchanged with NO mean(V) computation (anchor: byte-identical
/// to the pre-L-S2 build).
pub fn apply_coverage_homeostat(
    settings: &MorphogenesisSettings,
    field: &MorphogenesisField,
) -> MorphogenesisSettings {
    if settings.coverage_target <= 0.0 {
        return *settings;
    }
    let n = (field.v.len() as f32).max(1.0);
    let mean_v = field.v.iter().sum::<f32>() / n;
    let error = mean_v - settings.coverage_target;
    let shift = COVERAGE_GAIN * error;
    let mut shifted = *settings;
    shifted.feed = (settings.feed - shift).max(0.0);
    shifted.kill = (settings.kill + shift).max(0.0);
    shifted
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

/// The shared `V` -> greyscale pixel mapping used by every raw-field render
/// path (the S1 debug scaffold and the Field View milestone's sequence
/// output, `docs/MORPHOGENESIS_FIELD_VIEW_MILESTONE.md`) — kept as its own
/// function so the two call sites cannot drift apart.
fn v_to_grayscale_pixel(v: f32) -> [f32; 4] {
    [v, v, v, 1.0]
}

/// Debug visualization: `V` as a greyscale image at SIM resolution (the S1
/// scaffold's only composite — S2 replaces this with the real
/// pattern-mix/displace output for `render-morphogenesis-sequence`). Stays
/// sim-res raw; unchanged by the Field View milestone (its own niche).
pub fn render_v_field_grayscale(field: &MorphogenesisField) -> Result<ImageBufferF32, RenderError> {
    ImageBufferF32::from_fn(field.width, field.height, |x, y| {
        let idx = (y as usize) * (field.width as usize) + (x as usize);
        v_to_grayscale_pixel(field.v[idx])
    })
}

/// Field View milestone (`docs/MORPHOGENESIS_FIELD_VIEW_MILESTONE.md`, FV2):
/// the same `V` -> greyscale mapping as [`render_v_field_grayscale`], but
/// bilinearly upsampled from sim resolution to `carrier_width x
/// carrier_height` (via [`sample_scalar_grid`], the composite's own upsample
/// convention) so `render-morphogenesis-sequence`'s field-view output is
/// carrier-resolution regardless of `--sim-scale`. At `sim_scale == 1` (sim
/// resolution == carrier resolution) [`sample_scalar_grid`] samples each
/// output pixel at its own integer coordinate with zero interpolation
/// weight, so this is byte-identical to [`render_v_field_grayscale`]
/// sample-for-sample — the can't-drift proof, not a coincidence.
pub fn render_v_field_grayscale_upsampled(
    field: &MorphogenesisField,
    carrier_width: u32,
    carrier_height: u32,
) -> Result<ImageBufferF32, RenderError> {
    ImageBufferF32::from_fn(carrier_width, carrier_height, |x, y| {
        let v = sample_scalar_grid(
            field.width,
            field.height,
            &field.v,
            carrier_width,
            carrier_height,
            x as f32,
            y as f32,
        );
        v_to_grayscale_pixel(v)
    })
}

/// Which representation `render-morphogenesis-sequence` writes as its output
/// frame (`docs/MORPHOGENESIS_FIELD_VIEW_MILESTONE.md`). `Composite` is the
/// pre-milestone S2 behaviour (default, FV1 byte-identity); `Field` is the
/// raw `V` field, greyscale, upsampled to carrier resolution
/// ([`render_v_field_grayscale_upsampled`]) — the composite knobs
/// (`pattern_mix`/`displace`/`hue`/`mode`) stay legal but inert in this view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputView {
    #[default]
    Composite,
    Field,
}

// ─── S2 composite: pattern-mix colourize + displace-along-∇V ──────────────
//
// `out = carrier` reshaped by the Gray-Scott `V` field two ways, each with a
// strength knob (`docs/MORPHOGENESIS_MILESTONE.md`'s S2 entry): `displace`
// pushes the carrier sample along `∇V` (chemotaxis smear), and `pattern_mix`
// tints the (possibly displaced) sample toward a colourized version of
// itself, weighted by the local `V` value. Both knobs live in
// [`MorphogenesisCompositeSettings`], separate from [`MorphogenesisSettings`]
// because they don't affect field evolution — only `out`. They DO join the
// sequence's checkpoint contract in `morphogen-cli` (declared there): a
// resume with changed composite knobs would silently make new frames
// colour/warp differently from already-written ones, which the stale-state
// invariant forbids exactly like a changed field setting.

/// How [`composite_morphogenesis_frame`] chooses the pattern-mix tint colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatternColorMode {
    /// Tint toward a fixed hue (`pattern_hue`, turns) — growth is painted a
    /// uniform colour wash.
    Hue,
    /// Tint toward the sample's OWN hue, pushed to full saturation — growth
    /// reads as the footage's own colour turning vivid rather than being
    /// repainted a foreign hue ("the growth takes the local B colour").
    Inherit,
}

fn default_shade_height() -> f32 {
    3.0
}

fn default_shade_elevation() -> f32 {
    0.15
}

fn default_shade_shininess() -> f32 {
    16.0
}

/// Composite (S2) knobs. See the module section above for why these are
/// separate from [`MorphogenesisSettings`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MorphogenesisCompositeSettings {
    /// `[0,1]`: strength of the `V`-weighted colourize tint. `0` = the
    /// (possibly displaced) carrier sample passes through unmodified.
    pub pattern_mix: f32,
    /// Pixel displacement pushing the carrier sample along `∇V`. `0` = no
    /// displacement (the carrier is sampled at its own integer coordinate).
    pub displace: f32,
    /// Hue (turns, `[0,1)`) used by [`PatternColorMode::Hue`]; ignored by
    /// [`PatternColorMode::Inherit`].
    pub pattern_hue: f32,
    /// Which colour the pattern-mix tint targets.
    pub pattern_color_mode: PatternColorMode,
    /// Track B1 (`docs/MORPHOGENESIS_RELIEF_SHADING_MILESTONE.md`): relief-
    /// shading blend strength, `[0,1]`. `0` = off — the pre-slice tint/field
    /// output exactly (continuity anchor RS1). `#[serde(default)]` (`0.0`)
    /// so pre-slice checkpoints deserialize with shading off and stay
    /// resumable.
    #[serde(default)]
    pub shade: f32,
    /// Gradient→normal scale: how strongly `∇V` tilts the lit surface's
    /// normal away from `(0,0,1)`. `#[serde(default = "default_shade_height")]`
    /// so pre-slice checkpoints (no key) get the same value a fresh
    /// unshaded-by-default render would use.
    #[serde(default = "default_shade_height")]
    pub shade_height: f32,
    /// Light azimuth, turns (`[0,1)`, wraps). `#[serde(default)]` (`0.0`).
    #[serde(default)]
    pub shade_azimuth: f32,
    /// Light elevation above the horizon, turns (nominally `[0, 0.25]`).
    /// `#[serde(default = "default_shade_elevation")]`.
    #[serde(default = "default_shade_elevation")]
    pub shade_elevation: f32,
    /// Specular highlight strength, `[0,1]`. `#[serde(default)]` (`0.0`).
    #[serde(default)]
    pub shade_specular: f32,
    /// Specular exponent (Phong shininess). `#[serde(default =
    /// "default_shade_shininess")]`.
    #[serde(default = "default_shade_shininess")]
    pub shade_shininess: f32,
}

impl MorphogenesisCompositeSettings {
    /// Anchor A1: no colourize, no displacement — `composite_morphogenesis_frame`
    /// is byte-identical to the carrier regardless of the field.
    pub fn passthrough() -> Self {
        Self {
            pattern_mix: 0.0,
            displace: 0.0,
            pattern_hue: 0.0,
            pattern_color_mode: PatternColorMode::Hue,
            shade: 0.0,
            shade_height: default_shade_height(),
            shade_azimuth: 0.0,
            shade_elevation: default_shade_elevation(),
            shade_specular: 0.0,
            shade_shininess: default_shade_shininess(),
        }
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        if !(self.pattern_mix.is_finite() && (0.0..=1.0).contains(&self.pattern_mix)) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "pattern_mix must be finite and in [0, 1]".into(),
            ));
        }
        if !self.displace.is_finite() {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "displace must be finite".into(),
            ));
        }
        if !self.pattern_hue.is_finite() {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "pattern_hue must be finite".into(),
            ));
        }
        if !(self.shade.is_finite() && (0.0..=1.0).contains(&self.shade)) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "shade must be finite and in [0, 1]".into(),
            ));
        }
        if !self.shade_height.is_finite() {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "shade_height must be finite".into(),
            ));
        }
        if !self.shade_azimuth.is_finite() {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "shade_azimuth must be finite".into(),
            ));
        }
        if !self.shade_elevation.is_finite() {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "shade_elevation must be finite".into(),
            ));
        }
        if !(self.shade_specular.is_finite() && (0.0..=1.0).contains(&self.shade_specular)) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "shade_specular must be finite and in [0, 1]".into(),
            ));
        }
        if !(self.shade_shininess.is_finite() && self.shade_shininess > 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "shade_shininess must be finite and > 0".into(),
            ));
        }
        Ok(())
    }
}

impl Default for MorphogenesisCompositeSettings {
    /// `pattern_mix` defaults nonzero (~0.85, declared S2 default) so the
    /// first render shows the growth rather than a silent passthrough;
    /// `displace` defaults to `0` (a purely additive knob).
    fn default() -> Self {
        Self {
            pattern_mix: 0.85,
            displace: 0.0,
            pattern_hue: 0.02,
            pattern_color_mode: PatternColorMode::Hue,
            shade: 0.0,
            shade_height: default_shade_height(),
            shade_azimuth: 0.0,
            shade_elevation: default_shade_elevation(),
            shade_specular: 0.0,
            shade_shininess: default_shade_shininess(),
        }
    }
}

fn luma3(r: f32, g: f32, b: f32) -> f32 {
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// HSV (h in turns, s/v in [0,1]) → RGB in [0,1].
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
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

/// RGB in [0,1] → HSV (h in turns, s/v in [0,1]).
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max <= 0.0 { 0.0 } else { d / max };
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, v)
}

/// A fully-saturated colour at `hue` (turns), rescaled so its luma matches
/// `color`'s own luma ("colourize toward a hue, luma-preserving": the tint
/// keeps the pixel's brightness but replaces its colour).
fn colorize_luma_preserving(color: [f32; 4], hue: f32) -> [f32; 4] {
    let (r, g, b) = hsv_to_rgb(hue.rem_euclid(1.0), 1.0, 1.0);
    let hue_luma = luma3(r, g, b);
    let source_luma = luma3(color[0], color[1], color[2]);
    let scale = if hue_luma > 1e-6 {
        source_luma / hue_luma
    } else {
        0.0
    };
    [
        (r * scale).clamp(0.0, 1.0),
        (g * scale).clamp(0.0, 1.0),
        (b * scale).clamp(0.0, 1.0),
        color[3],
    ]
}

// ─── Track B1: gradient-lit relief shading ─────────────────────────────────
//
// `docs/MORPHOGENESIS_RELIEF_SHADING_MILESTONE.md`: treat `V` as a height
// field and light it with a directional lamp. `colorize_luma_preserving`
// (above) ties the tint's brightness to the CARRIER's own luma, which is
// exactly why growth is invisible on near-black footage (tinting black stays
// black). Relief shading's target colour instead gets its brightness from
// the LIGHT hitting the surface — independent of how dark the carrier is —
// which is what closes that gap (RS3).

/// Fixed ambient term for the relief-shading model — not user-exposed. Keeps
/// fully-shadowed regions faintly present while leaving clear diffuse/
/// specular contrast toward the lit side.
const SHADE_AMBIENT: f32 = 0.35;

/// The Phong-style relief lighting value at one cell, given its `V`-gradient
/// (`grad_x`, `grad_y` — the SAME gradient the displace pass computes, see
/// [`morphogenesis_v_gradient`]). `n` is the surface normal built by tilting
/// `(0,0,1)` by the (scaled) gradient; `l` is the light direction from
/// `shade_azimuth`/`shade_elevation` (both turns). Returns `ambient +
/// (1-ambient)*diffuse + shade_specular*specular`, UNCLAMPED (may exceed `1`
/// under a strong specular highlight) — callers blend/clamp when they mix it
/// into a pixel.
fn morphogenesis_shading_value(
    grad_x: f32,
    grad_y: f32,
    composite: &MorphogenesisCompositeSettings,
) -> f32 {
    let nx = -grad_x * composite.shade_height;
    let ny = -grad_y * composite.shade_height;
    let nz = 1.0_f32;
    let n_len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-6);
    let (nx, ny, nz) = (nx / n_len, ny / n_len, nz / n_len);

    let az = composite.shade_azimuth * std::f32::consts::TAU;
    let el = composite.shade_elevation * std::f32::consts::TAU;
    let lx = el.cos() * az.cos();
    let ly = el.cos() * az.sin();
    let lz = el.sin();

    let n_dot_l = nx * lx + ny * ly + nz * lz;
    let diffuse = n_dot_l.max(0.0);

    // reflect(-l, n) with GLSL's reflect(I, N) = I - 2*dot(N,I)*N, I = -l:
    // reflect(-l, n) = -l + 2*(n.l)*n. Its dot with the view dir (0,0,1) is
    // just its z component.
    let reflect_z = -lz + 2.0 * n_dot_l * nz;
    let specular = reflect_z.max(0.0).powf(composite.shade_shininess.max(1.0));

    SHADE_AMBIENT + (1.0 - SHADE_AMBIENT) * diffuse + composite.shade_specular * specular
}

/// The pattern-mix target colour, relief-shaded (Track B1). `shade <= 0`
/// delegates to [`colorize_luma_preserving`] verbatim — byte-identical
/// (anchor RS1). Otherwise linearly blends from that (carrier-luma-tied)
/// tint toward a colour whose brightness comes from
/// [`morphogenesis_shading_value`] instead — at `shade == 1` the target is
/// fully light-driven, independent of the carrier's own luma, which is what
/// makes growth visible on near-black footage (RS3).
fn colorize_relief(
    color: [f32; 4],
    hue: f32,
    grad_x: f32,
    grad_y: f32,
    composite: &MorphogenesisCompositeSettings,
) -> [f32; 4] {
    let preserved = colorize_luma_preserving(color, hue);
    if composite.shade <= 0.0 {
        return preserved;
    }
    let lit = morphogenesis_shading_value(grad_x, grad_y, composite);
    let (r, g, b) = hsv_to_rgb(hue.rem_euclid(1.0), 1.0, lit.max(0.0));
    [
        (preserved[0] + (r - preserved[0]) * composite.shade).clamp(0.0, 1.0),
        (preserved[1] + (g - preserved[1]) * composite.shade).clamp(0.0, 1.0),
        (preserved[2] + (b - preserved[2]) * composite.shade).clamp(0.0, 1.0),
        color[3],
    ]
}

/// Gradient of the `V` field (central differences, **clamped edges** — the
/// same stencil convention as [`morphogenesis_substep`]'s Laplacian) at sim
/// resolution. Computed once per composite call, then bilinearly upsampled
/// to frame size per pixel by [`sample_scalar_grid`] — the same
/// upsample-at-composite-time convention as the `V` field itself
/// (`docs/MORPHOGENESIS_MILESTONE.md`'s sim_scale contract).
fn morphogenesis_v_gradient(field: &MorphogenesisField) -> (Vec<f32>, Vec<f32>) {
    let w = field.width as usize;
    let mut gx = vec![0.0_f32; field.v.len()];
    let mut gy = vec![0.0_f32; field.v.len()];
    for y in 0..field.height {
        let y_prev = clamp_prev(y) as usize;
        let y_next = clamp_next(y, field.height) as usize;
        for x in 0..field.width {
            let x_prev = clamp_prev(x) as usize;
            let x_next = clamp_next(x, field.width) as usize;
            let idx = y as usize * w + x as usize;
            let left = field.v[y as usize * w + x_prev];
            let right = field.v[y as usize * w + x_next];
            let up = field.v[y_prev * w + x as usize];
            let down = field.v[y_next * w + x as usize];
            gx[idx] = (right - left) * 0.5;
            gy[idx] = (down - up) * 0.5;
        }
    }
    (gx, gy)
}

/// Bilinear sample of a sim-resolution scalar grid at a carrier-resolution
/// pixel coordinate `(px, py)`, clamped at the grid borders (the scalar
/// analogue of [`sample_bilinear_clamped`]). Pixel-centre alignment so a
/// `sim_scale` that doesn't evenly divide the carrier dimensions (the
/// `div_ceil` convention in [`morphogenesis_field_dimensions`]) still samples
/// smoothly to the frame edges.
fn sample_scalar_grid(
    width: u32,
    height: u32,
    values: &[f32],
    carrier_width: u32,
    carrier_height: u32,
    px: f32,
    py: f32,
) -> f32 {
    let fx = (px + 0.5) * width as f32 / carrier_width.max(1) as f32 - 0.5;
    let fy = (py + 0.5) * height as f32 / carrier_height.max(1) as f32 - 0.5;
    let x0f = fx.floor();
    let y0f = fy.floor();
    let tx = fx - x0f;
    let ty = fy - y0f;
    let x0 = (x0f as i64).clamp(0, width as i64 - 1) as u32;
    let y0 = (y0f as i64).clamp(0, height as i64 - 1) as u32;
    let x1 = ((x0f + 1.0) as i64).clamp(0, width as i64 - 1) as u32;
    let y1 = ((y0f + 1.0) as i64).clamp(0, height as i64 - 1) as u32;
    let w = width as usize;
    let v00 = values[y0 as usize * w + x0 as usize];
    let v10 = values[y0 as usize * w + x1 as usize];
    let v01 = values[y1 as usize * w + x0 as usize];
    let v11 = values[y1 as usize * w + x1 as usize];
    let top = v00 + (v10 - v00) * tx;
    let bottom = v01 + (v11 - v01) * tx;
    top + (bottom - top) * ty
}

/// The S2 composite: `out = carrier` reshaped by the Gray-Scott `V` field.
///
/// 1. **Displace** — sample the carrier at `(x,y) - displace * ∇V(x,y)` (the
///    gather convention: a pixel whose gradient points toward it is pulled
///    from upstream, the chemotaxis smear). `∇V` is
///    [`morphogenesis_v_gradient`] bilinearly upsampled via
///    [`sample_scalar_grid`].
/// 2. **Pattern-mix** — tint the displaced sample toward
///    [`colorize_luma_preserving`], `V`-weighted (`strength = pattern_mix *
///    V`); [`PatternColorMode::Hue`] tints toward the fixed `pattern_hue`,
///    [`PatternColorMode::Inherit`] tints toward the sample's own hue.
///
/// `pattern_mix == 0 && displace == 0` is anchor A1: every output pixel
/// samples the carrier at its own integer coordinate with a zero-strength
/// mix (short-circuited before any `V` sampling), so the output is
/// byte-identical to the carrier regardless of what the field is doing.
pub fn composite_morphogenesis_frame(
    carrier: &ImageBufferF32,
    field: &MorphogenesisField,
    settings: &MorphogenesisCompositeSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let (gx, gy) = morphogenesis_v_gradient(field);
    ImageBufferF32::from_fn(carrier.width, carrier.height, |x, y| {
        let px = x as f32;
        let py = y as f32;

        // Track B1: shading also needs ∇V at this pixel, so the gradient is
        // sampled whenever EITHER displace or shade is active (same
        // condition as the pre-slice displace-only gate when shade == 0,
        // preserving RS1 byte-identity exactly).
        let need_gradient = settings.displace != 0.0 || settings.shade > 0.0;
        let (grad_x, grad_y) = if need_gradient {
            (
                sample_scalar_grid(
                    field.width,
                    field.height,
                    &gx,
                    carrier.width,
                    carrier.height,
                    px,
                    py,
                ),
                sample_scalar_grid(
                    field.width,
                    field.height,
                    &gy,
                    carrier.width,
                    carrier.height,
                    px,
                    py,
                ),
            )
        } else {
            (0.0, 0.0)
        };

        let (sample_x, sample_y) = if settings.displace == 0.0 {
            (px, py)
        } else {
            (
                px - settings.displace * grad_x,
                py - settings.displace * grad_y,
            )
        };
        let color = sample_bilinear_clamped(carrier, sample_x, sample_y);

        if settings.pattern_mix <= 0.0 {
            return color;
        }
        let v = sample_scalar_grid(
            field.width,
            field.height,
            &field.v,
            carrier.width,
            carrier.height,
            px,
            py,
        )
        .clamp(0.0, 1.0);
        let strength = (settings.pattern_mix * v).clamp(0.0, 1.0);
        if strength <= 0.0 {
            return color;
        }
        let target = match settings.pattern_color_mode {
            PatternColorMode::Hue => {
                colorize_relief(color, settings.pattern_hue, grad_x, grad_y, settings)
            }
            PatternColorMode::Inherit => {
                let (h, _s, _v) = rgb_to_hsv(color[0], color[1], color[2]);
                colorize_relief(color, h, grad_x, grad_y, settings)
            }
        };
        [
            color[0] + (target[0] - color[0]) * strength,
            color[1] + (target[1] - color[1]) * strength,
            color[2] + (target[2] - color[2]) * strength,
            color[3],
        ]
    })
}

/// Track B1 field-view variant of [`render_v_field_grayscale_upsampled`]:
/// `shade <= 0` delegates verbatim (byte-identical, RS1). Otherwise blends
/// the plain greyscale `V` toward a relief-shaded version — `V *
/// morphogenesis_shading_value(...)` — by `composite.shade`, so the B/W
/// field becomes an embossed membrane rather than a flat readout.
pub fn render_v_field_grayscale_upsampled_with_shading(
    field: &MorphogenesisField,
    carrier_width: u32,
    carrier_height: u32,
    composite: &MorphogenesisCompositeSettings,
) -> Result<ImageBufferF32, RenderError> {
    if composite.shade <= 0.0 {
        return render_v_field_grayscale_upsampled(field, carrier_width, carrier_height);
    }
    let (gx, gy) = morphogenesis_v_gradient(field);
    ImageBufferF32::from_fn(carrier_width, carrier_height, |x, y| {
        let px = x as f32;
        let py = y as f32;
        let v = sample_scalar_grid(
            field.width,
            field.height,
            &field.v,
            carrier_width,
            carrier_height,
            px,
            py,
        );
        let grad_x = sample_scalar_grid(
            field.width,
            field.height,
            &gx,
            carrier_width,
            carrier_height,
            px,
            py,
        );
        let grad_y = sample_scalar_grid(
            field.width,
            field.height,
            &gy,
            carrier_width,
            carrier_height,
            px,
            py,
        );
        let lit = morphogenesis_shading_value(grad_x, grad_y, composite);
        let flat = v_to_grayscale_pixel(v);
        let shaded_value = (v * lit).clamp(0.0, 1.0);
        [
            (flat[0] + (shaded_value - flat[0]) * composite.shade).clamp(0.0, 1.0),
            (flat[1] + (shaded_value - flat[1]) * composite.shade).clamp(0.0, 1.0),
            (flat[2] + (shaded_value - flat[2]) * composite.shade).clamp(0.0, 1.0),
            1.0,
        ]
    })
}

// ─── Track A1: FitzHugh–Nagumo excitable media ─────────────────────────────
//
// `docs/MORPHOGENESIS_FHN_MILESTONE.md`: a second field model, selected by
// `--model`, reusing [`MorphogenesisField`] as its raw `(u, v)` grid (nothing
// about that struct assumes Gray-Scott's `[0,1]` range) and the SAME RGBA32F
// checkpoint codec. `u` is the fast, signed activator; `v` the slow,
// signed recovery variable — no Laplacian on `v`. See the milestone doc for
// the full design rationale (why a separate settings struct, why the
// resting-state solver, why inject is a forcing current not an additive V
// bump).

/// Algorithm identifier for the FHN model — a distinct id from
/// [`MORPHOGENESIS_ALGORITHM`] so a `--model` change on an existing output
/// directory can never be mistaken for a resumable Gray-Scott checkpoint.
pub const MORPHOGENESIS_FHN_ALGORITHM: &str = "morphogenesis_fhn_cpu_v1";

/// Safety box: both `u` and `v` are clamped to `[-SAFETY, SAFETY]` after
/// every substep. Not part of the physics — a guard against float blow-up
/// only; the excitable dynamics need signed values well outside `[0,1]`.
pub const FHN_SAFETY_BOX: f32 = 3.0;

/// FHN `inject`'s legal range — wider than Gray-Scott's `[0,1]` (the same
/// CLI/queue flag name, but a different physical quantity: a MULTIPLIER of
/// `stimulus`, not a fraction of `V`'s own `[0,1]` range). Real footage
/// motion weights are usually well under 1.0 (subtle movement), so
/// `inject * stimulus * w` needs headroom above 1.0 to ever reach the ≈half-
/// stimulus kick required to cross the firing threshold — confirmed
/// empirically: `inject == 0.1` (Gray-Scott's whole legal range) on real
/// footage produced a mean 5.4/255 delta after 143 frames with no visually
/// distinct NEW pulses, only faint boundary jitter on the already-seeded
/// front.
pub const FHN_INJECT_RANGE: (f32, f32) = (0.0, 8.0);

/// Which field model a `render-morphogenesis-sequence` render uses.
/// `#[serde(default)]` (`GrayScott`) so every pre-A1 checkpoint/queue task
/// (no `model` key at all) deserializes as Gray-Scott and stays resumable —
/// continuity anchor FHN0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MorphogenesisModel {
    #[default]
    GrayScott,
    FitzhughNagumo,
    /// Track A2 (`docs/MORPHOGENESIS_LENIA_MILESTONE.md`).
    Lenia,
}

/// FitzHugh–Nagumo parameters + sim/seeding knobs, parallel in shape to
/// [`MorphogenesisSettings`] but deliberately a separate struct — the
/// parameters mean different things physically and folding them together
/// would make every Gray-Scott-only field awkwardly optional.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FhnSettings {
    /// `u` diffusion rate (`Du` in the PDE).
    pub du: f32,
    /// Recovery time-scale separation (`ε`); small = slow recovery = longer,
    /// more persistent pulses.
    pub epsilon: f32,
    /// Nullcline shape parameter `a`.
    pub a: f32,
    /// Nullcline shape parameter `b`.
    pub b: f32,
    /// Per-substep integration step.
    pub dt: f32,
    /// Substeps run per output frame. `0` freezes the field (byte-identical
    /// to the frame-zero seed forever, same anchor shape as Gray-Scott's
    /// A2).
    pub substeps: u32,
    /// Sim resolution divisor relative to the carrier frame.
    pub sim_scale: u32,
    /// Frame-zero seed threshold: carrier luma `>=` this fires a stimulus.
    pub seed_threshold: f32,
    /// Deterministic seed for the frame-zero speckle.
    pub seed: u64,
    /// How far above the resting `u` a seeded/injected cell is pushed
    /// (`u = u_rest + stimulus`) — must clear the nullcline's local-max
    /// threshold to reliably fire a pulse rather than relaxing straight
    /// back to rest.
    pub stimulus: f32,
    /// Live coupling: `u += inject * stimulus * w(x,y)`, a discrete kick
    /// applied ONCE per output frame BEFORE the substeps (see
    /// [`apply_fhn_inject`] — scaled by `stimulus` so `inject == 1.0` at
    /// full weight reliably fires a new pulse, matching the seed's own
    /// kick strength). Legal range [`FHN_INJECT_RANGE`] — wider than
    /// Gray-Scott's `[0,1]` `inject`, since real motion weights are usually
    /// well under 1.0 and need headroom to reach a firing-strength kick.
    /// `0` = off. `#[serde(default)]` so pre-A1-S2 checkpoints deserialize
    /// unmodulated and stay resumable.
    #[serde(default)]
    pub inject: f32,
    /// Which weight field `--inject` reads (same [`InjectSource`] as
    /// Gray-Scott). `#[serde(default)]` (`InjectSource::Motion`).
    #[serde(default)]
    pub inject_source: InjectSource,
}

impl FhnSettings {
    /// `pulse`: excitable, a single stimulus fires and dies out — pure
    /// music-reactive one-shot, no self-sustaining structure.
    pub fn pulse() -> Self {
        Self {
            du: 1.0,
            epsilon: 0.08,
            a: 0.7,
            b: 0.8,
            dt: 0.1,
            substeps: 4,
            sim_scale: 2,
            seed_threshold: 0.5,
            seed: 71,
            stimulus: 2.5,
            inject: 0.0,
            inject_source: InjectSource::Motion,
        }
    }

    /// `spiral`: a broken-symmetry stimulus (an off-centre seed patch) seeds
    /// self-sustaining rotating wavefronts rather than a single dying pulse.
    pub fn spiral() -> Self {
        Self {
            epsilon: 0.05,
            stimulus: 3.0,
            substeps: 6,
            ..Self::pulse()
        }
    }

    /// `labyrinth`: a Turing-ish FHN regime — dense, standing wavefront
    /// structure rather than a clean travelling pulse.
    pub fn labyrinth() -> Self {
        Self {
            epsilon: 0.12,
            a: 0.5,
            b: 0.6,
            stimulus: 2.0,
            substeps: 4,
            ..Self::pulse()
        }
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        if !(self.du.is_finite() && self.du >= 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "du must be finite and >= 0".into(),
            ));
        }
        if !(self.epsilon.is_finite() && self.epsilon > 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "epsilon must be finite and > 0".into(),
            ));
        }
        if !self.a.is_finite() {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "a must be finite".into(),
            ));
        }
        if !(self.b.is_finite() && self.b > 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "b must be finite and > 0".into(),
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
        if !self.stimulus.is_finite() {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "stimulus must be finite".into(),
            ));
        }
        if !(self.inject.is_finite()
            && (FHN_INJECT_RANGE.0..=FHN_INJECT_RANGE.1).contains(&self.inject))
        {
            return Err(RenderError::InvalidMorphogenesisSettings(format!(
                "inject must be finite and in [{}, {}]",
                FHN_INJECT_RANGE.0, FHN_INJECT_RANGE.1
            )));
        }
        Ok(())
    }
}

impl Default for FhnSettings {
    fn default() -> Self {
        Self::pulse()
    }
}

/// Named FHN presets, parallel to [`MorphogenesisPreset`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FhnPreset {
    #[default]
    Pulse,
    Spiral,
    Labyrinth,
}

impl FhnPreset {
    pub fn settings(self) -> FhnSettings {
        match self {
            Self::Pulse => FhnSettings::pulse(),
            Self::Spiral => FhnSettings::spiral(),
            Self::Labyrinth => FhnSettings::labyrinth(),
        }
    }
}

/// The FHN fixed point (`I == 0`): solves `v = u - u³/3` (the `u`-nullcline
/// at equilibrium) simultaneously with `v = (u + a) / b` (the `v`-nullcline)
/// via their combined cubic `b/3·u³ - (b-1)·u + a = 0`, by Newton-Raphson
/// from `u₀ = 0`. Monotonic (derivative `b·u² - (b-1)` has no real root for
/// every preset's `b < 1`), so this converges to the single real root
/// deterministically in a handful of iterations — the resting state every
/// unstimulated cell starts at (FHN1/FHN2).
pub fn fhn_resting_state(a: f32, b: f32) -> (f32, f32) {
    let f = |u: f32| (b / 3.0) * u * u * u - (b - 1.0) * u + a;
    let f_prime = |u: f32| b * u * u - (b - 1.0);
    let mut u = 0.0_f32;
    for _ in 0..64 {
        let fu = f(u);
        let dfu = f_prime(u);
        if dfu.abs() < 1e-9 {
            break;
        }
        let next = u - fu / dfu;
        if (next - u).abs() < 1e-7 {
            u = next;
            break;
        }
        u = next;
    }
    let v = (u + a) / b;
    (u, v)
}

/// Frame-zero seed (declared, mirroring [`seed_morphogenesis_field`]'s
/// shape): every cell starts at the model's resting state
/// ([`fhn_resting_state`]); carrier-luma-thresholded cells (plus the
/// standard splitmix64 speckle) get `u = u_rest + settings.stimulus` (`v`
/// untouched) — "fires u, not v."
pub fn seed_fhn_field(
    carrier_frame_zero: &ImageBufferF32,
    settings: &FhnSettings,
) -> Result<MorphogenesisField, RenderError> {
    settings.validate()?;
    let (width, height) = morphogenesis_field_dimensions(
        carrier_frame_zero.width,
        carrier_frame_zero.height,
        settings.sim_scale,
    );
    let count = (width as usize) * (height as usize);
    let (u_rest, v_rest) = fhn_resting_state(settings.a, settings.b);
    let mut u = vec![u_rest; count];
    let v = vec![v_rest; count];

    for y in 0..height {
        for x in 0..width {
            let luma = carrier_luma_at_sim_cell(carrier_frame_zero, settings.sim_scale, x, y)?;
            let luma_seeded = luma >= settings.seed_threshold;
            let speckle_seeded = seed_hash_unit(settings.seed, x, y) < SPECKLE_DENSITY;
            if luma_seeded || speckle_seeded {
                let idx = (y as usize) * (width as usize) + (x as usize);
                u[idx] = u_rest + settings.stimulus;
            }
        }
    }

    MorphogenesisField::new(width, height, u, v)
}

/// One FHN substep: a 5-point Laplacian on `u` only (clamped edges, same
/// stencil convention as [`morphogenesis_substep`]), `v` has no spatial
/// term. Both channels clamp to `[-FHN_SAFETY_BOX, FHN_SAFETY_BOX]`
/// afterward (a float-blowup guard, not part of the physics).
pub fn fhn_substep(
    field: &MorphogenesisField,
    settings: &FhnSettings,
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

            let du = settings.du * lap_u + u_c - (u_c * u_c * u_c) / 3.0 - v_c;
            let dv = settings.epsilon * (u_c + settings.a - settings.b * v_c);

            new_u[idx] = (u_c + settings.dt * du).clamp(-FHN_SAFETY_BOX, FHN_SAFETY_BOX);
            new_v[idx] = (v_c + settings.dt * dv).clamp(-FHN_SAFETY_BOX, FHN_SAFETY_BOX);
        }
    }

    MorphogenesisField::new(width, height, new_u, new_v)
}

/// Advance one output frame: `settings.substeps` FHN substeps. `substeps ==
/// 0` leaves the field unchanged.
pub fn advance_fhn_frame(
    field: &MorphogenesisField,
    settings: &FhnSettings,
) -> Result<MorphogenesisField, RenderError> {
    let mut current = field.clone();
    for _ in 0..settings.substeps {
        current = fhn_substep(&current, settings)?;
    }
    Ok(current)
}

/// Live coupling: `--inject` as a DISCRETE per-frame kick to `u`, applied
/// ONCE per output frame BEFORE the substeps (the same declared order as
/// Gray-Scott's inject — see `docs/MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md`),
/// rather than as a continuous forcing current inside the ODE.
///
/// This replaces an earlier design (a continuous current `I(x,y)` added
/// into the `du` equation every substep) that was empirically too weak to
/// ever be useful: with `dt ≈ 0.1` and a handful of substeps per frame, a
/// realistic `inject·w` current only nudged `u` by a few hundredths per
/// frame — nowhere near the `stimulus` (≈2.5) needed to cross the nullcline
/// threshold and fire a genuine travelling pulse, so real footage motion
/// could never launch a NEW wave (confirmed by rendering: `--inject 0.1
/// --inject-source motion` vs. no inject at all differed by a mean 0.694/255
/// after 143 frames — imperceptible). Scaling the kick by the model's own
/// `stimulus` (the same magnitude the frame-zero seed uses to reliably fire
/// a cell) is what makes `inject == 1.0` at full weight actually launch a
/// new pulse, matching Gray-Scott's `inject`/`erode` being a meaningfully-
/// sized perturbation relative to `V`'s own dynamic range.
pub fn apply_fhn_inject(
    field: &MorphogenesisField,
    settings: &FhnSettings,
    w: &[f32],
) -> Result<MorphogenesisField, RenderError> {
    if w.len() != field.u.len() {
        return Err(RenderError::InvalidMorphogenesisField(format!(
            "FHN inject weight field expected {} samples, got {}",
            field.u.len(),
            w.len()
        )));
    }
    let mut u = field.u.clone();
    for (value, &w) in u.iter_mut().zip(w) {
        *value = (*value + settings.inject * settings.stimulus * w)
            .clamp(-FHN_SAFETY_BOX, FHN_SAFETY_BOX);
    }
    MorphogenesisField::new(field.width, field.height, u, field.v.clone())
}

/// Display adapter (Track A1): the SAME two output-view functions
/// ([`composite_morphogenesis_frame`], [`render_v_field_grayscale_upsampled_with_shading`])
/// that already exist for Gray-Scott's `V` field work UNCHANGED for FHN by
/// feeding them a throwaway [`MorphogenesisField`] whose `.v` is a
/// display-normalized `u` (`((u.clamp(-2,2) + 2) / 4).clamp(0,1)`, so resting
/// `u ≈ -1.2` reads near-black and a firing pulse near-white) and whose `.u`
/// is unused (a dummy `1.0` — neither function reads it). Proves the
/// handoff's "everything downstream of the substep is model-agnostic" claim
/// rather than asserting it: zero changes needed to either function.
pub fn fhn_display_field(field: &MorphogenesisField) -> Result<MorphogenesisField, RenderError> {
    let display_v: Vec<f32> = field
        .u
        .iter()
        .map(|&u| ((u.clamp(-2.0, 2.0) + 2.0) / 4.0).clamp(0.0, 1.0))
        .collect();
    let dummy_u = vec![1.0_f32; field.u.len()];
    MorphogenesisField::new(field.width, field.height, dummy_u, display_v)
}

// ─── Track A2: Lenia continuous cellular automata ──────────────────────────
//
// `docs/MORPHOGENESIS_LENIA_MILESTONE.md`: the third field model, selected by
// `--model lenia`. A single scalar channel `A ∈ [0,1]` (stored in
// [`MorphogenesisField`]'s `.v` — already the exact display/composite
// contract `.v` has for Gray-Scott, so NO display adapter is needed, unlike
// FHN's signed `u`) evolves under a ring-kernel convolution + a bell-shaped
// growth mapping:
//
// ```text
// A(t+dt) = clamp01( A + dt * G( (K * A)(x) ) )
// K        = normalized gaussian-shell ring kernel, radius R
// G(u)     = 2*exp(-(u-mu)^2 / (2*sigma^2)) - 1
// ```
//
// `.u` is unused (dummy `1.0`, mirroring FHN's throwaway-channel convention)
// — gather-only, deterministic, direct O(W*H*R^2) convolution (an FFT/
// separable port is an optimization slice, not the MVP, per the handoff).

/// Algorithm identifier for the Lenia model — distinct from both
/// [`MORPHOGENESIS_ALGORITHM`] and [`MORPHOGENESIS_FHN_ALGORITHM`] so a
/// `--model` change on an existing output directory can never be mistaken
/// for a resumable checkpoint of either other model.
pub const MORPHOGENESIS_LENIA_ALGORITHM: &str = "morphogenesis_lenia_cpu_v1";

/// Lenia's frame-zero/speckle seeding stamps a filled disc (radius
/// `settings.radius`) at every seed site rather than a single pixel (unlike
/// Gray-Scott/FHN): a lone active pixel has essentially zero mass under the
/// normalized ring kernel's convolution, so `G(K*A) = G(~0)` is strongly
/// negative for every preset's `mu > 0` and the pixel dies before the next
/// substep — there is no reaction term to spread it, unlike reaction-
/// diffusion. A filled disc gives the kernel enough local density to read a
/// meaningful `K*A` at the seed site. Because a full disc is stamped at
/// every seeded site, the speckle density is deliberately much sparser than
/// Gray-Scott/FHN's shared 0.2% (each hit here covers a whole neighbourhood,
/// not one cell).
const LENIA_SPECKLE_DENSITY: f32 = 0.0004;

/// Lenia parameters + sim/seeding knobs, parallel in shape to
/// [`FhnSettings`]. `inject`/`erode` are plain additive/multiplicative `A`
/// perturbations (the SAME `[0,1]` meaning as Gray-Scott's, unlike FHN's
/// stimulus-scaled kick) since `A` is already a `[0,1]` density like `V`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LeniaSettings {
    /// Ring kernel radius, in sim cells.
    pub radius: u32,
    /// Growth mapping centre.
    pub mu: f32,
    /// Growth mapping width.
    pub sigma: f32,
    /// Per-substep integration step.
    pub dt: f32,
    /// Substeps run per output frame. `0` freezes the field (same anchor
    /// shape as Gray-Scott's A2 / FHN's).
    pub substeps: u32,
    /// Sim resolution divisor relative to the carrier frame.
    pub sim_scale: u32,
    /// Frame-zero seed threshold: carrier luma `>=` this stamps a disc.
    pub seed_threshold: f32,
    /// Deterministic seed for the frame-zero speckle.
    pub seed: u64,
    /// Live coupling: `A += inject * w(x,y)`, clamped `[0,1]` — the SAME
    /// equation as Gray-Scott's inject (see [`apply_lenia_inject_erode`]).
    /// `0` = off.
    pub inject: f32,
    /// Live coupling: `A *= (1 - erode * (1 - w))`, same `w` as `inject`.
    /// `0` = off.
    pub erode: f32,
    /// Which weight field `--inject`/`--erode` read (same [`InjectSource`]
    /// as the other two models).
    pub inject_source: InjectSource,
}

impl LeniaSettings {
    /// `orbium`: settles a carrier-luma-seeded disc into a stable, bounded
    /// "breathing membrane" blob — neither dying out nor unboundedly
    /// expanding (empirically probed: `mu=0.2, sigma=0.1` plateaus within
    /// ~1% mass drift over 400 frames on a 96x96 test grid; the literature
    /// orbium atlas `mu≈0.15, sigma≈0.017` is tuned for an exact hand-crafted
    /// glider photograph and collapses to zero within ~20 frames when seeded
    /// from a plain disc/bump instead — this app seeds from carrier luma, not
    /// an exact pattern, so the growth window needs to be far more forgiving;
    /// see `docs/MORPHOGENESIS_LENIA_MILESTONE.md`).
    pub fn orbium() -> Self {
        Self {
            radius: 13,
            mu: 0.2,
            sigma: 0.1,
            dt: 0.05,
            substeps: 2,
            sim_scale: 2,
            seed_threshold: 0.5,
            seed: 71,
            inject: 0.0,
            erode: 0.0,
            inject_source: InjectSource::Motion,
        }
    }

    /// `geminium`: a larger-radius ring/membrane regime (also empirically
    /// probed for a stable, non-collapsing equilibrium).
    pub fn geminium() -> Self {
        Self {
            radius: 18,
            mu: 0.24,
            sigma: 0.09,
            ..Self::orbium()
        }
    }

    /// `soup`: a much lower seed threshold so most of the carrier's bright
    /// structure stamps overlapping discs — dense, full-frame texture rather
    /// than isolated creatures (the classic Lenia "primordial soup" look).
    pub fn soup() -> Self {
        Self {
            seed_threshold: 0.15,
            ..Self::orbium()
        }
    }

    pub fn validate(&self) -> Result<(), RenderError> {
        if self.radius == 0 {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "radius must be >= 1".into(),
            ));
        }
        if !self.mu.is_finite() {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "mu must be finite".into(),
            ));
        }
        if !(self.sigma.is_finite() && self.sigma > 0.0) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "sigma must be finite and > 0".into(),
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
        if !(self.inject.is_finite() && (0.0..=1.0).contains(&self.inject)) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "inject must be finite and in [0, 1]".into(),
            ));
        }
        if !(self.erode.is_finite() && (0.0..=1.0).contains(&self.erode)) {
            return Err(RenderError::InvalidMorphogenesisSettings(
                "erode must be finite and in [0, 1]".into(),
            ));
        }
        Ok(())
    }
}

impl Default for LeniaSettings {
    fn default() -> Self {
        Self::orbium()
    }
}

/// Named Lenia presets, parallel to [`MorphogenesisPreset`]/[`FhnPreset`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeniaPreset {
    #[default]
    Orbium,
    Geminium,
    Soup,
}

impl LeniaPreset {
    pub fn settings(self) -> LeniaSettings {
        match self {
            Self::Orbium => LeniaSettings::orbium(),
            Self::Geminium => LeniaSettings::geminium(),
            Self::Soup => LeniaSettings::soup(),
        }
    }
}

/// One non-zero ring-kernel tap: `(dx, dy, weight)`, weights normalized to
/// sum to `1.0` over the whole kernel. A single gaussian shell centred at
/// `0.5 * radius` (width `0.15 * radius`) — the "one ring" Lenia kernel
/// family (`orbium`/`geminium` are both single-ring species).
fn lenia_kernel_taps(radius: u32) -> Vec<(i32, i32, f32)> {
    let r = radius as f32;
    let ir = radius as i32;
    let shell_center = 0.5;
    let shell_width = 0.15;
    let mut taps = Vec::new();
    let mut sum = 0.0_f32;
    for dy in -ir..=ir {
        for dx in -ir..=ir {
            let dist = ((dx * dx + dy * dy) as f32).sqrt();
            if dist > 0.0 && dist <= r {
                let rn = dist / r;
                let shell = (rn - shell_center) / shell_width;
                let weight = (-0.5 * shell * shell).exp();
                if weight > 1e-6 {
                    taps.push((dx, dy, weight));
                    sum += weight;
                }
            }
        }
    }
    if sum > 0.0 {
        for tap in taps.iter_mut() {
            tap.2 /= sum;
        }
    }
    taps
}

/// The bell-shaped growth mapping: `2*exp(-(u-mu)^2 / (2*sigma^2)) - 1`,
/// ranging over `[-1, 1]` — positive (growth) near `u == mu`, negative
/// (decay) away from it.
fn lenia_growth(u: f32, mu: f32, sigma: f32) -> f32 {
    let z = (u - mu) / sigma;
    2.0 * (-0.5 * z * z).exp() - 1.0
}

/// Stamp a filled disc of `1.0` into `v` (clamped edges — out-of-bounds taps
/// are simply skipped, not wrapped, matching the stencil convention used
/// elsewhere in this module) — see the module-level comment for why Lenia
/// seeds discs rather than single pixels.
fn stamp_disc(v: &mut [f32], width: u32, height: u32, cx: u32, cy: u32, radius: i32) {
    let w = width as usize;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let x = cx as i32 + dx;
            let y = cy as i32 + dy;
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            let idx = (y as usize) * w + (x as usize);
            v[idx] = 1.0;
        }
    }
}

/// Frame-zero seed: `A = 0` everywhere except a filled disc (radius
/// `settings.radius`) stamped at every carrier-luma-thresholded cell plus the
/// deterministic speckle ([`LENIA_SPECKLE_DENSITY`]). `.u` is unused (dummy
/// `1.0`).
pub fn seed_lenia_field(
    carrier_frame_zero: &ImageBufferF32,
    settings: &LeniaSettings,
) -> Result<MorphogenesisField, RenderError> {
    settings.validate()?;
    let (width, height) = morphogenesis_field_dimensions(
        carrier_frame_zero.width,
        carrier_frame_zero.height,
        settings.sim_scale,
    );
    let count = (width as usize) * (height as usize);
    let mut v = vec![0.0_f32; count];
    let u = vec![1.0_f32; count];
    let disc_radius = settings.radius as i32;

    for y in 0..height {
        for x in 0..width {
            let luma = carrier_luma_at_sim_cell(carrier_frame_zero, settings.sim_scale, x, y)?;
            let luma_seeded = luma >= settings.seed_threshold;
            let speckle_seeded = seed_hash_unit(settings.seed, x, y) < LENIA_SPECKLE_DENSITY;
            if luma_seeded || speckle_seeded {
                stamp_disc(&mut v, width, height, x, y, disc_radius);
            }
        }
    }

    MorphogenesisField::new(width, height, u, v)
}

/// One Lenia substep: `A` convolved with the normalized ring kernel
/// ([`lenia_kernel_taps`]), passed through the growth mapping
/// ([`lenia_growth`]), integrated with a forward-Euler step, clamped
/// `[0,1]`. Clamped (not toroidal) edges: out-of-frame kernel taps are
/// simply skipped rather than sampled, the same "footage has a frame"
/// declaration as the other two models' Laplacian stencils. Gather-only from
/// the previous buffer, so raster order never affects the result.
pub fn lenia_substep(
    field: &MorphogenesisField,
    settings: &LeniaSettings,
) -> Result<MorphogenesisField, RenderError> {
    let width = field.width;
    let height = field.height;
    let w = width as usize;
    let kernel = lenia_kernel_taps(settings.radius);
    let mut new_v = vec![0.0_f32; field.v.len()];

    for y in 0..height {
        for x in 0..width {
            let idx = (y as usize) * w + (x as usize);
            let mut acc = 0.0_f32;
            for &(dx, dy, weight) in &kernel {
                let sx = x as i32 + dx;
                let sy = y as i32 + dy;
                if sx < 0 || sy < 0 || sx >= width as i32 || sy >= height as i32 {
                    continue;
                }
                acc += weight * field.v[(sy as usize) * w + (sx as usize)];
            }
            let growth = lenia_growth(acc, settings.mu, settings.sigma);
            new_v[idx] = (field.v[idx] + settings.dt * growth).clamp(0.0, 1.0);
        }
    }

    MorphogenesisField::new(width, height, field.u.clone(), new_v)
}

/// Advance one output frame: `settings.substeps` Lenia substeps. `substeps
/// == 0` leaves the field unchanged.
pub fn advance_lenia_frame(
    field: &MorphogenesisField,
    settings: &LeniaSettings,
) -> Result<MorphogenesisField, RenderError> {
    let mut current = field.clone();
    for _ in 0..settings.substeps {
        current = lenia_substep(&current, settings)?;
    }
    Ok(current)
}

/// Spreads each nonzero `w` sample onto every cell within `radius` of it,
/// taking the max where discs overlap. Mirrors [`stamp_disc`]'s max-fill
/// shape but keeps the source's continuous weight instead of forcing `1.0`.
/// Needed by [`apply_lenia_inject_erode`]: [`InjectSource::Motion`]'s weight
/// field is a thin, edge-like mask (`|luma(N) - luma(N-1)|` is only nonzero
/// where footage actually moved, typically a handful of cells wide), and —
/// exactly like [`seed_lenia_field`]'s single-pixel-seed problem — a
/// pointwise injection into `A` has ~zero local mass under the ring kernel,
/// so `G(K*A)` reads far from `mu` and the injected value decays away over
/// the next few substeps instead of igniting a persisting structure.
fn dilate_weight_field(w: &[f32], width: u32, height: u32, radius: i32) -> Vec<f32> {
    let width_i = width as i32;
    let height_i = height as i32;
    let stride = width as usize;
    let mut out = vec![0.0_f32; w.len()];
    for y in 0..height_i {
        for x in 0..width_i {
            let value = w[(y as usize) * stride + (x as usize)];
            if value <= 0.0 {
                continue;
            }
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx * dx + dy * dy > radius * radius {
                        continue;
                    }
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= width_i || ny >= height_i {
                        continue;
                    }
                    let nidx = (ny as usize) * stride + (nx as usize);
                    if value > out[nidx] {
                        out[nidx] = value;
                    }
                }
            }
        }
    }
    out
}

/// Live coupling: unlike Gray-Scott's [`apply_inject_erode`] (a raw additive
/// `A += inject * w`), Lenia's `inject` LERPS each cell toward `settings.mu`
/// — `A = A*(1-m) + mu*m`, `m = clamp01(inject * w)` — before the erode
/// multiply. A raw additive push (tried first; see the "Track A2" section
/// comment) can land density well outside the growth window
/// (`mu ± ~3*sigma`): [`lenia_growth`] is strongly NEGATIVE out there, so a
/// strong/bright motion injection was paradoxically the fastest to decay —
/// confirmed on real footage (a bright motion edge visibly died faster than
/// a faint one). Lerping toward `mu` instead GUARANTEES every injection
/// lands exactly where the growth mapping is most positive, so it reliably
/// grows/spreads under the model's own dynamics afterward rather than
/// occasionally overshooting into decay. `w` is first dilated by
/// [`dilate_weight_field`] to a disc of `settings.radius` — the SAME "needs
/// blob mass, not point mass" requirement [`seed_lenia_field`] already
/// declares: a thin motion-edge weight field has no local width for the
/// ring kernel to read, lerp target or not.
pub fn apply_lenia_inject_erode(
    field: &MorphogenesisField,
    settings: &LeniaSettings,
    w: &[f32],
) -> Result<MorphogenesisField, RenderError> {
    if w.len() != field.v.len() {
        return Err(RenderError::InvalidMorphogenesisField(format!(
            "Lenia inject/erode weight field expected {} samples, got {}",
            field.v.len(),
            w.len()
        )));
    }
    let w = dilate_weight_field(w, field.width, field.height, settings.radius as i32);
    let mut v = field.v.clone();
    for (value, &w) in v.iter_mut().zip(&w) {
        let m = (settings.inject * w).clamp(0.0, 1.0);
        *value = (*value * (1.0 - m) + settings.mu * m).clamp(0.0, 1.0);
        *value = (*value * (1.0 - settings.erode * (1.0 - w))).clamp(0.0, 1.0);
    }
    MorphogenesisField::new(field.width, field.height, field.u.clone(), v)
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

    // ─── S2 composite tests ────────────────────────────────────────────────

    fn varied_carrier(width: u32, height: u32) -> ImageBufferF32 {
        // Distinct, partially-saturated per-pixel colour so tint/displace
        // effects are individually observable (never pure grey, never a
        // primary at full saturation).
        ImageBufferF32::from_fn(width, height, |x, y| {
            let fx = x as f32 / width.max(1) as f32;
            let fy = y as f32 / height.max(1) as f32;
            [0.2 + 0.5 * fx, 0.3 + 0.4 * fy, 0.6 - 0.3 * fx, 1.0]
        })
        .unwrap()
    }

    /// A field at the SAME resolution as the carrier (`sim_scale`-1
    /// equivalent) with a hand-picked `V` gradient, so composite math can be
    /// cross-checked pixel-for-pixel without any bilinear-upsample rounding.
    fn varied_field(width: u32, height: u32) -> MorphogenesisField {
        let count = (width as usize) * (height as usize);
        let u = vec![1.0_f32; count];
        let mut v = vec![0.0_f32; count];
        for y in 0..height {
            for x in 0..width {
                let idx = (y as usize) * (width as usize) + (x as usize);
                v[idx] = (x as f32 / width.max(1) as f32).clamp(0.0, 1.0);
            }
        }
        MorphogenesisField::new(width, height, u, v).unwrap()
    }

    #[test]
    fn anchor_a1_passthrough_composite_matches_carrier_regardless_of_field() {
        let carrier = varied_carrier(8, 6);
        let field = varied_field(8, 6);
        let settings = MorphogenesisCompositeSettings::passthrough();

        let output = composite_morphogenesis_frame(&carrier, &field, &settings).expect("composite");
        assert_eq!(
            output, carrier,
            "A1: pattern_mix=0 && displace=0 must reproduce the carrier byte-for-byte"
        );
    }

    #[test]
    fn pattern_mix_zero_leaves_the_displaced_sample_untinted() {
        // pattern_mix=0 alone (displace nonzero) must skip the colourize step
        // entirely: the output is exactly the displaced sample, matching
        // `sample_bilinear_clamped` directly, with no dependence on V.
        let carrier = varied_carrier(10, 8);
        let field = varied_field(10, 8);
        let settings = MorphogenesisCompositeSettings {
            pattern_mix: 0.0,
            displace: 3.0,
            pattern_hue: 0.5,
            pattern_color_mode: PatternColorMode::Hue,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let output = composite_morphogenesis_frame(&carrier, &field, &settings).expect("composite");

        let (gx, gy) = morphogenesis_v_gradient(&field);
        for y in 0..carrier.height {
            for x in 0..carrier.width {
                let grad_x = sample_scalar_grid(
                    field.width,
                    field.height,
                    &gx,
                    carrier.width,
                    carrier.height,
                    x as f32,
                    y as f32,
                );
                let grad_y = sample_scalar_grid(
                    field.width,
                    field.height,
                    &gy,
                    carrier.width,
                    carrier.height,
                    x as f32,
                    y as f32,
                );
                let expected = sample_bilinear_clamped(
                    &carrier,
                    x as f32 - settings.displace * grad_x,
                    y as f32 - settings.displace * grad_y,
                );
                assert_eq!(output.pixel(x, y).unwrap(), expected, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn displace_zero_samples_the_carriers_own_pixel_under_full_pattern_mix() {
        // displace=0 must sample the carrier at its own integer coordinate —
        // proven under pattern_mix=1 (not just the A1 all-off case) by
        // reconstructing the expected tint from the UNDISPLACED source pixel.
        let carrier = varied_carrier(6, 5);
        let field = varied_field(6, 5);
        let settings = MorphogenesisCompositeSettings {
            pattern_mix: 1.0,
            displace: 0.0,
            pattern_hue: 0.35,
            pattern_color_mode: PatternColorMode::Hue,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let output = composite_morphogenesis_frame(&carrier, &field, &settings).expect("composite");

        for y in 0..carrier.height {
            for x in 0..carrier.width {
                let idx = (y as usize) * (carrier.width as usize) + (x as usize);
                let source = carrier.pixel(x, y).unwrap();
                let v = field.v[idx];
                let target = colorize_luma_preserving(source, settings.pattern_hue);
                let strength = (settings.pattern_mix * v).clamp(0.0, 1.0);
                let expected = [
                    source[0] + (target[0] - source[0]) * strength,
                    source[1] + (target[1] - source[1]) * strength,
                    source[2] + (target[2] - source[2]) * strength,
                    source[3],
                ];
                let actual = output.pixel(x, y).unwrap();
                for channel in 0..4 {
                    assert!(
                        (actual[channel] - expected[channel]).abs() < 1e-5,
                        "pixel ({x},{y}) channel {channel}: {actual:?} vs {expected:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn pattern_mix_one_at_full_v_preserves_luma_and_shifts_toward_the_target_hue() {
        let carrier = ImageBufferF32::from_fn(2, 2, |_, _| [0.6, 0.3, 0.2, 1.0]).unwrap();
        // Uniform V=1 field so every pixel gets the full-strength tint.
        let field = MorphogenesisField::new(2, 2, vec![1.0; 4], vec![1.0; 4]).unwrap();
        let settings = MorphogenesisCompositeSettings {
            pattern_mix: 1.0,
            displace: 0.0,
            pattern_hue: 0.55,
            pattern_color_mode: PatternColorMode::Hue,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let output = composite_morphogenesis_frame(&carrier, &field, &settings).expect("composite");

        let source_luma = luma3(0.6, 0.3, 0.2);
        for pixel in &output.pixels {
            let out_luma = luma3(pixel[0], pixel[1], pixel[2]);
            assert!(
                (out_luma - source_luma).abs() < 1e-4,
                "tint must preserve luma: source={source_luma} output={out_luma}"
            );
            let (h, _s, _v) = rgb_to_hsv(pixel[0], pixel[1], pixel[2]);
            assert!(
                (h - settings.pattern_hue).abs() < 1e-3,
                "tint must shift hue toward pattern_hue: got {h}, wanted {}",
                settings.pattern_hue
            );
        }
    }

    #[test]
    fn inherit_mode_differs_from_hue_mode_on_a_partially_saturated_carrier() {
        let carrier = ImageBufferF32::from_fn(2, 2, |_, _| [0.7, 0.4, 0.3, 1.0]).unwrap();
        let field = MorphogenesisField::new(2, 2, vec![1.0; 4], vec![1.0; 4]).unwrap();
        let hue_settings = MorphogenesisCompositeSettings {
            pattern_mix: 1.0,
            displace: 0.0,
            pattern_hue: 0.55, // far from the carrier's own (reddish) hue
            pattern_color_mode: PatternColorMode::Hue,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let inherit_settings = MorphogenesisCompositeSettings {
            pattern_color_mode: PatternColorMode::Inherit,
            ..hue_settings
        };

        let hue_out = composite_morphogenesis_frame(&carrier, &field, &hue_settings).expect("hue");
        let inherit_out =
            composite_morphogenesis_frame(&carrier, &field, &inherit_settings).expect("inherit");

        assert_ne!(
            hue_out.pixels, inherit_out.pixels,
            "hue mode must tint toward pattern_hue while inherit tints toward the local hue"
        );

        // Inherit's target hue is the carrier's OWN hue, so its luma-preserved
        // tint should reproduce the carrier's own hue (fully saturated) —
        // verify against colorize_luma_preserving with the extracted local hue.
        let (local_hue, _s, _v) = rgb_to_hsv(0.7, 0.4, 0.3);
        let expected = colorize_luma_preserving([0.7, 0.4, 0.3, 1.0], local_hue);
        for pixel in &inherit_out.pixels {
            for channel in 0..4 {
                assert!(
                    (pixel[channel] - expected[channel]).abs() < 1e-5,
                    "inherit tint must match the carrier's own hue: {pixel:?} vs {expected:?}"
                );
            }
        }
    }

    #[test]
    fn composite_is_deterministic_across_repeated_calls() {
        let carrier = varied_carrier(12, 9);
        let field = varied_field(12, 9);
        let settings = MorphogenesisCompositeSettings {
            pattern_mix: 0.6,
            displace: 4.0,
            pattern_hue: 0.1,
            pattern_color_mode: PatternColorMode::Inherit,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let a = composite_morphogenesis_frame(&carrier, &field, &settings).expect("first");
        let b = composite_morphogenesis_frame(&carrier, &field, &settings).expect("second");
        assert_eq!(a, b, "composite must be a pure, deterministic function");
    }

    #[test]
    fn composite_settings_validate_rejects_out_of_range_pattern_mix() {
        let settings = MorphogenesisCompositeSettings {
            pattern_mix: 1.5,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        assert!(settings.validate().is_err());
    }

    // ─── Track B1: relief shading tests ────────────────────────────────────

    #[test]
    fn anchor_rs1_shade_zero_is_byte_identical_regardless_of_other_shade_knobs() {
        let carrier = varied_carrier(10, 8);
        let field = varied_field(10, 8);
        let base = MorphogenesisCompositeSettings {
            pattern_mix: 0.9,
            displace: 2.0,
            pattern_hue: 0.4,
            pattern_color_mode: PatternColorMode::Hue,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        // shade == 0 must reproduce the pre-slice output even with the OTHER
        // shade knobs set to unusual (nonzero) values — the blend strength,
        // not the sub-knobs, is what gates the effect.
        let shade_off_but_dialed_in = MorphogenesisCompositeSettings {
            shade: 0.0,
            shade_height: 40.0,
            shade_azimuth: 0.33,
            shade_elevation: 0.2,
            shade_specular: 0.9,
            shade_shininess: 64.0,
            ..base
        };
        let plain = composite_morphogenesis_frame(&carrier, &field, &base).expect("plain");
        let dialed = composite_morphogenesis_frame(&carrier, &field, &shade_off_but_dialed_in)
            .expect("dialed");
        assert_eq!(
            plain, dialed,
            "RS1: shade == 0 must be byte-identical regardless of the other shade knobs"
        );
    }

    #[test]
    fn anchor_rs2_180_degree_azimuth_flip_mirrors_a_flipped_gradient() {
        let composite_az0 = MorphogenesisCompositeSettings {
            shade: 1.0,
            shade_height: 1.0,
            shade_azimuth: 0.0,
            shade_elevation: 0.15,
            shade_specular: 0.0,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let composite_az_flip = MorphogenesisCompositeSettings {
            shade_azimuth: 0.5,
            ..composite_az0
        };

        let lit_gx_positive = morphogenesis_shading_value(1.0, 0.0, &composite_az0);
        let lit_gx_negative = morphogenesis_shading_value(-1.0, 0.0, &composite_az0);
        let lit_flipped_light = morphogenesis_shading_value(1.0, 0.0, &composite_az_flip);

        assert!(
            (lit_gx_negative - lit_flipped_light).abs() < 1e-5,
            "flipping the gradient sign must match flipping the light 180 degrees instead: \
             {lit_gx_negative} vs {lit_flipped_light}"
        );
        assert!(
            (lit_gx_positive - lit_flipped_light).abs() > 1e-3,
            "the azimuth flip must actually change the lit value (not a degenerate no-op)"
        );
    }

    #[test]
    fn shade_makes_growth_visible_on_a_black_carrier_where_luma_preserving_tint_cannot() {
        // The exact mechanism behind RS3 (the dark-footage gap): a
        // luma-preserving tint of BLACK stays black regardless of pattern_mix
        // or hue, but a fully shade-blended target derives its brightness
        // from the light, not the carrier.
        let carrier = solid_carrier(6, 6, 0.0);
        let field = structured_carrier(6, 6);
        let field = seed_morphogenesis_field(
            &field,
            &MorphogenesisSettings {
                sim_scale: 1,
                ..MorphogenesisSettings::coral()
            },
        )
        .expect("seed");

        let unshaded = MorphogenesisCompositeSettings {
            pattern_mix: 1.0,
            displace: 0.0,
            pattern_hue: 0.5,
            pattern_color_mode: PatternColorMode::Hue,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let shaded = MorphogenesisCompositeSettings {
            shade: 1.0,
            shade_height: 5.0,
            shade_specular: 0.5,
            ..unshaded
        };

        let unshaded_out =
            composite_morphogenesis_frame(&carrier, &field, &unshaded).expect("unshaded");
        let shaded_out = composite_morphogenesis_frame(&carrier, &field, &shaded).expect("shaded");

        assert!(
            unshaded_out
                .pixels
                .iter()
                .all(|p| p == &[0.0, 0.0, 0.0, 1.0]),
            "luma-preserving tint of a black carrier must stay black (the known bug)"
        );
        assert!(
            shaded_out
                .pixels
                .iter()
                .any(|p| p[0] > 1e-3 || p[1] > 1e-3 || p[2] > 1e-3),
            "shade == 1 must show visible relief structure on a black carrier"
        );
    }

    // ─── S3 param-map tests ────────────────────────────────────────────────

    #[test]
    fn local_feed_kill_midpoint_and_endpoints_match_the_declared_segment() {
        let settings = MorphogenesisSettings::coral();
        // The segment's midpoint (luma == 0.5) is settings' own (feed, kill)
        // at ANY strength, including strength 0.
        let (mid_feed, mid_kill) = local_feed_kill(&settings, 2.0, 0.5);
        assert_eq!(mid_feed, settings.feed);
        assert_eq!(mid_kill, settings.kill);
        let (mid_feed_zero_strength, mid_kill_zero_strength) = local_feed_kill(&settings, 0.0, 1.0);
        assert_eq!(mid_feed_zero_strength, settings.feed);
        assert_eq!(mid_kill_zero_strength, settings.kill);

        // The bright (luma=1) and dark (luma=0) endpoints at strength=1 sit
        // exactly half the declared delta away from the midpoint, in
        // opposite directions.
        let (bright_feed, bright_kill) = local_feed_kill(&settings, 1.0, 1.0);
        let (dark_feed, dark_kill) = local_feed_kill(&settings, 1.0, 0.0);
        assert!((bright_feed - (settings.feed + 0.5 * PARAM_MAP_SEGMENT_DELTA_FEED)).abs() < 1e-6);
        assert!((bright_kill - (settings.kill + 0.5 * PARAM_MAP_SEGMENT_DELTA_KILL)).abs() < 1e-6);
        assert!((dark_feed - (settings.feed - 0.5 * PARAM_MAP_SEGMENT_DELTA_FEED)).abs() < 1e-6);
        assert!((dark_kill - (settings.kill - 0.5 * PARAM_MAP_SEGMENT_DELTA_KILL)).abs() < 1e-6);
        assert_ne!(
            bright_feed, dark_feed,
            "bright and dark endpoints must differ"
        );
    }

    #[test]
    fn param_map_strength_zero_is_byte_identical_to_the_uniform_sim() {
        let carrier = structured_carrier(20, 20);
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            ..MorphogenesisSettings::coral()
        };
        let seed = seed_morphogenesis_field(&carrier, &settings).expect("seed");
        let cell_luma = sample_carrier_luma_at_sim_resolution(
            &carrier,
            seed.width,
            seed.height,
            settings.sim_scale,
        )
        .expect("sample luma");

        let mut uniform = seed.clone();
        let mut param_mapped = seed;
        for _ in 0..8 {
            uniform = advance_morphogenesis_frame(&uniform, &settings).expect("uniform advance");
            // strength == 0.0 must reproduce the uniform sim exactly, even
            // with a non-degenerate (non-constant) luma map in hand.
            param_mapped = advance_morphogenesis_frame_with_param_map(
                &param_mapped,
                &settings,
                0.0,
                &cell_luma,
            )
            .expect("param-map advance at strength 0");
        }
        assert_eq!(
            param_mapped, uniform,
            "param_map_strength == 0 must be byte-identical to the uniform-(feed,kill) sim"
        );
    }

    #[test]
    fn param_map_advance_is_deterministic_and_diverges_from_uniform_when_active() {
        let carrier = structured_carrier(20, 20);
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            ..MorphogenesisSettings::coral()
        };
        let seed = seed_morphogenesis_field(&carrier, &settings).expect("seed");
        let cell_luma = sample_carrier_luma_at_sim_resolution(
            &carrier,
            seed.width,
            seed.height,
            settings.sim_scale,
        )
        .expect("sample luma");

        let run = || {
            let mut field = seed.clone();
            for _ in 0..8 {
                field = advance_morphogenesis_frame_with_param_map(
                    &field,
                    &settings,
                    PARAM_MAP_STRENGTH_DEFAULT,
                    &cell_luma,
                )
                .expect("param-map advance");
            }
            field
        };
        assert_eq!(run(), run(), "param-map advance must be deterministic");

        let mut uniform = seed.clone();
        for _ in 0..8 {
            uniform = advance_morphogenesis_frame(&uniform, &settings).expect("uniform advance");
        }
        assert_ne!(
            run().v,
            uniform.v,
            "an active param map over a non-uniform carrier must diverge from the uniform sim"
        );
    }

    #[test]
    fn param_map_rejects_a_mismatched_luma_sample_count() {
        let field = MorphogenesisField::new(4, 4, vec![1.0; 16], vec![0.0; 16]).unwrap();
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            ..MorphogenesisSettings::coral()
        };
        let wrong_size_luma = vec![0.5_f32; 4];
        assert!(
            morphogenesis_substep_with_param_map(&field, &settings, 1.0, &wrong_size_luma).is_err()
        );
    }

    // ─── Live Coupling L-S1: inject/erode ─────────────────────────────────

    #[test]
    fn settings_validate_rejects_out_of_range_inject() {
        let settings = MorphogenesisSettings {
            inject: 1.5,
            ..MorphogenesisSettings::coral()
        };
        assert!(settings.validate().is_err());
        let settings = MorphogenesisSettings {
            inject: -0.1,
            ..MorphogenesisSettings::coral()
        };
        assert!(settings.validate().is_err());
        let settings = MorphogenesisSettings {
            inject: f32::NAN,
            ..MorphogenesisSettings::coral()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn settings_validate_rejects_out_of_range_erode() {
        let settings = MorphogenesisSettings {
            erode: 1.5,
            ..MorphogenesisSettings::coral()
        };
        assert!(settings.validate().is_err());
        let settings = MorphogenesisSettings {
            erode: -0.1,
            ..MorphogenesisSettings::coral()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn settings_validate_rejects_out_of_range_coverage_target() {
        let settings = MorphogenesisSettings {
            coverage_target: 1.5,
            ..MorphogenesisSettings::coral()
        };
        assert!(settings.validate().is_err());
        let settings = MorphogenesisSettings {
            coverage_target: -0.1,
            ..MorphogenesisSettings::coral()
        };
        assert!(settings.validate().is_err());
        let settings = MorphogenesisSettings {
            coverage_target: f32::NAN,
            ..MorphogenesisSettings::coral()
        };
        assert!(settings.validate().is_err());
    }

    #[test]
    fn injection_weight_luma_matches_declared_formula() {
        let luma = vec![0.0, 0.5, 0.75, 1.0];
        let w = injection_weight_luma(&luma, 0.5);
        // threshold itself -> 0; full-bright -> 1; below threshold clamps to 0.
        assert_eq!(w, vec![0.0, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn injection_weight_luma_handles_degenerate_threshold_without_nan() {
        let luma = vec![1.0, 0.9, 0.0];
        let w = injection_weight_luma(&luma, 1.0);
        assert!(w.iter().all(|value| value.is_finite()), "no NaN: {w:?}");
        assert_eq!(w, vec![0.0, 0.0, 0.0]);
    }

    #[test]
    fn injection_weight_motion_frame_zero_is_all_zero() {
        let current = vec![0.1, 0.9, 0.5, 1.0];
        let w = injection_weight_motion(&current, None);
        assert_eq!(w, vec![0.0; current.len()]);
    }

    #[test]
    fn injection_weight_motion_tracks_luma_difference() {
        let previous = vec![0.2, 0.2, 0.9];
        let current = vec![0.2, 0.7, 0.1];
        let w = injection_weight_motion(&current, Some(&previous));
        let expected = [0.0, 0.5, 0.8];
        for (actual, expected) in w.iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 1e-5,
                "w={w:?} expected~{expected:?}"
            );
        }
    }

    #[test]
    fn apply_inject_erode_is_identity_at_zero_knobs() {
        let field = MorphogenesisField::new(
            3,
            3,
            vec![1.0; 9],
            vec![0.2, 0.4, 0.6, 0.8, 0.1, 0.9, 0.3, 0.5, 0.7],
        )
        .unwrap();
        let settings = MorphogenesisSettings {
            inject: 0.0,
            erode: 0.0,
            ..MorphogenesisSettings::coral()
        };
        // Even a non-trivial weight field must leave V untouched at 0/0.
        let w = vec![1.0, 0.0, 0.5, 1.0, 0.0, 0.3, 0.7, 0.9, 0.2];
        let result = apply_inject_erode(&field, &settings, &w).expect("apply");
        assert_eq!(
            result.v, field.v,
            "L1: zero inject/erode is an identity on V"
        );
        assert_eq!(result.u, field.u, "U must pass through unchanged");
    }

    #[test]
    fn apply_inject_erode_matches_declared_formula() {
        let field = MorphogenesisField::new(2, 2, vec![1.0; 4], vec![0.2, 0.5, 0.0, 0.9]).unwrap();
        let settings = MorphogenesisSettings {
            inject: 0.3,
            erode: 0.4,
            ..MorphogenesisSettings::coral()
        };
        let w = vec![0.0, 1.0, 0.5, 0.2];
        let result = apply_inject_erode(&field, &settings, &w).expect("apply");

        let expected: Vec<f32> = field
            .v
            .iter()
            .zip(&w)
            .map(|(&v, &w)| {
                let injected = (v + settings.inject * w).clamp(0.0, 1.0);
                (injected * (1.0 - settings.erode * (1.0 - w))).clamp(0.0, 1.0)
            })
            .collect();
        assert_eq!(result.v, expected);
    }

    #[test]
    fn apply_inject_erode_rejects_mismatched_weight_length() {
        let field = MorphogenesisField::new(2, 2, vec![1.0; 4], vec![0.0; 4]).unwrap();
        let settings = MorphogenesisSettings {
            inject: 0.5,
            ..MorphogenesisSettings::coral()
        };
        let wrong_size_w = vec![0.5_f32; 3];
        assert!(apply_inject_erode(&field, &settings, &wrong_size_w).is_err());
    }

    /// Anchor L2: a bright bar moving one sim-column per frame, with
    /// motion-sourced inject+erode active and substeps=0 (isolating the
    /// inject/erode passes from Gray-Scott's own diffusion so the moving
    /// mass is attributable ONLY to the live-coupling passes). The `V`
    /// column-center-of-mass in the LATE window must track the bar's CURRENT
    /// column, not its frame-0 column.
    #[test]
    fn anchor_l2_motion_tracking_center_of_mass_follows_the_moving_bar() {
        let width = 20u32;
        let height = 6u32;
        let bar_width = 2u32;

        // Per-frame luma grids (sim resolution, sim_scale=1): a bright bar
        // sweeping left-to-right, dark elsewhere.
        let luma_frame = |bar_x: u32| -> Vec<f32> {
            let mut luma = vec![0.0_f32; (width * height) as usize];
            for y in 0..height {
                for x in bar_x..(bar_x + bar_width).min(width) {
                    luma[(y * width + x) as usize] = 1.0;
                }
            }
            luma
        };

        let settings = MorphogenesisSettings {
            sim_scale: 1,
            substeps: 0, // isolate inject/erode from Gray-Scott diffusion
            inject: 0.8,
            erode: 0.6,
            inject_source: InjectSource::Motion,
            ..MorphogenesisSettings::coral()
        };

        // Seed from an all-dark frame zero (bar starts off-canvas at x=0,
        // width 0 contribution) so the seed itself contributes ~no V mass.
        let carrier_zero =
            ImageBufferF32::from_fn(width, height, |_, _| [0.0, 0.0, 0.0, 1.0]).unwrap();
        let mut field = seed_morphogenesis_field(&carrier_zero, &settings).expect("seed");

        let bar_positions: Vec<u32> = (0..width.saturating_sub(bar_width)).step_by(2).collect();
        let mut previous_luma: Option<Vec<f32>> = None;
        let mut last_bar_x = 0u32;

        for (index, &bar_x) in bar_positions.iter().enumerate() {
            let current_luma = luma_frame(bar_x);
            if index > 0 {
                let w = injection_weight_motion(&current_luma, previous_luma.as_deref());
                field = apply_inject_erode(&field, &settings, &w).expect("apply");
                field = advance_morphogenesis_frame(&field, &settings).expect("advance");
            }
            previous_luma = Some(current_luma);
            last_bar_x = bar_x;
        }

        // Column center-of-mass of the final V field.
        let mut weighted_sum = 0.0_f64;
        let mut total = 0.0_f64;
        for y in 0..height {
            for x in 0..width {
                let v = field.v[(y * width + x) as usize] as f64;
                weighted_sum += v * x as f64;
                total += v;
            }
        }
        assert!(total > 0.0, "the field must have accumulated some V mass");
        let center_of_mass = weighted_sum / total;

        let current_bar_center = last_bar_x as f64 + (bar_width as f64 - 1.0) / 2.0;
        let frame_zero_center = 0.0; // the (empty) frame-0 seed's bar position

        assert!(
            (center_of_mass - current_bar_center).abs() < 3.0,
            "L2: V center-of-mass ({center_of_mass}) must track the bar's CURRENT column ({current_bar_center})"
        );
        assert!(
            (center_of_mass - frame_zero_center).abs() > 3.0,
            "L2: V center-of-mass ({center_of_mass}) must NOT still sit at frame-0's column ({frame_zero_center})"
        );
    }

    // ─── Live Coupling L-S2: coverage-target homeostat ────────────────────

    #[test]
    fn apply_coverage_homeostat_is_identity_when_target_is_off() {
        let field = MorphogenesisField::new(2, 2, vec![1.0; 4], vec![0.9, 0.9, 0.9, 0.9]).unwrap();
        let settings = MorphogenesisSettings {
            coverage_target: 0.0,
            ..MorphogenesisSettings::coral()
        };
        let shifted = apply_coverage_homeostat(&settings, &field);
        assert_eq!(
            shifted, settings,
            "coverage_target == 0 must leave (feed, kill) untouched"
        );
    }

    #[test]
    fn apply_coverage_homeostat_shifts_toward_dissolution_when_above_target() {
        // mean(V) = 0.35, modestly above a 0.3 target (small enough that the
        // shift doesn't hit the feed >= 0 floor): feed must drop, kill must
        // rise, both by exactly COVERAGE_GAIN * (0.35 - 0.3).
        let field = MorphogenesisField::new(2, 2, vec![1.0; 4], vec![0.35; 4]).unwrap();
        let settings = MorphogenesisSettings {
            coverage_target: 0.3,
            ..MorphogenesisSettings::coral()
        };
        let shifted = apply_coverage_homeostat(&settings, &field);
        let expected_shift = COVERAGE_GAIN * (0.35 - 0.3);
        assert!((shifted.feed - (settings.feed - expected_shift)).abs() < 1e-6);
        assert!((shifted.kill - (settings.kill + expected_shift)).abs() < 1e-6);
        assert!(shifted.feed < settings.feed, "feed must drop (dissolution)");
        assert!(shifted.kill > settings.kill, "kill must rise (dissolution)");
    }

    #[test]
    fn apply_coverage_homeostat_shifts_toward_growth_when_below_target() {
        // mean(V) = 0.1, well below a 0.5 target: feed must rise, kill must
        // drop (floored at 0).
        let field = MorphogenesisField::new(2, 2, vec![1.0; 4], vec![0.1; 4]).unwrap();
        let settings = MorphogenesisSettings {
            coverage_target: 0.5,
            ..MorphogenesisSettings::coral()
        };
        let shifted = apply_coverage_homeostat(&settings, &field);
        assert!(shifted.feed > settings.feed, "feed must rise (growth)");
        assert!(shifted.kill < settings.kill, "kill must drop (growth)");
        assert!(shifted.kill >= 0.0, "kill must never go negative");
    }

    #[test]
    fn apply_coverage_homeostat_floors_kill_and_feed_at_zero() {
        // A pathologically large error must not drive feed/kill negative.
        let field = MorphogenesisField::new(2, 2, vec![1.0; 4], vec![1.0; 4]).unwrap();
        let settings = MorphogenesisSettings {
            coverage_target: 0.001,
            feed: 0.001,
            ..MorphogenesisSettings::coral()
        };
        let shifted = apply_coverage_homeostat(&settings, &field);
        assert!(shifted.feed >= 0.0);
        assert!(shifted.kill >= 0.0);
    }

    /// Anchor L4: with `coverage_target = 0.3` and pure-luma injection (no
    /// erode — the harder case, since nothing else opposes saturation) on a
    /// bright carrier, mean(V) must settle within ±0.1 of the target over the
    /// render's last 48 of 144 frames — vs saturating toward the injection
    /// ceiling without the homeostat (asserted here too, as the falsifiable
    /// contrast).
    #[test]
    fn anchor_l4_homeostat_settles_mean_v_within_band_of_coverage_target() {
        let width = 32u32;
        let height = 32u32;
        // A bright-but-textured carrier (ring on a dark field), NOT a flat
        // solid colour: a perfectly uniform carrier removes Gray-Scott's own
        // spatial diffusion (the Laplacian is exactly zero everywhere),
        // degenerating the sim into a bare 0-D reaction ODE that can ring/
        // oscillate independent of the homeostat. Real footage always has
        // spatial texture, so this is the representative "bright carrier".
        let carrier = structured_carrier(width, height);
        let total_frames = 144;
        let coverage_target = 0.3;

        let run = |coverage_target: f32| -> Vec<f32> {
            let settings = MorphogenesisSettings {
                sim_scale: 1,
                inject: 0.1,
                erode: 0.0,
                inject_source: InjectSource::Luma,
                seed_threshold: 0.5,
                coverage_target,
                ..MorphogenesisSettings::coral()
            };
            let mut field = seed_morphogenesis_field(&carrier, &settings).expect("seed");
            let carrier_luma = sample_carrier_luma_at_sim_resolution(
                &carrier,
                field.width,
                field.height,
                settings.sim_scale,
            )
            .expect("sample luma");
            let w = injection_weight_luma(&carrier_luma, settings.seed_threshold);
            let mut means = Vec::with_capacity(total_frames);
            for index in 0..total_frames {
                if index > 0 {
                    field = apply_inject_erode(&field, &settings, &w).expect("apply");
                    let advance_settings = apply_coverage_homeostat(&settings, &field);
                    field =
                        advance_morphogenesis_frame(&field, &advance_settings).expect("advance");
                }
                let mean_v = field.v.iter().sum::<f32>() / field.v.len() as f32;
                means.push(mean_v);
            }
            means
        };

        let homeostat_means = run(coverage_target);
        let tail = &homeostat_means[homeostat_means.len() - 48..];
        for &mean_v in tail {
            assert!(
                (mean_v - coverage_target).abs() <= 0.1,
                "L4: mean(V)={mean_v} must settle within +/-0.1 of target {coverage_target} (full run: {homeostat_means:?})"
            );
        }

        // Falsifiable contrast: WITHOUT the homeostat (coverage_target = 0),
        // pure-luma injection with no sink saturates toward the injection
        // ceiling well outside that band.
        let unregulated_means = run(0.0);
        let unregulated_final = *unregulated_means.last().unwrap();
        assert!(
            (unregulated_final - coverage_target).abs() > 0.1,
            "contrast: without the homeostat, mean(V)={unregulated_final} should NOT already sit near the target"
        );
    }

    /// Field View milestone unit pin (FV2, sim_scale == 1 half): at identity
    /// scale, [`render_v_field_grayscale_upsampled`] must sample every pixel
    /// at zero interpolation weight and reproduce
    /// [`render_v_field_grayscale`] exactly — the shared-renderer proof at
    /// the algorithm level (the CLI-level half lives in
    /// `morphogen-cli`'s smoke test comparing the two commands' PNGs).
    #[test]
    fn render_v_field_grayscale_upsampled_matches_debug_view_at_identity_scale() {
        let carrier = structured_carrier(20, 16);
        let settings = MorphogenesisSettings {
            sim_scale: 1,
            ..MorphogenesisSettings::coral()
        };
        let mut field = seed_morphogenesis_field(&carrier, &settings).expect("seed");
        field = advance_morphogenesis_frame(&field, &settings).expect("advance");

        let debug = render_v_field_grayscale(&field).expect("debug view");
        let upsampled = render_v_field_grayscale_upsampled(&field, field.width, field.height)
            .expect("upsampled view");
        assert_eq!(
            debug.pixels, upsampled.pixels,
            "identity-scale upsample must be byte-identical to the raw debug mapping"
        );
    }

    /// At a non-identity scale the upsample legitimately differs (bilinear
    /// interpolation vs raw sim-res dump) but must still stay within `[0,1]`
    /// and match the field's own extremes closely at the corners (a sanity
    /// check, not a byte pin — the declared FV2 divergence above sim_scale 1).
    #[test]
    fn render_v_field_grayscale_upsampled_scales_to_the_requested_carrier_resolution() {
        let carrier = structured_carrier(20, 16);
        let settings = MorphogenesisSettings {
            sim_scale: 4,
            ..MorphogenesisSettings::coral()
        };
        let field = seed_morphogenesis_field(&carrier, &settings).expect("seed");
        let upsampled = render_v_field_grayscale_upsampled(&field, 20, 16).expect("upsampled");
        assert_eq!(upsampled.width, 20);
        assert_eq!(upsampled.height, 16);
    }

    // ─── Track A1: FitzHugh-Nagumo ─────────────────────────────────────────

    /// FHN1: the Newton solver's fixed point satisfies both nullcline
    /// equations (`v = u - u^3/3` and `v = (u+a)/b`) to within 1e-4, for
    /// every shipped preset's `(a, b)`.
    #[test]
    fn fhn1_resting_state_satisfies_both_nullclines_for_every_preset() {
        for preset in [FhnPreset::Pulse, FhnPreset::Spiral, FhnPreset::Labyrinth] {
            let settings = preset.settings();
            let (u, v) = fhn_resting_state(settings.a, settings.b);
            let u_nullcline_v = u - u * u * u / 3.0;
            let v_nullcline_v = (u + settings.a) / settings.b;
            assert!(
                (v - u_nullcline_v).abs() < 1e-4,
                "{preset:?}: resting v={v} must satisfy the u-nullcline (expected {u_nullcline_v})"
            );
            assert!(
                (v - v_nullcline_v).abs() < 1e-4,
                "{preset:?}: resting v={v} must satisfy the v-nullcline (expected {v_nullcline_v})"
            );
        }
    }

    /// FHN0 continuity companion: `--model gray-scott` (i.e. simply not
    /// touching the FHN path at all) leaves every existing Gray-Scott anchor
    /// untouched — already proven by every test above staying green; this
    /// test additionally pins that the two algorithm ids are distinct so a
    /// model switch can never be mistaken for a resumable checkpoint of the
    /// other model.
    #[test]
    fn fhn0_algorithm_ids_are_distinct() {
        assert_ne!(MORPHOGENESIS_ALGORITHM, MORPHOGENESIS_FHN_ALGORITHM);
    }

    /// FHN2 (quiescence): a field built with EVERY cell at the resting state
    /// (no seed, no speckle — [`seed_fhn_field`]'s speckle is a real
    /// stimulus that can itself fire in an excitable medium, so it's
    /// deliberately bypassed here to isolate the claim under test) must stay
    /// at rest — proving [`fhn_resting_state`] is actually a fixed point of
    /// the DISCRETIZED system (dt, substeps, the safety clamp), not just the
    /// continuous ODE.
    #[test]
    fn fhn2_unstimulated_field_stays_quiescent() {
        for preset in [FhnPreset::Pulse, FhnPreset::Spiral, FhnPreset::Labyrinth] {
            let settings = FhnSettings {
                sim_scale: 1,
                ..preset.settings()
            };
            let (u_rest, v_rest) = fhn_resting_state(settings.a, settings.b);
            let count = 24 * 24;
            let mut field =
                MorphogenesisField::new(24, 24, vec![u_rest; count], vec![v_rest; count])
                    .expect("build resting field");
            let initial_variance = field.v_variance();

            let mut max_variance = initial_variance;
            for _ in 0..60 {
                field = advance_fhn_frame(&field, &settings).expect("advance");
                max_variance = max_variance.max(field.v_variance());
            }
            assert!(
                max_variance < 1e-4,
                "{preset:?}: unstimulated field must stay quiescent (max_variance={max_variance})"
            );
        }
    }

    /// FHN3 (wave propagation, falsifiable): a single interior point
    /// stimulus on an otherwise-resting field must propagate `u` past
    /// threshold outward over time — the max sim-lattice radius (from the
    /// stimulus) at which `u` has crossed a fixed threshold above rest must
    /// grow over the run. Paired with the FHN2 quiescence control on the
    /// SAME preset (no stimulus) proving the growth isn't just numerical
    /// drift. Built directly (not via [`seed_fhn_field`]'s carrier/speckle
    /// path) so the ONLY perturbation is the deliberate centre patch — a
    /// stray speckle hit elsewhere in the frame would otherwise contaminate
    /// the max-radius measurement with an unrelated, isolated point that
    /// diffuses away rather than propagating (excitable media need a
    /// critical nucleus SIZE, not just one point, to successfully fire).
    #[test]
    fn fhn3_point_stimulus_propagates_a_travelling_front() {
        for preset in [FhnPreset::Pulse, FhnPreset::Spiral, FhnPreset::Labyrinth] {
            let settings = FhnSettings {
                sim_scale: 1,
                ..preset.settings()
            };
            let size = 64u32;
            let (u_rest, v_rest) = fhn_resting_state(settings.a, settings.b);
            let mut u = vec![u_rest; (size * size) as usize];
            let v = vec![v_rest; (size * size) as usize];
            let patch = 5i64;
            let half = patch / 2;
            let (cxi, cyi) = (size as i64 / 2, size as i64 / 2);
            for dy in -half..=half {
                for dx in -half..=half {
                    let x = (cxi + dx) as u32;
                    let y = (cyi + dy) as u32;
                    let idx = (y as usize) * (size as usize) + (x as usize);
                    u[idx] = u_rest + settings.stimulus;
                }
            }
            let mut field = MorphogenesisField::new(size, size, u, v).expect("build field");
            let fire_threshold = u_rest + 0.5 * settings.stimulus;

            let cx = size as f32 / 2.0;
            let cy = size as f32 / 2.0;
            let max_radius_crossing_threshold = |field: &MorphogenesisField| -> f32 {
                let mut max_r: f32 = 0.0;
                for y in 0..field.height {
                    for x in 0..field.width {
                        let idx = (y as usize) * (field.width as usize) + (x as usize);
                        if field.u[idx] > fire_threshold {
                            let dx = x as f32 - cx;
                            let dy = y as f32 - cy;
                            max_r = max_r.max((dx * dx + dy * dy).sqrt());
                        }
                    }
                }
                max_r
            };

            let initial_radius = max_radius_crossing_threshold(&field);
            assert!(
                initial_radius > 0.0,
                "{preset:?}: the seed patch itself must cross the fire threshold"
            );

            let mut radii = vec![initial_radius];
            for _ in 0..40 {
                field = advance_fhn_frame(&field, &settings).expect("advance");
                radii.push(max_radius_crossing_threshold(&field));
            }
            let final_radius = *radii.last().unwrap();
            assert!(
                final_radius > initial_radius * 1.5,
                "{preset:?}: a stimulated front must propagate outward (radii={radii:?})"
            );
        }
    }

    /// FHN4 (checkpoint round-trip): resuming from the unquantized RGBA32F
    /// state must be byte-identical to an uninterrupted run — same shape as
    /// Gray-Scott's A4, proving the reused codec round-trips FHN's signed
    /// values without loss.
    #[test]
    fn fhn4_resume_matches_uninterrupted_via_rgba32f_round_trip() {
        let settings = FhnSettings {
            sim_scale: 1,
            ..FhnSettings::pulse()
        };
        let carrier = nucleus_carrier(24, 24);

        let mut uninterrupted = seed_fhn_field(&carrier, &settings).expect("seed");
        for _ in 0..5 {
            uninterrupted = advance_fhn_frame(&uninterrupted, &settings).expect("advance");
        }

        let mut resumed = seed_fhn_field(&carrier, &settings).expect("seed");
        for _ in 0..2 {
            resumed = advance_fhn_frame(&resumed, &settings).expect("advance");
        }
        let packed = morphogenesis_field_to_rgba32f(&resumed).expect("pack");
        let mut resumed = morphogenesis_field_from_rgba32f(&packed).expect("unpack");
        for _ in 0..3 {
            resumed = advance_fhn_frame(&resumed, &settings).expect("advance");
        }

        assert_eq!(
            resumed, uninterrupted,
            "FHN4: resuming from the unquantized RGBA32F state must be byte-identical to an uninterrupted run"
        );
    }

    /// FHN5 (composite/field-view reuse): piping [`fhn_display_field`] into
    /// the two EXISTING output-view functions (unchanged since Gray-Scott)
    /// produces non-flat output once a stimulus has fired — proving the
    /// reuse claim rather than asserting it.
    #[test]
    fn fhn5_display_adapter_feeds_the_existing_output_views_unchanged() {
        let settings = FhnSettings {
            sim_scale: 1,
            ..FhnSettings::pulse()
        };
        let carrier = nucleus_carrier(32, 32);
        let mut field = seed_fhn_field(&carrier, &settings).expect("seed");
        for _ in 0..8 {
            field = advance_fhn_frame(&field, &settings).expect("advance");
        }

        let display = fhn_display_field(&field).expect("display adapter");
        let field_view =
            render_v_field_grayscale_upsampled(&display, carrier.width, carrier.height)
                .expect("field view");
        let first = field_view.pixels[0];
        assert!(
            field_view.pixels.iter().any(|p| p != &first),
            "FHN5: the field view must be non-flat once a stimulus has fired"
        );

        let composite_settings = MorphogenesisCompositeSettings {
            pattern_mix: 0.9,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let composite = composite_morphogenesis_frame(&carrier, &display, &composite_settings)
            .expect("composite");
        assert!(
            composite.pixels.iter().any(|p| p != &carrier.pixels[0]),
            "FHN5: the composite view must diverge from the plain carrier once a stimulus has fired"
        );
    }

    /// FHN6 (live-coupling regression): `apply_fhn_inject` at `inject == 1.0`
    /// and full weight (`w == 1.0`) must push `u` past the same fire
    /// threshold FHN3 uses, in a region that was NEVER seeded — proving
    /// `--inject`/`--inject-source` can launch a genuinely NEW pulse from
    /// footage motion/luma, not just nudge the field imperceptibly. This is
    /// the regression test for the bug this fixed: the original design (a
    /// continuous ODE forcing current, scaled down by `dt` across every
    /// substep) was empirically too weak at any realistic `--inject` value
    /// to ever cross the threshold — a real render (`--inject 0.1
    /// --inject-source motion` vs. no inject) differed by a mean 0.694/255
    /// after 143 frames, imperceptible.
    #[test]
    fn fhn6_inject_fires_a_new_pulse_in_a_never_seeded_region() {
        let settings = FhnSettings {
            sim_scale: 1,
            inject: 1.0,
            ..FhnSettings::pulse()
        };
        let (u_rest, v_rest) = fhn_resting_state(settings.a, settings.b);
        let fire_threshold = u_rest + 0.5 * settings.stimulus;
        let count = 16 * 16;
        let field = MorphogenesisField::new(16, 16, vec![u_rest; count], vec![v_rest; count])
            .expect("build resting field");
        assert!(
            field.u.iter().all(|&u| u <= fire_threshold),
            "sanity: the whole field must start below the fire threshold"
        );

        let w = vec![1.0; count];
        let injected = apply_fhn_inject(&field, &settings, &w).expect("inject");
        assert!(
            injected.u.iter().all(|&u| u > fire_threshold),
            "FHN6: a full-strength inject must push u past the fire threshold everywhere \
             (a never-seeded region must be able to fire a new pulse from live footage)"
        );
    }

    // ─── Track A2: Lenia ────────────────────────────────────────────────────

    fn lenia_mass(field: &MorphogenesisField) -> f32 {
        field.v.iter().sum()
    }

    /// LEN0 (continuity): all three algorithm ids are pairwise distinct, so a
    /// `--model` change on an existing output directory can never be
    /// mistaken for a resumable checkpoint of either other model.
    #[test]
    fn len0_algorithm_ids_are_pairwise_distinct() {
        assert_ne!(MORPHOGENESIS_ALGORITHM, MORPHOGENESIS_LENIA_ALGORITHM);
        assert_ne!(MORPHOGENESIS_FHN_ALGORITHM, MORPHOGENESIS_LENIA_ALGORITHM);
    }

    /// LEN1 (quiescence): an entirely empty field (`A = 0` everywhere, no
    /// seed) stays at zero — `G(K*A) = G(0)` is strongly negative for every
    /// preset's `mu > 0`, so there is no spontaneous generation, and `0` is
    /// already the floor of the `[0,1]` clamp.
    #[test]
    fn len1_empty_field_stays_quiescent() {
        for preset in [
            LeniaPreset::Orbium,
            LeniaPreset::Geminium,
            LeniaPreset::Soup,
        ] {
            let settings = LeniaSettings {
                sim_scale: 1,
                ..preset.settings()
            };
            let count = 32 * 32;
            let mut field =
                MorphogenesisField::new(32, 32, vec![1.0; count], vec![0.0; count]).expect("build");
            for _ in 0..60 {
                field = advance_lenia_frame(&field, &settings).expect("advance");
            }
            assert!(
                field.v.iter().all(|&a| a == 0.0),
                "{preset:?}: an unseeded field must stay at exactly zero"
            );
        }
    }

    /// LEN2 (aliveness, falsifiable): a single carrier-luma-seeded disc
    /// neither dies out nor unboundedly expands — the handoff's own
    /// "mass stays in a band" criterion, adapted for a non-translating
    /// stable blob (this app's disc/luma seeding, unlike a hand-crafted
    /// glider photograph, settles to a static equilibrium rather than
    /// gliding — see [`LeniaSettings::orbium`]'s doc comment). Checked by
    /// comparing the mean mass of an EARLY window (after the initial
    /// transient) against a LATE window: a dying preset's late mass would be
    /// ~0, an exploding preset's late mass would keep climbing well past the
    /// early window.
    #[test]
    fn len2_seeded_disc_settles_to_a_bounded_stable_mass() {
        for preset in [LeniaPreset::Orbium, LeniaPreset::Geminium] {
            let settings = LeniaSettings {
                sim_scale: 1,
                ..preset.settings()
            };
            let size = 64u32;
            let count = (size * size) as usize;
            let mut v = vec![0.0_f32; count];
            stamp_disc(
                &mut v,
                size,
                size,
                size / 2,
                size / 2,
                settings.radius as i32,
            );
            let u = vec![1.0_f32; count];
            let mut field = MorphogenesisField::new(size, size, u, v).expect("build field");

            let mut masses = Vec::with_capacity(180);
            for _ in 0..180 {
                field = advance_lenia_frame(&field, &settings).expect("advance");
                masses.push(lenia_mass(&field));
            }
            let early_mean: f32 = masses[60..100].iter().sum::<f32>() / 40.0;
            let late_mean: f32 = masses[140..180].iter().sum::<f32>() / 40.0;

            assert!(
                early_mean > 50.0,
                "{preset:?}: must not die out (early_mean={early_mean})"
            );
            assert!(
                late_mean > early_mean * 0.5 && late_mean < early_mean * 1.5,
                "{preset:?}: mass must stay in a bounded band, not die or explode \
                 (early_mean={early_mean}, late_mean={late_mean})"
            );
        }
    }

    /// Anchor mirroring Gray-Scott's A2 / FHN's own frozen-field behaviour:
    /// `substeps == 0` leaves the field byte-identical forever.
    #[test]
    fn len3_zero_substeps_freezes_the_field() {
        let settings = LeniaSettings {
            substeps: 0,
            sim_scale: 1,
            ..LeniaSettings::orbium()
        };
        let size = 32u32;
        let count = (size * size) as usize;
        let mut v = vec![0.0_f32; count];
        stamp_disc(
            &mut v,
            size,
            size,
            size / 2,
            size / 2,
            settings.radius as i32,
        );
        let u = vec![1.0_f32; count];
        let field = MorphogenesisField::new(size, size, u, v).expect("build field");

        let advanced = advance_lenia_frame(&field, &settings).expect("advance");
        assert_eq!(
            advanced, field,
            "substeps == 0 must leave the field unchanged"
        );
    }

    /// LEN4 (checkpoint round-trip): resuming from the unquantized RGBA32F
    /// state must be byte-identical to an uninterrupted run, mirroring
    /// FHN4/Gray-Scott's A4 — proving the shared codec round-trips Lenia's
    /// `A` (stored in `.v`) without loss.
    #[test]
    fn len4_resume_matches_uninterrupted_via_rgba32f_round_trip() {
        let settings = LeniaSettings {
            sim_scale: 1,
            ..LeniaSettings::orbium()
        };
        let carrier = nucleus_carrier(24, 24);

        let mut uninterrupted = seed_lenia_field(&carrier, &settings).expect("seed");
        for _ in 0..5 {
            uninterrupted = advance_lenia_frame(&uninterrupted, &settings).expect("advance");
        }

        let mut resumed = seed_lenia_field(&carrier, &settings).expect("seed");
        for _ in 0..2 {
            resumed = advance_lenia_frame(&resumed, &settings).expect("advance");
        }
        let packed = morphogenesis_field_to_rgba32f(&resumed).expect("pack");
        let mut resumed = morphogenesis_field_from_rgba32f(&packed).expect("unpack");
        for _ in 0..3 {
            resumed = advance_lenia_frame(&resumed, &settings).expect("advance");
        }

        assert_eq!(
            resumed, uninterrupted,
            "LEN4: resuming from the unquantized RGBA32F state must be byte-identical to an uninterrupted run"
        );
    }

    /// LEN5 (composite/field-view reuse, no adapter needed): Lenia's `A` is
    /// stored directly in `.v` — already the exact `[0,1]` display contract
    /// Gray-Scott's `V` has — so the two existing output-view functions
    /// consume a raw Lenia field UNCHANGED, with no `fhn_display_field`-style
    /// adapter at all. Proves the claim rather than asserting it.
    #[test]
    fn len5_field_feeds_the_existing_output_views_with_no_adapter() {
        let settings = LeniaSettings {
            sim_scale: 1,
            ..LeniaSettings::orbium()
        };
        let carrier = nucleus_carrier(48, 48);
        let mut field = seed_lenia_field(&carrier, &settings).expect("seed");
        for _ in 0..30 {
            field = advance_lenia_frame(&field, &settings).expect("advance");
        }

        let field_view = render_v_field_grayscale_upsampled(&field, carrier.width, carrier.height)
            .expect("field view");
        let first = field_view.pixels[0];
        assert!(
            field_view.pixels.iter().any(|p| p != &first),
            "LEN5: the field view must be non-flat once the seed has grown"
        );

        let composite_settings = MorphogenesisCompositeSettings {
            pattern_mix: 0.9,
            ..MorphogenesisCompositeSettings::passthrough()
        };
        let composite = composite_morphogenesis_frame(&carrier, &field, &composite_settings)
            .expect("composite");
        assert!(
            composite.pixels.iter().any(|p| p != &carrier.pixels[0]),
            "LEN5: the composite view must diverge from the plain carrier once the seed has grown"
        );
    }

    /// LEN6 (live-coupling regression): injecting through a THIN, edge-like
    /// weight field (a single-cell-wide row, exactly the shape
    /// `injection_weight_motion` produces at a footage motion boundary) must
    /// leave a persisting, non-trivial mass after several frames — not
    /// decay away. This is the regression test for the bug this fixed:
    /// before `apply_lenia_inject_erode` dilated `w` to a disc of
    /// `settings.radius`, a pointwise injection had ~zero local mass under
    /// the ring kernel (the same "single pixel can't fire Lenia" problem
    /// [`seed_lenia_field`] already had to solve for seeding), so real
    /// motion-triggered injections visibly decayed to a faint trail within
    /// a few dozen frames instead of persisting/growing like a real
    /// injected creature (confirmed by rendering the cello fixture: the
    /// injected structure was a bright, well-defined blob at frame 60 but a
    /// thin, nearly-vanished trail by frame 100 before this fix).
    #[test]
    fn len6_dilated_inject_persists_from_a_thin_motion_edge() {
        let settings = LeniaSettings {
            sim_scale: 1,
            inject: 1.0,
            ..LeniaSettings::orbium()
        };
        let size = 96u32;
        let count = (size * size) as usize;
        let v = vec![0.0_f32; count];
        let u = vec![1.0_f32; count];
        let mut field = MorphogenesisField::new(size, size, u, v).expect("build field");

        // A single-row-thick weight field crossing the middle of the field —
        // the same shape a motion edge produces, not a filled disc.
        let mut w = vec![0.0_f32; count];
        let mid_row = (size / 2) as usize;
        for x in 0..size as usize {
            w[mid_row * size as usize + x] = 1.0;
        }

        field = apply_lenia_inject_erode(&field, &settings, &w).expect("inject");
        for _ in 0..20 {
            field = advance_lenia_frame(&field, &settings).expect("advance");
        }

        let mass = lenia_mass(&field);
        assert!(
            mass > 200.0,
            "LEN6: a thin motion-edge injection must persist into a non-trivial mass \
             after 20 frames, not decay away (mass={mass})"
        );
    }
}
