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

/// Which velocity field drives tile advection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CascadeFieldType {
    /// Curl-noise steady vortex field (the original mode).
    #[default]
    Vortex,
    /// Dominant uniform flow plus per-tile oscillating turbulence on the tile's current
    /// position — jitter compounds over time so trails curve and wander.
    River,
    /// Dominant uniform flow with oscillation applied to the *stamp position* relative to
    /// the tile's home. The drift is a straight flow; the oscillation is always centred on
    /// the drift line, so each ribbon traces a clean sinusoidal wave. The "root" of every
    /// ribbon bobs at its own rate — the look of kelp or river-grass in a current.
    RiverRoot,
    /// Tiles to the left of the canvas centre drift left; tiles to the right drift right.
    /// Oscillation is vertical (perpendicular to horizontal flow), applied at stamp time only
    /// so each ribbon traces a sinusoidal wave outward from the centre line. The effect
    /// mirrors `RiverRoot` about the vertical axis — like two rivers parting from a divide.
    CenterSplit,
    /// Tiles never drift — each oscillates in place around its home in both x and y with
    /// unique per-tile frequencies and phases derived deterministically from the home position.
    /// The persistent accumulator builds up the swept path so the cascade trail IS the
    /// oscillation history. Amplitude is `river_turbulence` (pixels); `advect` scales it.
    /// Pairs naturally with `temporal_tiles`: each tile carries a frozen temporal slice and
    /// paints it along its own 2-D Lissajous-like figure.
    Oscillate,
    /// Tiles appear at a new random position on the canvas each frame — no drift, no home.
    /// Only the square **outline** (border pixels) is stamped; the interior stays transparent.
    /// Because the accumulator never clears, the canvas fills over time with a growing field
    /// of scattered wireframe squares, each coloured from its source patch. `grid_spacing`
    /// controls density (more tiles = denser field). `advect` is an on/off gate: `0` freezes
    /// the initial random layout, any non-zero value re-randomises positions every frame.
    SquarePop,
}

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
    /// Which velocity field type to use.
    #[serde(default)]
    pub field: CascadeFieldType,
    /// River mode: flow direction in degrees (0 = right, 90 = down, 180 = left, 270 = up).
    #[serde(default)]
    pub river_direction: f32,
    /// River mode: base flow speed in pixels per frame.
    #[serde(default = "default_river_speed")]
    pub river_speed: f32,
    /// River mode: per-tile turbulence amplitude (pixels). Each tile gets a deterministic
    /// jitter offset derived from its home position, so nearby tiles drift similarly while
    /// distant tiles diverge — like water turbulence.
    #[serde(default = "default_river_turbulence")]
    pub river_turbulence: f32,
    /// When `true`, tiles are assigned distinct source frames at init rather than all sharing
    /// the seed frame. Tile index is spread evenly across the clip so the grid becomes a
    /// temporal slit-scan: different tiles carry different moments of the video and the drifting
    /// rivers interweave them. `live_refresh` is ignored when this is active — patches are
    /// captured once and held frozen forever.
    #[serde(default)]
    pub temporal_tiles: bool,
    /// Fraction of accumulator brightness lost each frame before new tiles are stamped.
    /// `0.0` (default) gives permanent trails. `0.08` fades a square to near-black in
    /// ~25 frames (≈1 s at 24 fps) — useful with `SquarePop` to keep visual density
    /// stable instead of filling the canvas solid.
    #[serde(default)]
    pub decay: f32,
}

fn default_river_speed() -> f32 {
    3.0
}

fn default_river_turbulence() -> f32 {
    0.8
}

