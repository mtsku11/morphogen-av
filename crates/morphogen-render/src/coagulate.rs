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
use crate::{ImageBufferF32, RenderError};

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
        // Map the pixel centre into cell-centre space.
        let fx = (x as f32 + 0.5) / self.patch_size as f32 - 0.5;
        let fy = (y as f32 + 0.5) / self.patch_size as f32 - 0.5;
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
    let hardness = settings.edge_hardness.clamp(0.0, 1.0);

    ImageBufferF32::from_fn(source_a.width, source_a.height, |x, y| {
        let w_soft = field.sample(x, y).clamp(0.0, 1.0);
        let w_eff = if hardness > 0.0 {
            let dither =
                (hash01(settings.seed ^ EDGE_SALT, u64::from(x), u64::from(y)) - 0.5)
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

/// Salt mixed into the seed for the per-pixel edge dither so it decorrelates from
/// the per-cell ownership hash.
const EDGE_SALT: u64 = 0xA5A5_5A5A_C3C3_3C3C;

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
    let h = hash_u64(
        seed ^ a.wrapping_mul(0x100_0000_01B3) ^ b.wrapping_mul(0xD6E8_FEB8_6659_FD93),
    );
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
        assert!(mean_luma(&out) > 0.5, "A should intrude: {}", mean_luma(&out));
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
}
