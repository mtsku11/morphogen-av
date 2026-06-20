use std::{fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    GrainDescriptor, GrainSelection, GranularMosaicSettings, RenderError, GRANULAR_MOSAIC_ALGORITHM,
};

const GRAIN_CACHE_VERSION: u32 = 1;
pub const GRAIN_DESCRIPTOR_CACHE_FILE_NAME: &str = "grain_descriptors.json";
pub const GRAIN_SELECTION_CACHE_FILE_NAME: &str = "grain_selection.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GranularMosaicDescriptorCache {
    pub version: u32,
    pub kind: String,
    pub algorithm: String,
    pub carrier_width: u32,
    pub carrier_height: u32,
    pub grain_size: u32,
    pub carrier_fingerprint: String,
    pub descriptors: Vec<GrainDescriptor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GranularMosaicSelectionCache {
    pub version: u32,
    pub kind: String,
    pub algorithm: String,
    pub modulator_fingerprint: String,
    pub carrier_fingerprint: String,
    pub carrier_width: u32,
    pub carrier_height: u32,
    pub grain_size: u32,
    pub variation: f32,
    pub seed: u64,
    pub selection: GrainSelection,
}

pub fn write_grain_descriptor_cache(
    directory: impl AsRef<Path>,
    carrier_width: u32,
    carrier_height: u32,
    grain_size: u32,
    carrier_fingerprint: impl Into<String>,
    descriptors: &[GrainDescriptor],
) -> Result<GranularMosaicDescriptorCache, RenderError> {
    let cache = GranularMosaicDescriptorCache {
        version: GRAIN_CACHE_VERSION,
        kind: "granular_mosaic_grain_descriptors".to_string(),
        algorithm: GRANULAR_MOSAIC_ALGORITHM.to_string(),
        carrier_width,
        carrier_height,
        grain_size,
        carrier_fingerprint: carrier_fingerprint.into(),
        descriptors: descriptors.to_vec(),
    };
    validate_descriptor_cache(&cache)?;
    let directory = directory.as_ref();
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join(GRAIN_DESCRIPTOR_CACHE_FILE_NAME),
        serde_json::to_string_pretty(&cache)?,
    )?;
    Ok(cache)
}

pub fn read_grain_descriptor_cache(
    directory: impl AsRef<Path>,
) -> Result<GranularMosaicDescriptorCache, RenderError> {
    let cache: GranularMosaicDescriptorCache = serde_json::from_str(&fs::read_to_string(
        directory.as_ref().join(GRAIN_DESCRIPTOR_CACHE_FILE_NAME),
    )?)?;
    validate_descriptor_cache(&cache)?;
    Ok(cache)
}

pub fn write_grain_selection_cache(
    directory: impl AsRef<Path>,
    modulator_fingerprint: impl Into<String>,
    carrier_fingerprint: impl Into<String>,
    carrier_width: u32,
    carrier_height: u32,
    settings: GranularMosaicSettings,
    selection: &GrainSelection,
) -> Result<GranularMosaicSelectionCache, RenderError> {
    let cache = GranularMosaicSelectionCache {
        version: GRAIN_CACHE_VERSION,
        kind: "granular_mosaic_selection".to_string(),
        algorithm: GRANULAR_MOSAIC_ALGORITHM.to_string(),
        modulator_fingerprint: modulator_fingerprint.into(),
        carrier_fingerprint: carrier_fingerprint.into(),
        carrier_width,
        carrier_height,
        grain_size: settings.grain_size,
        variation: settings.variation,
        seed: settings.seed,
        selection: selection.clone(),
    };
    validate_selection_cache(&cache)?;
    let directory = directory.as_ref();
    fs::create_dir_all(directory)?;
    fs::write(
        directory.join(GRAIN_SELECTION_CACHE_FILE_NAME),
        serde_json::to_string_pretty(&cache)?,
    )?;
    Ok(cache)
}

pub fn read_grain_selection_cache(
    directory: impl AsRef<Path>,
) -> Result<GranularMosaicSelectionCache, RenderError> {
    let cache: GranularMosaicSelectionCache = serde_json::from_str(&fs::read_to_string(
        directory.as_ref().join(GRAIN_SELECTION_CACHE_FILE_NAME),
    )?)?;
    validate_selection_cache(&cache)?;
    Ok(cache)
}

