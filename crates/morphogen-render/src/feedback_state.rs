use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

pub const FLOW_FEEDBACK_STATE_VERSION: u32 = 1;
const FEEDBACK_STATE_MAGIC: &[u8; 8] = b"MGFDBK01";
const HEADER_BYTES: usize = 28;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowFeedbackStateDescriptor {
    pub version: u32,
    pub width: u32,
    pub height: u32,
    pub checksum: String,
}

/// Writes unquantized row-major RGBA32F render state for a feedback checkpoint.
pub fn write_flow_feedback_state(
    path: impl AsRef<Path>,
    image: &ImageBufferF32,
) -> Result<FlowFeedbackStateDescriptor, RenderError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let pixel_bytes = rgba_f32_le_bytes(image)?;
    let checksum = checksum(&pixel_bytes);
    let capacity = HEADER_BYTES.checked_add(pixel_bytes.len()).ok_or_else(|| {
        RenderError::InvalidFlowFeedbackState("feedback state is too large".to_string())
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(FEEDBACK_STATE_MAGIC);
    bytes.extend_from_slice(&FLOW_FEEDBACK_STATE_VERSION.to_le_bytes());
    bytes.extend_from_slice(&image.width.to_le_bytes());
    bytes.extend_from_slice(&image.height.to_le_bytes());
    bytes.extend_from_slice(&checksum.to_le_bytes());
    bytes.extend_from_slice(&pixel_bytes);
    fs::write(path, bytes)?;

    Ok(FlowFeedbackStateDescriptor {
        version: FLOW_FEEDBACK_STATE_VERSION,
        width: image.width,
        height: image.height,
        checksum: format_checksum(checksum),
    })
}

/// Reads and verifies a feedback state file written by `write_flow_feedback_state`.
pub fn read_flow_feedback_state(
    path: impl AsRef<Path>,
) -> Result<(FlowFeedbackStateDescriptor, ImageBufferF32), RenderError> {
    let bytes = fs::read(path)?;
    if bytes.len() < HEADER_BYTES {
        return Err(RenderError::InvalidFlowFeedbackState(
            "feedback state is shorter than its header".to_string(),
        ));
    }
    if &bytes[0..8] != FEEDBACK_STATE_MAGIC {
        return Err(RenderError::InvalidFlowFeedbackState(
            "feedback state has an invalid magic value".to_string(),
        ));
    }

    let version = read_u32(&bytes[8..12])?;
    if version != FLOW_FEEDBACK_STATE_VERSION {
        return Err(RenderError::InvalidFlowFeedbackState(format!(
            "unsupported feedback state version {version}"
        )));
    }
    let width = read_u32(&bytes[12..16])?;
    let height = read_u32(&bytes[16..20])?;
    let expected_checksum = read_u64(&bytes[20..28])?;
    let expected_pixel_bytes = checked_pixel_bytes(width, height)?;
    let expected_len = HEADER_BYTES
        .checked_add(expected_pixel_bytes)
        .ok_or_else(|| {
            RenderError::InvalidFlowFeedbackState("feedback state is too large".to_string())
        })?;
    if bytes.len() != expected_len {
        return Err(RenderError::InvalidFlowFeedbackState(format!(
            "expected {expected_len} bytes, got {}",
            bytes.len()
        )));
    }

    let pixel_bytes = &bytes[HEADER_BYTES..];
    let actual_checksum = checksum(pixel_bytes);
    if actual_checksum != expected_checksum {
        return Err(RenderError::InvalidFlowFeedbackState(
            "feedback state checksum does not match its pixel data".to_string(),
        ));
    }

    let pixel_count = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| {
            RenderError::InvalidFlowFeedbackState(
                "feedback state dimensions are too large".to_string(),
            )
        })?;
    let mut pixels = Vec::with_capacity(pixel_count);
    for chunk in pixel_bytes.chunks_exact(16) {
        pixels.push([
            read_f32(&chunk[0..4])?,
            read_f32(&chunk[4..8])?,
            read_f32(&chunk[8..12])?,
            read_f32(&chunk[12..16])?,
        ]);
    }
    let image = ImageBufferF32::new(width, height, pixels)?;
    Ok((
        FlowFeedbackStateDescriptor {
            version,
            width,
            height,
            checksum: format_checksum(actual_checksum),
        },
        image,
    ))
}

pub fn feedback_state_path(directory: impl AsRef<Path>, frame_index: u32) -> PathBuf {
    directory
        .as_ref()
        .join("state")
        .join(format!("feedback_frame_{frame_index:06}.rgba32f"))
}

fn rgba_f32_le_bytes(image: &ImageBufferF32) -> Result<Vec<u8>, RenderError> {
    let byte_count = checked_pixel_bytes(image.width, image.height)?;
    let mut bytes = Vec::with_capacity(byte_count);
    for pixel in &image.pixels {
        for channel in pixel {
            bytes.extend_from_slice(&channel.to_le_bytes());
        }
    }
    Ok(bytes)
}

fn checked_pixel_bytes(width: u32, height: u32) -> Result<usize, RenderError> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|count| count.checked_mul(16))
        .ok_or_else(|| {
            RenderError::InvalidFlowFeedbackState(
                "feedback state dimensions are too large".to_string(),
            )
        })
}

fn checksum(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn format_checksum(checksum: u64) -> String {
    format!("fnv1a64:{checksum:016x}")
}

fn read_u32(bytes: &[u8]) -> Result<u32, RenderError> {
    if bytes.len() != 4 {
        return Err(RenderError::InvalidFlowFeedbackState(
            "expected four bytes for a feedback state u32".to_string(),
        ));
    }
    let mut value = [0_u8; 4];
    value.copy_from_slice(bytes);
    Ok(u32::from_le_bytes(value))
}

fn read_u64(bytes: &[u8]) -> Result<u64, RenderError> {
    if bytes.len() != 8 {
        return Err(RenderError::InvalidFlowFeedbackState(
            "expected eight bytes for a feedback state checksum".to_string(),
        ));
    }
    let mut value = [0_u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(value))
}

fn read_f32(bytes: &[u8]) -> Result<f32, RenderError> {
    if bytes.len() != 4 {
        return Err(RenderError::InvalidFlowFeedbackState(
            "expected four bytes for a feedback state f32".to_string(),
        ));
    }
    let mut value = [0_u8; 4];
    value.copy_from_slice(bytes);
    Ok(f32::from_le_bytes(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_state_round_trips_exact_float_pixels() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = feedback_state_path(temp_dir.path(), 0);
        let image =
            ImageBufferF32::new(2, 1, vec![[0.125, 0.25, 0.5, 1.0], [-0.25, 1.5, 0.0, 0.75]])
                .expect("image");

        let written = write_flow_feedback_state(&path, &image).expect("write state");
        let (read, restored) = read_flow_feedback_state(&path).expect("read state");

        assert_eq!(read, written);
        assert_eq!(restored, image);
    }

    #[test]
    fn feedback_state_rejects_corrupt_pixel_data() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = feedback_state_path(temp_dir.path(), 0);
        let image = ImageBufferF32::new(1, 1, vec![[0.0, 0.0, 0.0, 1.0]]).expect("image");
        write_flow_feedback_state(&path, &image).expect("write state");

        let mut bytes = fs::read(&path).expect("read bytes");
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).expect("corrupt state");

        assert!(read_flow_feedback_state(&path).is_err());
    }
}
