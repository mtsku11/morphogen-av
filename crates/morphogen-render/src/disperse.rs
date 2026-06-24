//! Colour-group dispersion blend — the *content-advecting* sibling of the
//! coagulation blend. Where [`crate::coagulate`] composites Source A and Source B
//! **in place** behind a moving ownership mask (a moving-edge dissolve), this path
//! displaces the **image content itself**, per block, so material from both sources
//! physically flows, scatters, and intermixes over time.
//!
//! The animation arc the effect targets: colour-grouped tiles first **flow together**
//! along a directional current, then a growing random walk makes them **shatter and
//! disperse** from their groups, intermixing the two images (perpetual churn).
//!
//! Mechanism (Slice 5, deterministic CPU):
//! - A stateful per-block **offset field** accumulates `coherent · current +
//!   dispersion · scatter` each frame, damped so the churn stays bounded. The
//!   coherent term is the block-mean optical flow (the current); the scatter term is
//!   an animated per-block random step whose weight `dispersion ∈ [0, 1]` ramps up.
//! - The composite samples **both** sources at the per-block displaced coordinate
//!   (fine tiles, like a glitch mosaic) and blends them by the colour-grouped
//!   ownership field (reused from [`crate::coagulate`]), itself sampled at the
//!   displaced coordinate so a tile's *source identity* travels with its content —
//!   which is what lets A-tiles and B-tiles interleave as they disperse.

use std::f32::consts::TAU;

use serde::{Deserialize, Serialize};

use crate::coagulate::CoagulationSettings;
use crate::{sample_bilinear_clamped, CoagulationField, FlowField, ImageBufferF32, RenderError};

/// Algorithm identifier for the Slice-5 CPU reference.
pub const DISPERSION_BLEND_ALGORITHM: &str = "colour_group_dispersion_blend_cpu_v1";

/// Knobs for the colour-group dispersion blend (Slice 5). Ownership grouping reuses
/// the coagulation descriptor field; the dispersion terms drive the content motion.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct DispersionSettings {
    /// Tile edge length in pixels (fine ⇒ a dense glitch spray).
    pub block_size: u32,
    /// Weight on per-cell mean-colour luminance in the ownership preference.
    pub color_weight: f32,
    /// Weight on per-cell texture energy in the ownership preference.
    pub texture_weight: f32,
    /// Master ownership coagulation amount (A-vs-B descriptor difference).
    pub coagulation_strength: f32,
    /// Seeded per-cell scatter on the ownership preference.
    pub randomness: f32,
    /// Spatial-coherence relaxation passes for the ownership field.
    pub coherence_passes: u32,
    /// Per-pass neighbour pull for the ownership field, in `[0, 1]`.
    pub coherence_strength: f32,
    /// Baseline A ownership added to every tile.
    pub bias: f32,
    /// Scales the coherent current (block-mean flow) added to each block's offset.
    pub coherent_amount: f32,
    /// Maximum per-frame random scatter step (pixels) at full dispersion.
    pub scatter_amount: f32,
    /// Per-frame damping of the accumulated offset in `[0, 1)` — keeps the churn
    /// bounded so blocks wander rather than fly off-screen.
    pub damping: f32,
    /// Seed for the deterministic per-block hashes.
    pub seed: u64,
}

impl Default for DispersionSettings {
    fn default() -> Self {
        Self {
            block_size: 8,
            color_weight: 1.0,
            texture_weight: 0.4,
            coagulation_strength: 1.6,
            randomness: 0.5,
            coherence_passes: 2,
            coherence_strength: 0.5,
            bias: 0.4,
            coherent_amount: 1.0,
            scatter_amount: 3.0,
            damping: 0.9,
            seed: 0,
        }
    }
}

impl DispersionSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.block_size == 0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "block_size must be greater than zero".to_string(),
            ));
        }
        for (name, value) in [
            ("color_weight", self.color_weight),
            ("texture_weight", self.texture_weight),
            ("coagulation_strength", self.coagulation_strength),
            ("randomness", self.randomness),
            ("coherence_strength", self.coherence_strength),
            ("bias", self.bias),
            ("coherent_amount", self.coherent_amount),
            ("scatter_amount", self.scatter_amount),
            ("damping", self.damping),
        ] {
            if !value.is_finite() {
                return Err(RenderError::InvalidCoagulationSettings(format!(
                    "{name} must be finite"
                )));
            }
        }
        Ok(())
    }

    /// The ownership-field (colour grouping) settings derived from these knobs. The
    /// dispersion composite samples ownership at displaced coordinates, so the
    /// edge/jitter knobs are irrelevant here and left at zero.
    pub fn ownership_settings(&self) -> CoagulationSettings {
        CoagulationSettings {
            patch_size: self.block_size,
            color_weight: self.color_weight,
            texture_weight: self.texture_weight,
            coherence_passes: self.coherence_passes,
            coherence_strength: self.coherence_strength,
            randomness: self.randomness,
            coagulation_strength: self.coagulation_strength,
            edge_hardness: 0.0,
            edge_dither: 0.0,
            bias: self.bias,
            block_jitter: 0.0,
            seed: self.seed,
        }
    }
}

