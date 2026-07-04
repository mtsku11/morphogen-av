use std::fs;
use std::path::{Path, PathBuf};

use image::{ImageBuffer, ImageReader, Rgba};
use morphogen_render::{FlowField, ImageBufferF32, RenderError};

use crate::error::CliError;
pub(crate) fn load_image_f32(path: &Path) -> Result<ImageBufferF32, CliError> {
    let decoded = ImageReader::open(path)?.decode()?.to_rgba32f();
    let pixels = decoded.pixels().map(|pixel| pixel.0).collect();
    ImageBufferF32::new(decoded.width(), decoded.height(), pixels).map_err(CliError::from)
}

pub(crate) fn collect_image_frames(directory: &Path) -> Result<Vec<PathBuf>, CliError> {
    let mut frames = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_file() && is_supported_image_frame(&path) {
            frames.push(path);
        }
    }
    frames.sort();
    Ok(frames)
}

pub(crate) fn is_supported_image_frame(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };

    ["png"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

pub(crate) fn synthetic_carrier(width: u32, height: u32) -> Result<ImageBufferF32, RenderError> {
    ImageBufferF32::from_fn(width, height, |x, y| {
        let fx = normalized_coordinate(x, width);
        let fy = normalized_coordinate(y, height);
        let checker = if ((x / 16) + (y / 16)) % 2 == 0 {
            0.24
        } else {
            0.82
        };
        [fx, fy, checker, 1.0]
    })
}

pub(crate) fn synthetic_flow(width: u32, height: u32) -> Result<FlowField, RenderError> {
    FlowField::from_fn(width, height, |x, y| {
        let nx = normalized_coordinate(x, width) * 2.0 - 1.0;
        let ny = normalized_coordinate(y, height) * 2.0 - 1.0;
        let swirl_x = -ny * 7.5;
        let swirl_y = nx * 7.5;
        let ripple = (nx * std::f32::consts::PI * 4.0).sin() * 2.0;
        [swirl_x + ripple, swirl_y]
    })
}

pub(crate) fn normalized_coordinate(value: u32, extent: u32) -> f32 {
    if extent <= 1 {
        return 0.0;
    }
    value as f32 / (extent - 1) as f32
}

pub(crate) fn save_png_with_bit_depth(
    image: &ImageBufferF32,
    output_path: &Path,
    bit_depth: u8,
) -> Result<(), CliError> {
    match bit_depth {
        8 => save_png(image, output_path),
        16 => save_png_16(image, output_path),
        _ => Err(CliError::Message(
            "PNG bit depth must be either 8 or 16".to_string(),
        )),
    }
}

pub(crate) fn save_png(image: &ImageBufferF32, output_path: &Path) -> Result<(), CliError> {
    let mut rgba: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(image.width, image.height);

    for y in 0..image.height {
        for x in 0..image.width {
            let pixel = image
                .pixel(x, y)
                .ok_or_else(|| CliError::Message(format!("missing pixel at {},{}", x, y)))?;
            rgba.put_pixel(
                x,
                y,
                Rgba([
                    float_to_u8(pixel[0]),
                    float_to_u8(pixel[1]),
                    float_to_u8(pixel[2]),
                    float_to_u8(pixel[3]),
                ]),
            );
        }
    }

    rgba.save(output_path)?;
    Ok(())
}

pub(crate) fn save_png_16(image: &ImageBufferF32, output_path: &Path) -> Result<(), CliError> {
    let mut rgba: ImageBuffer<Rgba<u16>, Vec<u16>> = ImageBuffer::new(image.width, image.height);

    for y in 0..image.height {
        for x in 0..image.width {
            let pixel = image
                .pixel(x, y)
                .ok_or_else(|| CliError::Message(format!("missing pixel at {},{}", x, y)))?;
            rgba.put_pixel(
                x,
                y,
                Rgba([
                    float_to_u16(pixel[0]),
                    float_to_u16(pixel[1]),
                    float_to_u16(pixel[2]),
                    float_to_u16(pixel[3]),
                ]),
            );
        }
    }

    rgba.save(output_path)?;
    Ok(())
}

pub(crate) fn float_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub(crate) fn float_to_u16(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * u16::MAX as f32).round() as u16
}

pub(crate) const BOX_DOWNSCALE_ALGORITHM: &str = "box_downscale_cpu_v1";

/// Output dimensions for a box-average downscale: `ceil(in_dim / scale)`, so
/// non-divisible dimensions are well-defined (the last block clips to bounds).
pub(crate) fn box_downscale_dimensions(in_width: u32, in_height: u32, scale: u32) -> (u32, u32) {
    (in_width.div_ceil(scale), in_height.div_ceil(scale))
}