fn validate_descriptor_cache(cache: &GranularMosaicDescriptorCache) -> Result<(), RenderError> {
    validate_common(
        cache.version,
        &cache.kind,
        &cache.algorithm,
        cache.carrier_width,
        cache.carrier_height,
        cache.grain_size,
    )?;
    if cache.kind != "granular_mosaic_grain_descriptors" {
        return Err(RenderError::InvalidGranularMosaicCache(
            "descriptor cache has the wrong kind".to_string(),
        ));
    }
    if cache.carrier_fingerprint.is_empty() {
        return Err(RenderError::InvalidGranularMosaicCache(
            "carrier fingerprint must not be empty".to_string(),
        ));
    }
    let expected = grain_count(cache.carrier_width, cache.carrier_height, cache.grain_size)?;
    if cache.descriptors.len() != expected {
        return Err(RenderError::InvalidGranularMosaicCache(format!(
            "expected {expected} descriptors, got {}",
            cache.descriptors.len()
        )));
    }
    for (expected_index, descriptor) in cache.descriptors.iter().enumerate() {
        let expected_index = expected_index as u32;
        let columns = div_ceil(cache.carrier_width, cache.grain_size);
        if descriptor.index != expected_index
            || descriptor.origin_x != (expected_index % columns) * cache.grain_size
            || descriptor.origin_y != (expected_index / columns) * cache.grain_size
            || !descriptor.mean_luminance.is_finite()
        {
            return Err(RenderError::InvalidGranularMosaicCache(
                "descriptor data does not match the carrier grain grid".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_selection_cache(cache: &GranularMosaicSelectionCache) -> Result<(), RenderError> {
    validate_common(
        cache.version,
        &cache.kind,
        &cache.algorithm,
        cache.carrier_width,
        cache.carrier_height,
        cache.grain_size,
    )?;
    if cache.kind != "granular_mosaic_selection" {
        return Err(RenderError::InvalidGranularMosaicCache(
            "selection cache has the wrong kind".to_string(),
        ));
    }
    if cache.modulator_fingerprint.is_empty() || cache.carrier_fingerprint.is_empty() {
        return Err(RenderError::InvalidGranularMosaicCache(
            "source fingerprints must not be empty".to_string(),
        ));
    }
    if !cache.variation.is_finite() || !(0.0..=1.0).contains(&cache.variation) {
        return Err(RenderError::InvalidGranularMosaicCache(
            "variation must be a finite value between zero and one".to_string(),
        ));
    }
    let columns = div_ceil(cache.carrier_width, cache.grain_size);
    let rows = div_ceil(cache.carrier_height, cache.grain_size);
    let expected = grain_count(cache.carrier_width, cache.carrier_height, cache.grain_size)?;
    if cache.selection.columns != columns
        || cache.selection.rows != rows
        || cache.selection.indices.len() != expected
        || cache
            .selection
            .indices
            .iter()
            .any(|index| *index as usize >= expected)
    {
        return Err(RenderError::InvalidGranularMosaicCache(
            "selection data does not match the carrier grain grid".to_string(),
        ));
    }
    Ok(())
}

fn validate_common(
    version: u32,
    kind: &str,
    algorithm: &str,
    carrier_width: u32,
    carrier_height: u32,
    grain_size: u32,
) -> Result<(), RenderError> {
    if version != GRAIN_CACHE_VERSION {
        return Err(RenderError::InvalidGranularMosaicCache(format!(
            "unsupported cache version {version}"
        )));
    }
    if !matches!(
        kind,
        "granular_mosaic_grain_descriptors" | "granular_mosaic_selection"
    ) {
        return Err(RenderError::InvalidGranularMosaicCache(format!(
            "unsupported cache kind {kind}"
        )));
    }
    if algorithm != GRANULAR_MOSAIC_ALGORITHM {
        return Err(RenderError::InvalidGranularMosaicCache(format!(
            "unsupported cache algorithm {algorithm}"
        )));
    }
    if carrier_width == 0 || carrier_height == 0 || grain_size == 0 {
        return Err(RenderError::InvalidGranularMosaicCache(
            "carrier dimensions and grain size must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

fn grain_count(width: u32, height: u32, grain_size: u32) -> Result<usize, RenderError> {
    (div_ceil(width, grain_size) as usize)
        .checked_mul(div_ceil(height, grain_size) as usize)
        .ok_or_else(|| {
            RenderError::InvalidGranularMosaicCache("grain grid is too large".to_string())
        })
}

fn div_ceil(value: u32, divisor: u32) -> u32 {
    value / divisor + u32::from(value % divisor != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caches_round_trip_valid_descriptors_and_selection() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let descriptors = vec![
            GrainDescriptor {
                index: 0,
                origin_x: 0,
                origin_y: 0,
                mean_luminance: 0.2,
            },
            GrainDescriptor {
                index: 1,
                origin_x: 2,
                origin_y: 0,
                mean_luminance: 0.8,
            },
        ];
        let settings = GranularMosaicSettings {
            grain_size: 2,
            rearrangement: 1.0,
            variation: 0.25,
            seed: 7,
        };
        let selection = GrainSelection {
            columns: 2,
            rows: 1,
            indices: vec![1, 0],
        };

        let descriptors_written =
            write_grain_descriptor_cache(temp_dir.path(), 4, 2, 2, "fnv1a64:carrier", &descriptors)
                .expect("write descriptors");
        let selection_written = write_grain_selection_cache(
            temp_dir.path(),
            "fnv1a64:modulator",
            "fnv1a64:carrier",
            4,
            2,
            settings,
            &selection,
        )
        .expect("write selection");

        assert_eq!(
            read_grain_descriptor_cache(temp_dir.path()).expect("read descriptors"),
            descriptors_written
        );
        assert_eq!(
            read_grain_selection_cache(temp_dir.path()).expect("read selection"),
            selection_written
        );
    }

    #[test]
    fn selection_cache_rejects_invalid_referenced_grain() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let cache = serde_json::json!({
            "version": 1,
            "kind": "granular_mosaic_selection",
            "algorithm": GRANULAR_MOSAIC_ALGORITHM,
            "modulator_fingerprint": "fnv1a64:modulator",
            "carrier_fingerprint": "fnv1a64:carrier",
            "carrier_width": 2,
            "carrier_height": 1,
            "grain_size": 1,
            "variation": 0.0,
            "seed": 0,
            "selection": { "columns": 2, "rows": 1, "indices": [0, 2] }
        });
        fs::write(
            temp_dir.path().join(GRAIN_SELECTION_CACHE_FILE_NAME),
            serde_json::to_string(&cache).expect("serialize cache"),
        )
        .expect("write cache");

        assert!(read_grain_selection_cache(temp_dir.path()).is_err());
    }
}
