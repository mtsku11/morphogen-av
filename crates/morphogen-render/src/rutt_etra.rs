//! Rutt-Etra scanline — luma-displaced scanlines on black.
//!
//! The classic analog-video-synth look: the frame is re-rendered as a sparse
//! set of horizontal scanlines on a black canvas, each displaced vertically
//! by its own local luminance (bright regions push the line up). Scanlines
//! are drawn top→bottom, **last-writer-wins**, so a displaced lower line can
//! occlude an earlier one — the classic wireframe-terrain feel.
//!
//! Stateless, single-source (carrier's own luma displaces its own
//! scanlines), CPU-only at this slice — see `docs/RUTT_ETRA_MILESTONE.md`.
//!
//! Off / identity anchor: `displacement_depth == 0.0` ⇒ every scanline stays
//! flat, so rows `y0..y0+line_thickness` equal the source row `y0` verbatim
//! (or white under `--mono`) and every other row is exactly black.

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

pub const RUTT_ETRA_ALGORITHM: &str = "rutt_etra_scanline_cpu_v1";

/// Settings for the Rutt-Etra scanline effect.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RuttEtraSettings {
    /// Rows between scanlines (top row `0` is always included). Must be >= 1.
    pub line_pitch: u32,
    /// Vertical displacement in px at luma `1.0`; sign sets direction
    /// (positive pushes up). Must be finite.
    pub displacement_depth: f32,
    /// Each filled cell extends downward by this many px. Must be >= 1.
    pub line_thickness: u32,
    /// Render every line white `[1,1,1,1]` instead of the source colour.
    pub mono: bool,
}

impl Default for RuttEtraSettings {
    fn default() -> Self {
        Self {
            line_pitch: 8,
            displacement_depth: 48.0,
            line_thickness: 1,
            mono: false,
        }
    }
}

impl RuttEtraSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.line_pitch < 1 {
            return Err(RenderError::InvalidRuttEtraSettings(
                "line_pitch must be >= 1".into(),
            ));
        }
        if self.line_thickness < 1 {
            return Err(RenderError::InvalidRuttEtraSettings(
                "line_thickness must be >= 1".into(),
            ));
        }
        if !self.displacement_depth.is_finite() {
            return Err(RenderError::InvalidRuttEtraSettings(
                "displacement_depth must be finite".into(),
            ));
        }
        Ok(())
    }
}

/// Rec. 709 luma, matching the `conv_blend.rs` / `datamosh.rs` convention.
fn luminance(pixel: [f32; 4]) -> f32 {
    pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722
}