/// Deterministic box average: output pixel `(x, y)` is the unweighted f32
/// mean over all 4 channels of the `scale x scale` input block at
/// `(scale*x, scale*y)`, clipped to the image bounds at the edges.
pub(crate) fn box_downscale(
    image: &ImageBufferF32,
    scale: u32,
) -> Result<ImageBufferF32, CliError> {
    let (out_width, out_height) = box_downscale_dimensions(image.width, image.height, scale);
    ImageBufferF32::from_fn(out_width, out_height, |x, y| {
        let start_x = scale * x;
        let start_y = scale * y;
        let end_x = (start_x + scale).min(image.width);
        let end_y = (start_y + scale).min(image.height);

        let mut sum = [0f32; 4];
        let mut count = 0u32;
        for iy in start_y..end_y {
            for ix in start_x..end_x {
                if let Some(pixel) = image.pixel(ix, iy) {
                    for channel in 0..4 {
                        sum[channel] += pixel[channel];
                    }
                    count += 1;
                }
            }
        }

        let count = count.max(1) as f32;
        [
            sum[0] / count,
            sum[1] / count,
            sum[2] / count,
            sum[3] / count,
        ]
    })
    .map_err(CliError::from)
}

pub(crate) fn write_parent_dirs(path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}
pub(crate) fn image_file_fingerprint(path: &Path) -> Result<String, CliError> {
    let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
    update_fnv1a(
        &mut checksum,
        path.file_name().unwrap_or_default().as_encoded_bytes(),
    );
    update_fnv1a(&mut checksum, &[0]);
    update_fnv1a(&mut checksum, &fs::read(path)?);
    Ok(format!("fnv1a64:{checksum:016x}"))
}

pub(crate) fn update_fnv1a(checksum: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *checksum ^= u64::from(*byte);
        *checksum = checksum.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_downscale_dimensions_divide_evenly() {
        assert_eq!(box_downscale_dimensions(4, 4, 2), (2, 2));
        assert_eq!(box_downscale_dimensions(8, 6, 4), (2, 2));
    }

    #[test]
    fn box_downscale_dimensions_ceil_on_non_divisible_input() {
        // ceil(5/2) = 3, ceil(3/2) = 2 — the last block clips to bounds.
        assert_eq!(box_downscale_dimensions(5, 3, 2), (3, 2));
        assert_eq!(box_downscale_dimensions(1, 1, 4), (1, 1));
    }

    #[test]
    fn box_downscale_scale_one_is_pixel_identity() {
        let image = ImageBufferF32::from_fn(3, 2, |x, y| {
            [x as f32 * 0.1, y as f32 * 0.2, (x + y) as f32 * 0.3, 1.0]
        })
        .expect("build source image");

        let downscaled = box_downscale(&image, 1).expect("downscale at scale 1");

        assert_eq!(downscaled.width, image.width);
        assert_eq!(downscaled.height, image.height);
        assert_eq!(downscaled.pixels, image.pixels);
    }

    #[test]
    fn box_downscale_pins_average_on_divisible_block() {
        // pixel(x, y) = [x, y, x+y, 1]; scale 2 over a 4x4 image gives four
        // exact 2x2 block means with no edge clipping.
        let image = ImageBufferF32::from_fn(4, 4, |x, y| [x as f32, y as f32, (x + y) as f32, 1.0])
            .expect("build source image");

        let downscaled = box_downscale(&image, 2).expect("downscale");

        assert_eq!((downscaled.width, downscaled.height), (2, 2));
        assert_eq!(downscaled.pixel(0, 0), Some([0.5, 0.5, 1.0, 1.0]));
        assert_eq!(downscaled.pixel(1, 0), Some([2.5, 0.5, 3.0, 1.0]));
        assert_eq!(downscaled.pixel(0, 1), Some([0.5, 2.5, 3.0, 1.0]));
        assert_eq!(downscaled.pixel(1, 1), Some([2.5, 2.5, 5.0, 1.0]));
    }

    #[test]
    fn box_downscale_pins_average_on_clipped_edge_block() {
        // 3x3 input, scale 2 -> ceil(3/2) = 2x2 output; the last row/column
        // block is a 1-wide/1-tall clip, so its mean is over the in-bounds
        // subset only (not padded with zero).
        let image = ImageBufferF32::from_fn(3, 3, |x, y| [x as f32, y as f32, 0.0, 1.0])
            .expect("build source image");

        let downscaled = box_downscale(&image, 2).expect("downscale");

        assert_eq!((downscaled.width, downscaled.height), (2, 2));
        assert_eq!(downscaled.pixel(0, 0), Some([0.5, 0.5, 0.0, 1.0]));
        assert_eq!(downscaled.pixel(1, 0), Some([2.0, 0.5, 0.0, 1.0]));
        assert_eq!(downscaled.pixel(0, 1), Some([0.5, 2.0, 0.0, 1.0]));
        assert_eq!(downscaled.pixel(1, 1), Some([2.0, 2.0, 0.0, 1.0]));
    }
}
