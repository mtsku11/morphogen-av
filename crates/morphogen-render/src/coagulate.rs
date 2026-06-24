//! Descriptor-coagulated flow blend — the first *mutual* two-source effect, where
//! material from both Source A and Source B is mangled together rather than A
//! merely modulating B. Cells of the screen group into irregular **coagulated
//! patches** by descriptor similarity (per-cell mean colour + spatial texture,
//! reusing the granular-mosaic feature definition), then a hard/dirty composite
//! interleaves A and B as patchy clumps.
//!
//! This is **Slice 1**: a deterministic, single-frame CPU reference with no
//! advection and no feedback (those land as later slices, before any Metal port).
//! The ownership field already carries the structure the temporal slice will
//! advect, so it is computed by a standalone, testable [`coagulation_field`].
//!
//! Continuity identity (the off case for off-vs-on readout): with
//! `coagulation_strength == 0`, `randomness == 0`, and `bias == 0` the ownership
//! field is everywhere zero and the frame is **Source B verbatim**.

use serde::{Deserialize, Serialize};

use crate::granular_mosaic::{average_carrier_tile_color, luminance, tile_texture};
use crate::{flow_displace_cpu, FlowField, ImageBufferF32, RenderError};

/// Algorithm identifier for the Slice-1 CPU reference. Bump when the field
/// formulation or feature set changes so stale caches/checkpoints invalidate.
pub const COAGULATED_BLEND_ALGORITHM: &str = "descriptor_coagulated_flow_blend_cpu_v1";

/// Knobs for the descriptor-coagulated flow blend (Slice 1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CoagulationSettings {
    /// Cell edge length in pixels for the low-resolution ownership field (>= 1).
    pub patch_size: u32,
    /// Weight on the per-cell mean-colour luminance in the A-vs-B preference.
    pub color_weight: f32,
    /// Weight on the per-cell texture energy (luma variance + gradient magnitude).
    pub texture_weight: f32,
    /// Number of spatial-coherence relaxation passes; each pulls a cell toward its
    /// 4-neighbour ownership mean so patches clump instead of forming a checkerboard.
    pub coherence_passes: u32,
    /// Per-pass neighbour pull in `[0, 1]` (clamped). `0` leaves the raw field.
    pub coherence_strength: f32,
    /// Seeded per-cell scatter added to the preference, breaking uniform crossfades.
    pub randomness: f32,
    /// Master coagulation amount scaling the A-vs-B descriptor difference. `0`
    /// (with `randomness == 0` and `bias == 0`) makes the frame Source B verbatim.
    pub coagulation_strength: f32,
    /// `0` = soft lerp between A and B; `1` = a dithered hard threshold (dirty edges).
    pub edge_hardness: f32,
    /// Seeded per-pixel jitter on the hard-threshold boundary (only bites when
    /// `edge_hardness > 0`), roughening patch edges.
    pub edge_dither: f32,
    /// Baseline A ownership added to every cell's preference. `0` keeps B dominant
    /// (A only intrudes where its descriptor energy exceeds B's).
    pub bias: f32,
    /// Per-cell coherent offset of the ownership-field lookup, in fractions of a
    /// cell (a seeded sub-block jitter that ragged-shifts whole blocks of the patch
    /// boundary — dirty, datamosh-y edges). `0` samples on the clean grid.
    pub block_jitter: f32,
    /// Seed for the deterministic per-cell / per-pixel hashes.
    pub seed: u64,
}

impl Default for CoagulationSettings {
    fn default() -> Self {
        Self {
            patch_size: 16,
            color_weight: 1.0,
            texture_weight: 0.0,
            coherence_passes: 2,
            coherence_strength: 0.5,
            randomness: 0.0,
            coagulation_strength: 0.0,
            edge_hardness: 0.0,
            edge_dither: 0.0,
            bias: 0.0,
            block_jitter: 0.0,
            seed: 0,
        }
    }
}

impl CoagulationSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.patch_size == 0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "patch_size must be greater than zero".to_string(),
            ));
        }
        for (name, value) in [
            ("color_weight", self.color_weight),
            ("texture_weight", self.texture_weight),
            ("block_jitter", self.block_jitter),
            ("coherence_strength", self.coherence_strength),
            ("randomness", self.randomness),
            ("coagulation_strength", self.coagulation_strength),
            ("edge_hardness", self.edge_hardness),
            ("edge_dither", self.edge_dither),
            ("bias", self.bias),
        ] {
            if !value.is_finite() {
                return Err(RenderError::InvalidCoagulationSettings(format!(
                    "{name} must be finite"
                )));
            }
        }
        Ok(())
    }

    /// True when the settings define the documented off case (field everywhere zero).
    fn is_passthrough(&self) -> bool {
        self.coagulation_strength == 0.0 && self.randomness == 0.0 && self.bias == 0.0
    }
}

