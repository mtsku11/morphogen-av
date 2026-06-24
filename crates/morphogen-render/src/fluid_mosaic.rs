//! Fluid colour-sort mosaic — a two-source effect that **relocates** tiles by
//! colour rather than compositing them in place. Where [`crate::coagulate`] and
//! [`crate::disperse`] keep each tile roughly where it started (blending or locally
//! advecting content), this path treats every tile of *both* sources as a crisp
//! **particle** carrying its mean colour, and lets two forces move it:
//!
//! 1. **Local colour cohesion + repulsion (emergent self-sorting).** Each tile is
//!    pulled toward the *local* mean position of nearby same-colour tiles while a
//!    colour-blind short-range repulsion keeps tiles spread to fill the frame. Like
//!    spinodal/Ising decomposition, the two together make colours **phase-separate
//!    into domains that tile the canvas** — no fixed colour→position map and no
//!    global centroid (which would centralize); the grouping emerges from local
//!    dynamics. A warmup *settle* pass runs this before frame zero so the first
//!    displayed frame is already colour-grouped (the "grouped at the start" condition).
//! 2. **A fluid velocity field.** A deterministic, divergence-free curl field
//!    (analytic streamfunction, frame-phased) advects every tile so the grouped
//!    colours then flow and intermix like dye — the marcscully.com fluid look — while
//!    the tiles keep their crisp edges (the hybrid "crisp tiles ride a fluid" model).
//!
//! Deterministic CPU reference. Each tile carries the original pixel **patch** of its
//! source cell, so with `carry_texture` on the render paints that patch (footage
//! texture survives, not just a flat colour) while the *sorting and motion stay keyed
//! on the patch's mean colour* — turning texture off renders the flat mean square and
//! is the only difference, so off-vs-on isolates exactly the texture. With
//! `adaptive_tiles` on, tiles are **variable-size**: a quadtree subdivides each source
//! from `tile_size` down to `min_tile_size`, splitting only where local colour
//! variance is high — flat regions stay large, detailed regions become fine — and the
//! repulsion target scales with the two tiles' sizes so the sheet stays space-filling.
//! Off (the default), every tile is `tile_size` and the path is byte-identical to the
//! uniform `v2` formulation. Each tile also remembers its source-**origin cell**, so a
//! caller can call [`refresh_fluid_mosaic_colors`] each frame to **re-sample its painted
//! colour and patch from the current source frame** — a render-only *live colour
//! refresh* that lets the two videos play through the flowing mosaic. The simulation
//! (positions, the frozen frame-zero colour bins that drive sorting) is untouched, so
//! the force balance is unchanged and refresh alters only the painted pixels.
//! [`resort_fluid_mosaic_colors`] goes one step further — a *sim-driving live re-sort*:
//! it re-samples the colour/patch **and re-bins each tile** from the current frame, so
//! the cohesion force (which keys on the bin) makes colour domains **migrate to follow
//! the video** rather than staying frozen in their frame-zero grouping. Positions and
//! velocities still carry forward; only the colour bin (and painted pixels) updates.
//! Without a refresh/resort call the path stays self-contained (seeded from each
//! source's first frame).
//!
//! `cluster_blob` swaps the cohesion *target*: instead of the local same-colour mean
//! (which phase-separates colours into domains in place), every tile is pulled toward
//! its colour bin's **global** centroid, so each colour collapses into a single compact
//! blob (stiff repulsion keeps the blob a disc rather than a point). It is the
//! "gather each colour into one cluster" reading of self-sorting, opposite to the
//! default's screen-filling decomposition.
//! Metal is a later slice. Determinism: splitmix64 hashing, fixed-timestep
//! integration, no wall clock.
//!
//! Continuity identity (the off case for off-vs-on readout): with `cohesion`,
//! `fluid_strength`, and `jitter` all `0` and `settle_iterations == 0`, every tile
//! stays at its grid centre, so the render is the two source grids overlaid verbatim.

use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

/// Algorithm identifier for the CPU reference. Bump when the force model, tile
/// formulation, or the content a tile paints changes so stale caches/checkpoints
/// invalidate. `v6` adds the cluster-blob layout (`cluster_blob`: cohesion pulls each
/// tile toward its colour bin's *global* centroid so each colour gathers into one blob,
/// vs the default local-mean phase separation); `v5` adds sim-driving live re-sort
/// (`live_resort`: refreshed colour also re-bins each tile so cohesion follows the
/// video); `v4` added render-only live colour refresh (each tile can re-sample its
/// painted colour/patch from the current source frame); `v3` added variable-size tiles
/// (`adaptive_tiles`: quadtree subdivision + size-aware repulsion); `v2` added texture
/// patches (`carry_texture`); `v1` was flat uniform mean colour only.
pub const FLUID_MOSAIC_ALGORITHM: &str = "fluid_mosaic_colour_sort_cpu_v6";