/// The stateful per-block content-offset field (pixels). `offsets[r * cols + c]` is
/// the displacement added to a block's sample coordinate this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DispersionField {
    pub cols: u32,
    pub rows: u32,
    pub block_size: u32,
    pub offsets: Vec<[f32; 2]>,
}

/// Salt for the per-block animated scatter direction.
const SCATTER_SALT: u64 = 0x5CA7_7E12_0FF5_E715;

/// Advance the per-block offset field one frame: accumulate the coherent current
/// (block-mean flow, in pixels) plus a `dispersion`-weighted animated random step,
/// then damp. Frame-zero is `previous: None` (offsets start at zero). `cell_flow`
/// is the ownership-grid flow in **cell units** (as from `downsample_flow_to_cells`),
/// converted back to pixels here by `× block_size`.
pub fn advance_dispersion_field(
    previous: Option<&DispersionField>,
    cell_flow: Option<&FlowField>,
    cols: u32,
    rows: u32,
    dispersion: f32,
    settings: DispersionSettings,
    frame_index: u32,
) -> Result<DispersionField, RenderError> {
    settings.validate()?;
    if let Some(previous) = previous {
        if previous.cols != cols || previous.rows != rows {
            return Err(RenderError::IncompatibleInputs(format!(
                "previous dispersion field is {}x{}, current is {}x{}",
                previous.cols, previous.rows, cols, rows
            )));
        }
    }
    if let Some(flow) = cell_flow {
        if flow.width != cols || flow.height != rows {
            return Err(RenderError::IncompatibleInputs(format!(
                "cell flow is {}x{}, dispersion grid is {}x{}",
                flow.width, flow.height, cols, rows
            )));
        }
    }

    // Frame zero starts in place: no accumulated motion until there is prior state.
    let Some(previous) = previous else {
        return Ok(DispersionField {
            cols,
            rows,
            block_size: settings.block_size,
            offsets: vec![[0.0, 0.0]; (cols as usize) * (rows as usize)],
        });
    };

    let dispersion = dispersion.clamp(0.0, 1.0);
    let damping = settings.damping.clamp(0.0, 1.0);
    let block_size = settings.block_size as f32;
    let mut offsets = vec![[0.0_f32, 0.0]; (cols as usize) * (rows as usize)];
    for cy in 0..rows {
        for cx in 0..cols {
            let index = (cy * cols + cx) as usize;
            let prior = previous.offsets[index];

            let current = cell_flow
                .and_then(|flow| flow.vector(cx, cy))
                .unwrap_or([0.0, 0.0]);
            let coherent = [
                current[0] * block_size * settings.coherent_amount,
                current[1] * block_size * settings.coherent_amount,
            ];

            // Animated per-block random step (decorrelated by frame ⇒ churn).
            let angle = hash01(
                settings.seed ^ SCATTER_SALT,
                index as u64,
                u64::from(frame_index),
            ) * TAU;
            let magnitude = settings.scatter_amount * dispersion;
            let scatter = [angle.cos() * magnitude, angle.sin() * magnitude];

            offsets[index] = [
                (prior[0] + coherent[0] + scatter[0]) * damping,
                (prior[1] + coherent[1] + scatter[1]) * damping,
            ];
        }
    }

    Ok(DispersionField {
        cols,
        rows,
        block_size: settings.block_size,
        offsets,
    })
}

/// Composite one dispersion-blend frame: sample both sources (and the colour-group
/// ownership) at each block's displaced coordinate and blend by ownership. Content
/// physically moves with the offset field, so the two images flow and intermix.
pub fn disperse_composite_cpu(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    ownership: &CoagulationField,
    dispersion_field: &DispersionField,
    block_size: u32,
) -> Result<ImageBufferF32, RenderError> {
    if source_a.width != source_b.width || source_a.height != source_b.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "source A is {}x{}, source B is {}x{}",
            source_a.width, source_a.height, source_b.width, source_b.height
        )));
    }
    if block_size == 0 {
        return Err(RenderError::InvalidCoagulationSettings(
            "block_size must be greater than zero".to_string(),
        ));
    }

    ImageBufferF32::from_fn(source_a.width, source_a.height, |x, y| {
        let cx = (x / block_size).min(dispersion_field.cols - 1);
        let cy = (y / block_size).min(dispersion_field.rows - 1);
        let offset = dispersion_field.offsets[(cy * dispersion_field.cols + cx) as usize];
        let sx = x as f32 + offset[0];
        let sy = y as f32 + offset[1];

        let a = sample_bilinear_clamped(source_a, sx, sy);
        let b = sample_bilinear_clamped(source_b, sx, sy);
        let w = ownership.sample_pixel(sx, sy).clamp(0.0, 1.0);
        [
            b[0] + (a[0] - b[0]) * w,
            b[1] + (a[1] - b[1]) * w,
            b[2] + (a[2] - b[2]) * w,
            b[3] + (a[3] - b[3]) * w,
        ]
    })
}