/// The low-resolution A/B ownership field. `weights[r * cols + c]` is in `[0, 1]`
/// where `0` = all Source B and `1` = all Source A for that cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CoagulationField {
    pub cols: u32,
    pub rows: u32,
    pub patch_size: u32,
    pub weights: Vec<f32>,
}

impl CoagulationField {
    /// Bilinearly sample the upsampled ownership weight at output pixel `(x, y)`,
    /// clamped at the grid borders (the scalar analogue of `sample_bilinear_clamped`).
    pub fn sample(&self, x: u32, y: u32) -> f32 {
        self.sample_pixel(x as f32, y as f32)
    }

    /// Bilinearly sample the upsampled ownership weight at fractional pixel
    /// coordinates (used by the block-jitter offset), clamped at the grid borders.
    pub fn sample_pixel(&self, px: f32, py: f32) -> f32 {
        // Map the pixel centre into cell-centre space.
        let fx = (px + 0.5) / self.patch_size as f32 - 0.5;
        let fy = (py + 0.5) / self.patch_size as f32 - 0.5;
        let x0 = fx.floor();
        let y0 = fy.floor();
        let tx = fx - x0;
        let ty = fy - y0;
        let cx0 = (x0 as i64).clamp(0, self.cols as i64 - 1) as u32;
        let cy0 = (y0 as i64).clamp(0, self.rows as i64 - 1) as u32;
        let cx1 = ((x0 + 1.0) as i64).clamp(0, self.cols as i64 - 1) as u32;
        let cy1 = ((y0 + 1.0) as i64).clamp(0, self.rows as i64 - 1) as u32;
        let w00 = self.weights[(cy0 * self.cols + cx0) as usize];
        let w10 = self.weights[(cy0 * self.cols + cx1) as usize];
        let w01 = self.weights[(cy1 * self.cols + cx0) as usize];
        let w11 = self.weights[(cy1 * self.cols + cx1) as usize];
        let top = w00 + (w10 - w00) * tx;
        let bottom = w01 + (w11 - w01) * tx;
        top + (bottom - top) * ty
    }
}

/// Build the A/B ownership field from per-cell descriptors, seeded scatter, and
/// spatial-coherence relaxation. Standalone so the temporal slice can advect it and
/// tests can assert its structure independently of the composite.
pub fn coagulation_field(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    settings: CoagulationSettings,
) -> Result<CoagulationField, RenderError> {
    settings.validate()?;
    require_matching_dims(source_a, source_b)?;

    let patch = settings.patch_size;
    let cols = source_a.width.div_ceil(patch);
    let rows = source_a.height.div_ceil(patch);
    let coherence = settings.coherence_strength.clamp(0.0, 1.0);

    let mut weights = vec![0.0_f32; (cols as usize) * (rows as usize)];
    for cy in 0..rows {
        for cx in 0..cols {
            let ox = cx * patch;
            let oy = cy * patch;
            let raw_a = cell_energy(source_a, ox, oy, patch, settings);
            let raw_b = cell_energy(source_b, ox, oy, patch, settings);
            let noise = (hash01(settings.seed, u64::from(cx), u64::from(cy)) - 0.5) * 2.0;
            let preference = settings.bias
                + settings.coagulation_strength * (raw_a - raw_b)
                + settings.randomness * noise;
            weights[(cy * cols + cx) as usize] = preference.clamp(0.0, 1.0);
        }
    }

    if coherence > 0.0 {
        for _ in 0..settings.coherence_passes {
            relax_once(&mut weights, cols, rows, coherence);
        }
    }

    Ok(CoagulationField {
        cols,
        rows,
        patch_size: patch,
        weights,
    })
}

/// Render one frame of the descriptor-coagulated flow blend (Slice 1: no advection,
/// no feedback). See the module docs for the continuity identity.
pub fn coagulated_blend_frame_cpu(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    settings: CoagulationSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    require_matching_dims(source_a, source_b)?;

    if settings.is_passthrough() {
        return Ok(source_b.clone());
    }

    let field = coagulation_field(source_a, source_b, settings)?;
    composite_with_field(source_a, source_b, &field, settings)
}