/// Knobs for the fluid colour-sort mosaic (Slice 1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct FluidMosaicSettings {
    /// Uniform tile edge length in pixels (>= 1).
    pub tile_size: u32,
    /// Quantization levels per RGB channel for colour binning (>= 2). The number of
    /// colour groups is `color_bins^3`; like-binned tiles attract one another.
    pub color_bins: u32,
    /// Per-step pull of each tile toward the **local** mean position of nearby
    /// same-colour tiles, in `[0, 1]`. Local (not global-centroid) cohesion makes
    /// like-colours coalesce into domains *in place* without collapsing the whole
    /// ensemble to the centre. `0` disables grouping.
    pub cohesion: f32,
    /// Neighbourhood radius (pixels) over which same-colour cohesion is gathered.
    pub cohesion_radius: f32,
    /// Short-range (colour-blind) repulsion strength between nearby tiles
    /// (pixels/step at full overlap). The pressure that keeps tiles spread to fill
    /// the frame so colour domains tile the canvas. `0` disables it.
    pub repulsion: f32,
    /// Radius (pixels) within which tiles repel one another (< `cohesion_radius`).
    pub repulsion_radius: f32,
    /// Amplitude of the fluid velocity field (pixels/step at unit field value).
    pub fluid_strength: f32,
    /// Spatial frequency of the curl field (radians per pixel). Smaller ⇒ broader,
    /// smoother currents.
    pub fluid_scale: f32,
    /// Temporal phase advance of the curl field per frame (how fast the fluid churns).
    pub fluid_drift: f32,
    /// Per-step velocity damping in `[0, 1)` — keeps the motion bounded so tiles
    /// flow rather than accelerate off-screen.
    pub damping: f32,
    /// Warmup cohesion+repulsion iterations applied before frame zero (no fluid), so
    /// the first displayed frame is already colour-grouped. `0` starts from the raw grids.
    pub settle_iterations: u32,
    /// Per-step animated random nudge (pixels) added to every tile — keeps groups
    /// alive and stops them collapsing to a perfect point.
    pub jitter: f32,
    /// When `true`, render each tile's original source pixel patch (footage texture
    /// survives). When `false`, render the flat mean-colour square (the v1 look).
    /// Sorting and motion are unaffected either way — they key on the mean colour —
    /// so this flag isolates exactly the texture in an off-vs-on comparison.
    #[serde(default = "default_carry_texture")]
    pub carry_texture: bool,
    /// When `true`, tiles are **variable-size**: a quadtree subdivides each `tile_size`
    /// cell down toward `min_tile_size`, splitting only where local colour variance
    /// exceeds `subdivide_threshold`, so flat regions stay coarse and detailed regions
    /// become fine. Repulsion then targets the two tiles' average size so the sheet
    /// stays space-filling. When `false`, every tile is `tile_size` (the `v2` look).
    #[serde(default = "default_adaptive_tiles")]
    pub adaptive_tiles: bool,
    /// Smallest tile edge length the quadtree may subdivide to (>= 1, <= `tile_size`).
    /// Only used when `adaptive_tiles` is on.
    #[serde(default = "default_min_tile_size")]
    pub min_tile_size: u32,
    /// Sum-of-per-channel colour variance above which a cell subdivides (only when
    /// `adaptive_tiles` is on). Lower ⇒ more aggressive subdivision (finer tiles).
    #[serde(default = "default_subdivide_threshold")]
    pub subdivide_threshold: f32,
    /// When `true`, the caller re-samples each tile's painted colour and patch from the
    /// **current** source frame every frame (via [`refresh_fluid_mosaic_colors`]) so the
    /// two videos play through the flowing mosaic. Render-only: the simulation (positions
    /// and the frozen frame-zero bins that drive sorting) is unaffected, so this isolates
    /// exactly the live content in an off-vs-on comparison. When `false`, the mosaic is
    /// seeded from each source's first frame and runs self-contained (the `v3` look).
    #[serde(default = "default_live_refresh")]
    pub live_refresh: bool,
    /// When `true`, the caller re-samples each tile's colour/patch from the current
    /// source frame **and re-bins it** every frame (via [`resort_fluid_mosaic_colors`]),
    /// so the cohesion force follows the live colour and domains migrate to track the
    /// video — a sim-driving live re-sort. Implies the live colour refresh (the painted
    /// pixels also update). When `false`, the colour bins stay frozen at their
    /// frame-zero values (the `v4` render-only refresh, or a self-contained sim).
    #[serde(default = "default_live_resort")]
    pub live_resort: bool,
    /// When `true`, cohesion pulls each tile toward its colour bin's **global**
    /// centroid (the mean position of *all* same-colour tiles on the canvas) instead
    /// of the local neighbourhood mean — so each colour gathers into a single compact
    /// **blob** rather than phase-separating into many in-place domains. Stiff
    /// repulsion still keeps a blob from collapsing to a point (it rests as a disc
    /// sized by its tile count). `cohesion_radius` is ignored for the cohesion pull in
    /// this mode (the reach is global). When `false`, cohesion is local (the default
    /// screen-filling self-sorting look). Caveat: colours that start spatially uniform
    /// share a near-identical centroid (the canvas centre), so the blobs separate only
    /// when each colour is spatially concentrated in the source.
    #[serde(default = "default_cluster_blob")]
    pub cluster_blob: bool,
    /// Seed for the deterministic per-tile hashes and the fluid field phase.
    pub seed: u64,
}

fn default_carry_texture() -> bool {
    true
}

fn default_adaptive_tiles() -> bool {
    false
}

fn default_min_tile_size() -> u32 {
    4
}

fn default_subdivide_threshold() -> f32 {
    0.004
}

fn default_live_refresh() -> bool {
    false
}

fn default_live_resort() -> bool {
    false
}

fn default_cluster_blob() -> bool {
    false
}

impl Default for FluidMosaicSettings {
    fn default() -> Self {
        Self {
            tile_size: 8,
            color_bins: 5,
            cohesion: 0.035,
            cohesion_radius: 24.0,
            repulsion: 1.4,
            repulsion_radius: 10.0,
            fluid_strength: 0.5,
            fluid_scale: 0.01,
            fluid_drift: 0.15,
            damping: 0.88,
            settle_iterations: 60,
            jitter: 0.03,
            carry_texture: true,
            adaptive_tiles: false,
            min_tile_size: 4,
            subdivide_threshold: 0.004,
            live_refresh: false,
            live_resort: false,
            cluster_blob: false,
            seed: 0,
        }
    }
}

impl FluidMosaicSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.tile_size == 0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "tile_size must be greater than zero".to_string(),
            ));
        }
        if self.color_bins < 2 {
            return Err(RenderError::InvalidCoagulationSettings(
                "color_bins must be at least 2".to_string(),
            ));
        }
        if self.adaptive_tiles && (self.min_tile_size == 0 || self.min_tile_size > self.tile_size) {
            return Err(RenderError::InvalidCoagulationSettings(format!(
                "min_tile_size ({}) must be in 1..=tile_size ({})",
                self.min_tile_size, self.tile_size
            )));
        }
        if self.adaptive_tiles && !(self.subdivide_threshold.is_finite() && self.subdivide_threshold >= 0.0) {
            return Err(RenderError::InvalidCoagulationSettings(
                "subdivide_threshold must be finite and non-negative".to_string(),
            ));
        }
        for (name, value) in [
            ("cohesion", self.cohesion),
            ("cohesion_radius", self.cohesion_radius),
            ("repulsion", self.repulsion),
            ("repulsion_radius", self.repulsion_radius),
            ("fluid_strength", self.fluid_strength),
            ("fluid_scale", self.fluid_scale),
            ("fluid_drift", self.fluid_drift),
            ("damping", self.damping),
            ("jitter", self.jitter),
        ] {
            if !value.is_finite() {
                return Err(RenderError::InvalidCoagulationSettings(format!(
                    "{name} must be finite"
                )));
            }
        }
        Ok(())
    }
}

/// The original pixel patch of a source cell, carried by its tile so the render can
/// paint footage texture rather than a flat colour. Row-major RGB; `width`/`height`
/// are the cell's own dimensions (edge cells may be smaller than `tile_size`).
#[derive(Debug, Clone, PartialEq)]
pub struct TilePatch {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[f32; 3]>,
}

/// Where a tile was born — its source (`0` = A, `1` = B) and the source-pixel cell
/// rectangle `[x0,x1)×[y0,y1)` it averaged. Fixed for the tile's life; lets
/// [`refresh_fluid_mosaic_colors`] re-sample the same cell from a later source frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileOrigin {
    pub source: u8,
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

/// The stateful particle set. All `Vec`s are parallel and index-aligned; tile order
/// is fixed (Source A tiles first, then Source B), which fixes the painter order in
/// [`render_fluid_mosaic`] and keeps the render deterministic.
#[derive(Debug, Clone, PartialEq)]
pub struct FluidMosaicState {
    pub width: u32,
    pub height: u32,
    pub tile_size: u32,
    pub color_bins: u32,
    /// Continuous tile centres in pixels.
    pub positions: Vec<[f32; 2]>,
    /// Per-tile velocity in pixels/step.
    pub velocities: Vec<[f32; 2]>,
    /// Fixed per-tile mean colour (RGB) — drives binning, cohesion, and the flat-mode render.
    pub colors: Vec<[f32; 3]>,
    /// Fixed per-tile original pixel patch (for the `carry_texture` render).
    pub patches: Vec<TilePatch>,
    /// Fixed per-tile colour bin index in `0..color_bins^3`.
    pub bins: Vec<u32>,
    /// Fixed per-tile edge length in pixels (the painted square's size and the
    /// size-aware repulsion target). All equal `tile_size` unless `adaptive_tiles`.
    pub sizes: Vec<u32>,
    /// Fixed per-tile source-origin cell (for the live colour refresh). Index-aligned.
    pub origin: Vec<TileOrigin>,
}

