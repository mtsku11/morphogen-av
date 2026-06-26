//! Persistent-trail vector-field cascade — a grid of source-image tiles advected along the
//! shared steady faux-fluid field ([`crate::vortex_field`]), each **stamping its crop every
//! frame onto a persistent canvas that is never cleared** (last-writer-wins). The image smears
//! into ribbons that trace the field's streamlines — the look of the Windows Solitaire win
//! cascade generalised to the whole frame and driven by a vector field instead of bounce physics.
//!
//! It composes two existing pieces with one new one:
//! - **Motion**: the same [`steady_vortex_velocity`](crate::vortex_field::steady_vortex_velocity)
//!   field that [`crate::field_particles`] rides. The big-vortex octave is steady, so trails
//!   follow consistent streamlines; we pass `time = 0` for a fully steady field.
//! - **Patch-carrying tiles**: each tile carries the source [`TilePatch`] of its origin cell
//!   (via [`sample_cell`]), so the cascade paints footage texture, not a flat colour. Like
//!   [`crate::fluid_mosaic`]'s live refresh, the patch is re-sampled from the *current* source
//!   frame each step (opt-in) so motion plays through the trails.
//! - **The new bit**: a persistent RGBA32F accumulator. Each frame the tiles are stamped onto
//!   it without clearing, so older copies survive as trails downstream of the flow.
//!
//! `grid_spacing == tile_size` tiles the frame densely (the whole image smears); `grid_spacing
//! > tile_size` seeds a *sparse* grid whose tiles leave distinct ribbons on black.
//!
//! Stateful temporal node: frame zero is the seeded grid stamped onto a black canvas (the
//! [`CascadeTrailState`] — positions + patches + accumulator — is the checkpoint, never a
//! re-read PNG); each later frame advances positions, optionally refreshes patches, and stamps.
//! Deterministic: the field hashing is splitmix64 and the stamp order is fixed (tile index).
//! CPU-only by design — cross-frame last-writer-wins accumulation is parity-hostile on the GPU
//! (same reasoning as [`crate::fluid_mosaic`]). v1 renders the whole sequence in one pass
//! (in-memory state); an on-disk resumable RGBA32F checkpoint (the [`crate::feedback_state`]
//! pattern) is a future slice.
//!
//! Continuity identity (the off case for an off-vs-on readout): `advect == 0.0` ⇒ the tiles
//! never move, so with `live_refresh` off every frame stamps the same patches at the same spots
//! and frames `1..n` are byte-identical to the frame-zero stamp.

use serde::{Deserialize, Serialize};

use crate::fluid_mosaic::sample_cell;
use crate::vortex_field::steady_vortex_velocity;
use crate::{ImageBufferF32, RenderError, TileOrigin, TilePatch};

/// Algorithm identifier for the CPU reference. Bump when the integration scheme, the field
/// model, the grid layout, or the stamp/accumulation changes so stale caches invalidate.
pub const CASCADE_TRAIL_ALGORITHM: &str = "persistent_trail_vortex_cascade_cpu_v1";

/// Settings for the persistent-trail vector-field cascade.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CascadeTrailSettings {
    /// Edge length (pixels) of each stamped tile / source patch.
    pub tile_size: u32,
    /// Spacing (pixels) between tile homes. `== tile_size` ⇒ dense (whole image smears);
    /// `> tile_size` ⇒ sparse tiles that leave distinct ribbons on black.
    pub grid_spacing: u32,
    /// Field strength applied to the velocity per frame (pixels). `0` holds the tiles on the
    /// static grid (no trails); higher ⇒ longer ribbons each step.
    pub advect: f32,
    /// Vortex frequency of the field (lattice cells per pixel). Smaller ⇒ larger vortices.
    pub turbulence_scale: f32,
    /// Weight of the fine detail octave relative to the steady big vortices (`0` = pure large
    /// vortices).
    pub detail: f32,
    /// When `true`, each tile's patch is re-sampled from its origin cell in the current source
    /// frame every frame so the video plays through the trails. When `false`, patches stay
    /// frozen at seed time.
    #[serde(default)]
    pub live_refresh: bool,
    /// Seed for the deterministic field.
    pub seed: u64,
}

impl Default for CascadeTrailSettings {
    fn default() -> Self {
        // The tuned sparse-ribbon "preset" the macOS Trail Cascade card ships with.
        Self {
            tile_size: 28,
            grid_spacing: 60,
            advect: 1.6,
            turbulence_scale: 0.008,
            detail: 0.1,
            live_refresh: true,
            seed: 0,
        }
    }
}

impl CascadeTrailSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.tile_size == 0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "tile_size must be greater than zero".to_string(),
            ));
        }
        if self.grid_spacing == 0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "grid_spacing must be greater than zero".to_string(),
            ));
        }
        for (name, value) in [
            ("advect", self.advect),
            ("turbulence_scale", self.turbulence_scale),
            ("detail", self.detail),
        ] {
            if !value.is_finite() {
                return Err(RenderError::InvalidCoagulationSettings(format!(
                    "{name} must be finite"
                )));
            }
        }
        if self.detail < 0.0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "detail must be non-negative".to_string(),
            ));
        }
        Ok(())
    }
}

