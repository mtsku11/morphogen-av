use serde::{Deserialize, Serialize};

use crate::{sample_bilinear_clamped, ImageBufferF32, RenderError};

pub const GRANULAR_MOSAIC_ALGORITHM: &str = "luma_nearest_grain_cpu_v1";

/// Algorithm identifier for the multimodal RGB nearest-neighbor selection path
/// (step 6). Distinct from [`GRANULAR_MOSAIC_ALGORITHM`] so a stale luma sidecar
/// never satisfies a multimodal request.
pub const MULTIMODAL_GRAIN_ALGORITHM: &str = "multimodal_nearest_grain_cpu_v1";

/// Algorithm identifier for the temporal-grain-pool joint-AV selection path
/// (step 6b). Grains are drawn from across time and matched on a combined
/// `[mean_color | audio]` feature vector. Distinct id invalidates stale
/// single-frame sidecars.
pub const POOLED_GRAIN_ALGORITHM: &str = "pooled_av_nearest_grain_cpu_v1";

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

/// Multimodal grain descriptor (step 6). Carries the per-channel mean color of a
/// Source B tile so selection can match on RGB rather than luminance alone. The
/// feature vector is intentionally stored as a fixed array now; audio dimensions
/// are appended in a later step under a new algorithm identifier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrainColorDescriptor {
    pub index: u32,
    pub origin_x: u32,
    pub origin_y: u32,
    pub mean_color: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrainSelection {
    pub columns: u32,
    pub rows: u32,
    pub indices: Vec<u32>,
}

/// A single grain in a temporal pool (step 6b). Carries its source frame, tile
/// origin, mean color, and the carrier-audio descriptor vector sampled at that
/// frame's source time (shared by every grain of the same frame). The combined
/// `[mean_color | audio]` vector is the joint-AV matching feature.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PooledGrainDescriptor {
    pub global_index: u32,
    pub frame_index: u32,
    pub origin_x: u32,
    pub origin_y: u32,
    pub mean_color: [f32; 3],
    pub audio: Vec<f32>,
}

/// A whole-clip temporal grain library (step 6b). Grains are assembled from `F`
/// Source B frames that share dimensions and grain grid, indexed globally in
/// frame-major then row-major order. `audio_dims` is the per-grain audio vector
/// length (`0` ⇒ color-only matching across time).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GrainPool {
    pub columns: u32,
    pub rows: u32,
    pub grain_size: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub audio_dims: usize,
    pub grains: Vec<PooledGrainDescriptor>,
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

/// Analyze fixed-size Source B tiles into RGB feature descriptors for multimodal
/// selection (step 6). Mirrors [`analyze_grains_cpu`] but records the per-channel
/// mean color instead of a single luminance.
pub fn analyze_grain_colors_cpu(
    carrier: &ImageBufferF32,
    grain_size: u32,
) -> Result<Vec<GrainColorDescriptor>, RenderError> {
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
            descriptors.push(GrainColorDescriptor {
                index: tile_y * columns + tile_x,
                origin_x,
                origin_y,
                mean_color: average_carrier_tile_color(carrier, origin_x, origin_y, grain_size),
            });
        }
    }

    Ok(descriptors)
}