/// Composite Source A over Source B by an ownership `field`, with the soft/hard
/// dithered edge blend. Shared by the stateless [`coagulated_blend_frame_cpu`] and
/// the temporal frame path so both produce identical pixels for the same field.
pub fn composite_with_field(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    field: &CoagulationField,
    settings: CoagulationSettings,
) -> Result<ImageBufferF32, RenderError> {
    require_matching_dims(source_a, source_b)?;
    let hardness = settings.edge_hardness.clamp(0.0, 1.0);
    let jitter = settings.block_jitter;

    ImageBufferF32::from_fn(source_a.width, source_a.height, |x, y| {
        let (px, py) = if jitter != 0.0 {
            // Per-cell coherent offset (in pixels): whole blocks of the boundary
            // shift together, ragged rather than fine per-pixel noise.
            let cx = u64::from(x / field.patch_size);
            let cy = u64::from(y / field.patch_size);
            let span = jitter * field.patch_size as f32;
            let ox = (hash01(settings.seed ^ JITTER_SALT_X, cx, cy) - 0.5) * 2.0 * span;
            let oy = (hash01(settings.seed ^ JITTER_SALT_Y, cx, cy) - 0.5) * 2.0 * span;
            (x as f32 + ox, y as f32 + oy)
        } else {
            (x as f32, y as f32)
        };
        let w_soft = field.sample_pixel(px, py).clamp(0.0, 1.0);
        let w_eff = if hardness > 0.0 {
            let dither = (hash01(settings.seed ^ EDGE_SALT, u64::from(x), u64::from(y)) - 0.5)
                * settings.edge_dither;
            let hard = if w_soft + dither >= 0.5 { 1.0 } else { 0.0 };
            w_soft + (hard - w_soft) * hardness
        } else {
            w_soft
        };

        let a = source_a.pixel(x, y).unwrap_or([0.0; 4]);
        let b = source_b.pixel(x, y).unwrap_or([0.0; 4]);
        [
            b[0] + (a[0] - b[0]) * w_eff,
            b[1] + (a[1] - b[1]) * w_eff,
            b[2] + (a[2] - b[2]) * w_eff,
            b[3] + (a[3] - b[3]) * w_eff,
        ]
    })
}

/// Source of the vector field that advects the ownership field each frame (Slice 2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CoagulationFlowSource {
    /// Optical flow estimated between consecutive Source A frames.
    AFlow,
    /// Optical flow estimated between consecutive Source B frames.
    BFlow,
    /// Per-cell mean of the A and B flows.
    Mixed,
    /// Deterministic synthetic turbulence (needs no input frames).
    Turbulence,
}

/// Downsample a pixel-resolution flow field to the ownership-field cell grid: the
/// per-cell mean motion, converted from pixels to **cell units** (divided by
/// `patch_size`) so it advects the field at the same spatial speed as the imagery.
pub fn downsample_flow_to_cells(
    flow: &FlowField,
    patch_size: u32,
) -> Result<FlowField, RenderError> {
    if patch_size == 0 {
        return Err(RenderError::InvalidCoagulationSettings(
            "patch_size must be greater than zero".to_string(),
        ));
    }
    let cols = flow.width.div_ceil(patch_size);
    let rows = flow.height.div_ceil(patch_size);
    let inv_patch = 1.0 / patch_size as f32;
    FlowField::from_fn(cols, rows, |cx, cy| {
        let x0 = cx * patch_size;
        let y0 = cy * patch_size;
        let x1 = (x0 + patch_size).min(flow.width);
        let y1 = (y0 + patch_size).min(flow.height);
        let mut sum = [0.0_f64, 0.0_f64];
        let mut count = 0_u64;
        for y in y0..y1 {
            for x in x0..x1 {
                let v = flow.vector(x, y).unwrap_or([0.0, 0.0]);
                sum[0] += f64::from(v[0]);
                sum[1] += f64::from(v[1]);
                count += 1;
            }
        }
        if count == 0 {
            [0.0, 0.0]
        } else {
            let inv = 1.0 / count as f64;
            [
                (sum[0] * inv) as f32 * inv_patch,
                (sum[1] * inv) as f32 * inv_patch,
            ]
        }
    })
}

