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
//! This is **Slice 1**: a deterministic CPU reference. Tiles are uniform-size and
//! carry each cell's mean colour (texture patches and varying tile sizes are
//! deferred); the simulation is seeded from the first frame of each source and runs
//! self-contained (live per-frame colour refresh is deferred). Metal is a later
//! slice. Determinism: splitmix64 hashing, fixed-timestep integration, no wall clock.
//!
//! Continuity identity (the off case for off-vs-on readout): with `cohesion`,
//! `fluid_strength`, and `jitter` all `0` and `settle_iterations == 0`, every tile
//! stays at its grid centre, so the render is the two source grids overlaid verbatim.

use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

/// Algorithm identifier for the Slice-1 CPU reference. Bump when the force model or
/// tile formulation changes so stale caches/checkpoints invalidate.
pub const FLUID_MOSAIC_ALGORITHM: &str = "fluid_mosaic_colour_sort_cpu_v1";

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
    /// Seed for the deterministic per-tile hashes and the fluid field phase.
    pub seed: u64,
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
    /// Fixed per-tile mean colour (RGB).
    pub colors: Vec<[f32; 3]>,
    /// Fixed per-tile colour bin index in `0..color_bins^3`.
    pub bins: Vec<u32>,
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

    let mut positions = Vec::new();
    let mut colors = Vec::new();
    let mut bins = Vec::new();
    for source in [source_a, source_b] {
        append_source_tiles(source, tile, settings.color_bins, &mut positions, &mut colors, &mut bins);
    }
    let velocities = vec![[0.0_f32, 0.0]; positions.len()];

    let mut state = FluidMosaicState {
        width,
        height,
        tile_size: tile,
        color_bins: settings.color_bins,
        positions,
        velocities,
        colors,
        bins,
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
        bins: state.bins.clone(),
        ..*state
    })
}

/// Render the current particle set as crisp colour tiles. Each tile paints a
/// `tile_size`×`tile_size` opaque square centred on its (rounded) position; tiles are
/// painted in fixed index order (painter's algorithm), so later tiles overwrite
/// earlier ones. Uncovered pixels stay opaque black.
pub fn render_fluid_mosaic(
    state: &FluidMosaicState,
    settings: FluidMosaicSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let width = state.width;
    let height = state.height;
    let half = (state.tile_size as i64) / 2;
    let mut pixels = vec![[0.0_f32, 0.0, 0.0, 1.0]; (width as usize) * (height as usize)];

    for (i, pos) in state.positions.iter().enumerate() {
        let color = state.colors[i];
        let cx = pos[0].round() as i64;
        let cy = pos[1].round() as i64;
        let x0 = (cx - half).max(0);
        let y0 = (cy - half).max(0);
        let x1 = (cx - half + state.tile_size as i64).min(width as i64);
        let y1 = (cy - half + state.tile_size as i64).min(height as i64);
        for y in y0..y1 {
            let row = (y as usize) * (width as usize);
            for x in x0..x1 {
                pixels[row + x as usize] = [color[0], color[1], color[2], 1.0];
            }
        }
    }

    ImageBufferF32::new(width, height, pixels)
}

/// Append one source's tiles (mean colour per `tile`-sized cell, centre position,
/// colour bin) to the parallel state vectors.
fn append_source_tiles(
    source: &ImageBufferF32,
    tile: u32,
    color_bins: u32,
    positions: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 3]>,
    bins: &mut Vec<u32>,
) {
    let cols = source.width.div_ceil(tile);
    let rows = source.height.div_ceil(tile);
    for cy in 0..rows {
        for cx in 0..cols {
            let x0 = cx * tile;
            let y0 = cy * tile;
            let x1 = (x0 + tile).min(source.width);
            let y1 = (y0 + tile).min(source.height);
            let mut sum = [0.0_f32; 3];
            let mut count = 0.0_f32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let px = source.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 1.0]);
                    sum[0] += px[0];
                    sum[1] += px[1];
                    sum[2] += px[2];
                    count += 1.0;
                }
            }
            let mean = if count > 0.0 {
                [sum[0] / count, sum[1] / count, sum[2] / count]
            } else {
                [0.0, 0.0, 0.0]
            };
            positions.push([
                (x0 + x1) as f32 * 0.5,
                (y0 + y1) as f32 * 0.5,
            ]);
            colors.push(mean);
            bins.push(color_bin(mean, color_bins));
        }
    }
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

/// Per-tile neighbour force = local same-colour **cohesion** (pull toward the mean
/// position of nearby same-bin tiles) plus colour-blind short-range **repulsion**.
/// A uniform spatial-hash grid (cell = `cohesion_radius`, the larger radius) keeps
/// this O(N · local density) rather than O(N²): each tile only tests neighbours in
/// its own and the eight adjacent cells. Exactly-coincident tiles (common at frame
/// zero, where A's and B's grids overlap) are separated along a deterministic
/// per-tile hashed direction.
fn neighbor_forces(state: &FluidMosaicState, settings: FluidMosaicSettings) -> Vec<[f32; 2]> {
    let n = state.positions.len();
    let cohesion_on = settings.cohesion > 0.0 && settings.cohesion_radius > 0.0;
    let repulsion_on = settings.repulsion > 0.0 && settings.repulsion_radius > 0.0;
    if !cohesion_on && !repulsion_on {
        return vec![[0.0, 0.0]; n];
    }

    let radius = settings
        .cohesion_radius
        .max(settings.repulsion_radius)
        .max(1.0);
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

                    if repulsion_on && d2 < rep_r2 {
                        if d2 <= 1e-12 {
                            // Coincident: push along a deterministic hashed direction.
                            let angle = hash01(settings.seed, i as u64, j as u64) * TAU;
                            rep[0] += angle.cos() * settings.repulsion;
                            rep[1] += angle.sin() * settings.repulsion;
                        } else {
                            let dist = d2.sqrt();
                            let falloff = 1.0 - dist / rep_r;
                            rep[0] += (dx / dist) * settings.repulsion * falloff;
                            rep[1] += (dy / dist) * settings.repulsion * falloff;
                        }
                    }

                    if cohesion_on && state.bins[j] == bin && d2 < coh_r2 {
                        coh_sum[0] += q[0];
                        coh_sum[1] += q[1];
                        coh_count += 1.0;
                    }
                }
            }
        }

        let mut ax = rep[0];
        let mut ay = rep[1];
        if coh_count > 0.0 {
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
}