/// Phase salt so a different seed gives a different fluid field.
const FLUID_PHASE_SALT: u64 = 0xF1D5_0FF5_E712_0A37;
const JITTER_SALT: u64 = 0x101A_7E55_2C0F_FEE1;

/// Seed the particle set from the first frame of each source and run the warmup
/// settle so frame zero is already colour-grouped.
pub fn initialize_fluid_mosaic(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    settings: FluidMosaicSettings,
) -> Result<FluidMosaicState, RenderError> {
    settings.validate()?;
    if source_a.width != source_b.width || source_a.height != source_b.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "source A is {}x{}, source B is {}x{}",
            source_a.width, source_a.height, source_b.width, source_b.height
        )));
    }

    let width = source_a.width;
    let height = source_a.height;
    let tile = settings.tile_size;

    let mut acc = TileAccumulator::default();
    for (source_index, source) in [source_a, source_b].into_iter().enumerate() {
        append_source_tiles(source, source_index as u8, settings, &mut acc);
    }
    let velocities = vec![[0.0_f32, 0.0]; acc.positions.len()];

    let mut state = FluidMosaicState {
        width,
        height,
        tile_size: tile,
        color_bins: settings.color_bins,
        positions: acc.positions,
        velocities,
        colors: acc.colors,
        patches: acc.patches,
        bins: acc.bins,
        sizes: acc.sizes,
        origin: acc.origin,
    };

    // Warmup: local same-colour cohesion + colour-blind repulsion (no fluid, no
    // velocity carry). Like-colours coalesce into domains in place while the
    // repulsion pressure keeps tiles spread across the frame — the grouped, yet
    // screen-filling, initial state.
    for _ in 0..settings.settle_iterations {
        let forces = neighbor_forces(&state, settings);
        for (pos, force) in state.positions.iter_mut().zip(&forces) {
            *pos = [
                (pos[0] + force[0]).clamp(0.0, width as f32),
                (pos[1] + force[1]).clamp(0.0, height as f32),
            ];
        }
    }

    Ok(state)
}

/// Advance the simulation one frame: colour attraction + fluid advection + jitter,
/// integrated with damping. Returns a fresh state (inputs are not mutated).
pub fn advance_fluid_mosaic(
    state: &FluidMosaicState,
    settings: FluidMosaicSettings,
    frame_index: u32,
) -> Result<FluidMosaicState, RenderError> {
    settings.validate()?;

    let damping = settings.damping.clamp(0.0, 1.0);
    let time = frame_index as f32 * settings.fluid_drift;
    let width = state.width as f32;
    let height = state.height as f32;

    let forces = neighbor_forces(state, settings);
    let mut positions = state.positions.clone();
    let mut velocities = state.velocities.clone();

    for (i, ((pos, vel), force)) in positions
        .iter_mut()
        .zip(velocities.iter_mut())
        .zip(&forces)
        .enumerate()
    {
        let p = *pos;

        // Local same-colour cohesion + colour-blind repulsion (emergent grouping
        // that fills the frame).
        let mut ax = force[0];
        let mut ay = force[1];

        // Fluid advection (divergence-free curl field) — the flowing/mixing current.
        let (fx, fy) = fluid_velocity(p[0], p[1], time, settings);
        ax += fx * settings.fluid_strength;
        ay += fy * settings.fluid_strength;

        // Animated jitter keeps groups alive (no perfect collapse).
        let angle = hash01(settings.seed ^ JITTER_SALT, i as u64, u64::from(frame_index)) * TAU;
        ax += angle.cos() * settings.jitter;
        ay += angle.sin() * settings.jitter;

        let nv = [(vel[0] + ax) * damping, (vel[1] + ay) * damping];
        *pos = [
            (p[0] + nv[0]).clamp(0.0, width),
            (p[1] + nv[1]).clamp(0.0, height),
        ];
        *vel = nv;
    }

    Ok(FluidMosaicState {
        positions,
        velocities,
        colors: state.colors.clone(),
        patches: state.patches.clone(),
        bins: state.bins.clone(),
        sizes: state.sizes.clone(),
        origin: state.origin.clone(),
        ..*state
    })
}

/// Render the current particle set as crisp tiles. Each tile paints a
/// `tile_size`×`tile_size` opaque square centred on its (rounded) position; tiles are
/// painted in fixed index order (painter's algorithm), so later tiles overwrite
/// earlier ones. Uncovered pixels stay opaque black. With `carry_texture` on, each
/// pixel of the square is sampled (nearest) from the tile's original source patch so
/// footage texture survives; off, the flat mean colour fills the square.
pub fn render_fluid_mosaic(
    state: &FluidMosaicState,
    settings: FluidMosaicSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let width = state.width;
    let height = state.height;
    let mut pixels = vec![[0.0_f32, 0.0, 0.0, 1.0]; (width as usize) * (height as usize)];

    // Paint largest tiles first so fine detail tiles land on top of coarse flat ones.
    // The sort is stable, so when sizes are uniform the order is the original index
    // order (Source A then B) and the output matches the uniform formulation exactly.
    let mut order: Vec<usize> = (0..state.positions.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(state.sizes[i]));

    for &i in &order {
        let pos = state.positions[i];
        let color = state.colors[i];
        let patch = &state.patches[i];
        let tile = state.sizes[i] as i64;
        let half = tile / 2;
        let cx = pos[0].round() as i64;
        let cy = pos[1].round() as i64;
        let left = cx - half;
        let top = cy - half;
        let x0 = left.max(0);
        let y0 = top.max(0);
        let x1 = (left + tile).min(width as i64);
        let y1 = (top + tile).min(height as i64);
        for y in y0..y1 {
            let row = (y as usize) * (width as usize);
            for x in x0..x1 {
                let rgb = if settings.carry_texture {
                    // Nearest-sample the square's local offset (0..tile_size, relative
                    // to the *unclamped* top-left so the patch isn't shifted) into the
                    // patch's own dimensions (edge cells may be smaller than tile_size).
                    let sx = (x - left).max(0);
                    let sy = (y - top).max(0);
                    let px = (sx * patch.width as i64 / tile).clamp(0, patch.width as i64 - 1);
                    let py = (sy * patch.height as i64 / tile).clamp(0, patch.height as i64 - 1);
                    patch.pixels[(py * patch.width as i64 + px) as usize]
                } else {
                    color
                };
                pixels[row + x as usize] = [rgb[0], rgb[1], rgb[2], 1.0];
            }
        }
    }

    ImageBufferF32::new(width, height, pixels)
}

/// Index-aligned parallel tile vectors, accumulated across both sources.
#[derive(Default)]
struct TileAccumulator {
    positions: Vec<[f32; 2]>,
    colors: Vec<[f32; 3]>,
    patches: Vec<TilePatch>,
    bins: Vec<u32>,
    sizes: Vec<u32>,
    origin: Vec<TileOrigin>,
}

