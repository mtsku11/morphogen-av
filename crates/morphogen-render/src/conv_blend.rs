//! Convolutional Audio/Video Blending — image-kernel MVP.
//!
//! Source A's frame supplies a small K×K image kernel (its luminance box-downsampled
//! into a K×K grid, normalized to unit sum); Source B's frame is spatially convolved
//! with that kernel, so B takes on A's coarse structure (a structure-aware blur /
//! spatial blend). Stateless, per-frame, deterministic. The CPU reference here is
//! ground truth for the parity-gated `convolution_blend` Metal kernel.
//!
//! See `docs/CONVOLUTIONAL_BLEND_MILESTONE.md` for the full contract.

use crate::{ImageBufferF32, RenderError};

/// CPU reference render id and kernel-extraction algorithm id.
pub const CONVOLUTION_BLEND_ALGORITHM: &str = "image_kernel_convolution_blend_cpu_v1";

/// A normalized K×K convolution kernel derived from a Source A frame: `weights`
/// is row-major with `size * size` entries summing to 1 (a weighted average).
#[derive(Debug, Clone, PartialEq)]
pub struct ConvolutionKernel {
    /// Kernel edge length `K` (odd, >= 1).
    pub size: u32,
    /// Row-major taps, `size * size` entries, summing to 1.
    pub weights: Vec<f32>,
}

/// Render parameters for the convolution blend.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConvolutionBlendSettings {
    /// Kernel edge length `K` (odd, >= 1).
    pub kernel_size: u32,
    /// Wet/dry blend: `0` = exact Source B passthrough, `1` = fully convolved.
    /// Must be finite and >= 0.
    pub amount: f32,
}

impl ConvolutionBlendSettings {
    pub fn validate(self) -> Result<(), RenderError> {
        validate_kernel_size(self.kernel_size)?;
        if !self.amount.is_finite() || self.amount < 0.0 {
            return Err(RenderError::InvalidConvolutionSettings(
                "amount must be finite and non-negative".to_string(),
            ));
        }
        Ok(())
    }
}

fn validate_kernel_size(size: u32) -> Result<(), RenderError> {
    if size == 0 {
        return Err(RenderError::InvalidConvolutionSettings(
            "kernel-size must be greater than zero".to_string(),
        ));
    }
    if size % 2 == 0 {
        return Err(RenderError::InvalidConvolutionSettings(
            "kernel-size must be odd so the kernel is centered".to_string(),
        ));
    }
    Ok(())
}

