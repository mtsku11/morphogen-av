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

use crate::{sampler::sample_bilinear_clamped, FlowField, ImageBufferF32, RenderError};

/// Algorithm identifier for single-source (self-mask) mode.
pub const PIXEL_SORT_ALGORITHM: &str = "pixel_sort_threshold_span_v1";
/// Algorithm identifier for cross-synth mask modes (a-luma, a-edge, a-flow).
pub const PIXEL_SORT_CROSS_SYNTH_ALGORITHM: &str = "pixel_sort_cross_synth_mask_v1";

/// What drives the span-detection mask (determines which pixels are sortable).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MaskSource {
    /// Self mask: B's own sort-key value determines sortability (single-source classic).
    #[serde(rename = "self")]
    SelfMask,
    /// A's Rec.709 luma (resampled to B's grid) determines sortability.
    ALuma,
    /// Sobel magnitude of A's luma (resampled to B's grid) — sorts between edges, not across them.
    AEdge,
    /// Optical-flow magnitude between consecutive A frames (peak-normalised) — moving regions sort.
    AFlow,
}

impl Default for MaskSource {
    fn default() -> Self {
        MaskSource::SelfMask
    }
}

impl MaskSource {
    /// Returns true when A frames are needed (to compute the mask).
    pub fn needs_source_a(self) -> bool {
        !matches!(self, MaskSource::SelfMask)
    }
    /// Returns true when two consecutive A frames are needed (optical flow).
    pub fn needs_flow(self) -> bool {
        matches!(self, MaskSource::AFlow)
    }
}

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

/// Knobs for the pixel-sort renderer.
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
    /// What drives the per-pixel sortability mask. `SelfMask` = B's own sort key (classic);
    /// `ALuma`/`AEdge`/`AFlow` = cross-synth (caller pre-computes and passes `a_mask`).
    #[serde(default)]
    pub mask_source: MaskSource,
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
            mask_source: MaskSource::SelfMask,
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

// ─── Cross-synth mask helpers ─────────────────────────────────────────────────

/// Resample A's Rec.709 luma onto B's pixel grid (bilinear).  Returns one
/// value per B pixel in row-major order, range [0, 1].
pub fn compute_a_luma_mask(source_a: &ImageBufferF32, b_width: u32, b_height: u32) -> Vec<f32> {
    let aw = source_a.width as f32;
    let ah = source_a.height as f32;
    let bw = b_width as f32;
    let bh = b_height as f32;
    (0..b_height)
        .flat_map(|y| {
            let ay = (y as f32 + 0.5) * ah / bh - 0.5;
            (0..b_width).map(move |x| {
                let ax = (x as f32 + 0.5) * aw / bw - 0.5;
                let px = sample_bilinear_clamped(source_a, ax, ay);
                luma(px[0], px[1], px[2])
            })
        })
        .collect()
}

/// Resample A's luma to B's grid then apply a 3×3 Sobel edge detector.
/// Output magnitude in [0, 1] (divided by 4√2, the maximum Sobel response for [0,1] inputs).
pub fn compute_a_edge_mask(source_a: &ImageBufferF32, b_width: u32, b_height: u32) -> Vec<f32> {
    let lumas = compute_a_luma_mask(source_a, b_width, b_height);
    let w = b_width as usize;
    let h = b_height as usize;
    let norm = 4.0 * std::f32::consts::SQRT_2;
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let s = |dy: i32, dx: i32| {
                let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                lumas[sy * w + sx]
            };
            let gx = -s(-1, -1) + s(-1, 1) - 2.0 * s(0, -1) + 2.0 * s(0, 1)
                - s(1, -1) + s(1, 1);
            let gy = -s(-1, -1) - 2.0 * s(-1, 0) - s(-1, 1)
                + s(1, -1) + 2.0 * s(1, 0) + s(1, 1);
            out.push(((gx * gx + gy * gy).sqrt() / norm).min(1.0));
        }
    }
    out
}