/// Per-cell mean of two cell-resolution flow fields (the `Mixed` source).
pub fn average_cell_flows(a: &FlowField, b: &FlowField) -> Result<FlowField, RenderError> {
    if a.width != b.width || a.height != b.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "cell flows are {}x{} and {}x{}",
            a.width, a.height, b.width, b.height
        )));
    }
    FlowField::from_fn(a.width, a.height, |x, y| {
        let va = a.vector(x, y).unwrap_or([0.0, 0.0]);
        let vb = b.vector(x, y).unwrap_or([0.0, 0.0]);
        [(va[0] + vb[0]) * 0.5, (va[1] + vb[1]) * 0.5]
    })
}

/// Deterministic synthetic turbulence in cell units, evolving with `frame_index` so
/// patches drift and swirl with no input frames. A small sum of rotated sinusoids
/// (cheap, reproducible, divergence-ful — patches collide and pile rather than
/// translating rigidly).
pub fn synthesize_turbulence_flow(
    cols: u32,
    rows: u32,
    frame_index: u32,
    strength: f32,
    seed: u64,
) -> Result<FlowField, RenderError> {
    let phase = (seed & 0xFFFF) as f32 * 0.001;
    let t = frame_index as f32;
    FlowField::from_fn(cols, rows, |x, y| {
        let fx = x as f32;
        let fy = y as f32;
        let vx = (0.13 * fx + 0.21 * fy + 0.30 * t + phase).sin()
            + 0.5 * (0.07 * fx - 0.11 * fy - 0.20 * t).sin();
        let vy = (0.11 * fx - 0.17 * fy + 0.25 * t + phase).cos()
            + 0.5 * (0.09 * fx + 0.05 * fy + 0.15 * t).cos();
        [vx * strength, vy * strength]
    })
}

/// Advect an ownership field by a cell-resolution flow, reusing the parity-gated
/// `flow_displace` backward warp on the field packed into an image channel (the
/// trick that makes field advection free). Borders clamp, so patches pile at edges.
pub fn advect_coagulation_field(
    field: &CoagulationField,
    cell_flow: &FlowField,
    amount: f32,
) -> Result<CoagulationField, RenderError> {
    if cell_flow.width != field.cols || cell_flow.height != field.rows {
        return Err(RenderError::IncompatibleInputs(format!(
            "ownership field is {}x{} cells, cell flow is {}x{}",
            field.cols, field.rows, cell_flow.width, cell_flow.height
        )));
    }
    let packed = ImageBufferF32::from_fn(field.cols, field.rows, |x, y| {
        let w = field.weights[(y * field.cols + x) as usize];
        [w, 0.0, 0.0, 1.0]
    })?;
    let advected = flow_displace_cpu(&packed, cell_flow, amount)?;
    let weights = advected.pixels.iter().map(|pixel| pixel[0]).collect();
    Ok(CoagulationField {
        cols: field.cols,
        rows: field.rows,
        patch_size: field.patch_size,
        weights,
    })
}

/// Render one frame of the **temporal** descriptor-coagulated flow blend (Slice 2).
///
/// Frame-zero behaviour (`previous_field: None`): the ownership field is built from
/// descriptors only (identical to the stateless Slice-1 field). On later frames the
/// prior-frame field is advected by `cell_flow` (the exact prior state consumed,
/// carried as an unquantized [`CoagulationField`], never a display PNG), then blended
/// toward the fresh descriptor field by `refresh` (`1` = re-seed every frame ≡ the
/// stateless path; `0` = the field only advects, ignoring new content). Returns the
/// composited frame and the new field to carry forward as the checkpoint.
///
/// With `cell_flow = None` (or `advect_amount = 0`) **and** `refresh = 1`, the output
/// is identical to [`coagulated_blend_frame_cpu`].
pub fn coagulated_blend_temporal_frame_cpu(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    cell_flow: Option<&FlowField>,
    previous_field: Option<&CoagulationField>,
    settings: CoagulationSettings,
    advect_amount: f32,
    refresh: f32,
) -> Result<(ImageBufferF32, CoagulationField), RenderError> {
    let field = advance_coagulation_field(
        source_a,
        source_b,
        cell_flow,
        previous_field,
        settings,
        advect_amount,
        refresh,
    )?;
    let image = composite_with_field(source_a, source_b, &field, settings)?;
    Ok((image, field))
}

