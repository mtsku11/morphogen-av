use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{FlowField, RenderError};

const FLOW_CACHE_VERSION: u32 = 2;
const FLOW_FRAME_MAGIC: &[u8; 8] = b"MGFLW001";
const MANIFEST_FILE_NAME: &str = "manifest.json";
const FRAME_FILE_NAME: &str = "frame_000000.flowf32";
pub const FLOW_VECTOR_CONVENTION: &str = "backward_sampling_offset";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowCacheManifest {
    pub version: u32,
    pub kind: String,
    pub algorithm: String,
    pub width: u32,
    pub height: u32,
    pub coordinate_space: String,
    pub vector_units: String,
    #[serde(default)]
    pub vector_convention: String,
    #[serde(default)]
    pub source_fingerprint: Option<String>,
    pub frames: Vec<FlowCacheFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FlowCacheFrame {
    pub frame_index: u64,
    pub path: String,
}

pub fn write_flow_cache(
    directory: impl AsRef<Path>,
    flow: &FlowField,
    algorithm: impl Into<String>,
) -> Result<FlowCacheManifest, RenderError> {
    write_flow_cache_with_source_fingerprint(directory, flow, algorithm, None)
}

pub fn write_flow_cache_with_source_fingerprint(
    directory: impl AsRef<Path>,
    flow: &FlowField,
    algorithm: impl Into<String>,
    source_fingerprint: Option<&str>,
) -> Result<FlowCacheManifest, RenderError> {
    let directory = directory.as_ref();
    fs::create_dir_all(directory)?;

    let frame = FlowCacheFrame {
        frame_index: 0,
        path: FRAME_FILE_NAME.to_string(),
    };
    write_flow_frame(directory.join(&frame.path), flow)?;

    let manifest = FlowCacheManifest {
        version: FLOW_CACHE_VERSION,
        kind: "flow_field_f32".to_string(),
        algorithm: algorithm.into(),
        width: flow.width,
        height: flow.height,
        coordinate_space: "output_pixel_coordinates".to_string(),
        vector_units: "pixels_before_amount_scale".to_string(),
        vector_convention: FLOW_VECTOR_CONVENTION.to_string(),
        source_fingerprint: source_fingerprint.map(str::to_string),
        frames: vec![frame],
    };

    let json = serde_json::to_string_pretty(&manifest)?;
    fs::write(directory.join(MANIFEST_FILE_NAME), json)?;
    Ok(manifest)
}

pub fn read_flow_cache(
    directory: impl AsRef<Path>,
) -> Result<(FlowCacheManifest, FlowField), RenderError> {
    let directory = directory.as_ref();
    let manifest_json = fs::read_to_string(directory.join(MANIFEST_FILE_NAME))?;
    let manifest: FlowCacheManifest = serde_json::from_str(&manifest_json)?;
    validate_manifest(&manifest)?;

    let frame = manifest
        .frames
        .first()
        .ok_or_else(|| RenderError::InvalidFlowCache("manifest has no frames".to_string()))?;
    let frame_path = relative_cache_path(&frame.path)?;
    let flow = read_flow_frame(directory.join(frame_path), manifest.width, manifest.height)?;

    Ok((manifest, flow))
}

fn validate_manifest(manifest: &FlowCacheManifest) -> Result<(), RenderError> {
    if !(1..=FLOW_CACHE_VERSION).contains(&manifest.version) {
        return Err(RenderError::InvalidFlowCache(format!(
            "unsupported flow cache version {}",
            manifest.version
        )));
    }
    if manifest.kind != "flow_field_f32" {
        return Err(RenderError::InvalidFlowCache(format!(
            "unsupported flow cache kind {}",
            manifest.kind
        )));
    }
    if manifest.width == 0 || manifest.height == 0 {
        return Err(RenderError::InvalidFlowCache(
            "width and height must be greater than zero".to_string(),
        ));
    }
    if manifest.frames.len() != 1 {
        return Err(RenderError::InvalidFlowCache(
            "single-frame flow caches must contain exactly one frame".to_string(),
        ));
    }
    Ok(())
}

fn relative_cache_path(path: &str) -> Result<PathBuf, RenderError> {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        return Err(RenderError::InvalidFlowCache(
            "cache frame path must be relative".to_string(),
        ));
    }
    Ok(path)
}

fn write_flow_frame(path: impl AsRef<Path>, flow: &FlowField) -> Result<(), RenderError> {
    let vector_count = checked_vector_count(flow.width, flow.height)?;
    let vector_bytes = vector_count.checked_mul(8).ok_or_else(|| {
        RenderError::InvalidFlowCache("flow frame byte length is too large".to_string())
    })?;
    let capacity = 16usize.checked_add(vector_bytes).ok_or_else(|| {
        RenderError::InvalidFlowCache("flow frame byte length is too large".to_string())
    })?;

    let mut bytes = Vec::with_capacity(capacity);
    bytes.extend_from_slice(FLOW_FRAME_MAGIC);
    bytes.extend_from_slice(&flow.width.to_le_bytes());
    bytes.extend_from_slice(&flow.height.to_le_bytes());
    for vector in &flow.vectors {
        bytes.extend_from_slice(&vector[0].to_le_bytes());
        bytes.extend_from_slice(&vector[1].to_le_bytes());
    }

    fs::write(path, bytes)?;
    Ok(())
}