/// The stateful cascade — the checkpoint representation. `positions`, `origins`, and `patches`
/// are parallel, index-aligned, and fixed in order (so the stamp is deterministic). The
/// `accumulator` is the persistent canvas, never cleared between frames.
#[derive(Debug, Clone, PartialEq)]
pub struct CascadeTrailState {
    width: u32,
    height: u32,
    tile_size: u32,
    /// Current tile top-left positions in pixels (advected; may leave the canvas — stamps clip).
    positions: Vec<[f32; 2]>,
    /// Per-tile origin cell (for the live patch refresh). Index-aligned.
    origins: Vec<TileOrigin>,
    /// Per-tile current source patch (refreshed from the live frame when `live_refresh`).
    patches: Vec<TilePatch>,
    /// The persistent RGBA32F canvas (opaque black where untouched).
    accumulator: ImageBufferF32,
}

impl CascadeTrailState {
    /// Number of tiles in the cascade grid.
    pub fn tile_count(&self) -> usize {
        self.positions.len()
    }

    /// Canvas dimensions `(width, height)`.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Seed the cascade grid from the source frame (frame zero): one tile per `grid_spacing` cell,
/// each carrying the `tile_size` patch of its origin, positioned at its home, then stamped onto
/// a fresh black accumulator. The accumulator now holds the frame-zero image.
pub fn initialize_cascade_trails(
    source: &ImageBufferF32,
    settings: CascadeTrailSettings,
) -> Result<CascadeTrailState, RenderError> {
    settings.validate()?;

    let width = source.width;
    let height = source.height;
    let mut positions = Vec::new();
    let mut origins = Vec::new();
    let mut patches = Vec::new();

    let mut gy = 0u32;
    while gy < height {
        let mut gx = 0u32;
        while gx < width {
            let x1 = (gx + settings.tile_size).min(width);
            let y1 = (gy + settings.tile_size).min(height);
            let (_, patch) = sample_cell(source, gx, gy, x1, y1);
            positions.push([gx as f32, gy as f32]);
            origins.push(TileOrigin {
                source: 1,
                x0: gx,
                y0: gy,
                x1,
                y1,
            });
            patches.push(patch);
            gx += settings.grid_spacing;
        }
        gy += settings.grid_spacing;
    }

    let accumulator = ImageBufferF32::from_fn(width, height, |_, _| [0.0, 0.0, 0.0, 1.0])?;
    let mut state = CascadeTrailState {
        width,
        height,
        tile_size: settings.tile_size,
        positions,
        origins,
        patches,
        accumulator,
    };
    stamp_all(&mut state);
    Ok(state)
}

/// Advance one frame: advect every tile along the steady field (`p ← p + v(p) · advect`), then
/// — when `live_refresh` — re-sample each tile's patch from its origin cell in `current_source`,
/// then stamp every tile onto the persistent accumulator (last-writer-wins, fixed index order).
pub fn advance_cascade_trails(
    state: &mut CascadeTrailState,
    current_source: &ImageBufferF32,
    settings: CascadeTrailSettings,
) -> Result<(), RenderError> {
    settings.validate()?;

    if settings.advect != 0.0 {
        // A fully steady field (`time = 0`): the big-vortex octave is fixed, so trails follow
        // consistent streamlines instead of wobbling.
        for pos in &mut state.positions {
            let (vx, vy) = steady_vortex_velocity(
                settings.seed,
                pos[0],
                pos[1],
                0.0,
                settings.turbulence_scale,
                settings.detail,
            );
            pos[0] += vx * settings.advect;
            pos[1] += vy * settings.advect;
        }
    }

    if settings.live_refresh {
        for (patch, origin) in state.patches.iter_mut().zip(state.origins.iter()) {
            let (_, fresh) = sample_cell(current_source, origin.x0, origin.y0, origin.x1, origin.y1);
            *patch = fresh;
        }
    }

    stamp_all(state);
    Ok(())
}

/// Render the current frame: a copy of the persistent accumulator.
pub fn render_cascade_trails(state: &CascadeTrailState) -> ImageBufferF32 {
    state.accumulator.clone()
}

/// Stamp every tile's patch onto the accumulator in fixed index order (last writer wins).
fn stamp_all(state: &mut CascadeTrailState) {
    let width = state.width as i64;
    let height = state.height as i64;
    let tile = state.tile_size as i64;
    let pixels = &mut state.accumulator.pixels;
    for (pos, patch) in state.positions.iter().zip(state.patches.iter()) {
        let left = pos[0].round() as i64;
        let top = pos[1].round() as i64;
        let pw = patch.width as i64;
        let ph = patch.height as i64;
        if pw == 0 || ph == 0 {
            continue;
        }
        for dy in 0..tile {
            let y = top + dy;
            if y < 0 || y >= height {
                continue;
            }
            // Nearest-sample the tile offset into the patch's own dimensions (edge cells may be
            // smaller than tile_size). Mirrors the fluid_mosaic carry_texture paint.
            let py = (dy * ph / tile).clamp(0, ph - 1);
            let row = (y as usize) * (state.width as usize);
            for dx in 0..tile {
                let x = left + dx;
                if x < 0 || x >= width {
                    continue;
                }
                let px = (dx * pw / tile).clamp(0, pw - 1);
                let rgb = patch.pixels[(py * pw + px) as usize];
                pixels[row + x as usize] = [rgb[0], rgb[1], rgb[2], 1.0];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small synthetic colour-gradient source so tiles carry distinguishable patches.
    fn gradient_source(width: u32, height: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |x, y| {
            [
                x as f32 / width as f32,
                y as f32 / height as f32,
                0.5,
                1.0,
            ]
        })
        .unwrap()
    }

    fn count_non_black(image: &ImageBufferF32) -> usize {
        image
            .pixels
            .iter()
            .filter(|p| p[0] != 0.0 || p[1] != 0.0 || p[2] != 0.0)
            .count()
    }

    #[test]
    fn tile_count_matches_grid_spacing() {
        let source = gradient_source(64, 64);
        let settings = CascadeTrailSettings {
            tile_size: 8,
            grid_spacing: 16,
            ..CascadeTrailSettings::default()
        };
        let state = initialize_cascade_trails(&source, settings).unwrap();
        // 64 / 16 = 4 homes per axis.
        assert_eq!(state.tile_count(), 4 * 4);
    }

    #[test]
    fn frame_zero_stamps_sparse_grid_on_black() {
        let source = gradient_source(64, 64);
        let settings = CascadeTrailSettings {
            tile_size: 8,
            grid_spacing: 32, // sparse: 8px tiles every 32px ⇒ gaps stay black
            advect: 1.0,
            ..CascadeTrailSettings::default()
        };
        let state = initialize_cascade_trails(&source, settings).unwrap();
        let frame0 = render_cascade_trails(&state);
        let non_black = count_non_black(&frame0);
        // 2x2 tiles of 8x8 = 256 painted pixels, far less than the 4096-pixel canvas.
        assert_eq!(non_black, 2 * 2 * 8 * 8);
        assert!(non_black < (64 * 64) as usize);
    }

    #[test]
    fn advect_zero_is_static_after_frame_zero() {
        let source = gradient_source(48, 48);
        let settings = CascadeTrailSettings {
            tile_size: 8,
            grid_spacing: 16,
            advect: 0.0,
            live_refresh: false,
            ..CascadeTrailSettings::default()
        };
        let mut state = initialize_cascade_trails(&source, settings).unwrap();
        let frame0 = render_cascade_trails(&state);
        for _ in 0..5 {
            advance_cascade_trails(&mut state, &source, settings).unwrap();
            let frame = render_cascade_trails(&state);
            assert_eq!(frame.pixels, frame0.pixels, "advect 0 must hold the frame");
        }
    }

    #[test]
    fn render_is_deterministic() {
        let source = gradient_source(48, 48);
        let settings = CascadeTrailSettings {
            tile_size: 6,
            grid_spacing: 10,
            advect: 2.0,
            ..CascadeTrailSettings::default()
        };
        let run = || {
            let mut state = initialize_cascade_trails(&source, settings).unwrap();
            let mut frames = vec![render_cascade_trails(&state)];
            for _ in 0..6 {
                advance_cascade_trails(&mut state, &source, settings).unwrap();
                frames.push(render_cascade_trails(&state));
            }
            frames
        };
        let a = run();
        let b = run();
        for (fa, fb) in a.iter().zip(b.iter()) {
            assert_eq!(fa.pixels, fb.pixels, "re-render must be byte-identical");
        }
    }

    #[test]
    fn accumulator_is_monotonic() {
        // Trails only ever ADD paint to the persistent canvas: the painted-pixel set grows.
        let source = gradient_source(64, 64);
        let settings = CascadeTrailSettings {
            tile_size: 6,
            grid_spacing: 12,
            advect: 2.5,
            live_refresh: true,
            ..CascadeTrailSettings::default()
        };
        let mut state = initialize_cascade_trails(&source, settings).unwrap();
        let mut prev = count_non_black(&render_cascade_trails(&state));
        let mut grew = false;
        for _ in 0..10 {
            advance_cascade_trails(&mut state, &source, settings).unwrap();
            let now = count_non_black(&render_cascade_trails(&state));
            assert!(now >= prev, "accumulator must never lose painted pixels");
            grew |= now > prev;
            prev = now;
        }
        assert!(grew, "with advect > 0 the trails should grow the painted area");
    }
}
