//! Palette quantize — reduce a frame to a limited colour palette.
//!
//! **Slice 1 — posterize:** each channel is uniformly quantized to `levels`
//! discrete steps via `round(c * (L-1)) / (L-1)`.  `L = 256` ⇒ the step grid
//! covers 8-bit values exactly, so any PNG-sourced frame (already quantized to
//! multiples of `1/255`) is returned byte-identical.
//!
//! **Off case (byte-identical passthrough):** `levels >= 256` ⇒ B verbatim.

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

pub const PALETTE_QUANTIZE_ALGORITHM: &str = "palette_quantize_posterize_cpu_v1";

/// Built-in neon palette: magenta / orange / teal / black.
/// Values are exact in f32 to guarantee CPU/GPU bit-equality.
pub const NEON_PALETTE: [[f32; 3]; 4] = [
    [1.0, 0.0, 1.0],   // magenta
    [1.0, 0.5, 0.0],   // neon orange
    [0.0, 0.75, 0.75], // teal
    [0.0, 0.0, 0.0],   // black
];

/// Quantize mode.  Only `Posterize` is implemented in Slice 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum QuantizeMode {
    #[default]
    Posterize,
    Palette,
    Kmeans,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PaletteQuantizeSettings {
    pub mode: QuantizeMode,
    /// Number of discrete steps per channel for `Posterize` mode (2–256).
    /// 256 → byte-identical passthrough for 8-bit PNG sources.
    pub levels: u32,
}

impl Default for PaletteQuantizeSettings {
    fn default() -> Self {
        Self {
            mode: QuantizeMode::Posterize,
            levels: 256,
        }
    }
}

impl PaletteQuantizeSettings {
    fn validate(&self) -> Result<(), RenderError> {
        if matches!(self.mode, QuantizeMode::Posterize) && self.levels < 2 {
            return Err(RenderError::InvalidPaletteQuantizeSettings(
                "levels must be >= 2 for posterize mode".into(),
            ));
        }
        Ok(())
    }

    fn is_passthrough(&self) -> bool {
        matches!(self.mode, QuantizeMode::Posterize) && self.levels >= 256
    }
}

pub fn render_palette_quantize_frame(
    source_b: &ImageBufferF32,
    settings: &PaletteQuantizeSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;

    if settings.is_passthrough() {
        return Ok(source_b.clone());
    }

    match settings.mode {
        QuantizeMode::Posterize => posterize(source_b, settings.levels),
        QuantizeMode::Palette => palette_map(source_b),
        QuantizeMode::Kmeans => Err(RenderError::InvalidPaletteQuantizeSettings(
            "kmeans mode is not yet implemented (Slice 3)".into(),
        )),
    }
}

fn palette_nearest(r: f32, g: f32, b: f32) -> [f32; 3] {
    let mut best_dist = f32::MAX;
    let mut best = 0;
    for (i, c) in NEON_PALETTE.iter().enumerate() {
        let dr = r - c[0];
        let dg = g - c[1];
        let db = b - c[2];
        let d = dr * dr + dg * dg + db * db;
        if d < best_dist {
            best_dist = d;
            best = i;
        }
    }
    NEON_PALETTE[best]
}

fn palette_map(source_b: &ImageBufferF32) -> Result<ImageBufferF32, RenderError> {
    ImageBufferF32::from_fn(source_b.width, source_b.height, |x, y| {
        let px = source_b.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 0.0]);
        let out = palette_nearest(px[0], px[1], px[2]);
        [out[0], out[1], out[2], px[3]]
    })
}

