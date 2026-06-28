//! Per-row/column threshold-bounded pixel sorting.
//!
//! The canvas is scanned one line at a time (row or column). Within each line,
//! contiguous runs of pixels whose sort key falls in `[threshold_low, threshold_high]`
//! form **spans**; each span is independently stable-sorted by the chosen key. Pixels
//! outside any span stay in place.
//!
//! **Off case (byte-identical passthrough):** `threshold_low > threshold_high` produces
//! an empty mask — no pixel is sortable — and returns B verbatim. Unit-tested.
//! **No RNG** — stable sort + fixed settings + fixed span boundaries ⇒ bit-reproducible.

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

/// Algorithm identifier — bump when the sorting logic or key computation changes.
pub const PIXEL_SORT_ALGORITHM: &str = "pixel_sort_threshold_span_v1";

/// Which component of a pixel drives the sort order.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    Luma,
    Hue,
    Sat,
    Red,
    Green,
    Blue,
}

impl Default for SortKey {
    fn default() -> Self {
        SortKey::Luma
    }
}

/// Sort order within each span.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

impl Default for SortDirection {
    fn default() -> Self {
        SortDirection::Asc
    }
}

/// Whether to sort along rows (horizontal streaks) or columns (vertical streaks).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SortAxis {
    Row,
    Col,
}

impl Default for SortAxis {
    fn default() -> Self {
        SortAxis::Row
    }
}

/// Knobs for the pixel-sort renderer (Slice 1: single-source, CPU reference).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PixelSortSettings {
    /// Sort direction: `Row` for horizontal streaks, `Col` for vertical.
    pub axis: SortAxis,
    /// Component used to order pixels within a span.
    pub key: SortKey,
    /// Whether spans are sorted lowest-first (`Asc`) or highest-first (`Desc`).
    pub direction: SortDirection,
    /// Lower bound of the sortable mask range [0, 1].
    pub threshold_low: f32,
    /// Upper bound. Set `threshold_low > threshold_high` for passthrough (empty mask).
    pub threshold_high: f32,
    /// Maximum streak length in pixels; `0` = unbounded. Spans longer than this are
    /// sorted in `max_span`-pixel chunks.
    pub max_span: u32,
}

impl Default for PixelSortSettings {
    fn default() -> Self {
        Self {
            axis: SortAxis::Row,
            key: SortKey::Luma,
            direction: SortDirection::Asc,
            threshold_low: 0.25,
            threshold_high: 0.80,
            max_span: 0,
        }
    }
}

impl PixelSortSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.threshold_low < 0.0 || self.threshold_low > 1.0 {
            return Err(RenderError::InvalidPixelSortSettings(
                "threshold_low must be in [0, 1]".into(),
            ));
        }
        if self.threshold_high < 0.0 || self.threshold_high > 1.0 {
            return Err(RenderError::InvalidPixelSortSettings(
                "threshold_high must be in [0, 1]".into(),
            ));
        }
        Ok(())
    }
}

// ─── Sort-key helpers ─────────────────────────────────────────────────────────

/// Rec.709 luma from linear RGB.
///
/// Uses explicit FMA (`mul_add`) to match Metal's `fma()` builtin — both reduce the
/// three multiply-add steps to two roundings in the same order, giving bit-identical
/// sort keys on CPU and GPU without requiring fast_math on either side.
fn luma(r: f32, g: f32, b: f32) -> f32 {
    (0.2126_f32).mul_add(r, (0.7152_f32).mul_add(g, 0.0722_f32 * b))
}