impl TileAccumulator {
    /// Emit one tile for the in-bounds cell `[x0,x1)×[y0,y1)` of `source` (index
    /// `source_index`) whose painted square edge is `nominal_size` (>= the clamped
    /// extent at edges). Pushes mean colour, the original pixel patch, the colour bin,
    /// the centre position, the size, and the origin cell (for live refresh).
    #[allow(clippy::too_many_arguments)]
    fn push_cell(
        &mut self,
        source: &ImageBufferF32,
        source_index: u8,
        x0: u32,
        y0: u32,
        x1: u32,
        y1: u32,
        nominal_size: u32,
        color_bins: u32,
    ) {
        let (mean, patch) = sample_cell(source, x0, y0, x1, y1);
        self.positions
            .push([(x0 + x1) as f32 * 0.5, (y0 + y1) as f32 * 0.5]);
        self.colors.push(mean);
        self.patches.push(patch);
        self.bins.push(color_bin(mean, color_bins));
        self.sizes.push(nominal_size);
        self.origin.push(TileOrigin {
            source: source_index,
            x0,
            y0,
            x1,
            y1,
        });
    }
}

/// Mean colour + original pixel patch of the in-bounds cell `[x0,x1)×[y0,y1)`. Shared
/// by the initial seed and the live refresh so a refreshed tile matches a freshly seeded
/// one byte-for-byte when the source frame is identical.
fn sample_cell(source: &ImageBufferF32, x0: u32, y0: u32, x1: u32, y1: u32) -> ([f32; 3], TilePatch) {
    let mut sum = [0.0_f32; 3];
    let mut count = 0.0_f32;
    let mut patch_pixels = Vec::with_capacity(((x1 - x0) * (y1 - y0)) as usize);
    for y in y0..y1 {
        for x in x0..x1 {
            let px = source.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            sum[0] += px[0];
            sum[1] += px[1];
            sum[2] += px[2];
            count += 1.0;
            patch_pixels.push([px[0], px[1], px[2]]);
        }
    }
    let mean = if count > 0.0 {
        [sum[0] / count, sum[1] / count, sum[2] / count]
    } else {
        [0.0, 0.0, 0.0]
    };
    (
        mean,
        TilePatch {
            width: x1 - x0,
            height: y1 - y0,
            pixels: patch_pixels,
        },
    )
}

/// Re-sample every tile's painted colour and patch from the **current** frame of its
/// source, leaving the simulation (positions, velocities, bins, sizes, origin)
/// untouched — a render-only live colour refresh so the two videos play through the
/// flowing mosaic. Sorting stays keyed on the frozen frame-zero bins, so the force
/// balance is unchanged. The frames must match the state's dimensions.
pub fn refresh_fluid_mosaic_colors(
    state: &mut FluidMosaicState,
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
) -> Result<(), RenderError> {
    resample_tiles(state, source_a, source_b, false)
}

/// Re-sample every tile's colour and patch from the **current** frame of its source
/// **and re-bin it** — a sim-driving live re-sort. Because the cohesion force keys on
/// the colour bin, re-binning makes colour domains migrate to follow the video: a tile
/// whose source cell changes hue joins the new colour's group and is pulled toward it.
/// Positions and velocities carry forward (the motion is continuous); only the bin and
/// painted pixels update. The frames must match the state's dimensions.
pub fn resort_fluid_mosaic_colors(
    state: &mut FluidMosaicState,
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
) -> Result<(), RenderError> {
    resample_tiles(state, source_a, source_b, true)
}

/// Shared body of the live colour refresh / re-sort. Re-samples each tile's colour and
/// patch from its origin cell in the current source frame; when `resort`, also recomputes
/// the colour bin so cohesion follows the live colour (otherwise the bin stays frozen).
fn resample_tiles(
    state: &mut FluidMosaicState,
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    resort: bool,
) -> Result<(), RenderError> {
    for source in [source_a, source_b] {
        if source.width != state.width || source.height != state.height {
            return Err(RenderError::IncompatibleInputs(format!(
                "refresh frame is {}x{}, state is {}x{}",
                source.width, source.height, state.width, state.height
            )));
        }
    }
    let color_bins = state.color_bins;
    for i in 0..state.origin.len() {
        let origin = state.origin[i];
        let source = if origin.source == 0 { source_a } else { source_b };
        let (mean, patch) = sample_cell(source, origin.x0, origin.y0, origin.x1, origin.y1);
        state.colors[i] = mean;
        state.patches[i] = patch;
        if resort {
            state.bins[i] = color_bin(mean, color_bins);
        }
    }
    Ok(())
}

/// Append one source's tiles to the accumulator. Uniform `tile_size` cells unless
/// `adaptive_tiles`, in which case each top-level cell is recursively quadtree-split.
fn append_source_tiles(
    source: &ImageBufferF32,
    source_index: u8,
    settings: FluidMosaicSettings,
    acc: &mut TileAccumulator,
) {
    let tile = settings.tile_size;
    let cols = source.width.div_ceil(tile);
    let rows = source.height.div_ceil(tile);
    for cy in 0..rows {
        for cx in 0..cols {
            let x0 = cx * tile;
            let y0 = cy * tile;
            if settings.adaptive_tiles {
                subdivide_cell(source, source_index, x0, y0, tile, settings, acc);
            } else {
                let x1 = (x0 + tile).min(source.width);
                let y1 = (y0 + tile).min(source.height);
                acc.push_cell(source, source_index, x0, y0, x1, y1, tile, settings.color_bins);
            }
        }
    }
}

/// Quadtree subdivision: split the `size`-edged cell at `(x0,y0)` into four while its
/// in-bounds colour variance exceeds `subdivide_threshold` and `size` is still above
/// `min_tile_size`; otherwise emit it as a single tile. Cells fully off the canvas are
/// skipped; edge cells are clamped (their patch is smaller than `size`).
fn subdivide_cell(
    source: &ImageBufferF32,
    source_index: u8,
    x0: u32,
    y0: u32,
    size: u32,
    settings: FluidMosaicSettings,
    acc: &mut TileAccumulator,
) {
    if x0 >= source.width || y0 >= source.height {
        return;
    }
    let x1 = (x0 + size).min(source.width);
    let y1 = (y0 + size).min(source.height);
    let half = size / 2;
    let can_split = size > settings.min_tile_size && half >= 1;
    if can_split && cell_variance(source, x0, y0, x1, y1) > settings.subdivide_threshold {
        subdivide_cell(source, source_index, x0, y0, half, settings, acc);
        subdivide_cell(source, source_index, x0 + half, y0, half, settings, acc);
        subdivide_cell(source, source_index, x0, y0 + half, half, settings, acc);
        subdivide_cell(source, source_index, x0 + half, y0 + half, half, settings, acc);
    } else {
        acc.push_cell(source, source_index, x0, y0, x1, y1, size, settings.color_bins);
    }
}