/// Compute per-pixel flow magnitude from a FlowField, resampled (nearest) to B's
/// dimensions and peak-normalised to [0, 1].  A static frame → all zeros → nothing sortable.
pub fn compute_a_flow_mask(flow: &FlowField, b_width: u32, b_height: u32) -> Vec<f32> {
    let fw = flow.width as usize;
    let fh = flow.height as usize;
    let raw: Vec<f32> = flow.vectors.iter().map(|v| v[0].hypot(v[1])).collect();
    let max_mag = raw.iter().cloned().fold(0.0_f32, f32::max).max(1.0);
    let normed: Vec<f32> = raw.iter().map(|&m| m / max_mag).collect();
    if flow.width == b_width && flow.height == b_height {
        return normed;
    }
    // Nearest-neighbour resample to B's dimensions.
    let bw = b_width as f32;
    let bh = b_height as f32;
    let mut out = Vec::with_capacity((b_width * b_height) as usize);
    for y in 0..b_height as usize {
        let fy = ((y as f32 + 0.5) * fh as f32 / bh - 0.5)
            .clamp(0.0, fh as f32 - 1.0) as usize;
        for x in 0..b_width as usize {
            let fx = ((x as f32 + 0.5) * fw as f32 / bw - 0.5)
                .clamp(0.0, fw as f32 - 1.0) as usize;
            out.push(normed[fy * fw + fx]);
        }
    }
    out
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
        let ord = ka.total_cmp(&kb);
        match direction {
            SortDirection::Asc => ord,
            SortDirection::Desc => ord.reverse(),
        }
    });
}