fn posterize(source_b: &ImageBufferF32, levels: u32) -> Result<ImageBufferF32, RenderError> {
    let scale = (levels - 1) as f32;
    ImageBufferF32::from_fn(source_b.width, source_b.height, |x, y| {
        let px = source_b.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 0.0]);
        [
            (px[0] * scale).round() / scale,
            (px[1] * scale).round() / scale,
            (px[2] * scale).round() / scale,
            px[3],
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageBufferF32;

    fn make_frame(pixels: Vec<[f32; 4]>, w: u32, h: u32) -> ImageBufferF32 {
        ImageBufferF32::new(w, h, pixels).unwrap()
    }

    #[test]
    fn levels_256_is_passthrough() {
        let px: Vec<[f32; 4]> = (0..16)
            .map(|i| {
                let v = i as f32 / 255.0;
                [v, v, v, 1.0]
            })
            .collect();
        let src = make_frame(px.clone(), 4, 4);
        let settings = PaletteQuantizeSettings {
            mode: QuantizeMode::Posterize,
            levels: 256,
        };
        let out = render_palette_quantize_frame(&src, &settings).unwrap();
        // byte-identical: every pixel unchanged
        for y in 0..4 {
            for x in 0..4 {
                let a = src.pixel(x, y).unwrap();
                let b = out.pixel(x, y).unwrap();
                assert_eq!(a, b, "pixel ({x},{y}) changed at levels=256");
            }
        }
    }

    #[test]
    fn posterize_levels_2_clamps_to_black_or_white() {
        // With levels=2, only 0.0 and 1.0 are valid output values.
        let pixels = vec![
            [0.1, 0.4, 0.6, 1.0],
            [0.9, 0.51, 0.49, 0.5],
            [0.0, 1.0, 0.5, 0.0],
            [0.25, 0.75, 0.3, 1.0],
        ];
        let src = make_frame(pixels, 2, 2);
        let settings = PaletteQuantizeSettings {
            mode: QuantizeMode::Posterize,
            levels: 2,
        };
        let out = render_palette_quantize_frame(&src, &settings).unwrap();
        for y in 0..2 {
            for x in 0..2 {
                let px = out.pixel(x, y).unwrap();
                for (ch, &v) in px.iter().take(3).enumerate() {
                    assert!(
                        v == 0.0 || v == 1.0,
                        "levels=2 pixel ({x},{y}) ch {ch}: expected 0 or 1, got {v}"
                    );
                }
            }
        }
    }

    #[test]
    fn posterize_levels_4_quantises_known_values() {
        // levels=4: steps at 0.0, 1/3, 2/3, 1.0
        // input 0.5 → round(0.5*3)/3 = round(1.5)/3 = 2/3 ≈ 0.6667
        let pixels = vec![[0.5f32, 0.0, 1.0, 1.0]];
        let src = make_frame(pixels, 1, 1);
        let settings = PaletteQuantizeSettings {
            mode: QuantizeMode::Posterize,
            levels: 4,
        };
        let out = render_palette_quantize_frame(&src, &settings).unwrap();
        let px = out.pixel(0, 0).unwrap();
        let expected_r = 2.0f32 / 3.0;
        assert!(
            (px[0] - expected_r).abs() < 1e-6,
            "0.5 → expected {expected_r}, got {}",
            px[0]
        );
        assert_eq!(px[1], 0.0, "0.0 → 0.0");
        assert_eq!(px[2], 1.0, "1.0 → 1.0");
        assert_eq!(px[3], 1.0, "alpha unchanged");
    }

    #[test]
    fn alpha_always_passes_through() {
        let pixels = vec![[0.5f32, 0.5, 0.5, 0.3]];
        let src = make_frame(pixels, 1, 1);
        let settings = PaletteQuantizeSettings {
            mode: QuantizeMode::Posterize,
            levels: 4,
        };
        let out = render_palette_quantize_frame(&src, &settings).unwrap();
        assert_eq!(out.pixel(0, 0).unwrap()[3], 0.3, "alpha must pass through");
    }

    #[test]
    fn levels_1_is_rejected() {
        let src = make_frame(vec![[0.5, 0.5, 0.5, 1.0]], 1, 1);
        let settings = PaletteQuantizeSettings {
            mode: QuantizeMode::Posterize,
            levels: 1,
        };
        assert!(render_palette_quantize_frame(&src, &settings).is_err());
    }

    #[test]
    fn palette_mode_maps_to_nearest_neon_colour() {
        // Pure red (1,0,0): nearest is magenta (1,0,1) not orange (1,0.5,0)
        // because dist-to-magenta = 1.0 < dist-to-orange = 0.25 ... wait
        // dist_magenta(1,0,0) = (0)^2+(0)^2+(1)^2 = 1.0
        // dist_orange(1,0,0)  = (0)^2+(0.5)^2+(0)^2 = 0.25
        // so nearest is orange
        let pixels = vec![
            [1.0f32, 0.0, 0.0, 1.0], // near orange (1,0.5,0): d=0.25 < magenta d=1.0
            [0.0, 0.0, 0.0, 0.5],    // black: exact match
            [1.0, 0.0, 1.0, 1.0],    // magenta: exact match
            [0.0, 1.0, 1.0, 0.0],    // nearest teal (0,0.75,0.75): d=0.0625*2=0.125
        ];
        let src = make_frame(pixels, 2, 2);
        let settings = PaletteQuantizeSettings {
            mode: QuantizeMode::Palette,
            levels: 256, // ignored in palette mode
        };
        let out = render_palette_quantize_frame(&src, &settings).unwrap();

        // (1,0,0) → orange (1,0.5,0)
        let p = out.pixel(0, 0).unwrap();
        assert_eq!([p[0], p[1], p[2]], [1.0, 0.5, 0.0], "red → orange");

        // (0,0,0) → black (0,0,0)
        let p = out.pixel(1, 0).unwrap();
        assert_eq!([p[0], p[1], p[2]], [0.0, 0.0, 0.0], "black → black");

        // (1,0,1) → magenta
        let p = out.pixel(0, 1).unwrap();
        assert_eq!([p[0], p[1], p[2]], [1.0, 0.0, 1.0], "magenta → magenta");

        // alpha passes through
        assert_eq!(out.pixel(1, 0).unwrap()[3], 0.5, "alpha passes through");
    }
}
