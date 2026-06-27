//! Hard binary tile collage — divides the canvas into NxN blocks; each block
//! independently shows either Source A or Source B determined by a spatially
//! coherent value-noise ownership field. **Hard cuts** at every tile boundary:
//! ownership is a pure binary (A or B), no blending, no per-pixel gradient.
//!
//! The ownership field is 3-D trilinear value noise sampled at
//! `(col × cluster_scale, row × cluster_scale, frame × evolution_speed)`.
//! Smaller `cluster_scale` → wider blobs (more tiles in the same group).
//! `evolution_speed = 0` freezes the pattern; non-zero lets it slowly
//! animate, tiles flipping one by one as the noise field drifts.
//!
//! Continuity identity (off-vs-on readout): `threshold = 0.0` shows Source A
//! everywhere; `threshold = 1.0` shows Source B everywhere.

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

/// Algorithm identifier — bump when the noise formulation or tile logic changes.
pub const BLOCK_COLLAGE_ALGORITHM: &str = "block_collage_value_noise_v1";

/// Knobs for the block collage renderer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct BlockCollageSettings {
    /// Block edge length in pixels (≥ 1). The reference image uses ~96 px.
    pub tile_size: u32,
    /// Fraction of tiles assigned to Source B in [0, 1].
    /// `0` = all A (identity); `0.5` = roughly half each; `1` = all B.
    pub threshold: f32,
    /// Noise sampling frequency in tiles. Smaller = larger coherent blobs.
    /// `0.25` gives ~4-tile-wide clusters (reference look); `1.0` ≈ checkerboard.
    pub cluster_scale: f32,
    /// Per-frame z-drift of the noise field. `0` = static ownership every frame;
    /// small values (0.02–0.1) let tiles slowly flip between A and B over time.
    pub evolution_speed: f32,
    /// Deterministic seed for the noise field.
    pub seed: u64,
}

impl Default for BlockCollageSettings {
    fn default() -> Self {
        Self {
            tile_size: 96,
            threshold: 0.5,
            cluster_scale: 0.25,
            evolution_speed: 0.0,
            seed: 0,
        }
    }
}

impl BlockCollageSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.tile_size == 0 {
            return Err(RenderError::InvalidBlockCollageSettings(
                "tile_size must be >= 1".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.threshold) {
            return Err(RenderError::InvalidBlockCollageSettings(
                "threshold must be in [0, 1]".into(),
            ));
        }
        if self.cluster_scale <= 0.0 {
            return Err(RenderError::InvalidBlockCollageSettings(
                "cluster_scale must be > 0".into(),
            ));
        }
        Ok(())
    }
}

// ─── Value noise internals ────────────────────────────────────────────────────

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Hash three integer lattice coordinates + a seed to a value in [0, 1].
fn hash_corner(seed: u64, ix: i64, iy: i64, iz: i64) -> f32 {
    let mut h = seed.wrapping_add(0xdeadbeefcafe1234_u64);
    h = splitmix(h.wrapping_add(ix as u64));
    h = splitmix(h.wrapping_add(iy as u64 ^ 0x8675309beefcafe5_u64));
    h = splitmix(h.wrapping_add(iz as u64 ^ 0x123456789abcdef0_u64));
    (h >> 32) as f32 / 4_294_967_295.0
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 3-D trilinear value noise sampled at `(x, y, z)`.
fn value_noise(x: f32, y: f32, z: f32, seed: u64) -> f32 {
    let ix = x.floor() as i64;
    let iy = y.floor() as i64;
    let iz = z.floor() as i64;
    let ux = smoothstep(x - ix as f32);
    let uy = smoothstep(y - iy as f32);
    let uz = smoothstep(z - iz as f32);

    let v = |dx: i64, dy: i64, dz: i64| hash_corner(seed, ix + dx, iy + dy, iz + dz);

    let v00 = lerp(v(0, 0, 0), v(1, 0, 0), ux);
    let v10 = lerp(v(0, 1, 0), v(1, 1, 0), ux);
    let v01 = lerp(v(0, 0, 1), v(1, 0, 1), ux);
    let v11 = lerp(v(0, 1, 1), v(1, 1, 1), ux);
    let vz0 = lerp(v00, v10, uy);
    let vz1 = lerp(v01, v11, uy);
    lerp(vz0, vz1, uz)
}

// ─── Frame renderer ───────────────────────────────────────────────────────────

/// Render one frame of the block collage.
///
/// Output dimensions match Source A. Source B pixels are clamped to its bounds
/// if the two sources differ in size.
pub fn render_block_collage_frame(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    settings: &BlockCollageSettings,
    frame: u32,
) -> Result<ImageBufferF32, RenderError> {
    let w = source_a.width;
    let h = source_a.height;
    let ts = settings.tile_size.max(1);
    let z = frame as f32 * settings.evolution_speed;
    let bw = source_b.width.saturating_sub(1);
    let bh = source_b.height.saturating_sub(1);

    ImageBufferF32::from_fn(w, h, |px, py| {
        let col = px / ts;
        let row = py / ts;
        let ownership = value_noise(
            col as f32 * settings.cluster_scale,
            row as f32 * settings.cluster_scale,
            z,
            settings.seed,
        );
        if ownership < settings.threshold {
            source_b
                .pixel(px.min(bw), py.min(bh))
                .unwrap_or([0.0, 0.0, 0.0, 1.0])
        } else {
            source_a.pixel(px, py).unwrap_or([0.0, 0.0, 0.0, 1.0])
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [f32; 4]) -> ImageBufferF32 {
        ImageBufferF32::from_fn(w, h, |_, _| c).unwrap()
    }

    #[test]
    fn threshold_zero_shows_all_a() {
        let a = solid(128, 128, [1.0, 0.0, 0.0, 1.0]);
        let b = solid(128, 128, [0.0, 1.0, 0.0, 1.0]);
        let s = BlockCollageSettings {
            threshold: 0.0,
            ..Default::default()
        };
        let out = render_block_collage_frame(&a, &b, &s, 0).unwrap();
        for p in &out.pixels {
            assert_eq!(*p, [1.0, 0.0, 0.0, 1.0], "threshold 0 must show A");
        }
    }

    #[test]
    fn threshold_one_shows_all_b() {
        let a = solid(128, 128, [1.0, 0.0, 0.0, 1.0]);
        let b = solid(128, 128, [0.0, 1.0, 0.0, 1.0]);
        let s = BlockCollageSettings {
            threshold: 1.0,
            ..Default::default()
        };
        let out = render_block_collage_frame(&a, &b, &s, 0).unwrap();
        for p in &out.pixels {
            assert_eq!(*p, [0.0, 1.0, 0.0, 1.0], "threshold 1 must show B");
        }
    }

    #[test]
    fn static_ownership_is_deterministic() {
        let a = solid(256, 256, [0.4, 0.0, 0.0, 1.0]);
        let b = solid(256, 256, [0.0, 0.4, 0.0, 1.0]);
        let s = BlockCollageSettings {
            evolution_speed: 0.0,
            ..Default::default()
        };
        let f0 = render_block_collage_frame(&a, &b, &s, 0).unwrap();
        let f5 = render_block_collage_frame(&a, &b, &s, 5).unwrap();
        assert_eq!(f0, f5, "static ownership must be identical across frames");
    }

    #[test]
    fn settings_validate_rejects_zero_tile_size() {
        let s = BlockCollageSettings {
            tile_size: 0,
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }
}
