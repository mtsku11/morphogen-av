//! Retro static — a deliberate scanline-filter misread glitch.
//!
//! Discovered by accident (a diagnostic script decoded PNG scanline data assuming
//! the wrong bytes-per-pixel and no adaptive-filter reconstruction) and rebuilt as
//! a real pixel-space effect: the source is (1) deterministically re-encoded as if
//! it were a scanline-filtered image (a PNG-style adaptive filter — `None`/`Sub`/
//! `Up`/`Average`/`Paeth` — predicting each byte from *raw*, never-filtered,
//! neighbour bytes), then (2) deliberately **misread** at a different row stride
//! (`assumed_bpp` instead of the `real_bpp` used to encode it). Filter residuals
//! get displayed as if they were absolute colour, and the per-row misalignment
//! compounds every row, shearing the image sideways — the "analog TV losing sync"
//! look. See `docs/RETRO_STATIC_MILESTONE.md` for the full mechanism.
//!
//! Every step is integer byte arithmetic (mod 256) with no raster dependency
//! chain (predictors read only the source's own quantized values, never other
//! filtered output), so it is embarrassingly parallel and CPU/Metal should be
//! bit-identical — no trig, no float accumulation to diverge.
//!
//! Stateless, single-source, per-frame: a frame depends only on
//! `(source frame, settings)`. Flicker across a real clip comes from the source's
//! own motion, not any internal state.
//!
//! Continuity identity (off-vs-on readout): `strength == 0.0` short-circuits to
//! the source verbatim (no computation), the exact off case.

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

pub const RETRO_STATIC_ALGORITHM: &str = "retro_static_scanline_misread_cpu_v1";

/// Simulated PNG-style adaptive scanline filter used at the (fake) encode step.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanlineFilter {
    /// Filtered bytes equal raw bytes (no residual noise; shear alone if
    /// `assumed_bpp != real_bpp`).
    #[default]
    None,
    /// Predict from the left neighbour.
    Sub,
    /// Predict from the neighbour above.
    Up,
    /// Predict from the average of left and above.
    Average,
    /// Paeth predictor (left, above, upper-left).
    Paeth,
}

/// Settings for the retro-static glitch.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RetroStaticSettings {
    /// Simulated encoder's bytes-per-pixel (3 = RGB, 4 = RGBA typical; the 4th
    /// "alpha" slot is always a constant 255 byte, slots beyond 4 are 0 padding).
    pub real_bpp: u32,
    /// The "wrong" decoder's bytes-per-pixel — the shear knob. Equal to
    /// `real_bpp` ⇒ no shear (filter-residual noise only).
    pub assumed_bpp: u32,
    /// Simulated adaptive filter applied at encode time.
    pub filter: ScanlineFilter,
    /// Blend toward the glitch, in `[0, 1]`. `0` ⇒ byte-identical passthrough.
    pub strength: f32,
}

impl Default for RetroStaticSettings {
    fn default() -> Self {
        Self {
            real_bpp: 4,
            assumed_bpp: 3,
            filter: ScanlineFilter::Paeth,
            strength: 1.0,
        }
    }
}

impl RetroStaticSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.real_bpp == 0 || self.assumed_bpp == 0 {
            return Err(RenderError::InvalidRetroStaticSettings(
                "real_bpp and assumed_bpp must be >= 1".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.strength) {
            return Err(RenderError::InvalidRetroStaticSettings(
                "strength must be in [0, 1]".into(),
            ));
        }
        Ok(())
    }
}

/// The raw (unfiltered) simulated R/G/B byte for pixel `(x, y)`. Only ever called
/// with `c` in `0..3` — [`raw_channel`] handles the synthetic alpha/padding slots.
fn raw_byte(source: &ImageBufferF32, x: i64, y: i64, c: u32) -> u8 {
    if x < 0 || y < 0 {
        return 0;
    }
    let (w, h) = (source.width as i64, source.height as i64);
    if x >= w || y >= h {
        return 0;
    }
    let px = source
        .pixel(x as u32, y as u32)
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    match c {
        0 => quantize(px[0]),
        1 => quantize(px[1]),
        _ => quantize(px[2]),
    }
}