/// Rec. 709 luma, matching the granular-mosaic / vocoder convention.
fn luminance(pixel: [f32; 4]) -> f32 {
    pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Box-downsample Source A's luminance into a `size × size` grid, then normalize
/// to unit sum. Each cell averages the luma over its (contiguous, clamped) region
/// of A. A fully black A (sum ~0) falls back to a **uniform** kernel (`1/K²` each)
/// so the result is always a well-defined weighted average.
pub fn analyze_convolution_kernel_cpu(
    modulator: &ImageBufferF32,
    size: u32,
) -> Result<ConvolutionKernel, RenderError> {
    validate_kernel_size(size)?;
    let cells = (size * size) as usize;
    let mut weights = vec![0.0_f32; cells];

    // Box-average A's luma into the K×K grid. Cell (cx, cy) covers the source
    // span [cx*W/K, (cx+1)*W/K) × [cy*H/K, (cy+1)*H/K), each side clamped so even
    // a 1×1 source contributes to every cell.
    let width = modulator.width.max(1);
    let height = modulator.height.max(1);
    for cy in 0..size {
        for cx in 0..size {
            let x0 = (cx as u64 * width as u64 / size as u64) as u32;
            let x1 = ((cx as u64 + 1) * width as u64 / size as u64).max(x0 as u64 + 1) as u32;
            let y0 = (cy as u64 * height as u64 / size as u64) as u32;
            let y1 = ((cy as u64 + 1) * height as u64 / size as u64).max(y0 as u64 + 1) as u32;
            let mut sum = 0.0_f32;
            let mut count = 0.0_f32;
            for y in y0..y1.min(height) {
                for x in x0..x1.min(width) {
                    if let Some(pixel) = modulator.pixel(x, y) {
                        sum += luminance(pixel).clamp(0.0, 1.0);
                        count += 1.0;
                    }
                }
            }
            let cell = (cy * size + cx) as usize;
            weights[cell] = if count > 0.0 { sum / count } else { 0.0 };
        }
    }

    let total: f32 = weights.iter().sum();
    if total > 0.0 {
        for weight in &mut weights {
            *weight /= total;
        }
    } else {
        weights.fill(1.0 / cells as f32);
    }

    Ok(ConvolutionKernel { size, weights })
}

/// Convolve Source B with a precomputed kernel and blend by `amount`. Output
/// dimensions follow the carrier; each pixel's RGB is the `amount`-blend of the
/// carrier and the centered K×K weighted sum (clamped-border sampling, taps
/// applied without flip), clamped to `[0, 1]`; alpha is preserved. `amount = 0`
/// is an exact passthrough.
pub fn convolution_blend_cpu(
    carrier: &ImageBufferF32,
    kernel: &ConvolutionKernel,
    amount: f32,
) -> Result<ImageBufferF32, RenderError> {
    validate_kernel_size(kernel.size)?;
    if kernel.weights.len() != (kernel.size * kernel.size) as usize {
        return Err(RenderError::InvalidConvolutionSettings(format!(
            "kernel has {} weights but size {} requires {}",
            kernel.weights.len(),
            kernel.size,
            kernel.size * kernel.size
        )));
    }
    if !amount.is_finite() || amount < 0.0 {
        return Err(RenderError::InvalidConvolutionSettings(
            "amount must be finite and non-negative".to_string(),
        ));
    }

    let width = carrier.width;
    let height = carrier.height;
    let radius = (kernel.size / 2) as i64;
    let max_x = width.saturating_sub(1) as i64;
    let max_y = height.saturating_sub(1) as i64;

    let mut pixels = Vec::with_capacity(carrier.pixels.len());
    for y in 0..height as i64 {
        for x in 0..width as i64 {
            let here = carrier.pixels[(y * width as i64 + x) as usize];
            let mut accum = [0.0_f32; 3];
            for ky in 0..kernel.size as i64 {
                let sy = (y + ky - radius).clamp(0, max_y);
                for kx in 0..kernel.size as i64 {
                    let sx = (x + kx - radius).clamp(0, max_x);
                    let weight = kernel.weights[(ky * kernel.size as i64 + kx) as usize];
                    let sample = carrier.pixels[(sy * width as i64 + sx) as usize];
                    accum[0] += weight * sample[0];
                    accum[1] += weight * sample[1];
                    accum[2] += weight * sample[2];
                }
            }
            pixels.push([
                lerp(here[0], accum[0], amount).clamp(0.0, 1.0),
                lerp(here[1], accum[1], amount).clamp(0.0, 1.0),
                lerp(here[2], accum[2], amount).clamp(0.0, 1.0),
                here[3],
            ]);
        }
    }

    ImageBufferF32::new(width, height, pixels)
}

/// Convenience: extract the kernel from Source A then convolve Source B in one
/// call (the still-image path). The sequence/queue paths analyze per frame.
pub fn convolution_blend_from_modulator_cpu(
    modulator: &ImageBufferF32,
    carrier: &ImageBufferF32,
    settings: ConvolutionBlendSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let kernel = analyze_convolution_kernel_cpu(modulator, settings.kernel_size)?;
    convolution_blend_cpu(carrier, &kernel, settings.amount)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, color: [f32; 4]) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |_, _| color).expect("buffer")
    }

    #[test]
    fn passthrough_amount_zero_preserves_carrier() {
        // A structured A would otherwise blur the carrier; amount 0 must not touch it.
        let modulator = ImageBufferF32::from_fn(6, 6, |x, _| {
            let v = x as f32 / 5.0;
            [v, v, v, 1.0]
        })
        .expect("modulator");
        let carrier = ImageBufferF32::from_fn(4, 4, |x, y| {
            let v = if (x + y) % 2 == 0 { 0.9 } else { 0.1 };
            [v, v * 0.5, 0.25, 1.0]
        })
        .expect("carrier");
        let kernel = analyze_convolution_kernel_cpu(&modulator, 3).expect("kernel");
        let out = convolution_blend_cpu(&carrier, &kernel, 0.0).expect("render");
        assert_eq!(out.pixels, carrier.pixels);
    }

    #[test]
    fn identity_kernel_size_one_is_passthrough_at_full_amount() {
        // K=1 -> a single unit tap -> convolution is the carrier itself, even wet.
        let carrier = ImageBufferF32::from_fn(3, 3, |x, y| {
            let v = (x * 3 + y) as f32 / 9.0;
            [v, 1.0 - v, 0.5, 1.0]
        })
        .expect("carrier");
        let kernel = analyze_convolution_kernel_cpu(&solid(2, 2, [0.5, 0.5, 0.5, 1.0]), 1)
            .expect("kernel");
        assert_eq!(kernel.weights, vec![1.0]);
        let out = convolution_blend_cpu(&carrier, &kernel, 1.0).expect("render");
        assert_eq!(out.pixels, carrier.pixels);
    }

    #[test]
    fn kernel_weights_normalize_to_unit_sum() {
        let modulator = ImageBufferF32::from_fn(9, 9, |x, y| {
            let v = ((x + y) as f32 / 16.0).clamp(0.0, 1.0);
            [v, v, v, 1.0]
        })
        .expect("modulator");
        let kernel = analyze_convolution_kernel_cpu(&modulator, 3).expect("kernel");
        assert_eq!(kernel.weights.len(), 9);
        let sum: f32 = kernel.weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "weights summed to {sum}");
    }

    #[test]
    fn black_modulator_falls_back_to_uniform_kernel() {
        let modulator = solid(8, 8, [0.0, 0.0, 0.0, 1.0]);
        let kernel = analyze_convolution_kernel_cpu(&modulator, 3).expect("kernel");
        let expected = 1.0 / 9.0;
        for weight in &kernel.weights {
            assert!((weight - expected).abs() < 1e-6, "got {weight}");
        }
    }

    #[test]
    fn kernel_follows_modulator_structure() {
        // A bright on its right half, dark on its left -> the right kernel column
        // carries more weight than the left.
        let modulator = ImageBufferF32::from_fn(6, 3, |x, _| {
            let v = if x < 3 { 0.05 } else { 0.95 };
            [v, v, v, 1.0]
        })
        .expect("modulator");
        let kernel = analyze_convolution_kernel_cpu(&modulator, 3).expect("kernel");
        // Compare column sums (left = cols 0, middle = 1, right = 2).
        let col = |c: usize| kernel.weights[c] + kernel.weights[3 + c] + kernel.weights[6 + c];
        assert!(col(2) > col(0), "right {} should exceed left {}", col(2), col(0));
    }

    #[test]
    fn uniform_kernel_averages_an_edge_toward_the_mean() {
        // A vertical black/white edge under a 3x3 box blur: the column straddling
        // the edge moves toward the local mean (gray), not the extremes.
        let carrier = ImageBufferF32::from_fn(4, 1, |x, _| {
            let v = if x < 2 { 0.0 } else { 1.0 };
            [v, v, v, 1.0]
        })
        .expect("carrier");
        let kernel = ConvolutionKernel {
            size: 3,
            weights: vec![1.0 / 9.0; 9],
        };
        let out = convolution_blend_cpu(&carrier, &kernel, 1.0).expect("render");
        // Pixel x=1 (last black before the edge) pulls up off pure black; x=2
        // (first white) pulls down off pure white. Interior moves toward gray.
        assert!(out.pixels[1][0] > 0.0 && out.pixels[1][0] < 0.5);
        assert!(out.pixels[2][0] < 1.0 && out.pixels[2][0] > 0.5);
    }

    #[test]
    fn large_kernel_size_convolves_without_cap() {
        // The kernel has no upper size cap: a large K is box-downsampled and
        // applied like any other. A big uniform-ish kernel over a high-frequency
        // carrier must pull interior pixels hard toward the local mean (a wider
        // blur than a small kernel), and the output dims follow the carrier.
        let carrier = ImageBufferF32::from_fn(16, 16, |x, y| {
            let v = if (x + y) % 2 == 0 { 0.95 } else { 0.05 };
            [v, v, v, 1.0]
        })
        .expect("carrier");
        let small = ConvolutionKernel {
            size: 3,
            weights: vec![1.0 / 9.0; 9],
        };
        let large = ConvolutionKernel {
            size: 11,
            weights: vec![1.0 / 121.0; 121],
        };
        let out_small = convolution_blend_cpu(&carrier, &small, 1.0).expect("small");
        let out_large = convolution_blend_cpu(&carrier, &large, 1.0).expect("large");
        assert_eq!(out_large.width, carrier.width);
        assert_eq!(out_large.height, carrier.height);
        // A center pixel under an 11x11 average sits far closer to the 0.5 mean
        // than under a 3x3 average of the same checkerboard.
        let center = (8 * 16 + 8) as usize;
        let dev_small = (out_small.pixels[center][0] - 0.5).abs();
        let dev_large = (out_large.pixels[center][0] - 0.5).abs();
        assert!(
            dev_large < dev_small,
            "large-K deviation {dev_large} should be below small-K {dev_small}"
        );
    }

    #[test]
    fn invalid_settings_are_rejected() {
        assert!(ConvolutionBlendSettings {
            kernel_size: 0,
            amount: 1.0
        }
        .validate()
        .is_err());
        assert!(ConvolutionBlendSettings {
            kernel_size: 2, // even -> not centerable
            amount: 1.0
        }
        .validate()
        .is_err());
        assert!(ConvolutionBlendSettings {
            kernel_size: 3,
            amount: f32::NAN
        }
        .validate()
        .is_err());
        assert!(ConvolutionBlendSettings {
            kernel_size: 3,
            amount: -0.5
        }
        .validate()
        .is_err());
    }
}
