//! Per-channel spatial offset (RGB split / chromatic aberration).
//!
//! Each colour channel is sampled from Source B at an independently shifted
//! position using clamped bilinear interpolation.  Alpha passes through
//! unchanged from the unshifted position.
//!
//! **Off case (byte-identical passthrough):** all six offsets zero and no
//! per-row flow shift ⇒ each channel is sampled at its natural position,
//! returning B verbatim.

use serde::{Deserialize, Serialize};

use crate::{sampler::sample_bilinear_clamped, FlowField, ImageBufferF32, RenderError};

pub const CHANNEL_SHIFT_ALGORITHM: &str = "channel_shift_constant_cpu_v1";
pub const CHANNEL_SHIFT_FLOW_ALGORITHM: &str = "channel_shift_flow_driven_cpu_v1";

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

/// Derive per-row X-shifts from an optical-flow field (Slice 3, A-flow mode).
///
/// For each row y the mean X-component of all flow vectors in that row is
/// multiplied by `gain` to give the row's extra horizontal shift.  The shift
/// is added to every channel's X-offset at that row.
///
/// `gain = 0.0` returns a vector of zeros (no-op, preserves byte-identical
/// passthrough when combined with zero constant offsets).
pub fn compute_per_row_shifts(flow: &FlowField, gain: f32) -> Vec<f32> {
    if flow.width == 0 {
        return vec![0.0; flow.height as usize];
    }
    (0..flow.height)
        .map(|y| {
            let base = (y * flow.width) as usize;
            let sum: f32 = flow.vectors[base..base + flow.width as usize]
                .iter()
                .map(|v| v[0])
                .sum();
            sum / flow.width as f32 * gain
        })
        .collect()
}

/// Render one frame of channel-shift.
///
/// `per_row_shift_x`: optional per-row X-offset added to every channel
/// (A-flow-driven mode, Slice 3).  Pass `&[]` for the constant-offset path.
pub fn render_channel_shift_frame(
    source_b: &ImageBufferF32,
    settings: &ChannelShiftSettings,
    per_row_shift_x: &[f32],
) -> Result<ImageBufferF32, RenderError> {
    if settings.is_passthrough() && per_row_shift_x.is_empty() {
        return Ok(source_b.clone());
    }

    let w = source_b.width;
    let h = source_b.height;

    ImageBufferF32::from_fn(w, h, |x, y| {
        let fx = x as f32;
        let fy = y as f32;
        let extra_x = per_row_shift_x.get(y as usize).copied().unwrap_or(0.0);

        let r = sample_bilinear_clamped(
            source_b,
            fx - settings.shift_r_x - extra_x,
            fy - settings.shift_r_y,
        )[0];
        let g = sample_bilinear_clamped(
            source_b,
            fx - settings.shift_g_x - extra_x,
            fy - settings.shift_g_y,
        )[1];
        let b = sample_bilinear_clamped(
            source_b,
            fx - settings.shift_b_x - extra_x,
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

    #[test]
    fn zero_shifts_is_byte_identical_passthrough() {
        let src = ramp(8);
        let settings = ChannelShiftSettings::default();
        let out = render_channel_shift_frame(&src, &settings, &[]).unwrap();
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
        let out = render_channel_shift_frame(&src, &settings, &[]).unwrap();

        // G and B are unshifted → same as source
        for x in 0..8u32 {
            let src_px = src.pixel(x, 0).unwrap();
            let out_px = out.pixel(x, 0).unwrap();
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
        let src = ImageBufferF32::from_fn(8, 1, |x, _| {
            [0.5, 0.5, 0.5, if x == 4 { 1.0 } else { 0.5 }]
        })
        .unwrap();
        let settings = ChannelShiftSettings {
            shift_r_x: 3.0,
            ..Default::default()
        };
        let out = render_channel_shift_frame(&src, &settings, &[]).unwrap();
        assert!((out.pixel(4, 0).unwrap()[3] - 1.0).abs() < 1e-6);
        assert!((out.pixel(5, 0).unwrap()[3] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn compute_per_row_shifts_scales_mean_x_flow() {
        // 4-wide × 2-tall flow; row 0 x-components: [1,3,1,3] mean=2; row 1: [0,0,0,4] mean=1
        let flow = FlowField::from_fn(4, 2, |x, y| {
            let vx = if y == 0 {
                if x % 2 == 1 { 3.0 } else { 1.0 }
            } else {
                if x == 3 { 4.0 } else { 0.0 }
            };
            [vx, 0.0]
        })
        .unwrap();
        let shifts = compute_per_row_shifts(&flow, 2.0);
        assert_eq!(shifts.len(), 2);
        assert!((shifts[0] - 4.0).abs() < 1e-5, "row 0: {}", shifts[0]); // mean=2 × gain=2 = 4
        assert!((shifts[1] - 2.0).abs() < 1e-5, "row 1: {}", shifts[1]); // mean=1 × gain=2 = 2
    }

    #[test]
    fn zero_gain_gives_zero_row_shifts() {
        let flow = FlowField::from_fn(4, 3, |_, _| [5.0, 5.0]).unwrap();
        let shifts = compute_per_row_shifts(&flow, 0.0);
        assert!(shifts.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn per_row_shift_displaces_all_channels_equally() {
        // Single-row ramp with all channels equal to x/7.
        let src = ImageBufferF32::from_fn(8, 1, |x, _| {
            let v = x as f32 / 7.0;
            [v, v, v, 1.0]
        })
        .unwrap();
        let settings = ChannelShiftSettings::default();
        let out = render_channel_shift_frame(&src, &settings, &[2.0]).unwrap();
        // All channels at position x should be drawn from x-2 in source.
        for x in 0..8u32 {
            let src_x = if x < 2 { 0 } else { x - 2 };
            let expected = src.pixel(src_x, 0).unwrap()[0];
            let px = out.pixel(x, 0).unwrap();
            for ch in 0..3usize {
                assert!(
                    (px[ch] - expected).abs() < 1e-6,
                    "ch {ch} mismatch at x={x}: got {} expected {expected}",
                    px[ch]
                );
            }
        }
    }
}
