use serde::{Deserialize, Serialize};

use crate::{sample_bilinear_clamped, ImageBufferF32, RenderError};

pub const GRANULAR_MOSAIC_ALGORITHM: &str = "luma_nearest_grain_cpu_v1";

/// Parameters for deterministic visual grain recomposition. A tile's average
/// Source A luminance selects a Source B tile; variation blends that choice
/// with a seeded alternate tile, while rearrangement blends the selected
/// carrier coordinates with their original output coordinates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GranularMosaicSettings {
    pub grain_size: u32,
    pub rearrangement: f32,
    pub variation: f32,
    pub seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrainDescriptor {
    pub index: u32,
    pub origin_x: u32,
    pub origin_y: u32,
    pub mean_luminance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrainSelection {
    pub columns: u32,
    pub rows: u32,
    pub indices: Vec<u32>,
}

impl GranularMosaicSettings {
    pub fn validate(self) -> Result<(), RenderError> {
        if self.grain_size == 0 {
            return Err(RenderError::InvalidGranularMosaicSettings(
                "grain_size must be greater than zero".to_string(),
            ));
        }

        for (name, value) in [
            ("rearrangement", self.rearrangement),
            ("variation", self.variation),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(RenderError::InvalidGranularMosaicSettings(format!(
                    "{name} must be a finite value between zero and one"
                )));
            }
        }

        Ok(())
    }
}

/// Recompose Source B into fixed-size visual grains selected by Source A
/// luminance. Output dimensions always follow the carrier. Source A may use a
/// different resolution; its pixels are sampled in output-normalized space.
/// Carrier samples use the repository's clamped bilinear border behavior.
pub fn granular_mosaic_cpu(
    modulator: &ImageBufferF32,
    carrier: &ImageBufferF32,
    settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;

    let descriptors = analyze_grains_cpu(carrier, settings.grain_size)?;
    let selection = select_grains_cpu(
        modulator,
        carrier.width,
        carrier.height,
        &descriptors,
        settings,
    )?;
    granular_mosaic_with_selection_cpu(carrier, &selection, settings)
}

/// Analyze fixed-size Source B tiles into the first descriptor set used by the
/// granular renderer. The descriptor is intentionally small and portable so it
/// can become an inspectable cache sidecar before richer color and audio
/// descriptors are added.
pub fn analyze_grains_cpu(
    carrier: &ImageBufferF32,
    grain_size: u32,
) -> Result<Vec<GrainDescriptor>, RenderError> {
    if grain_size == 0 {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain_size must be greater than zero".to_string(),
        ));
    }

    let columns = div_ceil(carrier.width, grain_size);
    let rows = div_ceil(carrier.height, grain_size);
    let mut descriptors = Vec::with_capacity((columns as usize) * (rows as usize));

    for tile_y in 0..rows {
        for tile_x in 0..columns {
            let origin_x = tile_x * grain_size;
            let origin_y = tile_y * grain_size;
            descriptors.push(GrainDescriptor {
                index: tile_y * columns + tile_x,
                origin_x,
                origin_y,
                mean_luminance: average_carrier_tile_luminance(
                    carrier, origin_x, origin_y, grain_size,
                ),
            });
        }
    }

    Ok(descriptors)
}

/// Select carrier grains for each output tile by nearest luma descriptor.
/// `variation` blends the deterministic nearest match with a seeded alternate
/// grain, leaving the descriptor-oriented path exact when variation is zero.
pub fn select_grains_cpu(
    modulator: &ImageBufferF32,
    carrier_width: u32,
    carrier_height: u32,
    descriptors: &[GrainDescriptor],
    settings: GranularMosaicSettings,
) -> Result<GrainSelection, RenderError> {
    settings.validate()?;
    let columns = div_ceil(carrier_width, settings.grain_size);
    let rows = div_ceil(carrier_height, settings.grain_size);
    validate_descriptors(
        descriptors,
        carrier_width,
        carrier_height,
        settings.grain_size,
    )?;

    let mut indices = Vec::with_capacity((columns as usize) * (rows as usize));
    for tile_y in 0..rows {
        for tile_x in 0..columns {
            let luminance = average_modulator_tile_luminance(
                modulator,
                carrier_width,
                carrier_height,
                tile_x,
                tile_y,
                settings.grain_size,
            );
            indices.push(select_grain_index(
                luminance,
                descriptors,
                settings.variation,
                settings.seed,
                tile_x,
                tile_y,
            ));
        }
    }

    Ok(GrainSelection {
        columns,
        rows,
        indices,
    })
}