/// HSV hue in [0, 1]. Returns 0 for achromatic pixels.
fn hue(r: f32, g: f32, b: f32) -> f32 {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta < 1e-6 {
        return 0.0;
    }
    let h = if (max - r).abs() < 1e-6 {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < 1e-6 {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    h / 6.0
}

/// HSV saturation in [0, 1].
fn saturation(r: f32, g: f32, b: f32) -> f32 {
    let max = r.max(g).max(b);
    if max < 1e-6 {
        return 0.0;
    }
    let min = r.min(g).min(b);
    (max - min) / max
}

fn pixel_sort_key(px: [f32; 4], key: SortKey) -> f32 {
    let [r, g, b, _] = px;
    match key {
        SortKey::Luma => luma(r, g, b),
        SortKey::Hue => hue(r, g, b),
        SortKey::Sat => saturation(r, g, b),
        SortKey::Red => r,
        SortKey::Green => g,
        SortKey::Blue => b,
    }
}

// ─── Core sorting logic ───────────────────────────────────────────────────────

/// Sort a contiguous slice of pixels in-place by key, ascending or descending.
/// Uses `sort_by` which is stable — tie-breaking follows original order.
fn sort_span(span: &mut [[f32; 4]], key: SortKey, direction: SortDirection) {
    span.sort_by(|a, b| {
        let ka = pixel_sort_key(*a, key);
        let kb = pixel_sort_key(*b, key);
        let ord = ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal);
        match direction {
            SortDirection::Asc => ord,
            SortDirection::Desc => ord.reverse(),
        }
    });
}

/// Walk one scan-line (row or column slice), find maximal sortable spans, and sort them.
fn sort_line(
    line: &mut [[f32; 4]],
    key: SortKey,
    low: f32,
    high: f32,
    direction: SortDirection,
    max_span: usize,
) {
    let n = line.len();
    let mut i = 0;
    while i < n {
        let k = pixel_sort_key(line[i], key);
        if k < low || k > high {
            i += 1;
            continue;
        }
        // Find the end of the maximal contiguous sortable span.
        let span_start = i;
        while i < n && {
            let k2 = pixel_sort_key(line[i], key);
            k2 >= low && k2 <= high
        } {
            i += 1;
        }
        let span = &mut line[span_start..i];
        if max_span == 0 || span.len() <= max_span {
            sort_span(span, key, direction);
        } else {
            for chunk in span.chunks_mut(max_span) {
                sort_span(chunk, key, direction);
            }
        }
    }
}

// ─── Frame renderer ───────────────────────────────────────────────────────────

/// Render one frame of the pixel-sort effect on `source`.
///
/// In Slice 1 this is single-source (mask is derived from `source` itself).
/// The caller passes Source A separately so the driver can pass it through to
/// future cross-synth slices without a breaking API change; it is unused here.
pub fn render_pixel_sort_frame(
    _source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    settings: &PixelSortSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;

    let w = source_b.width;
    let h = source_b.height;

    // Off case: empty mask → return B verbatim (byte-identical).
    if settings.threshold_low > settings.threshold_high {
        return Ok(source_b.clone());
    }

    let mut pixels = source_b.pixels.clone();
    let low = settings.threshold_low;
    let high = settings.threshold_high;
    let key = settings.key;
    let direction = settings.direction;
    let max_span = settings.max_span as usize;

    match settings.axis {
        SortAxis::Row => {
            for y in 0..h as usize {
                let row = &mut pixels[y * w as usize..(y + 1) * w as usize];
                sort_line(row, key, low, high, direction, max_span);
            }
        }
        SortAxis::Col => {
            for x in 0..w as usize {
                let mut col: Vec<[f32; 4]> = (0..h as usize)
                    .map(|y| pixels[y * w as usize + x])
                    .collect();
                sort_line(&mut col, key, low, high, direction, max_span);
                for y in 0..h as usize {
                    pixels[y * w as usize + x] = col[y];
                }
            }
        }
    }

    ImageBufferF32::new(w, h, pixels)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Greyscale pixels: R=G=B=l so that luma([l,l,l,1]) = l exactly.
    fn row_image(lumas: &[f32]) -> ImageBufferF32 {
        let pixels: Vec<[f32; 4]> = lumas.iter().map(|&l| [l, l, l, 1.0]).collect();
        ImageBufferF32::new(lumas.len() as u32, 1, pixels).unwrap()
    }

    fn dummy_a() -> ImageBufferF32 {
        ImageBufferF32::new(1, 1, vec![[0.0; 4]]).unwrap()
    }

    #[test]
    fn empty_mask_is_byte_identical_passthrough() {
        let src = row_image(&[0.8, 0.2, 0.5, 0.1, 0.9]);
        let s = PixelSortSettings {
            threshold_low: 0.9,
            threshold_high: 0.1, // low > high → empty mask
            ..Default::default()
        };
        let out = render_pixel_sort_frame(&dummy_a(), &src, &s).unwrap();
        assert_eq!(out.pixels, src.pixels, "empty mask must return B verbatim");
    }

    #[test]
    fn sorts_row_luma_ascending_all_sortable() {
        // All 8 pixels sortable (threshold [0,1]) → sorted ascending by luma.
        let lumas = [0.8f32, 0.2, 0.6, 0.4, 0.1, 0.9, 0.3, 0.7];
        let src = row_image(&lumas);
        let s = PixelSortSettings {
            threshold_low: 0.0,
            threshold_high: 1.0,
            ..Default::default()
        };
        let out = render_pixel_sort_frame(&dummy_a(), &src, &s).unwrap();
        let mut expected = lumas.to_vec();
        expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for (i, px) in out.pixels.iter().enumerate() {
            assert!(
                (px[0] - expected[i]).abs() < 1e-6,
                "pixel {i}: expected luma {}, got {}",
                expected[i],
                px[0]
            );
        }
    }

    #[test]
    fn sorts_span_bounded_by_threshold() {
        // lumas: [0.1, 0.5, 0.3, 0.9, 0.4, 0.7]
        // threshold [0.3, 0.6]: sortable at indices 1(0.5), 2(0.3), 4(0.4)
        // Contiguous spans: [1,2] and [4]; pixel 3 (0.9) and 5 (0.7) are not sortable.
        let lumas = [0.1f32, 0.5, 0.3, 0.9, 0.4, 0.7];
        let src = row_image(&lumas);
        let s = PixelSortSettings {
            threshold_low: 0.3,
            threshold_high: 0.6,
            ..Default::default()
        };
        let out = render_pixel_sort_frame(&dummy_a(), &src, &s).unwrap();
        // Unsortable pixels stay in place.
        assert!((out.pixels[0][0] - 0.1).abs() < 1e-6, "pixel 0 unchanged");
        assert!((out.pixels[3][0] - 0.9).abs() < 1e-6, "pixel 3 unchanged");
        assert!((out.pixels[5][0] - 0.7).abs() < 1e-6, "pixel 5 unchanged");
        // Span [1,2]: {0.5, 0.3} sorted asc → {0.3, 0.5}.
        assert!((out.pixels[1][0] - 0.3).abs() < 1e-6, "span[0] = 0.3");
        assert!((out.pixels[2][0] - 0.5).abs() < 1e-6, "span[1] = 0.5");
        // Span [4]: single element, unchanged.
        assert!((out.pixels[4][0] - 0.4).abs() < 1e-6, "single-pixel span unchanged");
    }

    #[test]
    fn descending_direction_reverses_order() {
        let lumas = [0.2f32, 0.5, 0.8];
        let src = row_image(&lumas);
        let s = PixelSortSettings {
            threshold_low: 0.0,
            threshold_high: 1.0,
            direction: SortDirection::Desc,
            ..Default::default()
        };
        let out = render_pixel_sort_frame(&dummy_a(), &src, &s).unwrap();
        // Descending: 0.8, 0.5, 0.2.
        assert!((out.pixels[0][0] - 0.8).abs() < 1e-6);
        assert!((out.pixels[1][0] - 0.5).abs() < 1e-6);
        assert!((out.pixels[2][0] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn max_span_chunks_long_span() {
        // 6 sortable pixels; max_span=3 → sorted in two chunks of 3.
        let lumas = [0.6f32, 0.1, 0.4, 0.9, 0.2, 0.7];
        let src = row_image(&lumas);
        let s = PixelSortSettings {
            threshold_low: 0.0,
            threshold_high: 1.0,
            max_span: 3,
            ..Default::default()
        };
        let out = render_pixel_sort_frame(&dummy_a(), &src, &s).unwrap();
        // Chunk 0: [0.6, 0.1, 0.4] → [0.1, 0.4, 0.6]
        assert!((out.pixels[0][0] - 0.1).abs() < 1e-6);
        assert!((out.pixels[1][0] - 0.4).abs() < 1e-6);
        assert!((out.pixels[2][0] - 0.6).abs() < 1e-6);
        // Chunk 1: [0.9, 0.2, 0.7] → [0.2, 0.7, 0.9]
        assert!((out.pixels[3][0] - 0.2).abs() < 1e-6);
        assert!((out.pixels[4][0] - 0.7).abs() < 1e-6);
        assert!((out.pixels[5][0] - 0.9).abs() < 1e-6);
    }
}