/// Walk one scan-line (row or column slice), find maximal sortable spans, and sort them.
/// `a_mask`: if non-empty, per-position mask values for span detection (cross-synth modes);
/// if empty, B's own sort key is used for both masking and sorting (self mode).
fn sort_line(
    line: &mut [[f32; 4]],
    key: SortKey,
    low: f32,
    high: f32,
    direction: SortDirection,
    max_span: usize,
    a_mask: &[f32],
) {
    let n = line.len();
    let mut i = 0;
    while i < n {
        let mv = if a_mask.is_empty() { pixel_sort_key(line[i], key) } else { a_mask[i] };
        if mv < low || mv > high {
            i += 1;
            continue;
        }
        let span_start = i;
        i += 1;
        while i < n {
            let mv2 = if a_mask.is_empty() { pixel_sort_key(line[i], key) } else { a_mask[i] };
            if mv2 < low || mv2 > high { break; }
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

/// Render one frame of the pixel-sort effect on `source_b`.
///
/// `a_mask`: pre-computed per-pixel mask values in row-major order (one f32 per B pixel).
/// - Empty (`&[]`) → **self mode**: B's own sort key determines sortability.
/// - Non-empty → **cross-synth mode**: `a_mask[y*w+x]` is tested against [low, high]
///   for span detection; spans are still sorted by B's sort key.
///   Use `compute_a_luma_mask` / `compute_a_edge_mask` / `compute_a_flow_mask` to build it.
pub fn render_pixel_sort_frame(
    source_b: &ImageBufferF32,
    settings: &PixelSortSettings,
    a_mask: &[f32],
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
                let mask_row: &[f32] = if a_mask.is_empty() {
                    &[]
                } else {
                    &a_mask[y * w as usize..(y + 1) * w as usize]
                };
                sort_line(row, key, low, high, direction, max_span, mask_row);
            }
        }
        SortAxis::Col => {
            for x in 0..w as usize {
                let mut col: Vec<[f32; 4]> = (0..h as usize)
                    .map(|y| pixels[y * w as usize + x])
                    .collect();
                let col_mask: Vec<f32> = if a_mask.is_empty() {
                    vec![]
                } else {
                    (0..h as usize)
                        .map(|y| a_mask[y * w as usize + x])
                        .collect()
                };
                sort_line(&mut col, key, low, high, direction, max_span, &col_mask);
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

    #[test]
    fn empty_mask_is_byte_identical_passthrough() {
        let src = row_image(&[0.8, 0.2, 0.5, 0.1, 0.9]);
        let s = PixelSortSettings {
            threshold_low: 0.9,
            threshold_high: 0.1, // low > high → empty mask
            ..Default::default()
        };
        let out = render_pixel_sort_frame(&src, &s, &[]).unwrap();
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
        let out = render_pixel_sort_frame(&src, &s, &[]).unwrap();
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
        let out = render_pixel_sort_frame(&src, &s, &[]).unwrap();
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
        let out = render_pixel_sort_frame(&src, &s, &[]).unwrap();
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
        let out = render_pixel_sort_frame(&src, &s, &[]).unwrap();
        // Chunk 0: [0.6, 0.1, 0.4] → [0.1, 0.4, 0.6]
        assert!((out.pixels[0][0] - 0.1).abs() < 1e-6);
        assert!((out.pixels[1][0] - 0.4).abs() < 1e-6);
        assert!((out.pixels[2][0] - 0.6).abs() < 1e-6);
        // Chunk 1: [0.9, 0.2, 0.7] → [0.2, 0.7, 0.9]
        assert!((out.pixels[3][0] - 0.2).abs() < 1e-6);
        assert!((out.pixels[4][0] - 0.7).abs() < 1e-6);
        assert!((out.pixels[5][0] - 0.9).abs() < 1e-6);
    }

    #[test]
    fn sorts_col_axis_ascending() {
        // 2-wide 4-tall. Col axis sorts each column top-to-bottom by luma.
        // Col 0 lumas: [0.8, 0.2, 0.6, 0.4] → sorted: [0.2, 0.4, 0.6, 0.8]
        // Col 1 lumas: [0.1, 0.9, 0.3, 0.7] → sorted: [0.1, 0.3, 0.7, 0.9]
        let pixels: Vec<[f32; 4]> = vec![
            [0.8, 0.8, 0.8, 1.0], [0.1, 0.1, 0.1, 1.0],
            [0.2, 0.2, 0.2, 1.0], [0.9, 0.9, 0.9, 1.0],
            [0.6, 0.6, 0.6, 1.0], [0.3, 0.3, 0.3, 1.0],
            [0.4, 0.4, 0.4, 1.0], [0.7, 0.7, 0.7, 1.0],
        ];
        let src = ImageBufferF32::new(2, 4, pixels).unwrap();
        let s = PixelSortSettings {
            axis: SortAxis::Col,
            threshold_low: 0.0,
            threshold_high: 1.0,
            ..Default::default()
        };
        let out = render_pixel_sort_frame(&src, &s, &[]).unwrap();
        let col0: Vec<f32> = (0..4).map(|y| out.pixels[y * 2][0]).collect();
        let col1: Vec<f32> = (0..4).map(|y| out.pixels[y * 2 + 1][0]).collect();
        let expected0 = [0.2f32, 0.4, 0.6, 0.8];
        let expected1 = [0.1f32, 0.3, 0.7, 0.9];
        for i in 0..4 {
            assert!((col0[i] - expected0[i]).abs() < 1e-6, "col0[{i}]");
            assert!((col1[i] - expected1[i]).abs() < 1e-6, "col1[{i}]");
        }
    }

    #[test]
    fn sorts_red_channel_key() {
        // 3 pixels with distinct R values; sorted by R ascending (key=Red).
        // Pixels: R=[0.8,0.3,0.6], G=[0.1,0.9,0.5] → by R: R=0.3(G=0.9), R=0.6(G=0.5), R=0.8(G=0.1)
        let pixels = vec![
            [0.8f32, 0.1, 0.0, 1.0],
            [0.3f32, 0.9, 0.0, 1.0],
            [0.6f32, 0.5, 0.0, 1.0],
        ];
        let src = ImageBufferF32::new(3, 1, pixels).unwrap();
        let s = PixelSortSettings {
            key: SortKey::Red,
            threshold_low: 0.0,
            threshold_high: 1.0,
            ..Default::default()
        };
        let out = render_pixel_sort_frame(&src, &s, &[]).unwrap();
        assert!((out.pixels[0][0] - 0.3).abs() < 1e-6, "px0.R=0.3");
        assert!((out.pixels[0][1] - 0.9).abs() < 1e-6, "px0.G=0.9 follows its R");
        assert!((out.pixels[1][0] - 0.6).abs() < 1e-6, "px1.R=0.6");
        assert!((out.pixels[2][0] - 0.8).abs() < 1e-6, "px2.R=0.8");
    }

    #[test]
    fn a_luma_mask_sorts_by_a_not_b() {
        // 3 B pixels (greyscale): lumas [0.8, 0.2, 0.5]
        // A mask values override span detection: [0.3, 0.3, 0.3] → all sortable
        // Threshold [0.25, 0.75]: all sortable via a_mask, sorted by B's luma asc → [0.2,0.5,0.8]
        let b_pixels: Vec<[f32; 4]> = vec![[0.8, 0.8, 0.8, 1.0], [0.2, 0.2, 0.2, 1.0], [0.5, 0.5, 0.5, 1.0]];
        let src = ImageBufferF32::new(3, 1, b_pixels).unwrap();
        let s = PixelSortSettings {
            threshold_low: 0.25,
            threshold_high: 0.75,
            ..Default::default()
        };
        // Without a_mask: only luma=0.5 is in [0.25,0.75]; 0.8 and 0.2 are outside → no sort.
        let out_self = render_pixel_sort_frame(&src, &s, &[]).unwrap();
        assert!((out_self.pixels[0][0] - 0.8).abs() < 1e-6, "self: 0.8 unsortable");
        // With a_mask=[0.4,0.4,0.4]: all three in [0.25,0.75] → all sort by B's luma.
        let a_mask = vec![0.4f32, 0.4, 0.4];
        let out_cross = render_pixel_sort_frame(&src, &s, &a_mask).unwrap();
        assert!((out_cross.pixels[0][0] - 0.2).abs() < 1e-6, "cross: 0.2 first");
        assert!((out_cross.pixels[1][0] - 0.5).abs() < 1e-6, "cross: 0.5 second");
        assert!((out_cross.pixels[2][0] - 0.8).abs() < 1e-6, "cross: 0.8 third");
    }

    #[test]
    fn a_luma_mask_helper_produces_b_sized_output() {
        // A is 2×2, B is 4×2; luma mask should return 4*2=8 values.
        let a_pixels: Vec<[f32; 4]> = vec![[0.0; 4]; 4];
        let a = ImageBufferF32::new(2, 2, a_pixels).unwrap();
        let mask = compute_a_luma_mask(&a, 4, 2);
        assert_eq!(mask.len(), 8, "mask length = b_width * b_height");
    }

    #[test]
    fn a_edge_mask_zero_on_uniform_image() {
        // Uniform A → all lumas equal → Sobel magnitude = 0 → no sorting.
        let a_pixels: Vec<[f32; 4]> = vec![[0.5, 0.5, 0.5, 1.0]; 9];
        let a = ImageBufferF32::new(3, 3, a_pixels).unwrap();
        let mask = compute_a_edge_mask(&a, 3, 3);
        for &m in &mask {
            assert!(m.abs() < 1e-5, "uniform → edge = 0, got {m}");
        }
    }

    #[test]
    fn a_flow_mask_zero_flow_is_zero() {
        // Zero flow field → mask all zeros → peak-normalised by floor 1 → all 0.
        let flow = FlowField::new(3, 1, vec![[0.0, 0.0]; 3]).unwrap();
        let mask = compute_a_flow_mask(&flow, 3, 1);
        for &m in &mask {
            assert!(m.abs() < 1e-5, "zero flow → mask = 0, got {m}");
        }
    }
}