pub fn render_rutt_etra_frame(
    source_b: &ImageBufferF32,
    settings: &RuttEtraSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;

    let width = source_b.width;
    let height = source_b.height;
    let mut pixels = vec![[0.0f32, 0.0, 0.0, 1.0]; width as usize * height as usize];

    let mut y0 = 0u32;
    while y0 < height {
        // Displaced row per column, computed once for this scanline.
        let mut rows: Vec<i64> = Vec::with_capacity(width as usize);
        for x in 0..width {
            let px = source_b.pixel(x, y0).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let luma = luminance(px).clamp(0.0, 1.0);
            let shift = (settings.displacement_depth * luma).round() as i64;
            rows.push((y0 as i64).saturating_sub(shift));
        }

        for x in 0..width {
            let y_a = rows[x as usize];
            let y_b = if x + 1 < width {
                rows[x as usize + 1]
            } else {
                y_a
            };
            let span_lo = y_a.min(y_b);
            let span_hi = y_a
                .max(y_b)
                .saturating_add(settings.line_thickness as i64 - 1);

            let clipped_lo = span_lo.max(0);
            let clipped_hi = span_hi.min(height as i64 - 1);
            if clipped_lo > clipped_hi {
                continue;
            }

            let colour = if settings.mono {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                source_b.pixel(x, y0).unwrap_or([0.0, 0.0, 0.0, 1.0])
            };

            for yy in clipped_lo..=clipped_hi {
                let index = yy as usize * width as usize + x as usize;
                pixels[index] = colour;
            }
        }

        y0 += settings.line_pitch;
    }

    ImageBufferF32::new(width, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_frame(pixels: Vec<[f32; 4]>, w: u32, h: u32) -> ImageBufferF32 {
        ImageBufferF32::new(w, h, pixels).unwrap()
    }

    fn gradient_frame(w: u32, h: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(w, h, |x, _y| {
            let v = x as f32 / (w - 1).max(1) as f32;
            [v, v, v, 1.0]
        })
        .unwrap()
    }

    #[test]
    fn depth_zero_is_identity_on_scanline_rows_and_black_elsewhere() {
        let src = gradient_frame(4, 6);
        let settings = RuttEtraSettings {
            line_pitch: 2,
            displacement_depth: 0.0,
            line_thickness: 1,
            mono: false,
        };
        let out = render_rutt_etra_frame(&src, &settings).unwrap();

        for y in 0..6u32 {
            for x in 0..4u32 {
                let out_px = out.pixel(x, y).unwrap();
                if y % 2 == 0 {
                    let src_px = src.pixel(x, y).unwrap();
                    assert_eq!(out_px, src_px, "scanline row ({x},{y}) should be verbatim");
                } else {
                    assert_eq!(
                        out_px,
                        [0.0, 0.0, 0.0, 1.0],
                        "off-scanline row ({x},{y}) should be black"
                    );
                }
            }
        }
    }

    #[test]
    fn solid_white_shifts_every_line_by_round_of_depth() {
        // pitch == height so only y0 == 0 is a scanline; use a source tall
        // enough to hold the shifted line without top-edge clipping.
        let src = make_frame(vec![[1.0, 1.0, 1.0, 1.0]; 4 * 12], 4, 12);
        let settings = RuttEtraSettings {
            line_pitch: 12,
            displacement_depth: -5.7, // negative depth pushes DOWN; round(-5.7) = -6
            line_thickness: 1,
            mono: false,
        };
        let out = render_rutt_etra_frame(&src, &settings).unwrap();

        // shift = round(depth * luma) = round(-5.7 * 1.0) = -6; y = 0 - (-6) = 6
        for x in 0..4u32 {
            for y in 0..12u32 {
                let px = out.pixel(x, y).unwrap();
                if y == 6 {
                    assert_eq!(
                        px,
                        [1.0, 1.0, 1.0, 1.0],
                        "shifted row ({x},{y}) should be white"
                    );
                } else {
                    assert_eq!(px, [0.0, 0.0, 0.0, 1.0], "row ({x},{y}) should be black");
                }
            }
        }
    }

    #[test]
    fn solid_black_source_matches_depth_zero_output() {
        let black_src = make_frame(vec![[0.0, 0.0, 0.0, 1.0]; 4 * 8], 4, 8);
        let settings_depth = RuttEtraSettings {
            line_pitch: 3,
            displacement_depth: 48.0,
            line_thickness: 2,
            mono: false,
        };
        let settings_zero = RuttEtraSettings {
            displacement_depth: 0.0,
            ..settings_depth
        };

        let out_depth = render_rutt_etra_frame(&black_src, &settings_depth).unwrap();
        let out_zero = render_rutt_etra_frame(&black_src, &settings_zero).unwrap();
        assert_eq!(out_depth, out_zero, "luma 0 must be unaffected by depth");
    }

    #[test]
    fn huge_depth_clips_at_the_top_edge_without_panicking_or_wrapping() {
        let src = make_frame(vec![[1.0, 1.0, 1.0, 1.0]; 4 * 8], 4, 8);
        let settings = RuttEtraSettings {
            line_pitch: 8,
            displacement_depth: f32::MAX,
            line_thickness: 4,
            mono: false,
        };
        let out = render_rutt_etra_frame(&src, &settings).unwrap();

        // Entirely clipped off the top: canvas stays exactly black, no wrap
        // to the bottom rows.
        for y in 0..8u32 {
            for x in 0..4u32 {
                assert_eq!(out.pixel(x, y).unwrap(), [0.0, 0.0, 0.0, 1.0]);
            }
        }

        // Also prove the negative-depth saturation direction never panics
        // or wraps to the top when it should clip at the bottom.
        let settings_neg = RuttEtraSettings {
            displacement_depth: f32::MIN,
            ..settings
        };
        let out_neg = render_rutt_etra_frame(&src, &settings_neg).unwrap();
        for y in 0..8u32 {
            for x in 0..4u32 {
                let _ = out_neg.pixel(x, y).unwrap();
            }
        }
    }

    #[test]
    fn mono_renders_white_lines_instead_of_source_colour() {
        let src = make_frame(vec![[0.2, 0.4, 0.6, 1.0]; 3 * 3], 3, 3);
        let settings = RuttEtraSettings {
            line_pitch: 3,
            displacement_depth: 0.0,
            line_thickness: 1,
            mono: true,
        };
        let out = render_rutt_etra_frame(&src, &settings).unwrap();
        for x in 0..3u32 {
            assert_eq!(out.pixel(x, 0).unwrap(), [1.0, 1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn thickness_extends_the_scanline_downward() {
        let src = make_frame(vec![[0.5, 0.25, 0.75, 1.0]; 4 * 5], 4, 5);
        let settings = RuttEtraSettings {
            line_pitch: 5, // only y0 == 0 is a scanline
            displacement_depth: 0.0,
            line_thickness: 3,
            mono: false,
        };
        let out = render_rutt_etra_frame(&src, &settings).unwrap();
        for x in 0..4u32 {
            for y in 0..3u32 {
                assert_eq!(
                    out.pixel(x, y).unwrap(),
                    [0.5, 0.25, 0.75, 1.0],
                    "row {y} should be filled by thickness"
                );
            }
            for y in 3..5u32 {
                assert_eq!(out.pixel(x, y).unwrap(), [0.0, 0.0, 0.0, 1.0]);
            }
        }
    }

    #[test]
    fn identical_renders_are_byte_identical() {
        let src = gradient_frame(6, 6);
        let settings = RuttEtraSettings::default();
        let a = render_rutt_etra_frame(&src, &settings).unwrap();
        let b = render_rutt_etra_frame(&src, &settings).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn invalid_settings_are_rejected() {
        let src = make_frame(vec![[0.0, 0.0, 0.0, 1.0]], 1, 1);
        assert!(render_rutt_etra_frame(
            &src,
            &RuttEtraSettings {
                line_pitch: 0,
                ..RuttEtraSettings::default()
            }
        )
        .is_err());
        assert!(render_rutt_etra_frame(
            &src,
            &RuttEtraSettings {
                line_thickness: 0,
                ..RuttEtraSettings::default()
            }
        )
        .is_err());
        assert!(render_rutt_etra_frame(
            &src,
            &RuttEtraSettings {
                displacement_depth: f32::NAN,
                ..RuttEtraSettings::default()
            }
        )
        .is_err());
    }
}