/// Advance the ownership field one temporal step without compositing: build the
/// fresh descriptor field, and (if a previous field is given) advect it by
/// `cell_flow` and blend toward the fresh field by `refresh`. Split out so a render
/// backend can build the field on the CPU and composite it on either CPU or GPU.
pub fn advance_coagulation_field(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    cell_flow: Option<&FlowField>,
    previous_field: Option<&CoagulationField>,
    settings: CoagulationSettings,
    advect_amount: f32,
    refresh: f32,
) -> Result<CoagulationField, RenderError> {
    settings.validate()?;
    require_matching_dims(source_a, source_b)?;

    let target = coagulation_field(source_a, source_b, settings)?;
    Ok(match previous_field {
        None => target,
        Some(previous) => {
            if previous.cols != target.cols || previous.rows != target.rows {
                return Err(RenderError::IncompatibleInputs(format!(
                    "previous field is {}x{} cells, current is {}x{}",
                    previous.cols, previous.rows, target.cols, target.rows
                )));
            }
            let advected = match cell_flow {
                Some(flow) => advect_coagulation_field(previous, flow, advect_amount)?,
                None => previous.clone(),
            };
            let r = refresh.clamp(0.0, 1.0);
            let weights = advected
                .weights
                .iter()
                .zip(&target.weights)
                .map(|(history, fresh)| history + (fresh - history) * r)
                .collect();
            CoagulationField {
                cols: target.cols,
                rows: target.rows,
                patch_size: target.patch_size,
                weights,
            }
        }
    })
}

/// Output feedback smear (Slice 3): hold a decayed fraction of the previous output
/// frame into the current composite, leaving trails as coagulated patches move.
/// Only the RGB channels smear — alpha is taken from the fresh composite so an
/// opaque blend stays opaque (unlike the flow-feedback engine, which treats alpha as
/// feedback state). `smear == 0` or no history returns the composite unchanged.
pub fn apply_history_smear(
    composite: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    smear: f32,
    decay: f32,
) -> Result<ImageBufferF32, RenderError> {
    let smear = smear.clamp(0.0, 1.0);
    let decay = decay.clamp(0.0, 1.0);
    let Some(previous) = previous_output else {
        return Ok(composite.clone());
    };
    if smear == 0.0 {
        return Ok(composite.clone());
    }
    if previous.width != composite.width || previous.height != composite.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "previous output is {}x{}, composite is {}x{}",
            previous.width, previous.height, composite.width, composite.height
        )));
    }
    ImageBufferF32::from_fn(composite.width, composite.height, |x, y| {
        let c = composite.pixel(x, y).unwrap_or([0.0; 4]);
        let p = previous.pixel(x, y).unwrap_or([0.0; 4]);
        let trail = decay * smear;
        [
            c[0] * (1.0 - smear) + p[0] * trail,
            c[1] * (1.0 - smear) + p[1] * trail,
            c[2] * (1.0 - smear) + p[2] * trail,
            c[3],
        ]
    })
}

/// Salt mixed into the seed for the per-pixel edge dither so it decorrelates from
/// the per-cell ownership hash.
const EDGE_SALT: u64 = 0xA5A5_5A5A_C3C3_3C3C;
/// Salts for the per-cell block-jitter offsets (decorrelated x/y).
const JITTER_SALT_X: u64 = 0x1234_5678_9ABC_DEF0;
const JITTER_SALT_Y: u64 = 0x0FED_CBA9_8765_4321;

fn require_matching_dims(a: &ImageBufferF32, b: &ImageBufferF32) -> Result<(), RenderError> {
    if a.width != b.width || a.height != b.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "source A is {}x{}, source B is {}x{}",
            a.width, a.height, b.width, b.height
        )));
    }
    Ok(())
}

/// Per-cell descriptor energy: weighted mean-colour luminance plus texture energy
/// (luma variance + gradient magnitude), reusing the granular-mosaic feature
/// definitions so this effect matches by the same descriptors.
fn cell_energy(
    image: &ImageBufferF32,
    origin_x: u32,
    origin_y: u32,
    patch: u32,
    settings: CoagulationSettings,
) -> f32 {
    let color = average_carrier_tile_color(image, origin_x, origin_y, patch);
    let lum = luminance([color[0], color[1], color[2], 1.0]);
    let texture = tile_texture(image, origin_x, origin_y, patch);
    settings.color_weight * lum + settings.texture_weight * (texture[0] + texture[1])
}