fn read_flow_frame(
    path: impl AsRef<Path>,
    expected_width: u32,
    expected_height: u32,
) -> Result<FlowField, RenderError> {
    let bytes = fs::read(path)?;
    let expected_vectors = checked_vector_count(expected_width, expected_height)?;
    let expected_vector_bytes = expected_vectors.checked_mul(8).ok_or_else(|| {
        RenderError::InvalidFlowCache("flow frame byte length is too large".to_string())
    })?;
    let expected_len = 16usize.checked_add(expected_vector_bytes).ok_or_else(|| {
        RenderError::InvalidFlowCache("flow frame byte length is too large".to_string())
    })?;

    if bytes.len() != expected_len {
        return Err(RenderError::InvalidFlowCache(format!(
            "expected {expected_len} bytes, got {}",
            bytes.len()
        )));
    }
    if &bytes[0..8] != FLOW_FRAME_MAGIC {
        return Err(RenderError::InvalidFlowCache(
            "invalid flow frame magic".to_string(),
        ));
    }

    let width = read_u32_le(&bytes[8..12])?;
    let height = read_u32_le(&bytes[12..16])?;
    if width != expected_width || height != expected_height {
        return Err(RenderError::InvalidFlowCache(format!(
            "frame dimensions {width}x{height} do not match manifest {expected_width}x{expected_height}"
        )));
    }

    let mut vectors = Vec::with_capacity(expected_vectors);
    let mut offset = 16;
    for _ in 0..expected_vectors {
        let x = read_f32_le(&bytes[offset..offset + 4])?;
        let y = read_f32_le(&bytes[offset + 4..offset + 8])?;
        vectors.push([x, y]);
        offset += 8;
    }

    FlowField::new(width, height, vectors)
}

fn checked_vector_count(width: u32, height: u32) -> Result<usize, RenderError> {
    (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| RenderError::InvalidFlowCache("flow dimensions are too large".to_string()))
}

fn read_u32_le(bytes: &[u8]) -> Result<u32, RenderError> {
    if bytes.len() != 4 {
        return Err(RenderError::InvalidFlowCache(
            "expected four bytes for u32".to_string(),
        ));
    }
    let mut value = [0_u8; 4];
    value.copy_from_slice(bytes);
    Ok(u32::from_le_bytes(value))
}

fn read_f32_le(bytes: &[u8]) -> Result<f32, RenderError> {
    if bytes.len() != 4 {
        return Err(RenderError::InvalidFlowCache(
            "expected four bytes for f32".to_string(),
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
    fn flow_cache_round_trips_manifest_and_frame_data() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let flow = FlowField::new(2, 2, vec![[0.0, 1.0], [2.0, 3.0], [-1.0, 0.5], [4.0, -2.0]])
            .expect("flow");

        let manifest =
            write_flow_cache(temp_dir.path(), &flow, "test_algorithm_v1").expect("write cache");
        let (decoded_manifest, decoded_flow) =
            read_flow_cache(temp_dir.path()).expect("read cache");

        assert_eq!(manifest, decoded_manifest);
        assert_eq!(decoded_manifest.algorithm, "test_algorithm_v1");
        assert_eq!(decoded_manifest.vector_convention, FLOW_VECTOR_CONVENTION);
        assert_eq!(decoded_manifest.source_fingerprint, None);
        assert_eq!(decoded_flow, flow);
    }

    #[test]
    fn flow_cache_persists_source_fingerprint() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let flow = FlowField::new(1, 1, vec![[0.0, 0.0]]).expect("flow");

        write_flow_cache_with_source_fingerprint(
            temp_dir.path(),
            &flow,
            "lucas_kanade_cpu_v2",
            Some("fnv1a64:1234"),
        )
        .expect("write cache");
        let (manifest, _) = read_flow_cache(temp_dir.path()).expect("read cache");

        assert_eq!(manifest.source_fingerprint.as_deref(), Some("fnv1a64:1234"));
    }

    #[test]
    fn version_one_cache_without_new_metadata_remains_readable() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let flow = FlowField::new(1, 1, vec![[0.25, -0.5]]).expect("flow");
        write_flow_frame(temp_dir.path().join(FRAME_FILE_NAME), &flow).expect("write frame");
        let manifest = serde_json::json!({
            "version": 1,
            "kind": "flow_field_f32",
            "algorithm": "luminance_gradient_cpu_v1",
            "width": 1,
            "height": 1,
            "coordinate_space": "output_pixel_coordinates",
            "vector_units": "pixels_before_amount_scale",
            "frames": [{ "frame_index": 0, "path": FRAME_FILE_NAME }]
        });
        fs::write(
            temp_dir.path().join(MANIFEST_FILE_NAME),
            serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
        )
        .expect("write manifest");

        let (decoded_manifest, decoded_flow) =
            read_flow_cache(temp_dir.path()).expect("read cache");

        assert_eq!(decoded_manifest.version, 1);
        assert!(decoded_manifest.vector_convention.is_empty());
        assert_eq!(decoded_manifest.source_fingerprint, None);
        assert_eq!(decoded_flow, flow);
    }

    #[test]
    fn flow_cache_rejects_absolute_frame_paths() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = FlowCacheManifest {
            version: FLOW_CACHE_VERSION,
            kind: "flow_field_f32".to_string(),
            algorithm: "test".to_string(),
            width: 1,
            height: 1,
            coordinate_space: "output_pixel_coordinates".to_string(),
            vector_units: "pixels_before_amount_scale".to_string(),
            vector_convention: FLOW_VECTOR_CONVENTION.to_string(),
            source_fingerprint: None,
            frames: vec![FlowCacheFrame {
                frame_index: 0,
                path: "/tmp/frame.flowf32".to_string(),
            }],
        };
        let json = serde_json::to_string_pretty(&manifest).expect("json");
        fs::write(temp_dir.path().join(MANIFEST_FILE_NAME), json).expect("write manifest");

        let error = read_flow_cache(temp_dir.path()).expect_err("absolute path rejected");

        assert!(error.to_string().contains("must be relative"));
    }
}