/// Sum of the per-channel colour variance over the in-bounds cell — the detail metric
/// that drives subdivision. `0` for a flat cell.
fn cell_variance(source: &ImageBufferF32, x0: u32, y0: u32, x1: u32, y1: u32) -> f32 {
    let mut sum = [0.0_f32; 3];
    let mut sq = [0.0_f32; 3];
    let mut n = 0.0_f32;
    for y in y0..y1 {
        for x in x0..x1 {
            let px = source.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            for c in 0..3 {
                sum[c] += px[c];
                sq[c] += px[c] * px[c];
            }
            n += 1.0;
        }
    }
    if n <= 0.0 {
        return 0.0;
    }
    let mut variance = 0.0;
    for c in 0..3 {
        let mean = sum[c] / n;
        variance += (sq[c] / n - mean * mean).max(0.0);
    }
    variance
}

/// Quantize a colour into a `color_bins^3` bin index.
fn color_bin(color: [f32; 3], color_bins: u32) -> u32 {
    let levels = color_bins.max(2);
    let q = |c: f32| -> u32 {
        let scaled = c.clamp(0.0, 1.0) * (levels - 1) as f32;
        scaled.round() as u32
    };
    (q(color[0]) * levels + q(color[1])) * levels + q(color[2])
}

/// Global centroid (mean position) of every colour bin, indexed by bin in
/// `0..color_bins^3`. `None` for an unoccupied bin. The cluster-blob cohesion target:
/// pulling each tile toward its bin's entry gathers a colour into one blob.
fn global_bin_centroids(state: &FluidMosaicState) -> Vec<Option<[f32; 2]>> {
    let levels = state.color_bins.max(2) as usize;
    let nbins = levels * levels * levels;
    let mut sum = vec![[0.0_f32, 0.0]; nbins];
    let mut count = vec![0.0_f32; nbins];
    for (i, p) in state.positions.iter().enumerate() {
        let b = state.bins[i] as usize;
        if b < nbins {
            sum[b][0] += p[0];
            sum[b][1] += p[1];
            count[b] += 1.0;
        }
    }
    sum.iter()
        .zip(&count)
        .map(|(s, &c)| {
            if c > 0.0 {
                Some([s[0] / c, s[1] / c])
            } else {
                None
            }
        })
        .collect()
}

/// Per-tile neighbour force = same-colour **cohesion** plus colour-blind short-range
/// **repulsion**. Cohesion pulls each tile toward the mean position of nearby same-bin
/// tiles (the local, phase-separating default) or — when `cluster_blob` — toward its
/// bin's *global* centroid (gathering each colour into one blob). Repulsion always uses
/// a uniform spatial-hash grid (cell = `cohesion_radius`, the larger radius) so the
/// near-field stays O(N · local density) rather than O(N²): each tile only tests
/// neighbours in its own and the eight adjacent cells. Exactly-coincident tiles (common
/// at frame zero, where A's and B's grids overlap) are separated along a deterministic
/// per-tile hashed direction.
fn neighbor_forces(state: &FluidMosaicState, settings: FluidMosaicSettings) -> Vec<[f32; 2]> {
    let n = state.positions.len();
    let cohesion_on = settings.cohesion > 0.0 && settings.cohesion_radius > 0.0;
    let repulsion_on = settings.repulsion > 0.0 && settings.repulsion_radius > 0.0;
    if !cohesion_on && !repulsion_on {
        return vec![[0.0, 0.0]; n];
    }

    // Cluster-blob cohesion targets each colour bin's global centroid (precomputed once)
    // instead of the local same-colour mean gathered per tile in the grid loop below.
    let cluster = cohesion_on && settings.cluster_blob;
    let centroids = if cluster {
        Some(global_bin_centroids(state))
    } else {
        None
    };

    // The repulsion reach is a fixed radius for uniform tiles, but with adaptive sizes
    // a pair's target spacing is (size_i + size_j)/2 ≤ max tile size, so the grid cell
    // must be at least that large for the 3×3 neighbourhood to catch every interaction.
    let rep_reach = if settings.adaptive_tiles {
        let max_size = state.sizes.iter().copied().max().unwrap_or(state.tile_size) as f32;
        max_size.max(settings.repulsion_radius)
    } else {
        settings.repulsion_radius
    };
    let radius = settings.cohesion_radius.max(rep_reach).max(1.0);
    let grid_cols = (state.width as f32 / radius).ceil().max(1.0) as i64;
    let grid_rows = (state.height as f32 / radius).ceil().max(1.0) as i64;
    let cell_of = |p: [f32; 2]| -> (i64, i64) {
        (
            ((p[0] / radius) as i64).clamp(0, grid_cols - 1),
            ((p[1] / radius) as i64).clamp(0, grid_rows - 1),
        )
    };

    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); (grid_cols * grid_rows) as usize];
    for (i, p) in state.positions.iter().enumerate() {
        let (gx, gy) = cell_of(*p);
        buckets[(gy * grid_cols + gx) as usize].push(i as u32);
    }

    let coh_r2 = settings.cohesion_radius * settings.cohesion_radius;
    let rep_r = settings.repulsion_radius;
    let rep_r2 = rep_r * rep_r;
    let mut accels = vec![[0.0_f32, 0.0]; n];
    for (i, accel) in accels.iter_mut().enumerate() {
        let p = state.positions[i];
        let bin = state.bins[i];
        let (gx, gy) = cell_of(p);
        let mut rep = [0.0_f32, 0.0];
        let mut coh_sum = [0.0_f32, 0.0];
        let mut coh_count = 0.0_f32;
        for ny in (gy - 1)..=(gy + 1) {
            for nx in (gx - 1)..=(gx + 1) {
                if nx < 0 || ny < 0 || nx >= grid_cols || ny >= grid_rows {
                    continue;
                }
                for &j in &buckets[(ny * grid_cols + nx) as usize] {
                    let j = j as usize;
                    if j == i {
                        continue;
                    }
                    let q = state.positions[j];
                    let dx = p[0] - q[0];
                    let dy = p[1] - q[1];
                    let d2 = dx * dx + dy * dy;

                    // Uniform tiles use the fixed repulsion_radius; adaptive tiles
                    // target the pair's average size so big and small tiles each rest
                    // at their own non-overlapping spacing (keeps coverage space-filling).
                    let (target, target2) = if settings.adaptive_tiles {
                        // Target the pair's average size, but never let a pair pack
                        // tighter than the proven incompressible spacing — small tiles
                        // with a tiny target otherwise over-pack and the cohesion
                        // collapses them into clumps, opening black voids over time.
                        let t = ((state.sizes[i] + state.sizes[j]) as f32 * 0.5).max(rep_r);
                        (t, t * t)
                    } else {
                        (rep_r, rep_r2)
                    };
                    if repulsion_on && d2 < target2 {
                        if d2 <= 1e-12 {
                            // Coincident: push along a deterministic hashed direction.
                            let angle = hash01(settings.seed, i as u64, j as u64) * TAU;
                            rep[0] += angle.cos() * settings.repulsion;
                            rep[1] += angle.sin() * settings.repulsion;
                        } else {
                            let dist = d2.sqrt();
                            let falloff = 1.0 - dist / target;
                            rep[0] += (dx / dist) * settings.repulsion * falloff;
                            rep[1] += (dy / dist) * settings.repulsion * falloff;
                        }
                    }

                    if cohesion_on && !cluster && state.bins[j] == bin && d2 < coh_r2 {
                        coh_sum[0] += q[0];
                        coh_sum[1] += q[1];
                        coh_count += 1.0;
                    }
                }
            }
        }

        let mut ax = rep[0];
        let mut ay = rep[1];
        if let Some(centroids) = &centroids {
            // Cluster-blob: pull toward this colour bin's global centroid.
            if let Some(Some(c)) = centroids.get(bin as usize) {
                ax += (c[0] - p[0]) * settings.cohesion;
                ay += (c[1] - p[1]) * settings.cohesion;
            }
        } else if coh_count > 0.0 {
            // Pull toward the local same-colour mean position.
            ax += (coh_sum[0] / coh_count - p[0]) * settings.cohesion;
            ay += (coh_sum[1] / coh_count - p[1]) * settings.cohesion;
        }
        *accel = [ax, ay];
    }
    accels
}

