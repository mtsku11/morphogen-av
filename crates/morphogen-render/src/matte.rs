//! Spatial modulation via analysis-derived mattes.
//!
//! Every modulation route to date is one scalar per frame — the whole frame gets
//! the same knob value. A **matte** makes the CV-to-pixel leap: a per-pixel
//! `[0,1]` field computed from analysis of a source (Source A, or any single
//! matte-media directory), used to blend a stateless effect's output against its
//! own unmodified input only where the matte says:
//!
//! ```text
//! out(x,y) = matte(x,y) * effected(x,y) + (1 - matte(x,y)) * original(x,y)
//! ```
//!
//! See `docs/SPATIAL_MATTE_MILESTONE.md` for the contract. S1 (this module) is
//! CPU-only and stateless-effects-only; Metal/queue/SwiftUI wiring is S2.

use crate::{
    pyramidal_lucas_kanade_flow_cpu, ImageBufferF32, RenderError, LUCAS_KANADE_WINDOW_RADIUS,
};

/// Algorithm identifier for the matte compute + blend, recorded in the manifest
/// alongside the effect's own algorithm id (the matte is a post-blend, not a new
/// effect).
pub const MATTE_BLEND_ALGORITHM: &str = "matte_blend_cpu_v1";

/// Optical-flow magnitude (in px) that maps to matte value `1.0` before `gain`.
/// A fixed declared scale, not a per-frame peak — determinism and temporal
/// stability over auto-levels (see `video-audio-route-readout`'s relative-
/// normalization trap).
pub const MATTE_FLOW_FULL_SCALE_PX: f32 = 8.0;

/// Fixed lift applied to the raw per-pixel Sobel gradient magnitude before
/// `gain`. Mirrors [`crate::cascade_collage`]'s `EDGE_DETECT_GAIN` precedent:
/// raw footage gradients are small (~0.05-0.3), so a fixed multiplier — not a
/// per-frame peak — lifts them into a usable `[0,1]` range.
pub const MATTE_EDGE_GAIN: f32 = 5.0;

/// Which analysis of the matte-media frame(s) drives the per-pixel field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatteSource {
    /// Rec.709 luma of the matte frame — already absolute `[0,1]`, then `* gain`.
    ALuma,
    /// Lucas-Kanade optical-flow magnitude between the previous and current
    /// matte-media frame, normalized by [`MATTE_FLOW_FULL_SCALE_PX`], then
    /// `* gain`. Frame 0 (no prior frame) is all zeros — no peeking forward.
    AFlow,
    /// Per-pixel Sobel gradient magnitude on luma, lifted by [`MATTE_EDGE_GAIN`],
    /// then `* gain`. Border pixels are `0` (no wraparound/clamp bias).
    AEdge,
}

/// A `width x height` per-pixel field, values clamped to `[0,1]`.
#[derive(Debug, Clone, PartialEq)]
pub struct MatteField {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

/// Rec.709 luma of a linear RGB pixel.
fn luma(pixel: [f32; 4]) -> f32 {
    pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722
}

/// Per-pixel Sobel gradient magnitude on luma. Kept per-pixel (border pixels
/// `0`) instead of averaged, unlike `frame_mean_edge_density` in
/// `morphogen-cli/src/audio.rs` which this mirrors the kernel of exactly (same
/// 3x3 Sobel weights on Rec.709 luma) but cannot import directly (cli depends on
/// render, not the reverse).
fn sobel_edge_field(image: &ImageBufferF32) -> Vec<f32> {
    let w = image.width as usize;
    let h = image.height as usize;
    let mut out = vec![0.0_f32; w * h];
    if w < 3 || h < 3 {
        return out;
    }
    let lumas: Vec<f32> = image.pixels.iter().map(|p| luma(*p)).collect();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let at = |dy: isize, dx: isize| {
                lumas[(y as isize + dy) as usize * w + (x as isize + dx) as usize]
            };
            let gx =
                at(-1, 1) + 2.0 * at(0, 1) + at(1, 1) - at(-1, -1) - 2.0 * at(0, -1) - at(1, -1);
            let gy =
                at(1, -1) + 2.0 * at(1, 0) + at(1, 1) - at(-1, -1) - 2.0 * at(-1, 0) - at(-1, 1);
            out[y * w + x] = (gx * gx + gy * gy).sqrt();
        }
    }
    out
}

