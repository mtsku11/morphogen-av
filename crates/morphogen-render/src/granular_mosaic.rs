use serde::{Deserialize, Serialize};

use crate::{sample_bilinear_clamped, ImageBufferF32, RenderError};

pub const GRANULAR_MOSAIC_ALGORITHM: &str = "luma_nearest_grain_cpu_v1";

/// Algorithm identifier for the multimodal RGB nearest-neighbor selection path
/// (step 6). Distinct from [`GRANULAR_MOSAIC_ALGORITHM`] so a stale luma sidecar
/// never satisfies a multimodal request.
pub const MULTIMODAL_GRAIN_ALGORITHM: &str = "multimodal_nearest_grain_cpu_v1";

/// Algorithm identifier for the temporal-grain-pool joint-AV selection path
/// (step 6b). Grains are drawn from across time and matched on a combined
/// `[mean_color | texture | audio]` feature vector. Distinct id invalidates stale
/// single-frame sidecars.
///
/// Bumped to `v2` when the per-grain texture descriptor (luma-variance + gradient
/// magnitude) was added: the pool sidecar schema changed, so a `v1` sidecar genuinely
/// lacks the texture dims and must be regenerated rather than silently read as zero.
pub const POOLED_GRAIN_ALGORITHM: &str = "pooled_av_nearest_grain_cpu_v2";

/// Algorithm identifier for the AV-granular OLA resynthesis audio output.
/// Distinct from `POOLED_GRAIN_ALGORITHM` so a stale audio artifact (e.g. from a run
/// without a carrier WAV) never satisfies a run that wants audio. Video frames are
/// byte-identical to a run without `--carrier-wav`; only the audio sidecar is new.
pub const POOLED_AV_AUDIO_ALGORITHM: &str = "pooled_av_ola_resynthesis_cpu_v1";

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
/// origin, mean color, a 2-dim spatial texture descriptor (`[luma_variance,
/// gradient_magnitude]`), and the carrier-audio descriptor vector sampled at that
/// frame's source time (shared by every grain of the same frame). The combined
/// `[mean_color | texture | audio]` vector is the joint matching feature.
///
/// `texture` lets selection discriminate grains of equal mean color by their
/// *spatial busyness* — luma variance and mean gradient magnitude over the tile —
/// so a smooth Source A region draws smooth carrier grains and a busy region draws
/// busy ones. It is always computed; a `texture_weight` of zero leaves it inert.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PooledGrainDescriptor {
    pub global_index: u32,
    pub frame_index: u32,
    pub origin_x: u32,
    pub origin_y: u32,
    pub mean_color: [f32; 3],
    pub texture: [f32; 2],
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
                    texture: tile_texture(frame, origin_x, origin_y, grain_size),
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

/// Restricts which pool frames a selection may draw grains from.
///
/// `WholeClip` (the default behavior) considers every grain in the pool.
/// `Trailing` bounds selection to a causal window: only grains from the `frames`
/// carrier frames up to and including `current_frame` are eligible, clamping to a
/// shrinking window at the clip start. The window is selection-only — the pool
/// sidecar stays whole-clip and reusable, and the Metal render path is unaffected
/// because it renders whatever index map the selection produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolSelectionWindow {
    WholeClip,
    Trailing { current_frame: u32, frames: u32 },
}

/// Cross-frame anti-repeat scheduling state (step 6b). Pushes temporal diversity
/// by penalizing grains that were selected in recent output frames, so the mosaic
/// keeps finding fresh material over time instead of locking onto the same grains.
///
/// `last_used_frame[global_index]` is the most recent output frame at which that
/// grain was selected (`None` ⇒ never). A grain used `age = current_frame - used`
/// frames ago incurs a penalty `weight * (cooldown - age) / cooldown` added to its
/// squared feature distance while `age < cooldown`, decaying linearly to zero.
/// Frame zero has an all-`None` history, so the first frame is identical to the
/// non-scheduled selection. The state is a plain `Vec<Option<u32>>`, so it is the
/// serializable checkpoint representation for this stateful temporal node.
#[derive(Debug, Clone, Copy)]
pub struct AntiRepeat<'a> {
    pub last_used_frame: &'a [Option<u32>],
    pub current_frame: u32,
    pub cooldown: u32,
    pub weight: f32,
}

impl AntiRepeat<'_> {
    fn penalty(&self, global_index: u32) -> f32 {
        if self.cooldown == 0 || self.weight <= 0.0 {
            return 0.0;
        }
        let Some(Some(used)) = self.last_used_frame.get(global_index as usize).copied() else {
            return 0.0;
        };
        if self.current_frame <= used {
            return 0.0;
        }
        let age = self.current_frame - used;
        if age >= self.cooldown {
            return 0.0;
        }
        self.weight * (self.cooldown - age) as f32 / self.cooldown as f32
    }
}

