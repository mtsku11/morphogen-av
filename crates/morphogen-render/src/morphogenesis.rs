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

        let sample_x;
        let sample_y;
        if settings.displace == 0.0 {
            sample_x = px;
            sample_y = py;
        } else {
            let grad_x = sample_scalar_grid(
                field.width,
                field.height,
                &gx,
                carrier.width,
                carrier.height,
                px,
                py,
            );
            let grad_y = sample_scalar_grid(
                field.width,
                field.height,
                &gy,
                carrier.width,
                carrier.height,
                px,
                py,
            );
            sample_x = px - settings.displace * grad_x;
            sample_y = py - settings.displace * grad_y;
        }
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
            PatternColorMode::Hue => colorize_luma_preserving(color, settings.pattern_hue),
            PatternColorMode::Inherit => {
                let (h, _s, _v) = rgb_to_hsv(color[0], color[1], color[2]);
                colorize_luma_preserving(color, h)
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
}