/// A deterministic, divergence-free fluid velocity from a two-octave analytic
/// streamfunction `psi`; `v = (∂psi/∂y, -∂psi/∂x)` is incompressible, giving the
/// swirling, dye-like flow. Field value is dimensionless (~unit scale); the caller
/// multiplies by `fluid_strength`.
fn fluid_velocity(x: f32, y: f32, time: f32, settings: FluidMosaicSettings) -> (f32, f32) {
    let k1 = settings.fluid_scale;
    let k2 = settings.fluid_scale * 2.0;
    // Seed-derived phase offsets so different seeds give different fields.
    let phase = hash01(settings.seed ^ FLUID_PHASE_SALT, 0, 0) * TAU;
    let x1 = k1 * x + time + phase;
    let y1 = k1 * y;
    let x2 = k2 * x - time + phase;
    let y2 = k2 * y;

    // psi = sin(x1)cos(y1) + 0.5 cos(x2) sin(y2)
    // ∂psi/∂y =  sin(x1)(-k1 sin(y1)) + 0.5 cos(x2)( k2 cos(y2))
    // ∂psi/∂x =  k1 cos(x1)cos(y1)    + 0.5(-k2 sin(x2)) sin(y2)
    let dpsi_dy = x1.sin() * (-k1 * y1.sin()) + 0.5 * x2.cos() * (k2 * y2.cos());
    let dpsi_dx = k1 * x1.cos() * y1.cos() + 0.5 * (-k2 * x2.sin()) * y2.sin();
    // Normalize out the spatial-frequency scale so fluid_strength reads in pixels.
    let inv = if settings.fluid_scale != 0.0 {
        1.0 / settings.fluid_scale
    } else {
        0.0
    };
    (dpsi_dy * inv, -dpsi_dx * inv)
}

