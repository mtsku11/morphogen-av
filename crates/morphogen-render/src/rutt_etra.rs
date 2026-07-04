//! Rutt-Etra scanline — luma-displaced scanlines on black.
//!
//! The classic analog-video-synth look: the frame is re-rendered as a sparse
//! set of horizontal scanlines on a black canvas, each displaced vertically
//! by its own local luminance (bright regions push the line up). Scanlines
//! are drawn top→bottom, **last-writer-wins**, so a displaced lower line can
//! occlude an earlier one — the classic wireframe-terrain feel.
//!
//! Stateless. Single-source drives the carrier's own luma; two-source
//! ([`render_rutt_etra_two_source_frame`]) lets Source A's luma drive the
//! displacement while Source B supplies the colour (B's material reorganised by
//! A's structure). See `docs/RUTT_ETRA_MILESTONE.md` and
//! `docs/RUTT_ETRA_TWO_SOURCE_MILESTONE.md`.
//!
//! Off / identity anchor: `displacement_depth == 0.0` ⇒ every scanline stays
//! flat, so rows `y0..y0+line_thickness` equal the source row `y0` verbatim
//! (or white under `--mono`) and every other row is exactly black.

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

pub const RUTT_ETRA_ALGORITHM: &str = "rutt_etra_scanline_cpu_v1";
pub const RUTT_ETRA_METAL_ALGORITHM: &str = "rutt_etra_scanline_metal_v1";
pub const RUTT_ETRA_TWO_SOURCE_ALGORITHM: &str = "rutt_etra_two_source_cpu_v1";

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

/// Single-source Rutt-Etra: the carrier's own luma displaces its own scanlines,
/// drawn in its own colour. This is the `luma_source == colour_source` special
/// case of [`render_rutt_etra_two_source_frame`].
pub fn render_rutt_etra_frame(
    source_b: &ImageBufferF32,
    settings: &RuttEtraSettings,
) -> Result<ImageBufferF32, RenderError> {
    render_rutt_etra_core(source_b, source_b, settings)
}

/// Two-source Rutt-Etra cross-synthesis: Source A's luma drives the vertical
/// scanline displacement while Source B supplies the drawn colour — B's material
/// reorganised by A's structure. A and B must share dimensions.
///
/// With `source_a == source_b` this is byte-identical to
/// [`render_rutt_etra_frame`] (the continuity identity), because the single-source
/// path delegates here with both arguments pointing at the carrier.
pub fn render_rutt_etra_two_source_frame(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    settings: &RuttEtraSettings,
) -> Result<ImageBufferF32, RenderError> {
    if source_a.width != source_b.width || source_a.height != source_b.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "Source A is {}x{}, Source B is {}x{}; rutt-etra two-source requires equal dimensions",
            source_a.width, source_a.height, source_b.width, source_b.height
        )));
    }
    render_rutt_etra_core(source_a, source_b, settings)
}

/// Shared gather core: `luma_source` supplies the displacement luma, `colour_source`
/// supplies the drawn colour and the output canvas dimensions. Callers guarantee
/// the two buffers share dimensions.
fn render_rutt_etra_core(
    luma_source: &ImageBufferF32,
    colour_source: &ImageBufferF32,
    settings: &RuttEtraSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;

    let width = colour_source.width;
    let height = colour_source.height;
    let mut pixels = vec![[0.0f32, 0.0, 0.0, 1.0]; width as usize * height as usize];

    let mut y0 = 0u32;
    while y0 < height {
        // Displaced row per column, computed once for this scanline — luma from A.
        let mut rows: Vec<i64> = Vec::with_capacity(width as usize);
        for x in 0..width {
            let px = luma_source.pixel(x, y0).unwrap_or([0.0, 0.0, 0.0, 1.0]);
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

            // Colour from B.
            let colour = if settings.mono {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                colour_source.pixel(x, y0).unwrap_or([0.0, 0.0, 0.0, 1.0])
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
    fn two_source_with_a_equal_b_matches_single_source() {
        // The keystone continuity identity: A == B must be byte-identical to the
        // single-source render across the full knob set.
        let src = gradient_frame(7, 9);
        let settings = RuttEtraSettings {
            line_pitch: 3,
            displacement_depth: 40.0,
            line_thickness: 2,
            mono: false,
        };
        let single = render_rutt_etra_frame(&src, &settings).unwrap();
        let two = render_rutt_etra_two_source_frame(&src, &src, &settings).unwrap();
        assert_eq!(single, two, "A==B must reduce to the single-source render");
    }

    #[test]
    fn two_source_displaces_by_a_and_colours_by_b() {
        // A = a horizontal luma ramp (drives displacement per column); B = a flat
        // distinctive colour (the fill). Every filled cell must carry B's colour,
        // and the displacement must follow A's luma, not B's (B is uniform → its
        // own single-source render would be a flat line at y0).
        let a = gradient_frame(4, 8); // luma 0 → 1 across x
        let b_colour = [0.1, 0.7, 0.3, 1.0];
        let b = make_frame(vec![b_colour; 4 * 8], 4, 8);
        let settings = RuttEtraSettings {
            line_pitch: 8,            // only y0 == 0 is a scanline
            displacement_depth: -6.0, // negative pushes DOWN; shift = round(-6*luma)
            line_thickness: 1,
            mono: false,
        };
        let out = render_rutt_etra_two_source_frame(&a, &b, &settings).unwrap();

        // Every non-black pixel is exactly B's colour (never A's ramp colour).
        for y in 0..8u32 {
            for x in 0..4u32 {
                let px = out.pixel(x, y).unwrap();
                if px != [0.0, 0.0, 0.0, 1.0] {
                    assert_eq!(px, b_colour, "fill at ({x},{y}) must be B's colour");
                }
            }
        }

        // Column 0 (A luma 0) stays at y0 == 0; the rightmost columns (A luma → 1)
        // push down to y = round(6) = 6. Contrast with B's own single-source render,
        // which — B being uniform mid-luma — would put every column on the same row.
        assert_eq!(
            out.pixel(0, 0).unwrap(),
            b_colour,
            "A-luma 0 column stays at y0"
        );
        // A's neighbour-span reaches the pushed-down row on the bright side.
        let bright_pushed = (0..8u32).any(|y| y >= 5 && out.pixel(3, y).unwrap() == b_colour);
        assert!(
            bright_pushed,
            "bright A column must displace B's line downward"
        );
    }

    #[test]
    fn two_source_dimension_mismatch_errors() {
        let a = make_frame(vec![[0.5, 0.5, 0.5, 1.0]; 4 * 4], 4, 4);
        let b = make_frame(vec![[0.5, 0.5, 0.5, 1.0]; 3 * 4], 3, 4);
        let result = render_rutt_etra_two_source_frame(&a, &b, &RuttEtraSettings::default());
        assert!(result.is_err(), "mismatched A/B dimensions must error");
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