fn quantize(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// The channel-cycled raw byte used for both the predictor and the filtered value
/// itself: slot `real_bpp`-relative index `slot` within a row maps to pixel
/// `x = slot / real_bpp`, channel `c = slot % real_bpp`.
fn raw_channel(source: &ImageBufferF32, x: i64, y: i64, slot_channel: u32) -> u8 {
    if slot_channel == 3 {
        return 255; // synthetic opaque alpha byte, matching a real RGBA PNG
    }
    if slot_channel > 3 {
        return 0; // padding beyond RGBA — exotic bpp knob values
    }
    raw_byte(source, x, y, slot_channel)
}

fn paeth_predictor(a: i32, b: i32, c: i32) -> i32 {
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// The filtered byte at row `y`, intra-row slot `slot` (`0..w*real_bpp`), given
/// the chosen filter. Predictor reads *raw* (never filtered) neighbour bytes.
fn filtered_byte(
    source: &ImageBufferF32,
    y: i64,
    slot: i64,
    real_bpp: u32,
    filter: ScanlineFilter,
) -> u8 {
    let x = slot / real_bpp as i64;
    let c = (slot % real_bpp as i64) as u32;
    let raw = raw_channel(source, x, y, c) as i32;
    let left = raw_channel(source, x - 1, y, c) as i32;
    let up = raw_channel(source, x, y - 1, c) as i32;
    let up_left = raw_channel(source, x - 1, y - 1, c) as i32;
    let predictor = match filter {
        ScanlineFilter::None => 0,
        ScanlineFilter::Sub => left,
        ScanlineFilter::Up => up,
        ScanlineFilter::Average => (left + up) / 2,
        ScanlineFilter::Paeth => paeth_predictor(left, up, up_left),
    };
    (raw - predictor).rem_euclid(256) as u8
}

/// Render one frame of the retro-static glitch.
pub fn render_retro_static_frame(
    source: &ImageBufferF32,
    settings: &RetroStaticSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    if settings.strength <= 0.0 {
        return Ok(source.clone());
    }

    let w = source.width;
    let h = source.height;
    let real_bpp = settings.real_bpp;
    let assumed_bpp = settings.assumed_bpp;
    let real_row_len = w as i64 * real_bpp as i64; // filtered bytes per row (marker byte excluded)
    let assumed_stride = w as i64 * assumed_bpp as i64 + 1; // + 1 assumed (unused) marker byte
    let total_filtered_len = h as i64 * real_row_len; // concatenated filtered stream, ignoring marker bytes

    // Concatenated filtered byte stream, marker bytes omitted (misread never
    // reads them meaningfully — it only skips a fixed 1-byte offset per row).
    let mut filtered = vec![0u8; total_filtered_len.max(0) as usize];
    for y in 0..h as i64 {
        let row_base = y * real_row_len;
        for slot in 0..real_row_len {
            filtered[(row_base + slot) as usize] =
                filtered_byte(source, y, slot, real_bpp, settings.filter);
        }
    }

    let strength = settings.strength;
    let out = ImageBufferF32::from_fn(w, h, |x, y| {
        let src = source.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 1.0]);
        // Misread: row y starts at y*assumed_stride, skip 1 assumed marker byte,
        // then read 3 bytes for (R,G,B) directly from the FILTERED stream at the
        // position a `real_bpp`-stride row would occupy -- the deliberate bug.
        let start = y as i64 * assumed_stride + 1;
        let mut rgb = [0u8; 3];
        for (i, slot) in rgb.iter_mut().enumerate() {
            let idx = start + x as i64 * 3 + i as i64;
            *slot = if idx >= 0 && idx < total_filtered_len {
                filtered[idx as usize]
            } else {
                0
            };
        }
        let glitch = [
            rgb[0] as f32 / 255.0,
            rgb[1] as f32 / 255.0,
            rgb[2] as f32 / 255.0,
            1.0,
        ];
        [
            src[0] + (glitch[0] - src[0]) * strength,
            src[1] + (glitch[1] - src[1]) * strength,
            src[2] + (glitch[2] - src[2]) * strength,
            src[3],
        ]
    })?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(w: u32, h: u32, seed: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(w, h, |x, y| {
            let v = ((x * 7 + y * 13 + seed) % 255) as f32 / 255.0;
            [v, (v * 0.6).fract(), (v * 0.3 + 0.2).fract(), 1.0]
        })
        .unwrap()
    }

    #[test]
    fn same_inputs_are_byte_identical() {
        let src = source(48, 32, 3);
        let s = RetroStaticSettings::default();
        let a = render_retro_static_frame(&src, &s).unwrap();
        let b = render_retro_static_frame(&src, &s).unwrap();
        assert_eq!(a, b, "A1: identical inputs must be byte-identical");
    }

    #[test]
    fn zero_strength_is_exact_passthrough() {
        let src = source(40, 24, 5);
        let s = RetroStaticSettings {
            strength: 0.0,
            ..Default::default()
        };
        let out = render_retro_static_frame(&src, &s).unwrap();
        assert_eq!(out, src, "A2: strength=0 must equal source exactly");
    }

    #[test]
    fn nonzero_strength_differs_from_source() {
        let src = source(40, 24, 7);
        let s = RetroStaticSettings::default();
        let out = render_retro_static_frame(&src, &s).unwrap();
        let d = out.max_channel_difference(&src).expect("comparable");
        assert!(d > 0.0, "A3: on must differ from source");
    }

    #[test]
    fn shear_differs_from_no_shear() {
        let src = source(48, 32, 11);
        let no_shear = RetroStaticSettings {
            real_bpp: 3,
            assumed_bpp: 3,
            filter: ScanlineFilter::Paeth,
            strength: 1.0,
        };
        let shear = RetroStaticSettings {
            real_bpp: 4,
            assumed_bpp: 3,
            filter: ScanlineFilter::Paeth,
            strength: 1.0,
        };
        let a = render_retro_static_frame(&src, &no_shear).unwrap();
        let b = render_retro_static_frame(&src, &shear).unwrap();
        let d = a.max_channel_difference(&b).expect("comparable");
        assert!(
            d > 0.0,
            "A4: real_bpp != assumed_bpp must differ from real_bpp == assumed_bpp"
        );
    }

    #[test]
    fn validate_rejects_zero_bpp_and_bad_strength() {
        let mut s = RetroStaticSettings {
            real_bpp: 0,
            ..Default::default()
        };
        assert!(s.validate().is_err());
        s = RetroStaticSettings {
            strength: 1.5,
            ..Default::default()
        };
        assert!(s.validate().is_err());
    }
}