/// Select carrier grains for each output tile by nearest RGB descriptor (step 6).
/// Matching uses weighted Euclidean distance over the color feature vector, with
/// ties broken by ascending grain index. `variation` blends the deterministic
/// nearest match with a seeded alternate grain exactly as the luma path does, so
/// `variation = 0` leaves the RGB match exact.
pub fn select_grains_multimodal_cpu(
    modulator: &ImageBufferF32,
    carrier_width: u32,
    carrier_height: u32,
    descriptors: &[GrainColorDescriptor],
    settings: GranularMosaicSettings,
) -> Result<GrainSelection, RenderError> {
    settings.validate()?;
    let columns = div_ceil(carrier_width, settings.grain_size);
    let rows = div_ceil(carrier_height, settings.grain_size);
    validate_color_descriptors(
        descriptors,
        carrier_width,
        carrier_height,
        settings.grain_size,
    )?;

    let mut indices = Vec::with_capacity((columns as usize) * (rows as usize));
    for tile_y in 0..rows {
        for tile_x in 0..columns {
            let target = average_modulator_tile_color(
                modulator,
                carrier_width,
                carrier_height,
                tile_x,
                tile_y,
                settings.grain_size,
            );
            indices.push(select_color_grain_index(
                target,
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

/// Assemble a whole-clip temporal grain pool (step 6b) from `F` Source B frames
/// and their per-frame carrier-audio descriptor vectors. All frames must share
/// dimensions and the audio vectors must share length; each frame contributes a
/// full grain grid whose grains inherit that frame's audio descriptor.
pub fn analyze_grain_pool_cpu(
    frames: &[ImageBufferF32],
    frame_audio: &[Vec<f32>],
    grain_size: u32,
) -> Result<GrainPool, RenderError> {
    if grain_size == 0 {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain_size must be greater than zero".to_string(),
        ));
    }
    if frames.is_empty() {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain pool requires at least one frame".to_string(),
        ));
    }
    if frames.len() != frame_audio.len() {
        return Err(RenderError::InvalidGranularMosaicSettings(format!(
            "expected one audio vector per frame: {} frames, {} audio vectors",
            frames.len(),
            frame_audio.len()
        )));
    }

    let frame_width = frames[0].width;
    let frame_height = frames[0].height;
    let audio_dims = frame_audio[0].len();
    for (frame, audio) in frames.iter().zip(frame_audio.iter()) {
        if frame.width != frame_width || frame.height != frame_height {
            return Err(RenderError::InvalidGranularMosaicSettings(
                "all grain-pool frames must share dimensions".to_string(),
            ));
        }
        if audio.len() != audio_dims {
            return Err(RenderError::InvalidGranularMosaicSettings(
                "all grain-pool audio vectors must share length".to_string(),
            ));
        }
        if !audio.iter().all(|value| value.is_finite()) {
            return Err(RenderError::InvalidGranularMosaicSettings(
                "grain-pool audio descriptors must be finite".to_string(),
            ));
        }
    }

    let columns = div_ceil(frame_width, grain_size);
    let rows = div_ceil(frame_height, grain_size);
    let per_frame = columns as usize * rows as usize;
    let mut grains = Vec::with_capacity(per_frame * frames.len());
    let mut global_index = 0_u32;
    for (frame_index, (frame, audio)) in frames.iter().zip(frame_audio.iter()).enumerate() {
        for tile_y in 0..rows {
            for tile_x in 0..columns {
                let origin_x = tile_x * grain_size;
                let origin_y = tile_y * grain_size;
                grains.push(PooledGrainDescriptor {
                    global_index,
                    frame_index: frame_index as u32,
                    origin_x,
                    origin_y,
                    mean_color: average_carrier_tile_color(frame, origin_x, origin_y, grain_size),
                    audio: audio.clone(),
                });
                global_index += 1;
            }
        }
    }

    Ok(GrainPool {
        columns,
        rows,
        grain_size,
        frame_width,
        frame_height,
        audio_dims,
        grains,
    })
}

/// Select pool grains for each output tile by nearest combined `[color | audio]`
/// descriptor (step 6b). The query is Source A's per-tile mean color plus this
/// output frame's audio descriptor vector; `audio_weight` scales every audio
/// dimension. Selected indices are global pool indices. `variation` blends the
/// nearest match with a seeded alternate pool grain, leaving the match exact at
/// `variation = 0`.
pub fn select_grains_from_pool_cpu(
    modulator: &ImageBufferF32,
    carrier_width: u32,
    carrier_height: u32,
    query_audio: &[f32],
    pool: &GrainPool,
    settings: GranularMosaicSettings,
    audio_weight: f32,
) -> Result<GrainSelection, RenderError> {
    settings.validate()?;
    validate_pool(pool, settings.grain_size)?;
    if query_audio.len() != pool.audio_dims {
        return Err(RenderError::InvalidGranularMosaicSettings(format!(
            "query audio length {} does not match pool audio dims {}",
            query_audio.len(),
            pool.audio_dims
        )));
    }
    if !audio_weight.is_finite() || audio_weight < 0.0 {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "audio_weight must be a finite, non-negative value".to_string(),
        ));
    }
    if !query_audio.iter().all(|value| value.is_finite()) {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "query audio descriptors must be finite".to_string(),
        ));
    }

    let columns = div_ceil(carrier_width, settings.grain_size);
    let rows = div_ceil(carrier_height, settings.grain_size);
    let mut indices = Vec::with_capacity((columns as usize) * (rows as usize));
    for tile_y in 0..rows {
        for tile_x in 0..columns {
            let target = average_modulator_tile_color(
                modulator,
                carrier_width,
                carrier_height,
                tile_x,
                tile_y,
                settings.grain_size,
            );
            indices.push(select_pool_grain_index(
                target,
                query_audio,
                audio_weight,
                pool,
                settings,
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

/// Render a temporal-pool mosaic (step 6b). Because a selected grain and the
/// current carrier pixel live in different frames, `rearrangement` is a
/// cross-frame value blend: `0` yields the current carrier exactly, `1` yields
/// the selected grain's pixel from its source frame, and values between linearly
/// blend the two sampled colors. `pool_frames` must be the same frames the pool
/// was analyzed from (indexed by `frame_index`).
pub fn granular_mosaic_with_pool_selection_cpu(
    pool_frames: &[ImageBufferF32],
    pool: &GrainPool,
    carrier: &ImageBufferF32,
    selection: &GrainSelection,
    settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    validate_pool(pool, settings.grain_size)?;
    if carrier.width != pool.frame_width || carrier.height != pool.frame_height {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "carrier dimensions do not match the grain pool".to_string(),
        ));
    }
    validate_selection(
        selection,
        pool.grains.len(),
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
            let grain = &pool.grains[selection.indices[tile_index] as usize];
            let frame = pool_frames.get(grain.frame_index as usize).ok_or_else(|| {
                RenderError::InvalidGranularMosaicSettings(
                    "grain selection references a frame outside the supplied pool frames"
                        .to_string(),
                )
            })?;
            let grain_x = grain.origin_x + x % settings.grain_size;
            let grain_y = grain.origin_y + y % settings.grain_size;
            let grain_pixel = clamped_pixel(frame, grain_x, grain_y);
            let carrier_pixel = clamped_pixel(carrier, x, y);
            let mut blended = [0.0_f32; 4];
            for channel in 0..4 {
                blended[channel] = lerp(
                    carrier_pixel[channel],
                    grain_pixel[channel],
                    settings.rearrangement,
                );
            }
            pixels.push(blended);
        }
    }

    ImageBufferF32::new(carrier.width, carrier.height, pixels)
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

fn average_modulator_tile_color(
    modulator: &ImageBufferF32,
    output_width: u32,
    output_height: u32,
    tile_x: u32,
    tile_y: u32,
    grain_size: u32,
) -> [f32; 3] {
    let start_x = tile_x * grain_size;
    let start_y = tile_y * grain_size;
    let end_x = (start_x + grain_size).min(output_width);
    let end_y = (start_y + grain_size).min(output_height);
    let mut total = [0.0_f32; 3];
    let mut count = 0_u32;

    for y in start_y..end_y {
        for x in start_x..end_x {
            let pixel = sample_bilinear_clamped(
                modulator,
                remap_coordinate(x, output_width, modulator.width),
                remap_coordinate(y, output_height, modulator.height),
            );
            total[0] += pixel[0];
            total[1] += pixel[1];
            total[2] += pixel[2];
            count += 1;
        }
    }

    let inverse = 1.0 / count as f32;
    [total[0] * inverse, total[1] * inverse, total[2] * inverse]
}

fn average_carrier_tile_color(
    carrier: &ImageBufferF32,
    start_x: u32,
    start_y: u32,
    grain_size: u32,
) -> [f32; 3] {
    let end_x = (start_x + grain_size).min(carrier.width);
    let end_y = (start_y + grain_size).min(carrier.height);
    let mut total = [0.0_f32; 3];
    let mut count = 0_u32;

    for y in start_y..end_y {
        for x in start_x..end_x {
            let pixel = carrier.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 0.0]);
            total[0] += pixel[0];
            total[1] += pixel[1];
            total[2] += pixel[2];
            count += 1;
        }
    }

    let inverse = 1.0 / count as f32;
    [total[0] * inverse, total[1] * inverse, total[2] * inverse]
}