/// Render a granular mosaic from a previously computed selection map. This is
/// the cache-friendly form used by offline sequence rendering.
pub fn granular_mosaic_with_selection_cpu(
    carrier: &ImageBufferF32,
    selection: &GrainSelection,
    settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let descriptor_count = (div_ceil(carrier.width, settings.grain_size) as usize)
        .checked_mul(div_ceil(carrier.height, settings.grain_size) as usize)
        .ok_or_else(|| {
            RenderError::InvalidGranularMosaicSettings("grain grid is too large".to_string())
        })?;
    validate_selection(
        selection,
        descriptor_count,
        carrier.width,
        carrier.height,
        settings.grain_size,
    )?;

    let mut pixels = Vec::with_capacity(carrier.pixels.len());
    for y in 0..carrier.height {
        for x in 0..carrier.width {
            let tile_x = x / settings.grain_size;
            let tile_y = y / settings.grain_size;
            let tile_index = tile_y as usize * selection.columns as usize + tile_x as usize;
            let source_index = selection.indices[tile_index];
            let source_x =
                (source_index % selection.columns) * settings.grain_size + x % settings.grain_size;
            let source_y =
                (source_index / selection.columns) * settings.grain_size + y % settings.grain_size;
            let sample_x = lerp(x as f32, source_x as f32, settings.rearrangement);
            let sample_y = lerp(y as f32, source_y as f32, settings.rearrangement);
            pixels.push(sample_bilinear_clamped(carrier, sample_x, sample_y));
        }
    }

    ImageBufferF32::new(carrier.width, carrier.height, pixels)
}

fn average_modulator_tile_luminance(
    modulator: &ImageBufferF32,
    output_width: u32,
    output_height: u32,
    tile_x: u32,
    tile_y: u32,
    grain_size: u32,
) -> f32 {
    let start_x = tile_x * grain_size;
    let start_y = tile_y * grain_size;
    let end_x = (start_x + grain_size).min(output_width);
    let end_y = (start_y + grain_size).min(output_height);
    let mut total = 0.0_f32;
    let mut count = 0_u32;

    for y in start_y..end_y {
        for x in start_x..end_x {
            let pixel = sample_bilinear_clamped(
                modulator,
                remap_coordinate(x, output_width, modulator.width),
                remap_coordinate(y, output_height, modulator.height),
            );
            total += luminance(pixel);
            count += 1;
        }
    }

    total / count as f32
}

fn average_carrier_tile_luminance(
    carrier: &ImageBufferF32,
    start_x: u32,
    start_y: u32,
    grain_size: u32,
) -> f32 {
    let end_x = (start_x + grain_size).min(carrier.width);
    let end_y = (start_y + grain_size).min(carrier.height);
    let mut total = 0.0_f32;
    let mut count = 0_u32;

    for y in start_y..end_y {
        for x in start_x..end_x {
            total += luminance(carrier.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 0.0]));
            count += 1;
        }
    }

    total / count as f32
}

fn select_grain_index(
    luminance: f32,
    descriptors: &[GrainDescriptor],
    variation: f32,
    seed: u64,
    tile_x: u32,
    tile_y: u32,
) -> u32 {
    let luma_selected = descriptors
        .iter()
        .min_by(|left, right| {
            let left_distance = (left.mean_luminance - luminance).abs();
            let right_distance = (right.mean_luminance - luminance).abs();
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.index.cmp(&right.index))
        })
        .map(|descriptor| descriptor.index)
        .unwrap_or(0);
    let random_selected = tile_hash(seed, tile_x, tile_y) % descriptors.len() as u64;
    lerp(luma_selected as f32, random_selected as f32, variation).round() as u32
}