/// splitmix64 finalizer (matches `coagulate`/`disperse`).
fn hash_u64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn hash01(seed: u64, a: u64, b: u64) -> f32 {
    let h = hash_u64(seed ^ a.wrapping_mul(0x100_0000_01B3) ^ b.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    (h >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgb: [f32; 3]) -> ImageBufferF32 {
        ImageBufferF32::new(
            width,
            height,
            vec![[rgb[0], rgb[1], rgb[2], 1.0]; (width * height) as usize],
        )
        .expect("solid image")
    }

    /// A frame split into four coloured quadrants — same colours appear scattered so
    /// the settle pass has something to group.
    fn quadrants(size: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(size, size, |x, y| {
            let left = x < size / 2;
            let top = y < size / 2;
            match (left, top) {
                (true, true) => [0.9, 0.1, 0.1, 1.0],
                (false, true) => [0.1, 0.9, 0.1, 1.0],
                (true, false) => [0.1, 0.1, 0.9, 1.0],
                (false, false) => [0.9, 0.9, 0.1, 1.0],
            }
        })
        .expect("quadrants")
    }

    /// Mean fraction of each tile's neighbours (within `radius`) that share its
    /// colour bin — a measure of local colour purity. Rises as like-colours
    /// phase-separate into domains. Brute-force O(N²); fine at test sizes.
    fn local_colour_purity(state: &FluidMosaicState, radius: f32) -> f32 {
        let r2 = radius * radius;
        let n = state.positions.len();
        let mut total = 0.0;
        let mut counted = 0.0;
        for i in 0..n {
            let p = state.positions[i];
            let mut same = 0.0;
            let mut all = 0.0;
            for j in 0..n {
                if i == j {
                    continue;
                }
                let q = state.positions[j];
                let dx = p[0] - q[0];
                let dy = p[1] - q[1];
                if dx * dx + dy * dy < r2 {
                    all += 1.0;
                    if state.bins[j] == state.bins[i] {
                        same += 1.0;
                    }
                }
            }
            if all > 0.0 {
                total += same / all;
                counted += 1.0;
            }
        }
        if counted > 0.0 {
            total / counted
        } else {
            0.0
        }
    }

    #[test]
    fn initialize_is_deterministic() {
        let a = quadrants(32);
        let b = quadrants(32);
        let s = FluidMosaicSettings::default();
        let first = initialize_fluid_mosaic(&a, &b, s).expect("first");
        let again = initialize_fluid_mosaic(&a, &b, s).expect("again");
        assert_eq!(first, again);
    }

    /// A frame whose tile colours are spatially scrambled across four hues, so like
    /// colours start scattered and local purity is low — there is something for the
    /// cohesion+repulsion phase separation to sort.
    fn scrambled_colours(size: u32, tile: u32) -> ImageBufferF32 {
        const PALETTE: [[f32; 3]; 4] = [
            [0.9, 0.1, 0.1],
            [0.1, 0.9, 0.1],
            [0.1, 0.1, 0.9],
            [0.9, 0.9, 0.1],
        ];
        ImageBufferF32::from_fn(size, size, |x, y| {
            let cell = (x / tile) ^ ((y / tile).wrapping_mul(7));
            let idx = (hash_u64(u64::from(cell)) % 4) as usize;
            let c = PALETTE[idx];
            [c[0], c[1], c[2], 1.0]
        })
        .expect("scrambled")
    }

    #[test]
    fn settle_groups_like_colours_together() {
        // Like colours start scrambled across the frame; the settle pass (local
        // cohesion + repulsion, the real config) must phase-separate them so each
        // tile sits among more same-colour neighbours — measured at a *local* radius
        // well below the image size.
        let a = scrambled_colours(128, 8);
        let b = scrambled_colours(128, 8);
        let settled = FluidMosaicSettings {
            tile_size: 8,
            settle_iterations: 80,
            fluid_strength: 0.0,
            jitter: 0.0,
            ..FluidMosaicSettings::default()
        };
        let unsettled = FluidMosaicSettings {
            settle_iterations: 0,
            ..settled
        };
        let measure_radius = 14.0;
        let before = local_colour_purity(
            &initialize_fluid_mosaic(&a, &b, unsettled).expect("before"),
            measure_radius,
        );
        let after = local_colour_purity(
            &initialize_fluid_mosaic(&a, &b, settled).expect("after"),
            measure_radius,
        );
        assert!(
            after > before * 1.2,
            "settle should raise local colour purity: before={before}, after={after}"
        );
    }

    #[test]
    fn identity_no_forces_keeps_tiles_on_grid() {
        let a = quadrants(32);
        let b = solid(32, 32, [0.5, 0.5, 0.5]);
        let s = FluidMosaicSettings {
            cohesion: 0.0,
            repulsion: 0.0,
            fluid_strength: 0.0,
            jitter: 0.0,
            settle_iterations: 0,
            ..FluidMosaicSettings::default()
        };
        let initial = initialize_fluid_mosaic(&a, &b, s).expect("initial");
        let advanced = advance_fluid_mosaic(&initial, s, 1).expect("advance");
        assert_eq!(initial.positions, advanced.positions);
    }

    #[test]
    fn fluid_advection_moves_tiles() {
        let a = quadrants(32);
        let b = quadrants(32);
        let s = FluidMosaicSettings {
            cohesion: 0.0,
            repulsion: 0.0,
            fluid_strength: 2.0,
            jitter: 0.0,
            settle_iterations: 0,
            ..FluidMosaicSettings::default()
        };
        let initial = initialize_fluid_mosaic(&a, &b, s).expect("initial");
        let advanced = advance_fluid_mosaic(&initial, s, 1).expect("advance");
        assert!(
            initial
                .positions
                .iter()
                .zip(&advanced.positions)
                .any(|(p, q)| p != q),
            "fluid field should displace tiles"
        );
        // Determinism: same step reproduces byte-identical positions.
        let again = advance_fluid_mosaic(&initial, s, 1).expect("again");
        assert_eq!(advanced.positions, again.positions);
    }

    #[test]
    fn render_paints_tile_colours_over_black() {
        let a = solid(16, 16, [0.2, 0.4, 0.6]);
        let b = solid(16, 16, [0.2, 0.4, 0.6]);
        let s = FluidMosaicSettings {
            cohesion: 0.0,
            fluid_strength: 0.0,
            jitter: 0.0,
            settle_iterations: 0,
            tile_size: 8,
            ..FluidMosaicSettings::default()
        };
        let state = initialize_fluid_mosaic(&a, &b, s).expect("state");
        let frame = render_fluid_mosaic(&state, s).expect("frame");
        // The grid fully tiles the canvas, so a central pixel carries the tile colour.
        let center = frame.pixel(8, 8).expect("center");
        assert!((center[0] - 0.2).abs() < 1e-6);
        assert!((center[1] - 0.4).abs() < 1e-6);
        assert!((center[2] - 0.6).abs() < 1e-6);
    }

    /// A source whose red channel ramps across each tile so within-tile detail exists.
    /// `carry_texture` on must reproduce that ramp; off must paint a flat mean square.
    fn red_ramp(size: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(size, size, |x, _| [(x % 8) as f32 / 7.0, 0.0, 0.0, 1.0])
            .expect("red ramp")
    }

    #[test]
    fn carry_texture_preserves_within_tile_detail() {
        let a = red_ramp(32);
        let b = red_ramp(32);
        let base = FluidMosaicSettings {
            cohesion: 0.0,
            repulsion: 0.0,
            fluid_strength: 0.0,
            jitter: 0.0,
            settle_iterations: 0,
            tile_size: 8,
            ..FluidMosaicSettings::default()
        };
        let textured = FluidMosaicSettings {
            carry_texture: true,
            ..base
        };
        let flat = FluidMosaicSettings {
            carry_texture: false,
            ..base
        };
        // No forces ⇒ tiles stay on the grid; the top-left tile spans x∈[0,8).
        let state = initialize_fluid_mosaic(&a, &b, base).expect("state");
        let tex = render_fluid_mosaic(&state, textured).expect("textured");
        let mean = render_fluid_mosaic(&state, flat).expect("flat");

        // Textured: the ramp survives — left edge dark, right edge bright.
        let tex_left = tex.pixel(0, 0).expect("tex left")[0];
        let tex_right = tex.pixel(7, 0).expect("tex right")[0];
        assert!(tex_left < 0.05, "ramp start should be dark: {tex_left}");
        assert!(tex_right > 0.95, "ramp end should be bright: {tex_right}");

        // Flat: every pixel of the tile is the same mean value (no within-tile detail).
        let flat_left = mean.pixel(0, 0).expect("flat left")[0];
        let flat_right = mean.pixel(7, 0).expect("flat right")[0];
        assert!(
            (flat_left - flat_right).abs() < 1e-6,
            "flat tile must be uniform: {flat_left} vs {flat_right}"
        );
        // Sanity: the flat value is the ramp's mean (≈0.5), distinct from both edges.
        assert!((flat_left - 0.5).abs() < 0.1, "flat ≈ mean: {flat_left}");
    }

    /// A per-pixel black/white checkerboard — maximal within-cell colour variance, so
    /// every cell above the minimum size subdivides fully.
    fn fine_checker(size: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(size, size, |x, y| {
            let v = if (x + y) % 2 == 0 { 1.0 } else { 0.0 };
            [v, v, v, 1.0]
        })
        .expect("checker")
    }

    #[test]
    fn adaptive_tiles_subdivide_only_detailed_regions() {
        let flat = solid(32, 32, [0.3, 0.5, 0.7]);
        let busy = fine_checker(32);
        let base = FluidMosaicSettings {
            adaptive_tiles: true,
            tile_size: 16,
            min_tile_size: 4,
            settle_iterations: 0,
            cohesion: 0.0,
            repulsion: 0.0,
            fluid_strength: 0.0,
            jitter: 0.0,
            ..FluidMosaicSettings::default()
        };

        // Flat sources: variance 0 everywhere ⇒ nothing subdivides, every tile is 16px.
        let flat_state = initialize_fluid_mosaic(&flat, &flat, base).expect("flat");
        assert!(flat_state.sizes.iter().all(|&s| s == 16));
        // Adaptive remains deterministic.
        let again = initialize_fluid_mosaic(&flat, &flat, base).expect("again");
        assert_eq!(flat_state, again);

        // Busy sources: every 16px cell is high-variance ⇒ fully subdivides to 4px.
        // 32/16 = 2×2 = 4 top cells/source, each → (16/4)² = 16 leaves; ×2 sources = 128.
        let busy_state = initialize_fluid_mosaic(&busy, &busy, base).expect("busy");
        assert!(busy_state.sizes.iter().all(|&s| s == 4));
        assert_eq!(busy_state.sizes.len(), 128);
        assert!(busy_state.sizes.len() > flat_state.sizes.len());

        // Off ⇒ uniform 16px tiles regardless of content (the v2 formulation).
        let off = FluidMosaicSettings {
            adaptive_tiles: false,
            ..base
        };
        let off_state = initialize_fluid_mosaic(&busy, &busy, off).expect("off");
        assert!(off_state.sizes.iter().all(|&s| s == 16));
        assert_eq!(off_state.sizes.len(), 8);
    }

    #[test]
    fn live_refresh_repaints_without_disturbing_sim() {
        // Seed grey A + grey B, advance a few frames so positions/velocities are
        // non-trivial, then refresh from differently-coloured frames.
        let a0 = solid(16, 16, [0.2, 0.2, 0.2]);
        let b0 = solid(16, 16, [0.2, 0.2, 0.2]);
        let s = FluidMosaicSettings {
            tile_size: 8,
            settle_iterations: 4,
            ..FluidMosaicSettings::default()
        };
        let mut state = initialize_fluid_mosaic(&a0, &b0, s).expect("seed");
        state = advance_fluid_mosaic(&state, s, 1).expect("advance");
        let before = state.clone();

        // Refresh from a red A frame + blue B frame: painted colours must update, but
        // positions / bins (sorting) / sizes / origin (the simulation) must not.
        let a1 = solid(16, 16, [0.8, 0.1, 0.1]);
        let b1 = solid(16, 16, [0.1, 0.1, 0.8]);
        refresh_fluid_mosaic_colors(&mut state, &a1, &b1).expect("refresh");
        assert_ne!(state.colors, before.colors, "refresh should repaint colours");
        assert_eq!(state.positions, before.positions, "sim positions unchanged");
        assert_eq!(state.velocities, before.velocities, "sim velocities unchanged");
        assert_eq!(state.bins, before.bins, "sorting bins frozen");
        assert_eq!(state.sizes, before.sizes, "sizes unchanged");
        assert_eq!(state.origin, before.origin, "origin cells unchanged");
        // A tiles (first 4 for a 16px/8 grid) take A's colour, B tiles take B's.
        assert!((state.colors[0][0] - 0.8).abs() < 1e-6 && state.colors[0][2] < 0.2);
        assert!(state.colors[4][2] > 0.7 && state.colors[4][0] < 0.2);

        // Refreshing back to the seed frames restores byte-identical colours + patches.
        refresh_fluid_mosaic_colors(&mut state, &a0, &b0).expect("refresh back");
        assert_eq!(state.colors, before.colors);
        assert_eq!(state.patches, before.patches);

        // A mismatched frame size is rejected.
        let wrong = solid(8, 8, [0.0, 0.0, 0.0]);
        assert!(refresh_fluid_mosaic_colors(&mut state, &wrong, &b0).is_err());
    }

    #[test]
    fn resort_rebins_so_cohesion_follows_the_live_colour() {
        // Seed grey A + grey B, advance a frame so the sim is non-trivial.
        let a0 = solid(16, 16, [0.2, 0.2, 0.2]);
        let b0 = solid(16, 16, [0.2, 0.2, 0.2]);
        let s = FluidMosaicSettings {
            tile_size: 8,
            settle_iterations: 4,
            ..FluidMosaicSettings::default()
        };
        let mut state = initialize_fluid_mosaic(&a0, &b0, s).expect("seed");
        state = advance_fluid_mosaic(&state, s, 1).expect("advance");
        let before = state.clone();

        // Re-sort from a red A frame + blue B frame: colours AND bins must update (so
        // the next advance sorts on the new colours), while positions / velocities carry
        // forward unchanged.
        let a1 = solid(16, 16, [0.8, 0.1, 0.1]);
        let b1 = solid(16, 16, [0.1, 0.1, 0.8]);
        resort_fluid_mosaic_colors(&mut state, &a1, &b1).expect("resort");
        assert_ne!(state.colors, before.colors, "resort should repaint colours");
        assert_ne!(state.bins, before.bins, "resort should re-bin tiles");
        assert_eq!(state.positions, before.positions, "sim positions carry forward");
        assert_eq!(state.velocities, before.velocities, "sim velocities carry forward");
        // The new bins match the new colours (red A tiles vs blue B tiles diverge).
        assert_eq!(state.bins[0], color_bin([0.8, 0.1, 0.1], s.color_bins));
        assert_eq!(state.bins[4], color_bin([0.1, 0.1, 0.8], s.color_bins));
        assert_ne!(state.bins[0], state.bins[4], "A and B now sort into different groups");

        // Determinism: re-sorting the same `before` state with the same frames reproduces
        // byte-identical bins and colours.
        let mut again = before.clone();
        resort_fluid_mosaic_colors(&mut again, &a1, &b1).expect("resort again");
        assert_eq!(again.bins, state.bins);
        assert_eq!(again.colors, state.colors);

        // Re-sort diverges from the render-only refresh: refresh freezes the bins, so a
        // subsequent advance follows a different trajectory than the re-sorted one.
        let mut refreshed = before.clone();
        refresh_fluid_mosaic_colors(&mut refreshed, &a1, &b1).expect("refresh");
        assert_eq!(refreshed.bins, before.bins, "refresh keeps bins frozen");
        let resort_next = advance_fluid_mosaic(&state, s, 2).expect("resort advance");
        let refresh_next = advance_fluid_mosaic(&refreshed, s, 2).expect("refresh advance");
        assert_ne!(
            resort_next.positions, refresh_next.positions,
            "re-sorted bins must steer cohesion differently than frozen bins"
        );
    }

    #[test]
    fn cluster_blob_pulls_same_colour_to_global_centroid() {
        // Two same-colour tiles placed far apart (beyond cohesion_radius) with no other
        // tiles and repulsion off. Local cohesion can't see across the gap → ~zero
        // force; cluster-blob pulls each toward their shared global centroid (the
        // midpoint), so the colour gathers into one blob regardless of separation.
        let red = [0.9_f32, 0.1, 0.1];
        let bin = color_bin(red, 5);
        let state = FluidMosaicState {
            width: 200,
            height: 200,
            tile_size: 8,
            color_bins: 5,
            positions: vec![[20.0, 100.0], [180.0, 100.0]],
            velocities: vec![[0.0, 0.0]; 2],
            colors: vec![red; 2],
            patches: vec![
                TilePatch {
                    width: 0,
                    height: 0,
                    pixels: Vec::new(),
                };
                2
            ],
            bins: vec![bin; 2],
            sizes: vec![8; 2],
            origin: vec![
                TileOrigin {
                    source: 0,
                    x0: 0,
                    y0: 0,
                    x1: 8,
                    y1: 8,
                };
                2
            ],
        };
        let base = FluidMosaicSettings {
            cohesion: 0.05,
            cohesion_radius: 24.0,
            repulsion: 0.0,
            ..FluidMosaicSettings::default()
        };

        // Local cohesion: 160px apart ≫ radius 24 ⇒ neither tile sees the other ⇒ no pull.
        let local = neighbor_forces(
            &state,
            FluidMosaicSettings {
                cluster_blob: false,
                ..base
            },
        );
        assert!(
            local[0][0].abs() < 1e-6 && local[1][0].abs() < 1e-6,
            "local cohesion must not reach across the gap: {local:?}"
        );

        // Cluster-blob: centroid x = 100, so the left tile (x=20) pulls +x and the right
        // (x=180) pulls −x, by 80·0.05 = 4.0 each (no y component — both at y=100).
        let blob = neighbor_forces(
            &state,
            FluidMosaicSettings {
                cluster_blob: true,
                ..base
            },
        );
        assert!((blob[0][0] - 4.0).abs() < 1e-4, "left tile pulled to centre: {blob:?}");
        assert!((blob[1][0] + 4.0).abs() < 1e-4, "right tile pulled to centre: {blob:?}");
        assert!(blob[0][1].abs() < 1e-6 && blob[1][1].abs() < 1e-6, "no y pull: {blob:?}");

        // Determinism.
        let again = neighbor_forces(
            &state,
            FluidMosaicSettings {
                cluster_blob: true,
                ..base
            },
        );
        assert_eq!(blob, again);
    }
}