fn select_color_grain_index(
    target: [f32; 3],
    descriptors: &[GrainColorDescriptor],
    variation: f32,
    seed: u64,
    tile_x: u32,
    tile_y: u32,
) -> u32 {
    // Equal per-channel weights in this slice; the distance is written over
    // feature slices so audio dimensions can be appended later.
    const FEATURE_WEIGHTS: [f32; 3] = [1.0, 1.0, 1.0];
    let feature_selected = descriptors
        .iter()
        .min_by(|left, right| {
            let left_distance = weighted_distance_sq(&left.mean_color, &target, &FEATURE_WEIGHTS);
            let right_distance = weighted_distance_sq(&right.mean_color, &target, &FEATURE_WEIGHTS);
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.index.cmp(&right.index))
        })
        .map(|descriptor| descriptor.index)
        .unwrap_or(0);
    let random_selected = tile_hash(seed, tile_x, tile_y) % descriptors.len() as u64;
    lerp(feature_selected as f32, random_selected as f32, variation).round() as u32
}

fn select_pool_grain_index(
    target_color: [f32; 3],
    query_audio: &[f32],
    audio_weight: f32,
    pool: &GrainPool,
    settings: GranularMosaicSettings,
    tile_x: u32,
    tile_y: u32,
) -> u32 {
    let feature_selected = pool
        .grains
        .iter()
        .min_by(|left, right| {
            let left_distance =
                pooled_distance_sq(left, &target_color, query_audio, audio_weight);
            let right_distance =
                pooled_distance_sq(right, &target_color, query_audio, audio_weight);
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.global_index.cmp(&right.global_index))
        })
        .map(|grain| grain.global_index)
        .unwrap_or(0);
    let random_selected = tile_hash(settings.seed, tile_x, tile_y) % pool.grains.len() as u64;
    lerp(
        feature_selected as f32,
        random_selected as f32,
        settings.variation,
    )
    .round() as u32
}