fn validate_descriptors(
    descriptors: &[GrainDescriptor],
    carrier_width: u32,
    carrier_height: u32,
    grain_size: u32,
) -> Result<(), RenderError> {
    let columns = div_ceil(carrier_width, grain_size);
    let rows = div_ceil(carrier_height, grain_size);
    let expected = columns as usize * rows as usize;
    if descriptors.len() != expected {
        return Err(RenderError::InvalidGranularMosaicSettings(format!(
            "expected {expected} grain descriptors, got {}",
            descriptors.len()
        )));
    }
    for (expected_index, descriptor) in descriptors.iter().enumerate() {
        let expected_index = expected_index as u32;
        let expected_x = (expected_index % columns) * grain_size;
        let expected_y = (expected_index / columns) * grain_size;
        if descriptor.index != expected_index
            || descriptor.origin_x != expected_x
            || descriptor.origin_y != expected_y
            || !descriptor.mean_luminance.is_finite()
        {
            return Err(RenderError::InvalidGranularMosaicSettings(
                "grain descriptors do not match the carrier grid".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_selection(
    selection: &GrainSelection,
    descriptor_count: usize,
    carrier_width: u32,
    carrier_height: u32,
    grain_size: u32,
) -> Result<(), RenderError> {
    let columns = div_ceil(carrier_width, grain_size);
    let rows = div_ceil(carrier_height, grain_size);
    if selection.columns != columns || selection.rows != rows {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain selection dimensions do not match the carrier grid".to_string(),
        ));
    }
    let expected = columns as usize * rows as usize;
    if selection.indices.len() != expected {
        return Err(RenderError::InvalidGranularMosaicSettings(format!(
            "expected {expected} grain selections, got {}",
            selection.indices.len()
        )));
    }
    if selection
        .indices
        .iter()
        .any(|index| *index as usize >= descriptor_count)
    {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain selection references an unknown descriptor".to_string(),
        ));
    }
    Ok(())
}

fn remap_coordinate(value: u32, source_extent: u32, target_extent: u32) -> f32 {
    if source_extent <= 1 || target_extent <= 1 {
        return 0.0;
    }

    value as f32 * (target_extent - 1) as f32 / (source_extent - 1) as f32
}

fn div_ceil(value: u32, divisor: u32) -> u32 {
    value / divisor + u32::from(value % divisor != 0)
}

fn luminance(pixel: [f32; 4]) -> f32 {
    pixel[0] * 0.2126 + pixel[1] * 0.7152 + pixel[2] * 0.0722
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn tile_hash(seed: u64, tile_x: u32, tile_y: u32) -> u64 {
    let mut value = seed
        ^ u64::from(tile_x).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ u64::from(tile_y).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(values: &[f32]) -> ImageBufferF32 {
        ImageBufferF32::new(
            values.len() as u32,
            1,
            values
                .iter()
                .copied()
                .map(|value| [value, value, value, 1.0])
                .collect(),
        )
        .expect("valid test image")
    }

    #[test]
    fn zero_rearrangement_preserves_the_carrier() {
        let modulator = image(&[1.0, 0.0, 1.0, 0.0]);
        let carrier = image(&[0.1, 0.3, 0.6, 0.9]);
        let rendered = granular_mosaic_cpu(
            &modulator,
            &carrier,
            GranularMosaicSettings {
                grain_size: 1,
                rearrangement: 0.0,
                variation: 1.0,
                seed: 9,
            },
        )
        .expect("render mosaic");

        assert_eq!(rendered, carrier);
    }

    #[test]
    fn luma_selects_carrier_grains_without_variation() {
        let modulator = image(&[0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0]);
        let carrier = image(&[0.1, 0.3, 0.6, 0.9]);
        let rendered = granular_mosaic_cpu(
            &modulator,
            &carrier,
            GranularMosaicSettings {
                grain_size: 1,
                rearrangement: 1.0,
                variation: 0.0,
                seed: 0,
            },
        )
        .expect("render mosaic");

        assert_eq!(rendered, carrier);
    }

    #[test]
    fn seeded_variation_is_deterministic() {
        let modulator = image(&[0.5, 0.5, 0.5, 0.5]);
        let carrier = image(&[0.1, 0.3, 0.6, 0.9]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 1.0,
            seed: 42,
        };

        let first = granular_mosaic_cpu(&modulator, &carrier, settings).expect("first mosaic");
        let second = granular_mosaic_cpu(&modulator, &carrier, settings).expect("second mosaic");

        assert_eq!(first, second);
        assert_ne!(first, carrier);
    }

    #[test]
    fn cached_selection_renders_the_same_mosaic_as_the_one_shot_path() {
        let modulator = image(&[0.0, 0.5, 1.0, 0.5]);
        let carrier = image(&[0.1, 0.3, 0.6, 0.9]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.4,
            seed: 8,
        };

        let descriptors = analyze_grains_cpu(&carrier, settings.grain_size).expect("descriptors");
        let selection = select_grains_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &descriptors,
            settings,
        )
        .expect("selection");
        let cached = granular_mosaic_with_selection_cpu(&carrier, &selection, settings)
            .expect("cached mosaic");
        let one_shot =
            granular_mosaic_cpu(&modulator, &carrier, settings).expect("one-shot mosaic");

        assert_eq!(cached, one_shot);
    }

    #[test]
    fn settings_reject_non_positive_grain_size_and_invalid_controls() {
        let mut settings = GranularMosaicSettings {
            grain_size: 0,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };
        assert!(settings.validate().is_err());

        settings.grain_size = 8;
        settings.variation = 1.1;
        assert!(settings.validate().is_err());
    }
}