/// splitmix64 finalizer (matches `coagulate`).
fn hash_u64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn hash01(seed: u64, a: u64, b: u64) -> f32 {
    let h =
        hash_u64(seed ^ a.wrapping_mul(0x100_0000_01B3) ^ b.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    (h >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coagulation_field;

    fn solid(width: u32, height: u32, rgb: [f32; 3]) -> ImageBufferF32 {
        ImageBufferF32::new(
            width,
            height,
            vec![[rgb[0], rgb[1], rgb[2], 1.0]; (width * height) as usize],
        )
        .expect("solid image")
    }

    #[test]
    fn frame_zero_has_zero_offsets() {
        let field =
            advance_dispersion_field(None, None, 4, 3, 1.0, DispersionSettings::default(), 0)
                .expect("frame zero");
        assert!(field.offsets.iter().all(|o| o == &[0.0, 0.0]));
    }

    #[test]
    fn zero_dispersion_and_current_keeps_offsets_zero() {
        // No coherent current and no scatter ⇒ the accumulator stays at the origin,
        // so the composite samples in place (the off case for the dispersion knob).
        let settings = DispersionSettings {
            coherent_amount: 0.0,
            scatter_amount: 0.0,
            ..DispersionSettings::default()
        };
        let prev = DispersionField {
            cols: 4,
            rows: 3,
            block_size: 8,
            offsets: vec![[0.0, 0.0]; 12],
        };
        let field =
            advance_dispersion_field(Some(&prev), None, 4, 3, 1.0, settings, 5).expect("advance");
        assert!(field.offsets.iter().all(|o| o == &[0.0, 0.0]));
    }

    #[test]
    fn scatter_displaces_blocks_and_is_deterministic() {
        let settings = DispersionSettings {
            coherent_amount: 0.0,
            scatter_amount: 5.0,
            damping: 1.0,
            seed: 7,
            ..DispersionSettings::default()
        };
        let first = advance_dispersion_field(None, None, 6, 4, 1.0, settings, 1).expect("first");
        let again = advance_dispersion_field(None, None, 6, 4, 1.0, settings, 1).expect("again");
        assert_eq!(first.offsets, again.offsets);
        // With dispersion on, frame zero is still zero (no prior to accumulate), so
        // step from a non-zero prior must move at least one block.
        let prev = DispersionField {
            cols: 6,
            rows: 4,
            block_size: 8,
            offsets: vec![[0.0, 0.0]; 24],
        };
        let stepped =
            advance_dispersion_field(Some(&prev), None, 6, 4, 1.0, settings, 2).expect("stepped");
        assert!(
            stepped.offsets.iter().any(|o| o != &[0.0, 0.0]),
            "scatter should displace blocks"
        );
    }

    #[test]
    fn coherent_current_translates_content() {
        // A uniform rightward cell flow with no scatter shifts every block's sample
        // coordinate to the right, so a vertical edge in B moves left in the output.
        let a = solid(16, 8, [0.0, 0.0, 0.0]);
        let b = ImageBufferF32::from_fn(16, 8, |x, _| {
            let v = if x < 8 { 0.2 } else { 0.8 };
            [v, v, v, 1.0]
        })
        .expect("b edge");
        let settings = DispersionSettings {
            block_size: 4,
            coagulation_strength: 0.0,
            randomness: 0.0,
            bias: 0.0, // ownership all-B ⇒ output is displaced B
            coherent_amount: 1.0,
            scatter_amount: 0.0,
            damping: 1.0,
            ..DispersionSettings::default()
        };
        let cols = 16u32.div_ceil(4);
        let rows = 8u32.div_ceil(4);
        let flow = FlowField::new(cols, rows, vec![[1.0, 0.0]; (cols * rows) as usize])
            .expect("cell flow"); // 1 cell-unit = block_size px rightward sampling
        let ownership =
            coagulation_field(&a, &b, settings.ownership_settings()).expect("ownership");
        let disp = advance_dispersion_field(None, Some(&flow), cols, rows, 0.0, settings, 0)
            .expect("disp frame0");
        // frame 0 offsets are zero; advance once from that state to apply the current.
        let disp1 =
            advance_dispersion_field(Some(&disp), Some(&flow), cols, rows, 0.0, settings, 1)
                .expect("disp frame1");
        let out =
            disperse_composite_cpu(&a, &b, &ownership, &disp1, settings.block_size).expect("out");
        // Sampling shifted right by block_size px ⇒ the bright/dark boundary in the
        // output sits left of its input column 8.
        let left = out.pixel(2, 4).expect("left")[0];
        let right = out.pixel(13, 4).expect("right")[0];
        assert!(
            right > left,
            "content should translate: left={left}, right={right}"
        );
    }
}