/// Cross-frame temporal-coherence scheduling state (step 6b) — the smooth-motion
/// complement to [`AntiRepeat`]. Where anti-repeat penalizes recently-used grains
/// to push diversity, coherence rewards a tile picking a grain whose *source
/// frame* is close to that same tile's previous selection, so each tile's source
/// frame drifts smoothly through the pool instead of jumping across the clip (the
/// dominant flicker source in a per-tile nearest-neighbor mosaic).
///
/// `prev_selection[tile_index]` is the global grain index the tile (row-major)
/// selected on the previous output frame (`None` ⇒ no previous selection). Two
/// additive continuity penalties share the same `reach` normalization:
///
/// - **Frame continuity** (`weight`): a candidate whose source frame differs from
///   the previous pick's frame by `delta` adds `weight * min(delta, reach) / reach`
///   — zero when the source frame is unchanged, saturating at `weight` once
///   `delta >= reach`.
/// - **Spatial-origin continuity** (`spatial_weight`): a candidate whose grain
///   origin differs from the previous pick's origin adds
///   `spatial_weight * min(dist_tiles, reach) / reach`, where `dist_tiles` is the
///   Euclidean distance between the two origins measured in grain-tile units
///   (`origin / grain_size`). Zero when the grain origin is unchanged, saturating
///   at `spatial_weight` once the origins are `reach` tiles apart. This keeps a
///   tile's pick from teleporting across the *frame* even when it stays on a
///   nearby source frame.
///
/// Both default off at weight `0`. Frame zero has an all-`None` history, so the
/// first frame is identical to the non-scheduled selection (declared frame-zero
/// behavior). The state is a plain `Vec<Option<u32>>` (one entry per output
/// tile), the serializable checkpoint representation for this stateful temporal
/// node. The penalty reshapes only the nearest-match distance, not the seeded
/// alternate; the Metal render path is unaffected (selection is CPU-side).
#[derive(Debug, Clone, Copy)]
pub struct TemporalCoherence<'a> {
    pub prev_selection: &'a [Option<u32>],
    pub reach: u32,
    pub weight: f32,
    pub spatial_weight: f32,
}

impl TemporalCoherence<'_> {
    /// Penalty for a candidate grain given the tile's previous pick (`None` ⇒ no
    /// previous selection ⇒ no penalty). `prev` carries the previous pick's source
    /// `(frame, origin_x, origin_y)`; `grain_size` converts pixel origins to
    /// grain-tile units for the spatial term.
    fn penalty(
        &self,
        candidate_frame: u32,
        candidate_origin: (u32, u32),
        prev: Option<(u32, u32, u32)>,
        grain_size: u32,
    ) -> f32 {
        if self.reach == 0 {
            return 0.0;
        }
        let Some((prev_frame, prev_origin_x, prev_origin_y)) = prev else {
            return 0.0;
        };
        let reach = self.reach as f32;
        let mut penalty = 0.0;
        if self.weight > 0.0 {
            let delta = candidate_frame.abs_diff(prev_frame).min(self.reach);
            penalty += self.weight * delta as f32 / reach;
        }
        if self.spatial_weight > 0.0 && grain_size > 0 {
            let dx = (candidate_origin.0 as f32 - prev_origin_x as f32) / grain_size as f32;
            let dy = (candidate_origin.1 as f32 - prev_origin_y as f32) / grain_size as f32;
            let dist_tiles = (dx * dx + dy * dy).sqrt().min(reach);
            penalty += self.spatial_weight * dist_tiles / reach;
        }
        penalty
    }
}

