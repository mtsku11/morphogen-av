//! Per-channel spatial offset (RGB split / chromatic aberration).
//!
//! Each colour channel is sampled from Source B at an independently shifted
//! position using clamped bilinear interpolation.  Alpha passes through
//! unchanged from the unshifted position.
//!
//! **Off case (byte-identical passthrough):** all six offsets zero ⇒ each
//! channel is sampled at its natural position, returning B verbatim.

use serde::{Deserialize, Serialize};

use crate::{sampler::sample_bilinear_clamped, ImageBufferF32, RenderError};

pub const CHANNEL_SHIFT_ALGORITHM: &str = "channel_shift_constant_cpu_v1";

/// Per-channel spatial offsets in pixels.  Positive `dx` shifts the channel
/// rightward in the output (the R sample is drawn from `x - shift_r_x` in B).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChannelShiftSettings {
    pub shift_r_x: f32,
    pub shift_r_y: f32,
    pub shift_g_x: f32,
    pub shift_g_y: f32,
    pub shift_b_x: f32,
    pub shift_b_y: f32,
}

impl Default for ChannelShiftSettings {
    fn default() -> Self {
        Self {
            shift_r_x: 0.0,
            shift_r_y: 0.0,
            shift_g_x: 0.0,
            shift_g_y: 0.0,
            shift_b_x: 0.0,
            shift_b_y: 0.0,
        }
    }
}

impl ChannelShiftSettings {
    fn is_passthrough(&self) -> bool {
        self.shift_r_x == 0.0
            && self.shift_r_y == 0.0
            && self.shift_g_x == 0.0
            && self.shift_g_y == 0.0
            && self.shift_b_x == 0.0
            && self.shift_b_y == 0.0
    }
}

/// Render one frame of channel-shift.
///
/// `_source_a` is reserved for future A-flow-driven per-row shift (Slice 3);
/// Slice 1 ignores it and operates on `source_b` alone.
pub fn render_channel_shift_frame(
    _source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    settings: &ChannelShiftSettings,
) -> Result<ImageBufferF32, RenderError> {
    if settings.is_passthrough() {
        return Ok(source_b.clone());
    }

    let w = source_b.width;
    let h = source_b.height;

    ImageBufferF32::from_fn(w, h, |x, y| {
        let fx = x as f32;
        let fy = y as f32;

        let r = sample_bilinear_clamped(
            source_b,
            fx - settings.shift_r_x,
            fy - settings.shift_r_y,
        )[0];
        let g = sample_bilinear_clamped(
            source_b,
            fx - settings.shift_g_x,
            fy - settings.shift_g_y,
        )[1];
        let b = sample_bilinear_clamped(
            source_b,
            fx - settings.shift_b_x,
            fy - settings.shift_b_y,
        )[2];
        let a = source_b.pixel(x, y).map_or(0.0, |px| px[3]);

        [r, g, b, a]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(w: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(w, 1, |x, _| {
            let v = x as f32 / (w - 1) as f32;
            [v, 0.5, 0.25, 1.0]
        })
        .unwrap()
    }

    fn dummy_a() -> ImageBufferF32 {
        ImageBufferF32::new(1, 1, vec![[0.0; 4]]).unwrap()
    }

    #[test]
    fn zero_shifts_is_byte_identical_passthrough() {
        let src = ramp(8);
        let settings = ChannelShiftSettings::default();
        let out = render_channel_shift_frame(&dummy_a(), &src, &settings).unwrap();
        assert_eq!(out.pixels, src.pixels);
    }

    #[test]
    fn integer_r_shift_displaces_r_channel() {
        // Ramp: pixel x has R = x/7 (for width=8).
        // Shift R right by 2 → out.R(x) = B.R(x - 2).
        let src = ramp(8);
        let settings = ChannelShiftSettings {
            shift_r_x: 2.0,
            ..Default::default()
        };
        let out = render_channel_shift_frame(&dummy_a(), &src, &settings).unwrap();

        // G and B are unshifted → same as source
        for x in 0..8u32 {
            let src_px = src.pixel(x, 0).unwrap();
            let out_px = out.pixel(x, 0).unwrap();
            // G (ch 1) and B (ch 2) unchanged
            assert_eq!(out_px[1], src_px[1], "G channel changed at x={x}");
            assert_eq!(out_px[2], src_px[2], "B channel changed at x={x}");
        }

        // R at position x should equal source R at (x - 2), clamped at left edge.
        for x in 0..8u32 {
            let src_x = if x < 2 { 0 } else { x - 2 };
            let expected_r = src.pixel(src_x, 0).unwrap()[0];
            let out_r = out.pixel(x, 0).unwrap()[0];
            assert!(
                (out_r - expected_r).abs() < 1e-6,
                "R mismatch at x={x}: got {out_r} expected {expected_r}"
            );
        }
    }

    #[test]
    fn alpha_is_unshifted() {
        // Source has alpha=0.5 everywhere except x=4 which has alpha=1.0
        let src = ImageBufferF32::from_fn(8, 1, |x, _| {
            [0.5, 0.5, 0.5, if x == 4 { 1.0 } else { 0.5 }]
        })
        .unwrap();
        let settings = ChannelShiftSettings {
            shift_r_x: 3.0,
            ..Default::default()
        };
        let out = render_channel_shift_frame(&dummy_a(), &src, &settings).unwrap();
        // Alpha at x=4 should still be 1.0 (from unshifted B.A)
        assert!((out.pixel(4, 0).unwrap()[3] - 1.0).abs() < 1e-6);
        // Alpha at x=5 should be 0.5
        assert!((out.pixel(5, 0).unwrap()[3] - 0.5).abs() < 1e-6);
    }
}