/// Combined squared distance over the `[color(3) | audio(k)]` feature vector:
/// equal per-channel color weights and a single scalar `audio_weight` on every
/// audio dimension. Alloc-free so it stays cheap inside the per-tile nearest
/// search over the whole pool.
fn pooled_distance_sq(
    grain: &PooledGrainDescriptor,
    target_color: &[f32; 3],
    query_audio: &[f32],
    audio_weight: f32,
) -> f32 {
    let mut sum = 0.0_f32;
    for (grain_channel, target_channel) in grain.mean_color.iter().zip(target_color.iter()) {
        let delta = grain_channel - target_channel;
        sum += delta * delta;
    }
    for (grain_value, query_value) in grain.audio.iter().zip(query_audio.iter()) {
        let delta = grain_value - query_value;
        sum += audio_weight * delta * delta;
    }
    sum
}

fn validate_pool(pool: &GrainPool, grain_size: u32) -> Result<(), RenderError> {
    if grain_size != pool.grain_size {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain_size does not match the grain pool".to_string(),
        ));
    }
    if pool.grains.is_empty() {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain pool is empty".to_string(),
        ));
    }
    let columns = div_ceil(pool.frame_width, grain_size);
    let rows = div_ceil(pool.frame_height, grain_size);
    if pool.columns != columns || pool.rows != rows {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain pool grid does not match its frame dimensions".to_string(),
        ));
    }
    let per_frame = columns as usize * rows as usize;
    if per_frame == 0 || pool.grains.len() % per_frame != 0 {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "grain pool size is not a whole number of frame grids".to_string(),
        ));
    }
    for (expected_index, grain) in pool.grains.iter().enumerate() {
        let within_frame = expected_index % per_frame;
        let expected_x = (within_frame as u32 % columns) * grain_size;
        let expected_y = (within_frame as u32 / columns) * grain_size;
        if grain.global_index != expected_index as u32
            || grain.frame_index != (expected_index / per_frame) as u32
            || grain.origin_x != expected_x
            || grain.origin_y != expected_y
            || grain.audio.len() != pool.audio_dims
            || !grain.mean_color.iter().all(|value| value.is_finite())
            || !grain.audio.iter().all(|value| value.is_finite())
        {
            return Err(RenderError::InvalidGranularMosaicSettings(
                "grain pool descriptors do not match the pool geometry".to_string(),
            ));
        }
    }
    Ok(())
}

fn clamped_pixel(image: &ImageBufferF32, x: u32, y: u32) -> [f32; 4] {
    let clamped_x = x.min(image.width.saturating_sub(1));
    let clamped_y = y.min(image.height.saturating_sub(1));
    image
        .pixel(clamped_x, clamped_y)
        .unwrap_or([0.0, 0.0, 0.0, 0.0])
}

/// Weighted squared Euclidean distance over two equal-length feature slices.
/// Shorter inputs are compared over their common length so future feature sets
/// can grow without a rewrite.
fn weighted_distance_sq(a: &[f32], b: &[f32], weights: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .zip(weights.iter())
        .map(|((left, right), weight)| {
            let delta = left - right;
            weight * delta * delta
        })
        .sum()
}