/// One spatial-coherence relaxation pass: each cell moves toward the mean of its
/// existing 4-neighbours by `strength`. Reads from a snapshot so the pass is
/// order-independent (deterministic).
fn relax_once(weights: &mut [f32], cols: u32, rows: u32, strength: f32) {
    let snapshot = weights.to_vec();
    for cy in 0..rows {
        for cx in 0..cols {
            let mut sum = 0.0_f32;
            let mut count = 0.0_f32;
            let mut add = |nx: i64, ny: i64| {
                if nx >= 0 && nx < cols as i64 && ny >= 0 && ny < rows as i64 {
                    sum += snapshot[(ny as u32 * cols + nx as u32) as usize];
                    count += 1.0;
                }
            };
            add(cx as i64 - 1, cy as i64);
            add(cx as i64 + 1, cy as i64);
            add(cx as i64, cy as i64 - 1);
            add(cx as i64, cy as i64 + 1);
            let idx = (cy * cols + cx) as usize;
            if count > 0.0 {
                let mean = sum / count;
                weights[idx] = (snapshot[idx] + (mean - snapshot[idx]) * strength).clamp(0.0, 1.0);
            }
        }
    }
}

/// splitmix64 finalizer — a deterministic, well-distributed integer hash.
fn hash_u64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Deterministic hash of `(seed, a, b)` into `[0, 1)`.
fn hash01(seed: u64, a: u64, b: u64) -> f32 {
    let h =
        hash_u64(seed ^ a.wrapping_mul(0x100_0000_01B3) ^ b.wrapping_mul(0xD6E8_FEB8_6659_FD93));
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

    fn mean_luma(image: &ImageBufferF32) -> f32 {
        let mut total = 0.0;
        for y in 0..image.height {
            for x in 0..image.width {
                total += luminance(image.pixel(x, y).expect("pixel"));
            }
        }
        total / (image.width * image.height) as f32
    }

    fn field_total_variation(field: &CoagulationField) -> f32 {
        let mut total = 0.0;
        for cy in 0..field.rows {
            for cx in 0..field.cols {
                let here = field.weights[(cy * field.cols + cx) as usize];
                if cx + 1 < field.cols {
                    total += (field.weights[(cy * field.cols + cx + 1) as usize] - here).abs();
                }
                if cy + 1 < field.rows {
                    total += (field.weights[((cy + 1) * field.cols + cx) as usize] - here).abs();
                }
            }
        }
        total
    }

    #[test]
    fn passthrough_settings_return_source_b_verbatim() {
        let a = solid(8, 8, [0.9, 0.1, 0.2]);
        let b = solid(8, 8, [0.1, 0.4, 0.7]);
        let settings = CoagulationSettings {
            coagulation_strength: 0.0,
            randomness: 0.0,
            bias: 0.0,
            ..CoagulationSettings::default()
        };

        let out = coagulated_blend_frame_cpu(&a, &b, settings).expect("frame");
        assert_eq!(out, b);
    }

    #[test]
    fn coagulation_pulls_bright_source_a_into_the_blend() {
        let a = solid(16, 16, [1.0, 1.0, 1.0]);
        let b = solid(16, 16, [0.0, 0.0, 0.0]);
        let settings = CoagulationSettings {
            patch_size: 4,
            coagulation_strength: 1.0,
            randomness: 0.0,
            bias: 0.0,
            coherence_passes: 0,
            ..CoagulationSettings::default()
        };

        let out = coagulated_blend_frame_cpu(&a, &b, settings).expect("frame");
        // A is fully more energetic than B (raw_a=1, raw_b=0) so ownership saturates
        // to A; output luma rises far above the all-black carrier.
        assert!(
            mean_luma(&out) > 0.5,
            "A should intrude: {}",
            mean_luma(&out)
        );
    }

    #[test]
    fn same_seed_is_deterministic_and_seed_changes_the_field() {
        let a = solid(16, 16, [0.6, 0.5, 0.4]);
        let b = solid(16, 16, [0.4, 0.5, 0.6]);
        let base = CoagulationSettings {
            patch_size: 4,
            coagulation_strength: 0.0,
            randomness: 1.0,
            bias: 0.5,
            coherence_passes: 0,
            seed: 7,
            ..CoagulationSettings::default()
        };

        let first = coagulation_field(&a, &b, base).expect("first");
        let again = coagulation_field(&a, &b, base).expect("again");
        assert_eq!(first, again);

        let other = coagulation_field(&a, &b, CoagulationSettings { seed: 99, ..base })
            .expect("other seed");
        assert_ne!(first.weights, other.weights);
    }

    #[test]
    fn coherence_relaxation_smooths_a_noisy_field() {
        let a = solid(32, 32, [0.6, 0.5, 0.4]);
        let b = solid(32, 32, [0.4, 0.5, 0.6]);
        let noisy = CoagulationSettings {
            patch_size: 4,
            coagulation_strength: 0.0,
            randomness: 1.0,
            bias: 0.5,
            coherence_passes: 0,
            coherence_strength: 0.5,
            seed: 3,
            ..CoagulationSettings::default()
        };
        let relaxed = CoagulationSettings {
            coherence_passes: 6,
            ..noisy
        };

        let noisy_field = coagulation_field(&a, &b, noisy).expect("noisy");
        let relaxed_field = coagulation_field(&a, &b, relaxed).expect("relaxed");
        assert!(
            field_total_variation(&relaxed_field) < field_total_variation(&noisy_field),
            "relaxation should reduce field variation: noisy={}, relaxed={}",
            field_total_variation(&noisy_field),
            field_total_variation(&relaxed_field)
        );
    }

    #[test]
    fn invalid_patch_size_and_mismatched_dims_error() {
        let a = solid(8, 8, [0.5, 0.5, 0.5]);
        let b = solid(8, 8, [0.5, 0.5, 0.5]);
        let bad = CoagulationSettings {
            patch_size: 0,
            ..CoagulationSettings::default()
        };
        assert!(matches!(
            coagulated_blend_frame_cpu(&a, &b, bad),
            Err(RenderError::InvalidCoagulationSettings(_))
        ));

        let wide = solid(16, 8, [0.5, 0.5, 0.5]);
        assert!(matches!(
            coagulated_blend_frame_cpu(&a, &wide, CoagulationSettings::default()),
            Err(RenderError::IncompatibleInputs(_))
        ));
    }

    #[test]
    fn hard_edges_quantize_ownership_to_a_or_b() {
        // A gradient of ownership through edge_hardness=1 must collapse every pixel
        // to exactly A or exactly B (a dithered hard threshold), never an in-between.
        let a = solid(16, 16, [1.0, 1.0, 1.0]);
        let b = solid(16, 16, [0.0, 0.0, 0.0]);
        let settings = CoagulationSettings {
            patch_size: 4,
            coagulation_strength: 0.5,
            randomness: 1.0,
            bias: 0.5,
            edge_hardness: 1.0,
            edge_dither: 0.0,
            seed: 11,
            ..CoagulationSettings::default()
        };

        let out = coagulated_blend_frame_cpu(&a, &b, settings).expect("frame");
        for pixel in &out.pixels {
            assert!(
                pixel[0] == 0.0 || pixel[0] == 1.0,
                "hard edges must be binary, got {}",
                pixel[0]
            );
        }
    }

    #[test]
    fn block_jitter_perturbs_edges_deterministically() {
        let a = solid(32, 32, [1.0, 1.0, 1.0]);
        let b = solid(32, 32, [0.0, 0.0, 0.0]);
        let base = CoagulationSettings {
            patch_size: 4,
            coagulation_strength: 1.0,
            randomness: 1.0,
            bias: 0.5,
            coherence_passes: 0,
            seed: 5,
            ..CoagulationSettings::default()
        };

        let unjittered = coagulated_blend_frame_cpu(&a, &b, base).expect("unjittered");
        let jittered = coagulated_blend_frame_cpu(
            &a,
            &b,
            CoagulationSettings {
                block_jitter: 0.7,
                ..base
            },
        )
        .expect("jittered");
        let again = coagulated_blend_frame_cpu(
            &a,
            &b,
            CoagulationSettings {
                block_jitter: 0.7,
                ..base
            },
        )
        .expect("again");

        assert_ne!(jittered, unjittered, "block jitter should move the edges");
        assert_eq!(jittered, again, "block jitter must be deterministic");
    }

    #[test]
    fn history_smear_leaves_a_decayed_trail_and_keeps_alpha() {
        let composite = solid(4, 4, [0.0, 0.0, 0.0]); // patch has moved on; now dark
        let previous = solid(4, 4, [1.0, 1.0, 1.0]); // patch was bright here last frame

        // No history -> passthrough.
        assert_eq!(
            apply_history_smear(&composite, None, 0.5, 0.8).expect("no history"),
            composite
        );
        // smear 0 -> passthrough even with history.
        assert_eq!(
            apply_history_smear(&composite, Some(&previous), 0.0, 0.8).expect("zero smear"),
            composite
        );

        let smeared = apply_history_smear(&composite, Some(&previous), 0.5, 0.8).expect("smeared");
        // out = 0*(1-0.5) + 1*(0.8*0.5) = 0.4 ghost; alpha stays 1.
        let pixel = smeared.pixel(0, 0).expect("pixel");
        assert!((pixel[0] - 0.4).abs() < 1e-6, "trail value: {}", pixel[0]);
        assert_eq!(pixel[3], 1.0);
    }

    fn intrusion_settings() -> CoagulationSettings {
        CoagulationSettings {
            patch_size: 4,
            coagulation_strength: 1.2,
            randomness: 0.3,
            bias: 0.2,
            coherence_passes: 2,
            seed: 4,
            ..CoagulationSettings::default()
        }
    }

    #[test]
    fn temporal_frame_zero_matches_the_stateless_frame() {
        let a = solid(16, 16, [0.9, 0.8, 0.2]);
        let b = solid(16, 16, [0.1, 0.2, 0.5]);
        let settings = intrusion_settings();

        let stateless = coagulated_blend_frame_cpu(&a, &b, settings).expect("stateless");
        let (temporal, _field) =
            coagulated_blend_temporal_frame_cpu(&a, &b, None, None, settings, 1.0, 1.0)
                .expect("temporal frame zero");
        assert_eq!(temporal, stateless);
    }

    #[test]
    fn refresh_one_without_advection_reduces_to_the_stateless_path() {
        let a = solid(16, 16, [0.9, 0.8, 0.2]);
        let b = solid(16, 16, [0.1, 0.2, 0.5]);
        let settings = intrusion_settings();

        let previous = coagulation_field(&a, &b, settings).expect("previous field");
        let stateless = coagulated_blend_frame_cpu(&a, &b, settings).expect("stateless");
        let (temporal, _field) =
            coagulated_blend_temporal_frame_cpu(&a, &b, None, Some(&previous), settings, 0.0, 1.0)
                .expect("temporal");
        assert_eq!(temporal, stateless);
    }

    #[test]
    fn refresh_zero_without_flow_persists_history_and_ignores_new_content() {
        let a1 = solid(16, 16, [0.9, 0.9, 0.9]);
        let b1 = solid(16, 16, [0.1, 0.1, 0.1]);
        let settings = intrusion_settings();
        let previous = coagulation_field(&a1, &b1, settings).expect("previous field");

        // Entirely different content this frame; with refresh 0 and no flow the field
        // must stay exactly the carried-forward history.
        let a2 = solid(16, 16, [0.2, 0.4, 0.1]);
        let b2 = solid(16, 16, [0.8, 0.6, 0.9]);
        let (_image, field) = coagulated_blend_temporal_frame_cpu(
            &a2,
            &b2,
            None,
            Some(&previous),
            settings,
            1.0,
            0.0,
        )
        .expect("temporal");
        assert_eq!(field.weights, previous.weights);
    }

    #[test]
    fn advecting_the_field_shifts_ownership_along_the_flow() {
        let field = CoagulationField {
            cols: 4,
            rows: 1,
            patch_size: 4,
            weights: vec![0.0, 0.0, 1.0, 0.0],
        };
        let flow = FlowField::new(4, 1, vec![[1.0, 0.0]; 4]).expect("uniform cell flow");

        let advected = advect_coagulation_field(&field, &flow, 1.0).expect("advected");
        // Backward warp with +x flow pulls each cell's value from its right neighbour,
        // so the lit cell moves one step left; the border clamps.
        assert_eq!(advected.weights, vec![0.0, 1.0, 0.0, 0.0]);
    }

    #[test]
    fn turbulence_flow_is_deterministic_and_evolves_with_frame_index() {
        let first = synthesize_turbulence_flow(8, 6, 3, 0.5, 1).expect("first");
        let again = synthesize_turbulence_flow(8, 6, 3, 0.5, 1).expect("again");
        assert_eq!(first.vectors, again.vectors);

        let later = synthesize_turbulence_flow(8, 6, 9, 0.5, 1).expect("later");
        assert_ne!(first.vectors, later.vectors);
    }

    #[test]
    fn downsample_flow_to_cells_averages_and_rescales_to_cell_units() {
        // A 4x4 pixel flow, patch 4 -> a single cell holding the mean motion divided
        // by patch_size (8 px / 4 = 2 cell units).
        let flow = FlowField::new(4, 4, vec![[8.0, 0.0]; 16]).expect("flow");
        let cells = downsample_flow_to_cells(&flow, 4).expect("cells");
        assert_eq!(cells.width, 1);
        assert_eq!(cells.height, 1);
        assert_eq!(cells.vector(0, 0), Some([2.0, 0.0]));
    }
}