/// Compute the matte field for the matte-media frame at the current index.
///
/// `prev` is the immediately preceding matte-media frame (`None` at frame 0 —
/// the declared frame-zero rule for [`MatteSource::AFlow`]; ignored by the other
/// two sources, which depend only on `current`). `gain` is applied after the
/// source's own fixed normalization/lift, then the result is clamped to
/// `[0,1]`.
pub fn compute_matte_field(
    prev: Option<&ImageBufferF32>,
    current: &ImageBufferF32,
    source: MatteSource,
    gain: f32,
) -> Result<MatteField, RenderError> {
    let width = current.width;
    let height = current.height;
    let values = match source {
        MatteSource::ALuma => current
            .pixels
            .iter()
            .map(|p| (luma(*p) * gain).clamp(0.0, 1.0))
            .collect(),
        MatteSource::AFlow => match prev {
            None => vec![0.0_f32; current.pixels.len()],
            Some(previous) => {
                let estimate = pyramidal_lucas_kanade_flow_cpu(
                    previous,
                    current,
                    width,
                    height,
                    LUCAS_KANADE_WINDOW_RADIUS,
                )?;
                estimate
                    .flow
                    .vectors
                    .iter()
                    .map(|v| {
                        let magnitude = (v[0] * v[0] + v[1] * v[1]).sqrt();
                        ((magnitude / MATTE_FLOW_FULL_SCALE_PX) * gain).clamp(0.0, 1.0)
                    })
                    .collect()
            }
        },
        MatteSource::AEdge => sobel_edge_field(current)
            .into_iter()
            .map(|magnitude| (magnitude * MATTE_EDGE_GAIN * gain).clamp(0.0, 1.0))
            .collect(),
    };
    Ok(MatteField {
        width,
        height,
        values,
    })
}