impl Default for CascadeTrailSettings {
    fn default() -> Self {
        Self {
            tile_size: 28,
            grid_spacing: 60,
            advect: 1.6,
            turbulence_scale: 0.008,
            detail: 0.1,
            live_refresh: true,
            seed: 0,
            field: CascadeFieldType::Vortex,
            river_direction: 0.0,
            river_speed: default_river_speed(),
            river_turbulence: default_river_turbulence(),
            temporal_tiles: false,
            decay: 0.0,
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
            ("river_direction", self.river_direction),
            ("river_speed", self.river_speed),
            ("river_turbulence", self.river_turbulence),
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
        if self.decay < 0.0 || self.decay >= 1.0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "decay must be in [0, 1)".to_string(),
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

    // SquarePop: scatter initial positions. When river_turbulence > 0, tiles jitter within
    // ±turbulence px of their home cell; when turbulence == 0 they stay exactly at home.
    // Full-canvas scatter (the original behaviour) requires turbulence >= canvas diagonal.
    if settings.field == CascadeFieldType::SquarePop {
        let max_x = width.saturating_sub(settings.tile_size) as f32;
        let max_y = height.saturating_sub(settings.tile_size) as f32;
        let radius = settings.river_turbulence;
        for (pos, origin) in positions.iter_mut().zip(origins.iter()) {
            let h0 = tile_hash(settings.seed, origin.x0, origin.y0);
            let h1 = splitmix(h0);
            if radius == 0.0 {
                // No jitter: start exactly at the home grid position.
                // (pos is already set to [gx, gy] from the grid loop above)
            } else {
                let ox = ((h0 & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0) * radius;
                let oy = ((h1 & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0) * radius;
                pos[0] = (origin.x0 as f32 + ox).clamp(0.0, max_x);
                pos[1] = (origin.y0 as f32 + oy).clamp(0.0, max_y);
            }
        }
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
    if settings.field == CascadeFieldType::SquarePop {
        stamp_outlines(&mut state);
    } else {
        stamp_all(&mut state);
    }
    Ok(state)
}

/// Repaint each tile's patch from a different source frame, spreading all tiles evenly across
/// the clip. Tile 0 gets frame 0, tile N−1 gets the last frame, intermediate tiles get
/// proportionally intermediate frames. Call once after `initialize_cascade_trails` when
/// `temporal_tiles` is enabled; patches stay frozen after this — do NOT call with `live_refresh`.
pub fn assign_temporal_patches(
    state: &mut CascadeTrailState,
    frames: &[ImageBufferF32],
) {
    let n_tiles = state.patches.len();
    let n_frames = frames.len();
    if n_tiles == 0 || n_frames == 0 {
        return;
    }
    for (i, (patch, origin)) in state
        .patches
        .iter_mut()
        .zip(state.origins.iter())
        .enumerate()
    {
        let frame_idx = (i * n_frames) / n_tiles;
        let (_, fresh) =
            sample_cell(&frames[frame_idx], origin.x0, origin.y0, origin.x1, origin.y1);
        *patch = fresh;
    }
    // Re-stamp the accumulator with the newly assigned patches so frame 0 already shows the
    // temporal spread rather than waiting until the first advance.
    stamp_all(state);
}

/// Advance one frame: advect every tile along the steady field (`p ← p + v(p) · advect`), then
/// — when `live_refresh` — re-sample each tile's patch from its origin cell in `current_source`,
/// then stamp every tile onto the persistent accumulator (last-writer-wins, fixed index order).
/// `frame` is the 1-based frame index (first call = frame 1) used by river mode to drive
/// time-varying oscillation.
pub fn advance_cascade_trails(
    state: &mut CascadeTrailState,
    current_source: &ImageBufferF32,
    settings: CascadeTrailSettings,
    frame: u32,
) -> Result<(), RenderError> {
    settings.validate()?;

    // Advance positions and optionally produce per-tile stamp offsets.  RiverRoot is the
    // only mode that separates drift (accumulated on positions) from oscillation (applied
    // only at stamp time), so it returns offsets; the other modes return None.
    let stamp_offsets: Option<Vec<[f32; 2]>> = if settings.advect != 0.0 {
        match settings.field {
            CascadeFieldType::Vortex => {
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
                None
            }
            CascadeFieldType::River => {
                let angle = settings.river_direction.to_radians();
                let base_vx = angle.cos() * settings.river_speed;
                let base_vy = angle.sin() * settings.river_speed;
                for (pos, origin) in state.positions.iter_mut().zip(state.origins.iter()) {
                    let (jx, jy) = river_jitter(
                        settings.seed,
                        origin.x0,
                        origin.y0,
                        settings.river_turbulence,
                        angle,
                        frame,
                    );
                    pos[0] += (base_vx + jx) * settings.advect;
                    pos[1] += (base_vy + jy) * settings.advect;
                }
                None
            }
            CascadeFieldType::RiverRoot => {
                let angle = settings.river_direction.to_radians();
                let (sin_a, cos_a) = angle.sin_cos();
                let base_vx = cos_a * settings.river_speed;
                let base_vy = sin_a * settings.river_speed;
                // Advance by pure steady flow — no oscillation accumulates in positions.
                for pos in &mut state.positions {
                    pos[0] += base_vx * settings.advect;
                    pos[1] += base_vy * settings.advect;
                }
                // Oscillation is centred on the home, applied only at stamp time.
                let offsets = state
                    .origins
                    .iter()
                    .map(|origin| {
                        let (jx, jy) = river_jitter(
                            settings.seed,
                            origin.x0,
                            origin.y0,
                            settings.river_turbulence,
                            angle,
                            frame,
                        );
                        [jx, jy]
                    })
                    .collect();
                Some(offsets)
            }
            CascadeFieldType::CenterSplit => {
                let cx = state.width as f32 / 2.0;
                let amplitude = settings.river_turbulence;
                let t = frame as f32;
                // Position is computed directly from home + drift(frame) + oscillation(frame)
                // rather than accumulated incrementally. This means the tile's actual position
                // each frame is the oscillating drift path — the accumulator records every stamp
                // so the root tiles visibly move in 2D before they drift far from home.
                for (pos, origin) in state.positions.iter_mut().zip(state.origins.iter()) {
                    let home_x = origin.x0 as f32;
                    let home_y = origin.y0 as f32;
                    let home_cx = (origin.x0 as f32 + origin.x1 as f32) / 2.0;
                    let dir = if home_cx < cx { -1.0_f32 } else { 1.0_f32 };
                    let h0 = tile_hash(settings.seed, origin.x0, origin.y0);
                    let h1 = splitmix(h0);
                    let h2 = splitmix(h1);
                    let h3 = splitmix(h2);
                    let fx = 0.020 + (h0 & 0xFFFF) as f32 / 65535.0 * 0.060;
                    let px = (h1 & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
                    let fy = 0.013 + (h2 & 0xFFFF) as f32 / 65535.0 * 0.040;
                    let py = (h3 & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
                    pos[0] = home_x + dir * settings.river_speed * settings.advect * t
                        + amplitude * (t * fx + px).sin();
                    pos[1] = home_y + amplitude * (t * fy + py).sin();
                }
                None
            }
            CascadeFieldType::Oscillate => {
                // Positions never drift — tiles stay rooted at home. Each tile oscillates
                // independently in both x and y using unique frequencies and phases from a
                // deterministic hash of its home position. The accumulator records the swept
                // path, so the trail IS the oscillation history rather than a flow ribbon.
                let amplitude = settings.river_turbulence * settings.advect;
                let offsets = state
                    .origins
                    .iter()
                    .map(|origin| {
                        let h0 = tile_hash(settings.seed, origin.x0, origin.y0);
                        let h1 = splitmix(h0);
                        let h2 = splitmix(h1);
                        let h3 = splitmix(h2);
                        // X: frequency in [0.020, 0.080] rad/frame
                        let fx = 0.020 + (h0 & 0xFFFF) as f32 / 65535.0 * 0.060;
                        let px = (h1 & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
                        // Y: frequency in [0.013, 0.053] rad/frame — different range keeps x/y
                        // frequencies incommensurate so the figure-8 path never closes early.
                        let fy = 0.013 + (h2 & 0xFFFF) as f32 / 65535.0 * 0.040;
                        let py = (h3 & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
                        let t = frame as f32;
                        [amplitude * (t * fx + px).sin(), amplitude * (t * fy + py).sin()]
                    })
                    .collect();
                Some(offsets)
            }
            CascadeFieldType::SquarePop => {
                // Each tile appears at a new random position every frame. When river_turbulence
                // > 0 the position is bounded within ±turbulence px of the tile's home cell —
                // smaller turbulence keeps squares close to their grid origin (denser, more
                // ordered look); larger turbulence spreads them further. turbulence == 0 holds
                // all squares exactly at home (static grid of outlines, no popping).
                let max_x = state.width.saturating_sub(state.tile_size) as f32;
                let max_y = state.height.saturating_sub(state.tile_size) as f32;
                let radius = settings.river_turbulence;
                let frame_seed = settings.seed.wrapping_add((frame as u64).wrapping_mul(0x517CC1B727220A95));
                for (pos, origin) in state.positions.iter_mut().zip(state.origins.iter()) {
                    let h0 = tile_hash(frame_seed, origin.x0, origin.y0);
                    let h1 = splitmix(h0);
                    if radius == 0.0 {
                        pos[0] = origin.x0 as f32;
                        pos[1] = origin.y0 as f32;
                    } else {
                        let ox = ((h0 & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0) * radius;
                        let oy = ((h1 & 0xFFFF) as f32 / 65535.0 * 2.0 - 1.0) * radius;
                        pos[0] = (origin.x0 as f32 + ox).clamp(0.0, max_x);
                        pos[1] = (origin.y0 as f32 + oy).clamp(0.0, max_y);
                    }
                }
                None
            }
        }
    } else {
        None
    };

    // Decay: fade the accumulator toward black before stamping so old squares disappear
    // as new ones appear. Alpha (channel 3) is left at 1.0 — only RGB fades.
    if settings.decay > 0.0 {
        let keep = 1.0 - settings.decay;
        for px in &mut state.accumulator.pixels {
            px[0] *= keep;
            px[1] *= keep;
            px[2] *= keep;
        }
    }

    // temporal_tiles: patches are frozen at temporal-assignment time — never refresh.
    if settings.live_refresh && !settings.temporal_tiles {
        for (patch, origin) in state.patches.iter_mut().zip(state.origins.iter()) {
            let (_, fresh) = sample_cell(current_source, origin.x0, origin.y0, origin.x1, origin.y1);
            *patch = fresh;
        }
    }

    match stamp_offsets {
        None => {
            if settings.field == CascadeFieldType::SquarePop {
                stamp_outlines(state);
            } else {
                stamp_all(state);
            }
        }
        Some(ref offsets) => stamp_with_offsets(state, offsets),
    }
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

/// Like `stamp_all` but each tile's position is shifted by its corresponding offset before
/// stamping. Used by `RiverRoot` to separate drift (in `positions`) from oscillation.
fn stamp_with_offsets(state: &mut CascadeTrailState, offsets: &[[f32; 2]]) {
    let width = state.width as i64;
    let height = state.height as i64;
    let tile = state.tile_size as i64;
    let pixels = &mut state.accumulator.pixels;
    for ((pos, patch), offset) in state
        .positions
        .iter()
        .zip(state.patches.iter())
        .zip(offsets.iter())
    {
        let left = (pos[0] + offset[0]).round() as i64;
        let top = (pos[1] + offset[1]).round() as i64;
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

/// Stamp only the **border pixels** (1-pixel outline) of each tile's patch — the interior is
/// left untouched. Used by `SquarePop` to produce wireframe squares rather than filled blocks.
/// The border pixels sample the source patch as normal so the outline carries image colour.
fn stamp_outlines(state: &mut CascadeTrailState) {
    let width = state.width as i64;
    let height = state.height as i64;
    let tile = state.tile_size as i64;
    if tile < 2 {
        return;
    }
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
            let py = (dy * ph / tile).clamp(0, ph - 1);
            let row = (y as usize) * (state.width as usize);
            for dx in 0..tile {
                // Border only: skip interior pixels.
                if dy != 0 && dy != tile - 1 && dx != 0 && dx != tile - 1 {
                    continue;
                }
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

/// Per-tile oscillating jitter for river mode. Each tile gets a unique oscillation frequency
/// and phase derived deterministically from its home position, so tiles wiggle at different
/// rates and are never in sync — the look of particles in a flowing river. The oscillation
/// is primarily *perpendicular* to the flow (lateral wiggle) with a smaller along-flow
/// component that makes each tile's speed vary slightly (particles bunch and spread).
fn river_jitter(
    seed: u64,
    home_x: u32,
    home_y: u32,
    amplitude: f32,
    flow_angle: f32,
    frame: u32,
) -> (f32, f32) {
    if amplitude == 0.0 {
        return (0.0, 0.0);
    }
    // Derive unique per-tile params via two rounds of splitmix-style hashing.
    let h0 = tile_hash(seed, home_x, home_y);
    let h1 = splitmix(h0);

    // Oscillation frequency in radians/frame. Range [0.025, 0.09] gives periods of
    // 70–250 frames (≈3–10 s at 24 fps) — slow enough to look like river undulation.
    let freq = 0.025 + (h0 & 0xFFFF) as f32 / 65535.0 * 0.065;
    // Phase offset [0, 2π] so tiles start at different points in their cycle.
    let phase = (h1 & 0xFFFF) as f32 / 65535.0 * std::f32::consts::TAU;
    // A second, incommensurate frequency (golden-ratio multiple) drives the along-flow
    // speed variation so that component never locks to the perpendicular one.
    let freq2 = freq * 1.618;

    let t = frame as f32;
    // Perpendicular (cross-flow) oscillation — the main lateral wiggle.
    let perp = amplitude * (t * freq + phase).sin();
    // Along-flow speed variation — smaller, creates the "bunching" of river particles.
    let along = amplitude * 0.3 * (t * freq2 + phase * 0.7).sin();

    // Perpendicular unit vector is 90° CCW from flow: (-sin θ, cos θ).
    let (sin_a, cos_a) = flow_angle.sin_cos();
    let jx = -sin_a * perp + cos_a * along;
    let jy = cos_a * perp + sin_a * along;
    (jx, jy)
}

/// Splitmix64-style hash — mixes a single u64.
#[inline]
fn splitmix(x: u64) -> u64 {
    let x = x.wrapping_add(0x9E3779B97F4A7C15);
    let x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    let x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^ (x >> 31)
}

/// Derive a u64 hash from the tile's home grid position and the global seed.
#[inline]
fn tile_hash(seed: u64, home_x: u32, home_y: u32) -> u64 {
    splitmix(seed ^ ((home_x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (home_y as u64).wrapping_mul(0x6C62272E07BB0142)))
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
            advance_cascade_trails(&mut state, &source, settings, 1).unwrap();
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
                advance_cascade_trails(&mut state, &source, settings, 1).unwrap();
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
            advance_cascade_trails(&mut state, &source, settings, 1).unwrap();
            let now = count_non_black(&render_cascade_trails(&state));
            assert!(now >= prev, "accumulator must never lose painted pixels");
            grew |= now > prev;
            prev = now;
        }
        assert!(grew, "with advect > 0 the trails should grow the painted area");
    }
}