fn validate_color_descriptors(
    descriptors: &[GrainColorDescriptor],
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
            || !descriptor.mean_color.iter().all(|value| value.is_finite())
        {
            return Err(RenderError::InvalidGranularMosaicSettings(
                "grain descriptors do not match the carrier grid".to_string(),
            ));
        }
    }
    Ok(())
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

    fn rgb_image(colors: &[[f32; 3]]) -> ImageBufferF32 {
        ImageBufferF32::new(
            colors.len() as u32,
            1,
            colors.iter().map(|c| [c[0], c[1], c[2], 1.0]).collect(),
        )
        .expect("valid rgb test image")
    }

    #[test]
    fn multimodal_matches_on_color_where_luma_cannot() {
        // grain 0 is a gray whose luminance equals the green modulator's, so the
        // luma path is pulled to it; grain 1 is the near-identical green the RGB
        // path should pick instead.
        let carrier = rgb_image(&[[0.3576, 0.3576, 0.3576], [0.0, 0.45, 0.0]]);
        let modulator = rgb_image(&[[0.0, 0.5, 0.0], [0.0, 0.5, 0.0]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        let color_descriptors =
            analyze_grain_colors_cpu(&carrier, settings.grain_size).expect("color descriptors");
        let multimodal = select_grains_multimodal_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &color_descriptors,
            settings,
        )
        .expect("multimodal selection");
        assert_eq!(multimodal.indices, vec![1, 1]);

        let luma_descriptors =
            analyze_grains_cpu(&carrier, settings.grain_size).expect("luma descriptors");
        let luma = select_grains_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &luma_descriptors,
            settings,
        )
        .expect("luma selection");
        assert_eq!(luma.indices, vec![0, 0]);
    }

    #[test]
    fn multimodal_ties_break_by_ascending_index() {
        let carrier = rgb_image(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
        let modulator = rgb_image(&[[0.5, 0.0, 0.0]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        let descriptors =
            analyze_grain_colors_cpu(&carrier, settings.grain_size).expect("descriptors");
        let selection = select_grains_multimodal_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &descriptors,
            settings,
        )
        .expect("selection");
        assert_eq!(selection.indices, vec![0, 0]);
    }

    #[test]
    fn multimodal_zero_rearrangement_preserves_the_carrier() {
        let modulator = rgb_image(&[[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]);
        let carrier = rgb_image(&[[0.2, 0.1, 0.9], [0.7, 0.3, 0.0]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 0.0,
            variation: 1.0,
            seed: 5,
        };

        let descriptors =
            analyze_grain_colors_cpu(&carrier, settings.grain_size).expect("descriptors");
        let selection = select_grains_multimodal_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &descriptors,
            settings,
        )
        .expect("selection");
        let rendered = granular_mosaic_with_selection_cpu(&carrier, &selection, settings)
            .expect("render mosaic");

        assert_eq!(rendered, carrier);
    }

    #[test]
    fn multimodal_selection_is_deterministic() {
        let modulator = rgb_image(&[[0.0, 0.5, 0.0], [0.9, 0.1, 0.2], [0.3, 0.3, 0.8]]);
        let carrier = rgb_image(&[[0.1, 0.9, 0.2], [0.8, 0.2, 0.1], [0.2, 0.2, 0.7]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.5,
            seed: 17,
        };

        let descriptors =
            analyze_grain_colors_cpu(&carrier, settings.grain_size).expect("descriptors");
        let first = select_grains_multimodal_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &descriptors,
            settings,
        )
        .expect("first selection");
        let second = select_grains_multimodal_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &descriptors,
            settings,
        )
        .expect("second selection");

        assert_eq!(first, second);
    }

    #[test]
    fn pool_inherits_per_frame_audio_and_geometry() {
        let frames = [
            rgb_image(&[[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]),
            rgb_image(&[[0.7, 0.8, 0.9], [0.0, 0.1, 0.2]]),
        ];
        let audio = vec![vec![0.25_f32], vec![0.75_f32]];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 1).expect("pool");

        assert_eq!(pool.columns, 2);
        assert_eq!(pool.rows, 1);
        assert_eq!(pool.audio_dims, 1);
        // two tiles per frame across two frames, indexed frame-major.
        assert_eq!(pool.grains.len(), 4);
        assert_eq!(pool.grains[0].frame_index, 0);
        assert_eq!(pool.grains[0].global_index, 0);
        assert_eq!(pool.grains[0].audio, vec![0.25]);
        assert_eq!(pool.grains[3].frame_index, 1);
        assert_eq!(pool.grains[3].global_index, 3);
        assert_eq!(pool.grains[3].audio, vec![0.75]);
    }

    #[test]
    fn pool_audio_breaks_a_color_tie() {
        // Two single-grain frames with identical color but different audio. The
        // query color matches both equally, so the audio dimension is the only
        // thing that can decide the selection — proving audio enters ranking.
        let frames = [rgb_image(&[[0.5, 0.5, 0.5]]), rgb_image(&[[0.5, 0.5, 0.5]])];
        let audio = vec![vec![0.0_f32], vec![1.0_f32]];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 1).expect("pool");
        let modulator = rgb_image(&[[0.5, 0.5, 0.5]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        // Query audio = frame 1's descriptor; weighted audio picks grain 1.
        let weighted = select_grains_from_pool_cpu(&modulator, 1, 1, &[1.0], &pool, settings, 1.0)
            .expect("weighted selection");
        assert_eq!(weighted.indices, vec![1]);

        // audio_weight 0 ignores audio; the color tie breaks to ascending index.
        let unweighted = select_grains_from_pool_cpu(&modulator, 1, 1, &[1.0], &pool, settings, 0.0)
            .expect("unweighted selection");
        assert_eq!(unweighted.indices, vec![0]);
    }

    #[test]
    fn pool_render_rearrangement_blends_across_frames() {
        // A whole-frame grain (grain_size == frame size) from frame 1 is selected
        // while the current carrier is frame 0. rearrangement = 0 must yield the
        // carrier exactly; rearrangement = 1 must yield the selected grain's frame.
        let frames = [
            ImageBufferF32::new(2, 2, vec![[0.2, 0.2, 0.2, 1.0]; 4]).expect("frame a"),
            ImageBufferF32::new(2, 2, vec![[0.8, 0.1, 0.4, 1.0]; 4]).expect("frame b"),
        ];
        let audio = vec![vec![0.0_f32], vec![1.0_f32]];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 2).expect("pool");
        let selection = GrainSelection {
            columns: 1,
            rows: 1,
            indices: vec![1],
        };
        let preserve = GranularMosaicSettings {
            grain_size: 2,
            rearrangement: 0.0,
            variation: 0.0,
            seed: 0,
        };

        let preserved =
            granular_mosaic_with_pool_selection_cpu(&frames, &pool, &frames[0], &selection, preserve)
                .expect("preserved");
        assert_eq!(preserved, frames[0]);

        let take_grain = GranularMosaicSettings {
            rearrangement: 1.0,
            ..preserve
        };
        let grain =
            granular_mosaic_with_pool_selection_cpu(&frames, &pool, &frames[0], &selection, take_grain)
                .expect("grain");
        assert_eq!(grain, frames[1]);
    }

    #[test]
    fn pool_selection_is_deterministic() {
        let frames = [
            rgb_image(&[[0.1, 0.9, 0.2], [0.8, 0.2, 0.1]]),
            rgb_image(&[[0.2, 0.2, 0.7], [0.5, 0.5, 0.5]]),
        ];
        let audio = vec![vec![0.3_f32, 0.6], vec![0.9, 0.1]];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 1).expect("pool");
        let modulator = rgb_image(&[[0.0, 0.5, 0.0], [0.9, 0.1, 0.2]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.5,
            seed: 17,
        };

        let first = select_grains_from_pool_cpu(&modulator, 2, 1, &[0.4, 0.4], &pool, settings, 0.7)
            .expect("first selection");
        let second = select_grains_from_pool_cpu(&modulator, 2, 1, &[0.4, 0.4], &pool, settings, 0.7)
            .expect("second selection");

        assert_eq!(first, second);
    }

    #[test]
    fn pool_rejects_mismatched_audio_lengths() {
        let frames = [rgb_image(&[[0.1, 0.2, 0.3]]), rgb_image(&[[0.4, 0.5, 0.6]])];
        let audio = vec![vec![0.1_f32], vec![0.2, 0.3]];
        assert!(analyze_grain_pool_cpu(&frames, &audio, 1).is_err());
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