/// Blend `effected` toward `original` per-pixel: `out = m*effected + (1-m)*original`.
/// Alpha is taken from `effected`. `effected`, `original`, and `matte` must share
/// dimensions; a mismatch is `RenderError::IncompatibleInputs`.
pub fn apply_matte(
    effected: &ImageBufferF32,
    original: &ImageBufferF32,
    matte: &MatteField,
) -> Result<ImageBufferF32, RenderError> {
    if effected.width != original.width || effected.height != original.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "matte blend: effected is {}x{}, original is {}x{}",
            effected.width, effected.height, original.width, original.height
        )));
    }
    if matte.width != effected.width || matte.height != effected.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "matte blend: matte field is {}x{}, carrier is {}x{}",
            matte.width, matte.height, effected.width, effected.height
        )));
    }

    let pixels = effected
        .pixels
        .iter()
        .zip(&original.pixels)
        .zip(&matte.values)
        .map(|((fx, orig), &m)| {
            [
                m * fx[0] + (1.0 - m) * orig[0],
                m * fx[1] + (1.0 - m) * orig[1],
                m * fx[2] + (1.0 - m) * orig[2],
                fx[3],
            ]
        })
        .collect();
    ImageBufferF32::new(effected.width, effected.height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn textured_frame(width: u32, height: u32, shift_x: f32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |x, y| {
            let fx = x as f32 - shift_x;
            let fy = y as f32;
            let value = 0.5 + 0.2 * (0.31 * fx).sin() + 0.2 * (0.37 * fy).sin();
            [value, value, value, 1.0]
        })
        .expect("valid frame")
    }

    fn solid(width: u32, height: u32, value: [f32; 4]) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |_, _| value).expect("valid frame")
    }

    #[test]
    fn matte_one_is_identity_on_the_effected_output() {
        // "Matte-1 identity": an all-white a-luma matte (gain 1) ⇒ output
        // byte-identical to the effected frame (no --matte behaviour).
        let white = solid(4, 4, [1.0, 1.0, 1.0, 1.0]);
        let field = compute_matte_field(None, &white, MatteSource::ALuma, 1.0).expect("field");
        assert!(field.values.iter().all(|&v| (v - 1.0).abs() < 1e-6));

        let effected = textured_frame(4, 4, 0.0);
        let original = textured_frame(4, 4, 2.0);
        let blended = apply_matte(&effected, &original, &field).expect("blend");
        assert_eq!(
            blended, effected,
            "matte=1 must reproduce the effected frame exactly"
        );
    }

    #[test]
    fn matte_zero_is_plain_carrier_passthrough() {
        // "Matte-0 identity": all-black matte ⇒ output byte-identical to the
        // plain carrier (pure passthrough of `original`), alpha still from effected.
        let black = solid(4, 4, [0.0, 0.0, 0.0, 1.0]);
        let field = compute_matte_field(None, &black, MatteSource::ALuma, 1.0).expect("field");
        assert!(field.values.iter().all(|&v| v == 0.0));

        let effected = textured_frame(4, 4, 0.0);
        let original = textured_frame(4, 4, 2.0);
        let blended = apply_matte(&effected, &original, &field).expect("blend");
        for (index, (px, orig)) in blended.pixels.iter().zip(&original.pixels).enumerate() {
            assert_eq!(px[0], orig[0], "pixel {index} r channel is plain carrier");
            assert_eq!(px[1], orig[1], "pixel {index} g channel is plain carrier");
            assert_eq!(px[2], orig[2], "pixel {index} b channel is plain carrier");
            assert_eq!(
                px[3], effected.pixels[index][3],
                "alpha comes from effected"
            );
        }
    }

    #[test]
    fn flow_matte_frame_zero_is_all_zeros() {
        // Frame-zero rule (declared): no prior frame ⇒ all-zero field, no
        // peeking at a "next" frame to fake a delta.
        let current = textured_frame(8, 8, 3.0);
        let field = compute_matte_field(None, &current, MatteSource::AFlow, 1.0).expect("field");
        assert!(
            field.values.iter().all(|&v| v == 0.0),
            "a-flow frame 0 must be all zeros"
        );
    }

    #[test]
    fn flow_matte_responds_to_motion_between_frames() {
        let previous = textured_frame(32, 32, 0.0);
        let current = textured_frame(32, 32, 4.0);
        let field =
            compute_matte_field(Some(&previous), &current, MatteSource::AFlow, 1.0).expect("field");
        assert!(
            field.values.iter().any(|&v| v > 0.0),
            "a-flow with real motion must produce a nonzero field"
        );
    }

    #[test]
    fn edge_matte_is_zero_on_a_flat_frame_and_nonzero_on_an_edge() {
        let flat = solid(8, 8, [0.5, 0.5, 0.5, 1.0]);
        let flat_field = compute_matte_field(None, &flat, MatteSource::AEdge, 1.0).expect("field");
        assert!(
            flat_field.values.iter().all(|&v| v == 0.0),
            "flat frame ⇒ no edges"
        );

        let edge = ImageBufferF32::from_fn(8, 8, |x, _| {
            if x < 4 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        })
        .expect("edge frame");
        let edge_field = compute_matte_field(None, &edge, MatteSource::AEdge, 1.0).expect("field");
        assert!(
            edge_field.values.iter().any(|&v| v > 0.0),
            "a hard edge must produce a nonzero field"
        );
    }

    #[test]
    fn edge_matte_borders_are_zero() {
        let edge = ImageBufferF32::from_fn(8, 8, |x, _| {
            if x < 4 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        })
        .expect("edge frame");
        let field = compute_matte_field(None, &edge, MatteSource::AEdge, 1.0).expect("field");
        for x in 0..8u32 {
            assert_eq!(field.values[x as usize], 0.0, "top row border is 0");
            assert_eq!(
                field.values[7 * 8 + x as usize],
                0.0,
                "bottom row border is 0"
            );
        }
        for y in 0..8u32 {
            assert_eq!(field.values[y as usize * 8], 0.0, "left col border is 0");
            assert_eq!(
                field.values[y as usize * 8 + 7],
                0.0,
                "right col border is 0"
            );
        }
    }

    #[test]
    fn higher_gain_raises_the_matte_field() {
        let mid = solid(4, 4, [0.4, 0.4, 0.4, 1.0]);
        let low_gain = compute_matte_field(None, &mid, MatteSource::ALuma, 0.5).expect("field");
        let high_gain = compute_matte_field(None, &mid, MatteSource::ALuma, 2.0).expect("field");
        assert!(low_gain.values[0] < high_gain.values[0]);
        assert!((low_gain.values[0] - 0.2).abs() < 1e-5);
        assert!((high_gain.values[0] - 0.8).abs() < 1e-5);
    }

    #[test]
    fn apply_matte_rejects_dimension_mismatch() {
        let effected = solid(4, 4, [1.0, 0.0, 0.0, 1.0]);
        let original = solid(4, 4, [0.0, 1.0, 0.0, 1.0]);
        let mismatched_field = MatteField {
            width: 2,
            height: 2,
            values: vec![1.0; 4],
        };
        let result = apply_matte(&effected, &original, &mismatched_field);
        assert!(matches!(result, Err(RenderError::IncompatibleInputs(_))));
    }

    #[test]
    fn compute_matte_field_is_deterministic() {
        let current = textured_frame(16, 16, 1.0);
        let previous = textured_frame(16, 16, 0.0);
        let a = compute_matte_field(Some(&previous), &current, MatteSource::AFlow, 1.0)
            .expect("field a");
        let b = compute_matte_field(Some(&previous), &current, MatteSource::AFlow, 1.0)
            .expect("field b");
        assert_eq!(a, b, "identical inputs must be byte-identical");
    }
}
