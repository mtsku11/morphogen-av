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