/// Select pool grains for each output tile by nearest combined
/// `[color | texture | audio]` descriptor (step 6b). The query is Source A's
/// per-tile mean color, that same tile's 2-dim texture descriptor (luma variance +
/// gradient magnitude), and this output frame's audio descriptor vector;
/// `texture_weight` scales both texture dims and `audio_weight` every audio
/// dimension. Selected indices are global pool indices. `variation` blends the
/// nearest match with a seeded alternate pool grain, leaving the match exact at
/// `variation = 0`. `window` bounds which pool frames are eligible; `anti_repeat`
/// penalizes recently-used grains, and `coherence` rewards per-tile source-frame
/// continuity, in the nearest-match search (the seeded alternate is left
/// untouched).
#[allow(clippy::too_many_arguments)]
pub fn select_grains_from_pool_cpu(
    modulator: &ImageBufferF32,
    carrier_width: u32,
    carrier_height: u32,
    query_audio: &[f32],
    pool: &GrainPool,
    settings: GranularMosaicSettings,
    audio_weight: f32,
    texture_weight: f32,
    window: PoolSelectionWindow,
    anti_repeat: Option<AntiRepeat<'_>>,
    coherence: Option<TemporalCoherence<'_>>,
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
    if !texture_weight.is_finite() || texture_weight < 0.0 {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "texture_weight must be a finite, non-negative value".to_string(),
        ));
    }
    if !query_audio.iter().all(|value| value.is_finite()) {
        return Err(RenderError::InvalidGranularMosaicSettings(
            "query audio descriptors must be finite".to_string(),
        ));
    }

    let columns = div_ceil(carrier_width, settings.grain_size);
    let rows = div_ceil(carrier_height, settings.grain_size);
    let (eligible_start, eligible_end) = eligible_grain_range(pool, window);
    let eligible = &pool.grains[eligible_start..eligible_end];
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
            let query_texture = average_modulator_tile_texture(
                modulator,
                carrier_width,
                carrier_height,
                tile_x,
                tile_y,
                settings.grain_size,
            );
            // Resolve this tile's previous pick for temporal coherence: map the
            // tile's prior global selection back to its pool frame and origin.
            let coherence_prev = coherence.as_ref().and_then(|coherence| {
                let tile_index = (tile_y * columns + tile_x) as usize;
                coherence
                    .prev_selection
                    .get(tile_index)
                    .copied()
                    .flatten()
                    .and_then(|global| pool.grains.get(global as usize))
                    .map(|grain| (grain.frame_index, grain.origin_x, grain.origin_y))
            });
            indices.push(select_pool_grain_index(
                target,
                query_texture,
                query_audio,
                audio_weight,
                texture_weight,
                eligible,
                eligible_start,
                anti_repeat.as_ref(),
                coherence.as_ref(),
                coherence_prev,
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
                .total_cmp(&right_distance)
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

pub(crate) fn average_carrier_tile_color(
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

/// Spatial texture descriptor of a carrier tile: `[luma_variance,
/// gradient_magnitude]`. Variance is the population variance of per-pixel
/// luminance over the tile; gradient magnitude is the mean of
/// `sqrt(dx² + dy²)` of forward luma differences within the tile (the tile's
/// right and bottom edges contribute a zero one-sided difference). Both grow with
/// spatial busyness and are zero for a flat tile, so they discriminate grains of
/// equal mean color by structure.
pub(crate) fn tile_texture(
    image: &ImageBufferF32,
    start_x: u32,
    start_y: u32,
    grain_size: u32,
) -> [f32; 2] {
    let end_x = (start_x + grain_size).min(image.width);
    let end_y = (start_y + grain_size).min(image.height);
    let width = (end_x - start_x) as usize;
    let height = (end_y - start_y) as usize;
    let mut luma = Vec::with_capacity(width * height);
    for y in start_y..end_y {
        for x in start_x..end_x {
            luma.push(luminance(image.pixel(x, y).unwrap_or([0.0, 0.0, 0.0, 0.0])));
        }
    }
    texture_from_luma(&luma, width, height)
}

/// Texture descriptor of a Source A tile, sampled in output-normalized space with
/// the clamped bilinear border (mirroring [`average_modulator_tile_color`]) so the
/// query texture is comparable to the carrier-tile texture regardless of modulator
/// resolution.
fn average_modulator_tile_texture(
    modulator: &ImageBufferF32,
    output_width: u32,
    output_height: u32,
    tile_x: u32,
    tile_y: u32,
    grain_size: u32,
) -> [f32; 2] {
    let start_x = tile_x * grain_size;
    let start_y = tile_y * grain_size;
    let end_x = (start_x + grain_size).min(output_width);
    let end_y = (start_y + grain_size).min(output_height);
    let width = (end_x - start_x) as usize;
    let height = (end_y - start_y) as usize;
    let mut luma = Vec::with_capacity(width * height);
    for y in start_y..end_y {
        for x in start_x..end_x {
            let pixel = sample_bilinear_clamped(
                modulator,
                remap_coordinate(x, output_width, modulator.width),
                remap_coordinate(y, output_height, modulator.height),
            );
            luma.push(luminance(pixel));
        }
    }
    texture_from_luma(&luma, width, height)
}

/// Shared `[variance, gradient_magnitude]` computation over a row-major luma tile.
fn texture_from_luma(luma: &[f32], width: usize, height: usize) -> [f32; 2] {
    if luma.is_empty() {
        return [0.0, 0.0];
    }
    let count = luma.len() as f32;
    let mut sum = 0.0_f32;
    let mut sum_sq = 0.0_f32;
    for &value in luma {
        sum += value;
        sum_sq += value * value;
    }
    let mean = sum / count;
    let variance = (sum_sq / count - mean * mean).max(0.0);

    let mut gradient_sum = 0.0_f32;
    for y in 0..height {
        for x in 0..width {
            let here = luma[y * width + x];
            let dx = if x + 1 < width {
                luma[y * width + x + 1] - here
            } else {
                0.0
            };
            let dy = if y + 1 < height {
                luma[(y + 1) * width + x] - here
            } else {
                0.0
            };
            gradient_sum += (dx * dx + dy * dy).sqrt();
        }
    }
    [variance, gradient_sum / count]
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
                .total_cmp(&right_distance)
                .then_with(|| left.index.cmp(&right.index))
        })
        .map(|descriptor| descriptor.index)
        .unwrap_or(0);
    let random_selected = tile_hash(seed, tile_x, tile_y) % descriptors.len() as u64;
    lerp(feature_selected as f32, random_selected as f32, variation).round() as u32
}

/// The contiguous global-index range `[start, end)` of grains eligible under
/// `window`. Because grains are stored frame-major, a trailing frame window maps
/// to a single contiguous slice of the pool.
fn eligible_grain_range(pool: &GrainPool, window: PoolSelectionWindow) -> (usize, usize) {
    let per_frame = pool.columns as usize * pool.rows as usize;
    match window {
        PoolSelectionWindow::WholeClip => (0, pool.grains.len()),
        PoolSelectionWindow::Trailing {
            current_frame,
            frames,
        } => {
            if frames == 0 || per_frame == 0 {
                return (0, pool.grains.len());
            }
            let frame_count = pool.grains.len() / per_frame;
            if frame_count == 0 {
                return (0, pool.grains.len());
            }
            let current = (current_frame as usize).min(frame_count - 1);
            let start_frame = current.saturating_sub(frames as usize - 1);
            (start_frame * per_frame, (current + 1) * per_frame)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn select_pool_grain_index(
    target_color: [f32; 3],
    query_texture: [f32; 2],
    query_audio: &[f32],
    audio_weight: f32,
    texture_weight: f32,
    eligible: &[PooledGrainDescriptor],
    eligible_start: usize,
    anti_repeat: Option<&AntiRepeat<'_>>,
    coherence: Option<&TemporalCoherence<'_>>,
    coherence_prev: Option<(u32, u32, u32)>,
    settings: GranularMosaicSettings,
    tile_x: u32,
    tile_y: u32,
) -> u32 {
    // `eligible` is a contiguous window of the pool; the nearest match and the
    // seeded alternate both resolve to global indices that stay inside it. The
    // anti-repeat and coherence penalties reshape only the nearest-match distance.
    let distance = |grain: &PooledGrainDescriptor| {
        let mut distance = pooled_distance_sq(
            grain,
            &target_color,
            &query_texture,
            query_audio,
            texture_weight,
            audio_weight,
        );
        if let Some(anti_repeat) = anti_repeat {
            distance += anti_repeat.penalty(grain.global_index);
        }
        if let Some(coherence) = coherence {
            distance += coherence.penalty(
                grain.frame_index,
                (grain.origin_x, grain.origin_y),
                coherence_prev,
                settings.grain_size,
            );
        }
        distance
    };
    let feature_selected = eligible
        .iter()
        .min_by(|left, right| {
            distance(left)
                .total_cmp(&distance(right))
                .then_with(|| left.global_index.cmp(&right.global_index))
        })
        .map(|grain| grain.global_index)
        .unwrap_or(0);
    let random_selected =
        eligible_start as u64 + tile_hash(settings.seed, tile_x, tile_y) % eligible.len() as u64;
    lerp(
        feature_selected as f32,
        random_selected as f32,
        settings.variation,
    )
    .round() as u32
}

/// Combined squared distance over the `[color(3) | texture(2) | audio(k)]` feature
/// vector: equal per-channel color weights, a single scalar `texture_weight` on
/// both texture dims, and a single scalar `audio_weight` on every audio dimension.
/// Alloc-free so it stays cheap inside the per-tile nearest search over the whole
/// pool.
fn pooled_distance_sq(
    grain: &PooledGrainDescriptor,
    target_color: &[f32; 3],
    query_texture: &[f32; 2],
    query_audio: &[f32],
    texture_weight: f32,
    audio_weight: f32,
) -> f32 {
    let mut sum = 0.0_f32;
    for (grain_channel, target_channel) in grain.mean_color.iter().zip(target_color.iter()) {
        let delta = grain_channel - target_channel;
        sum += delta * delta;
    }
    for (grain_value, query_value) in grain.texture.iter().zip(query_texture.iter()) {
        let delta = grain_value - query_value;
        sum += texture_weight * delta * delta;
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
            || !grain.texture.iter().all(|value| value.is_finite())
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

pub(crate) fn luminance(pixel: [f32; 4]) -> f32 {
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

/// OLA (overlap-add) audio resynthesis for the granular pool renderer.
///
/// For each output video frame `i`, every selected grain (one per spatial tile)
/// contributes its source audio window at carrier position `grain.frame_index ×
/// hop_size`. All tile windows land at the same output time (`i × hop_size`), so
/// they are mixed (summed then divided by the contributing tile count). A
/// rectangular window is used — no Hann — because there is no inter-frame temporal
/// overlap to smooth; adjacent output frames do not share samples.
///
/// The video frames are NOT touched here: this function only produces audio, so the
/// video output remains byte-identical to a run without a carrier WAV.
///
/// # Arguments
/// - `carrier_samples` — interleaved PCM (f32), `channels × audio-frames` layout.
/// - `carrier_channels` — number of interleaved channels (1 or 2 typical).
/// - `carrier_total_frames` — total audio frames in `carrier_samples`.
/// - `frame_selections` — one `GrainSelection` per output video frame.
/// - `pool` — the grain pool (used for `frame_index` lookups).
/// - `hop_size` — audio frames per video frame (`round(sample_rate / frame_rate)`).
///
/// # Returns
/// Interleaved f32 PCM with `frame_count × hop_size × channels` samples.
pub fn ola_resynthesis_cpu(
    carrier_samples: &[f32],
    carrier_channels: usize,
    carrier_total_frames: usize,
    frame_selections: &[GrainSelection],
    pool: &GrainPool,
    hop_size: usize,
) -> Vec<f32> {
    if hop_size == 0 || carrier_channels == 0 || frame_selections.is_empty() {
        return Vec::new();
    }
    let output_len = frame_selections.len() * hop_size * carrier_channels;
    let mut acc = vec![0.0_f32; output_len];

    for (out_frame, selection) in frame_selections.iter().enumerate() {
        let out_offset = out_frame * hop_size;
        let mut tile_count: u32 = 0;
        for &global_idx in &selection.indices {
            let grain = match pool.grains.get(global_idx as usize) {
                Some(g) => g,
                None => continue,
            };
            let src_start = grain.frame_index as usize * hop_size;
            for k in 0..hop_size {
                let src_frame = src_start + k;
                if src_frame >= carrier_total_frames {
                    break;
                }
                for ch in 0..carrier_channels {
                    acc[(out_offset + k) * carrier_channels + ch] +=
                        carrier_samples[src_frame * carrier_channels + ch];
                }
            }
            tile_count += 1;
        }
        // Normalise this frame's contribution by the number of contributing tiles.
        if tile_count > 1 {
            let scale = 1.0 / tile_count as f32;
            let start = out_offset * carrier_channels;
            let end = (out_offset + hop_size) * carrier_channels;
            for s in acc[start..end].iter_mut() {
                *s *= scale;
            }
        }
    }
    acc
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
        let weighted = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[1.0],
            &pool,
            settings,
            1.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("weighted selection");
        assert_eq!(weighted.indices, vec![1]);

        // audio_weight 0 ignores audio; the color tie breaks to ascending index.
        let unweighted = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[1.0],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("unweighted selection");
        assert_eq!(unweighted.indices, vec![0]);
    }

    #[test]
    fn pool_second_audio_dim_changes_selection() {
        // Same frames and color (a color tie), but adding a second audio dim
        // (e.g. spectral centroid alongside RMS) flips which grain wins — proving
        // k > 1 audio dims discriminate, not just k = 1.
        let frames = [rgb_image(&[[0.5, 0.5, 0.5]]), rgb_image(&[[0.5, 0.5, 0.5]])];
        let modulator = rgb_image(&[[0.5, 0.5, 0.5]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        // k = 1 (RMS only): grain 0's RMS (0.5) is nearest the 0.52 query.
        let rms_only = vec![vec![0.5_f32], vec![0.6_f32]];
        let pool_k1 = analyze_grain_pool_cpu(&frames, &rms_only, 1).expect("k1 pool");
        let pick_k1 = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[0.52],
            &pool_k1,
            settings,
            1.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("k1 selection");
        assert_eq!(pick_k1.indices, vec![0]);

        // k = 2 ([RMS, centroid]): grain 1's centroid (0.9) matches the query,
        // overturning the RMS-only winner.
        let rms_centroid = vec![vec![0.5_f32, 0.0], vec![0.6, 0.9]];
        let pool_k2 = analyze_grain_pool_cpu(&frames, &rms_centroid, 1).expect("k2 pool");
        assert_eq!(pool_k2.audio_dims, 2);
        let pick_k2 = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[0.52, 0.9],
            &pool_k2,
            settings,
            1.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("k2 selection");
        assert_eq!(pick_k2.indices, vec![1]);
    }

    #[test]
    fn pool_texture_breaks_a_color_tie() {
        // Two 2x2 single-grain frames with identical mean color (0.5 gray) but
        // different spatial structure: frame 0 is flat (zero variance/gradient),
        // frame 1 is a checkerboard (high variance/gradient). The query color
        // matches both equally, so only the texture dims can decide the
        // selection — proving texture enters ranking. The modulator carries frame
        // 1's checkerboard, so its texture query matches frame 1.
        let flat = ImageBufferF32::new(2, 2, vec![[0.5, 0.5, 0.5, 1.0]; 4]).expect("flat frame");
        let checker = ImageBufferF32::new(
            2,
            2,
            vec![
                [0.0, 0.0, 0.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0, 1.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        )
        .expect("checker frame");
        let modulator = checker.clone();
        let frames = [flat, checker];
        let audio = vec![Vec::new(); 2];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 2).expect("pool");
        // Mean color ties: both grains are 0.5 gray.
        assert_eq!(pool.grains[0].mean_color, [0.5, 0.5, 0.5]);
        assert_eq!(pool.grains[1].mean_color, [0.5, 0.5, 0.5]);
        // Frame 0 is flat (zero texture); frame 1 is busy (nonzero on both dims).
        assert_eq!(pool.grains[0].texture, [0.0, 0.0]);
        assert!(pool.grains[1].texture[0] > 0.0 && pool.grains[1].texture[1] > 0.0);
        let settings = GranularMosaicSettings {
            grain_size: 2,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        // texture_weight 0 ignores texture; the color tie breaks to ascending index.
        let untextured = select_grains_from_pool_cpu(
            &modulator,
            2,
            2,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("untextured selection");
        assert_eq!(untextured.indices, vec![0]);

        // texture_weight 1 makes the busy modulator query select the busy grain 1.
        let textured = select_grains_from_pool_cpu(
            &modulator,
            2,
            2,
            &[],
            &pool,
            settings,
            0.0,
            1.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("textured selection");
        assert_eq!(textured.indices, vec![1]);
    }

    #[test]
    fn pool_trailing_window_restricts_eligible_frames() {
        // Four single-grain frames with distinct colors. A trailing window of 1
        // frame must force every output frame to select its own frame's grain
        // (global index == current frame), regardless of color match. WholeClip on
        // the same query is free to pick the best color match across all frames.
        let frames = [
            rgb_image(&[[1.0, 0.0, 0.0]]),
            rgb_image(&[[0.0, 1.0, 0.0]]),
            rgb_image(&[[0.0, 0.0, 1.0]]),
            rgb_image(&[[1.0, 1.0, 1.0]]),
        ];
        let audio = vec![Vec::new(); 4];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 1).expect("pool");
        // Query color is pure red — color-nearest is always frame 0.
        let modulator = rgb_image(&[[1.0, 0.0, 0.0]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        for current in 0..4u32 {
            let trailing = select_grains_from_pool_cpu(
                &modulator,
                1,
                1,
                &[],
                &pool,
                settings,
                0.0,
                0.0,
                PoolSelectionWindow::Trailing {
                    current_frame: current,
                    frames: 1,
                },
                None,
                None,
            )
            .expect("trailing selection");
            // Only the current frame's grain is eligible.
            assert_eq!(trailing.indices, vec![current]);
        }

        // Whole-clip picks the red frame (index 0) for every current frame.
        let whole = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("whole selection");
        assert_eq!(whole.indices, vec![0]);
    }

    #[test]
    fn pool_anti_repeat_penalizes_recently_used_grain() {
        // Two near-red frames; the query is pure red, so color always favors
        // frame 0 (index 0). Anti-repeat must overturn that once frame 0's grain
        // has been used recently, steering selection to frame 1.
        let frames = [rgb_image(&[[1.0, 0.0, 0.0]]), rgb_image(&[[0.9, 0.0, 0.0]])];
        let audio = vec![Vec::new(); 2];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 1).expect("pool");
        let modulator = rgb_image(&[[1.0, 0.0, 0.0]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        // Frame zero: empty history ⇒ identical to non-scheduled (picks grain 0).
        let history = vec![None, None];
        let frame0 = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            Some(AntiRepeat {
                last_used_frame: &history,
                current_frame: 0,
                cooldown: 4,
                weight: 1.0,
            }),
            None,
        )
        .expect("frame 0");
        assert_eq!(frame0.indices, vec![0]);

        // Frame one with grain 0 used at frame 0: the penalty steers to grain 1...
        let history = vec![Some(0), None];
        let penalized = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            Some(AntiRepeat {
                last_used_frame: &history,
                current_frame: 1,
                cooldown: 4,
                weight: 1.0,
            }),
            None,
        )
        .expect("frame 1 penalized");
        assert_eq!(penalized.indices, vec![1]);

        // ...while the same frame without the scheduler still picks the red grain.
        let unpenalized = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("frame 1 unpenalized");
        assert_eq!(unpenalized.indices, vec![0]);
    }

    #[test]
    fn pool_coherence_rewards_previous_source_frame() {
        // Three near-red frames; the query is pure red, so color always favors
        // frame 0 (index 0). Temporal coherence must overturn that once the tile's
        // previous pick was frame 2, steering selection back toward frame 2.
        let frames = [
            rgb_image(&[[1.0, 0.0, 0.0]]),
            rgb_image(&[[0.9, 0.0, 0.0]]),
            rgb_image(&[[0.8, 0.0, 0.0]]),
        ];
        let audio = vec![Vec::new(); 3];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 1).expect("pool");
        let modulator = rgb_image(&[[1.0, 0.0, 0.0]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        // Frame zero: no previous selection ⇒ identical to non-scheduled (grain 0).
        let history = vec![None];
        let frame0 = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            Some(TemporalCoherence {
                prev_selection: &history,
                reach: 4,
                weight: 1.0,
                spatial_weight: 0.0,
            }),
        )
        .expect("frame 0");
        assert_eq!(frame0.indices, vec![0]);

        // Tile's previous pick was frame 2's grain: coherence pulls selection back
        // to frame 2 (color 0.04 + penalty 0 beats color 0 + penalty 0.5)...
        let history = vec![Some(2)];
        let coherent = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            Some(TemporalCoherence {
                prev_selection: &history,
                reach: 4,
                weight: 1.0,
                spatial_weight: 0.0,
            }),
        )
        .expect("frame 1 coherent");
        assert_eq!(coherent.indices, vec![2]);

        // ...while the same frame without the scheduler still picks the red grain.
        let unscheduled = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("frame 1 unscheduled");
        assert_eq!(unscheduled.indices, vec![0]);
    }

    #[test]
    fn pool_spatial_coherence_rewards_previous_origin() {
        // One frame, two grains side by side: grain 0 (origin x=0) is an exact
        // color match for the red query; grain 1 (origin x=1) is a near miss. With
        // no scheduler color picks grain 0. Spatial-origin coherence (frame weight
        // 0) must overturn that once the tile's previous pick was grain 1's origin:
        // grain 0 then carries a spatial penalty for moving one tile, grain 1 none.
        let frames = [rgb_image(&[[1.0, 0.0, 0.0], [0.9, 0.0, 0.0]])];
        let audio = vec![Vec::new(); 1];
        let pool = analyze_grain_pool_cpu(&frames, &audio, 1).expect("pool");
        assert_eq!(pool.grains[0].origin_x, 0);
        assert_eq!(pool.grains[1].origin_x, 1);
        let modulator = rgb_image(&[[1.0, 0.0, 0.0]]);
        let settings = GranularMosaicSettings {
            grain_size: 1,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 0,
        };

        // Frame zero: no previous selection ⇒ identical to non-scheduled (grain 0).
        let history = vec![None];
        let frame0 = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            Some(TemporalCoherence {
                prev_selection: &history,
                reach: 4,
                weight: 0.0,
                spatial_weight: 1.0,
            }),
        )
        .expect("frame 0");
        assert_eq!(frame0.indices, vec![0]);

        // Previous pick was grain 1 (origin x=1): the spatial penalty on grain 0
        // (1 tile away ⇒ 1.0*1/4 = 0.25) beats its color win (0), while grain 1
        // pays no spatial penalty against its small color miss (0.01).
        let history = vec![Some(1)];
        let coherent = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            Some(TemporalCoherence {
                prev_selection: &history,
                reach: 4,
                weight: 0.0,
                spatial_weight: 1.0,
            }),
        )
        .expect("frame 1 spatially coherent");
        assert_eq!(coherent.indices, vec![1]);

        // ...while the same frame without the scheduler still picks the exact-color
        // grain 0, confirming spatial coherence alone caused the flip.
        let unscheduled = select_grains_from_pool_cpu(
            &modulator,
            1,
            1,
            &[],
            &pool,
            settings,
            0.0,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("frame 1 unscheduled");
        assert_eq!(unscheduled.indices, vec![0]);
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

        let preserved = granular_mosaic_with_pool_selection_cpu(
            &frames, &pool, &frames[0], &selection, preserve,
        )
        .expect("preserved");
        assert_eq!(preserved, frames[0]);

        let take_grain = GranularMosaicSettings {
            rearrangement: 1.0,
            ..preserve
        };
        let grain = granular_mosaic_with_pool_selection_cpu(
            &frames, &pool, &frames[0], &selection, take_grain,
        )
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

        let first = select_grains_from_pool_cpu(
            &modulator,
            2,
            1,
            &[0.4, 0.4],
            &pool,
            settings,
            0.7,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
        .expect("first selection");
        let second = select_grains_from_pool_cpu(
            &modulator,
            2,
            1,
            &[0.4, 0.4],
            &pool,
            settings,
            0.7,
            0.0,
            PoolSelectionWindow::WholeClip,
            None,
            None,
        )
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

    #[test]
    fn ola_resynthesis_empty_inputs_return_empty() {
        let pool = GrainPool {
            columns: 0,
            rows: 0,
            grain_size: 1,
            frame_width: 0,
            frame_height: 0,
            audio_dims: 0,
            grains: vec![],
        };
        let out = ola_resynthesis_cpu(&[], 1, 0, &[], &pool, 48);
        assert!(out.is_empty());
    }

    #[test]
    fn ola_resynthesis_single_grain_reproduces_dc_signal() {
        let hop = 4_usize;
        let frame_count = 3_usize;
        // Carrier: DC signal at 0.5 for all samples (2 carrier frames × hop samples × 1 channel)
        let carrier_samples: Vec<f32> = vec![0.5; 2 * hop];
        let pool = GrainPool {
            columns: 1,
            rows: 1,
            grain_size: 1,
            frame_width: 1,
            frame_height: 1,
            audio_dims: 0,
            grains: vec![
                PooledGrainDescriptor {
                    global_index: 0,
                    frame_index: 0,
                    origin_x: 0,
                    origin_y: 0,
                    mean_color: [0.0; 3],
                    texture: [0.0; 2],
                    audio: vec![],
                },
                PooledGrainDescriptor {
                    global_index: 1,
                    frame_index: 1,
                    origin_x: 0,
                    origin_y: 0,
                    mean_color: [0.0; 3],
                    texture: [0.0; 2],
                    audio: vec![],
                },
            ],
        };
        // Selections: frame 0 picks grain 0, frame 1 picks grain 1, frame 2 picks grain 0.
        let frame_selections: Vec<GrainSelection> = (0..frame_count)
            .map(|i| GrainSelection {
                columns: 1,
                rows: 1,
                indices: vec![if i == 1 { 1 } else { 0 }],
            })
            .collect();
        let out = ola_resynthesis_cpu(&carrier_samples, 1, 2 * hop, &frame_selections, &pool, hop);
        assert_eq!(out.len(), frame_count * hop);
        // All source samples are 0.5, so after OLA + normalisation output should be ~0.5
        for &s in &out {
            assert!((s - 0.5).abs() < 1e-5, "expected ~0.5 got {s}");
        }
    }

    #[test]
    fn ola_resynthesis_is_deterministic() {
        let hop = 8_usize;
        let carrier_samples: Vec<f32> = (0..hop * 4).map(|i| i as f32 / 100.0).collect();
        let pool = GrainPool {
            columns: 1,
            rows: 1,
            grain_size: 1,
            frame_width: 1,
            frame_height: 1,
            audio_dims: 0,
            grains: (0..4)
                .map(|i| PooledGrainDescriptor {
                    global_index: i,
                    frame_index: i,
                    origin_x: 0,
                    origin_y: 0,
                    mean_color: [0.0; 3],
                    texture: [0.0; 2],
                    audio: vec![],
                })
                .collect(),
        };
        let frame_selections: Vec<GrainSelection> = (0..4)
            .map(|i| GrainSelection {
                columns: 1,
                rows: 1,
                indices: vec![i],
            })
            .collect();
        let a = ola_resynthesis_cpu(&carrier_samples, 1, 4 * hop, &frame_selections, &pool, hop);
        let b = ola_resynthesis_cpu(&carrier_samples, 1, 4 * hop, &frame_selections, &pool, hop);
        assert_eq!(a, b);
    }
}
