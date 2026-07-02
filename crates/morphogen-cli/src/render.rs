use std::{
    fs,
    path::{Path, PathBuf},
};

use morphogen_audio::{
    load_wav_f32, rms_envelope, spectral_centroid_from_magnitudes, AudioAnalysisCache,
    AudioDescriptorFrame, OnsetStrengthCache, StftAnalysisCache,
};
use morphogen_core::{
    AnalysisKind, DatamoshBitstreamPreset, DatamoshPreset, FlowSource, GrainSelectionMode,
    GranularAudioModulation, KernelMode, RenderBackend, RenderJobAnalysisCacheProvenance,
    RenderJobProvenance, RenderJobSourceProvenance, RenderTimingMetadata, SourceRole,
};
use morphogen_render::{
    advance_cascade_trails, advance_coagulation_field, advance_dispersion_field,
    advance_field_particles, advance_fluid_mosaic, analyze_convolution_kernel_cpu,
    analyze_convolution_kernels_color_cpu, analyze_grain_colors_cpu, analyze_grain_pool_cpu,
    analyze_grains_cpu, analyze_luma_band_envelope_cpu, apply_history_smear, apply_tone_map_cpu,
    assign_temporal_patches, average_cell_flows, coagulation_field, composite_with_field,
    compute_a_edge_mask, compute_a_flow_mask, compute_a_luma_mask, compute_per_row_shifts,
    convolution_blend_color_cpu, convolution_blend_cpu, datamosh_block_refresh_composite,
    datamosh_codec_engrave_frame_cpu, datamosh_residual_flow, datamosh_scanline_smear_frame_cpu,
    disperse_composite_cpu, downsample_flow_to_cells, feedback_state_path, flow_displace_cpu,
    flow_feedback_frame_cpu, flow_temporal_supersample_cpu, fluid_advect_frame_cpu,
    fluid_advect_two_source_frame_cpu, granular_mosaic_with_pool_selection_cpu,
    granular_mosaic_with_selection_cpu, initialize_cascade_trails, initialize_field_particles,
    initialize_fluid_mosaic, is_datamosh_keyframe, luma_specification_tone_map,
    luminance_gradient_flow_cpu, pyramidal_lucas_kanade_flow_cpu, quantize_flow_to_blocks,
    read_flow_cache, read_flow_feedback_state, read_grain_color_descriptor_cache,
    read_grain_descriptor_cache, read_grain_pool_descriptor_cache, read_grain_selection_cache,
    refresh_field_particle_colors, refresh_fluid_mosaic_colors, remix_block_vectors,
    render_block_collage_frame, render_cascade_collage_frame, render_cascade_trails,
    render_channel_shift_frame, render_field_particles, render_fluid_mosaic,
    render_palette_quantize_frame, render_pixel_sort_frame, render_retro_static_frame,
    reset_residual_in_refreshed_blocks, resort_fluid_mosaic_colors, select_grains_cpu,
    select_grains_from_pool_cpu, select_grains_multimodal_cpu, synthesize_turbulence_flow,
    uniform_displacement_field, video_vocoder_cpu, write_flow_cache,
    write_flow_cache_with_source_fingerprint, write_flow_feedback_state,
    write_grain_color_descriptor_cache, write_grain_descriptor_cache,
    write_grain_pool_descriptor_cache, write_grain_selection_cache, zero_flow, AntiRepeat,
    BlockCollageSettings, CascadeCollageSettings, CascadeTrailSettings, ChannelShiftSettings,
    CoagulationField, CoagulationFlowSource, CoagulationSettings, CodecEngraveSettings,
    ConvolutionBlendSettings, ConvolutionKernel, DispersionField, DispersionSettings,
    FieldParticleSettings, FlowFeedbackSettings, FlowFeedbackStateDescriptor, FlowField,
    FluidAdvectSettings, FluidAdvectTwoSourceSettings, FluidMosaicSettings, GrainColorDescriptor,
    GrainDescriptor, GrainPool, GrainSelection, GranularMosaicSettings, ImageBufferF32, MaskSource,
    PaletteQuantizeSettings, ParticleField, PixelSortSettings, PoolSelectionWindow, QuantizeMode,
    RetroStaticSettings, RmsDisplacementEnvelope, ScanlineSmearSettings, TemporalCoherence,
    VectorRemixMode, VideoVocoderSettings, DATAMOSH_CODEC_ENGRAVE_ALGORITHM,
    DATAMOSH_SCANLINE_SMEAR_ALGORITHM, FLOW_VECTOR_CONVENTION,
    GRAIN_COLOR_DESCRIPTOR_CACHE_FILE_NAME, GRAIN_DESCRIPTOR_CACHE_FILE_NAME,
    GRAIN_POOL_DESCRIPTOR_CACHE_FILE_NAME, GRAIN_SELECTION_CACHE_FILE_NAME,
    GRANULAR_MOSAIC_ALGORITHM, LUCAS_KANADE_WINDOW_RADIUS, MULTIMODAL_GRAIN_ALGORITHM,
    PIXEL_SORT_CROSS_SYNTH_ALGORITHM, POOLED_GRAIN_ALGORITHM,
};
use morphogen_render::{
    apply_channel_shift_modulation, apply_pixel_sort_modulation, apply_retro_static_modulation,
    ModulationSampling,
};
use serde::{Deserialize, Serialize};

use crate::args::*;
use crate::error::CliError;
use crate::imaging::*;
use crate::modulate::{build_modulation_plan, ModulationPlan, ModulationRequest};
/// The shared `--modulate` argument bundle carried by modulatable sequence
/// requests; resolved into a `ModulationPlan` by the handler.
#[derive(Default)]
pub(crate) struct ModulationCliArgs<'a> {
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) sampling: ModulationSampling,
    pub(crate) fps: f64,
}

impl ModulationCliArgs<'_> {
    fn build_plan(&self) -> Result<Option<ModulationPlan>, CliError> {
        build_modulation_plan(ModulationRequest {
            specs: self.modulate,
            modulator_audio: self.modulator_audio,
            modulator_frames: self.modulator_frames,
            sampling: self.sampling,
            fps: self.fps,
        })
    }
}

pub(crate) fn render_test(output_path: &Path) -> Result<(), CliError> {
    let width = 256;
    let height = 256;
    let carrier = synthetic_carrier(width, height)?;
    let flow = synthetic_flow(width, height)?;
    let displaced = flow_displace_cpu(&carrier, &flow, 1.0)?;

    write_parent_dirs(output_path)?;
    save_png(&displaced, output_path)?;
    println!("wrote CPU reference render to {}", output_path.display());
    Ok(())
}

pub(crate) fn metal_render_test(output_path: &Path) -> Result<(), CliError> {
    #[cfg(target_os = "macos")]
    {
        let width = 256;
        let height = 256;
        let carrier = synthetic_carrier(width, height)?;
        let flow = synthetic_flow(width, height)?;
        let displaced = morphogen_metal::flow_displace_metal(&carrier, &flow, 1.0)?;

        write_parent_dirs(output_path)?;
        save_png(&displaced, output_path)?;
        println!(
            "wrote Metal flow-displace render to {}",
            output_path.display()
        );
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = output_path;
        Err(CliError::Message(
            "metal-render-test is only available on macOS.".to_string(),
        ))
    }
}

pub(crate) fn render_two_source(
    modulator_image: &Path,
    carrier_image: &Path,
    output_path: &Path,
    amount: f32,
    flow_cache_dir: Option<&Path>,
) -> Result<(), CliError> {
    if !amount.is_finite() {
        return Err(CliError::Message("amount must be finite".to_string()));
    }

    let modulator = load_image_f32(modulator_image)?;
    let carrier = load_image_f32(carrier_image)?;
    let flow = luminance_gradient_flow_cpu(&modulator, carrier.width, carrier.height)?;
    let displaced = flow_displace_cpu(&carrier, &flow, amount)?;

    if let Some(cache_dir) = flow_cache_dir {
        let manifest = write_flow_cache(cache_dir, &flow, "luminance_gradient_cpu_v1")?;
        println!(
            "wrote luminance flow cache with {} frame(s) to {}",
            manifest.frames.len(),
            cache_dir.display()
        );
    }

    write_parent_dirs(output_path)?;
    save_png(&displaced, output_path)?;
    println!(
        "rendered two-source CPU displacement from {} modulating {} to {}",
        modulator_image.display(),
        carrier_image.display(),
        output_path.display()
    );
    Ok(())
}

pub(crate) fn render_granular_mosaic(
    modulator_image: &Path,
    carrier_image: &Path,
    output_path: &Path,
    settings: GranularMosaicSettings,
    grain_cache_dir: Option<&Path>,
    backend: RenderBackend,
    selection_mode: GrainSelectionMode,
) -> Result<(), CliError> {
    settings.validate()?;
    let modulator = load_image_f32(modulator_image)?;
    let carrier = load_image_f32(carrier_image)?;
    let rendered = if let Some(cache_directory) = grain_cache_dir {
        let modulator_fingerprint = image_file_fingerprint(modulator_image)?;
        let carrier_fingerprint = image_file_fingerprint(carrier_image)?;
        render_granular_mosaic_frame(
            &modulator,
            &carrier,
            settings,
            Some(GranularMosaicCacheContext {
                directory: cache_directory,
                modulator_fingerprint: &modulator_fingerprint,
                carrier_fingerprint: &carrier_fingerprint,
            }),
            backend,
            selection_mode,
        )?
    } else {
        render_granular_mosaic_frame(
            &modulator,
            &carrier,
            settings,
            None,
            backend,
            selection_mode,
        )?
    };

    write_parent_dirs(output_path)?;
    save_png(&rendered.image, output_path)?;
    print_granular_cache_summary(grain_cache_dir, rendered);
    println!(
        "rendered granular mosaic from {} modulating {} to {}",
        modulator_image.display(),
        carrier_image.display(),
        output_path.display()
    );
    Ok(())
}

pub(crate) fn render_video_vocoder_frame(
    modulator: &ImageBufferF32,
    carrier: &ImageBufferF32,
    settings: VideoVocoderSettings,
    mode: CliVocoderMode,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match mode {
        CliVocoderMode::Gain => {
            if backend == RenderBackend::Metal {
                return Err(CliError::Message(
                    "the Metal backend is only implemented for --mode match; use --backend cpu for gain mode".to_string(),
                ));
            }
            let envelope = analyze_luma_band_envelope_cpu(modulator, settings.bands)?;
            Ok(video_vocoder_cpu(carrier, &envelope, settings)?)
        }
        CliVocoderMode::Match => {
            let tone = luma_specification_tone_map(modulator, carrier);
            match backend {
                RenderBackend::Cpu => Ok(apply_tone_map_cpu(carrier, &tone, settings.amount)?),
                RenderBackend::Metal => {
                    render_video_vocoder_match_metal(carrier, &tone, settings.amount)
                }
            }
        }
    }
}

pub(crate) fn render_video_vocoder(
    modulator_image: &Path,
    carrier_image: &Path,
    output_path: &Path,
    settings: VideoVocoderSettings,
    mode: CliVocoderMode,
    backend: RenderBackend,
) -> Result<(), CliError> {
    settings.validate()?;
    let modulator = load_image_f32(modulator_image)?;
    let carrier = load_image_f32(carrier_image)?;
    let rendered = render_video_vocoder_frame(&modulator, &carrier, settings, mode, backend)?;

    write_parent_dirs(output_path)?;
    save_png(&rendered, output_path)?;
    println!(
        "rendered video vocoder ({:?} mode, {} bands, amount {}) from {} modulating {} to {}",
        mode,
        settings.bands,
        settings.amount,
        modulator_image.display(),
        carrier_image.display(),
        output_path.display()
    );
    Ok(())
}

pub(crate) fn render_video_vocoder_sequence(
    modulator_dir: &Path,
    carrier_dir: &Path,
    output_dir: &Path,
    settings: VideoVocoderSettings,
    mode: CliVocoderMode,
    backend: RenderBackend,
    max_frames: Option<usize>,
) -> Result<FrameSequenceRenderResult, CliError> {
    settings.validate()?;
    if matches!(max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let modulator_frames = collect_image_frames(modulator_dir)?;
    let carrier_frames = collect_image_frames(carrier_dir)?;
    if modulator_frames.is_empty() || carrier_frames.is_empty() {
        return Err(CliError::Message(
            "video vocoder requires at least one PNG frame in each source directory".to_string(),
        ));
    }

    let paired_count = modulator_frames.len().min(carrier_frames.len());
    let frame_count = max_frames
        .map(|limit| limit.min(paired_count))
        .unwrap_or(paired_count);
    fs::create_dir_all(output_dir)?;

    for index in 0..frame_count {
        let modulator = load_image_f32(&modulator_frames[index])?;
        let carrier = load_image_f32(&carrier_frames[index])?;
        let rendered = render_video_vocoder_frame(&modulator, &carrier, settings, mode, backend)?;
        save_png(&rendered, &output_dir.join(format!("frame_{index:06}.png")))?;
    }

    if modulator_frames.len() != carrier_frames.len() {
        println!(
            "source frame counts differ: {} modulator frame(s), {} carrier frame(s); rendered common prefix",
            modulator_frames.len(),
            carrier_frames.len()
        );
    }
    println!(
        "rendered video vocoder sequence with {} frame(s) ({:?} mode, {} bands, amount {}) from {} modulating {} to {}",
        frame_count,
        mode,
        settings.bands,
        settings.amount,
        modulator_dir.display(),
        carrier_dir.display(),
        output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) struct AudioVideoRouteSequenceRequest<'a> {
    pub(crate) modulator_wav: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) amount: f32,
    pub(crate) shift_x: f32,
    pub(crate) shift_y: f32,
    pub(crate) rms_window: u32,
    pub(crate) rms_hop: u32,
    pub(crate) fps: f64,
    pub(crate) backend: RenderBackend,
    pub(crate) max_frames: Option<usize>,
}

pub(crate) fn render_audio_video_route_sequence(
    request: AudioVideoRouteSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    if !request.amount.is_finite() || request.amount < 0.0 {
        return Err(CliError::Message(
            "amount must be finite and non-negative".to_string(),
        ));
    }
    if !request.shift_x.is_finite() || !request.shift_y.is_finite() {
        return Err(CliError::Message(
            "shift-x and shift-y must be finite".to_string(),
        ));
    }
    if !request.fps.is_finite() || request.fps <= 0.0 {
        return Err(CliError::Message(
            "fps must be a finite value greater than zero".to_string(),
        ));
    }
    if matches!(request.max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let buffer = load_wav_f32(request.modulator_wav)?;
    let rms_frames = rms_envelope(
        &buffer,
        request.rms_window as usize,
        request.rms_hop as usize,
    )?;
    let samples: Vec<(f64, f32)> = rms_frames
        .iter()
        .map(|frame| (frame.time_seconds, frame.rms))
        .collect();
    let envelope = RmsDisplacementEnvelope::from_rms_samples(&samples);

    let carrier_frames = collect_image_frames(request.carrier_dir)?;
    if carrier_frames.is_empty() {
        return Err(CliError::Message(
            "audio-to-video routing requires at least one PNG frame in the carrier directory"
                .to_string(),
        ));
    }
    let frame_count = request
        .max_frames
        .map(|limit| limit.min(carrier_frames.len()))
        .unwrap_or(carrier_frames.len());
    fs::create_dir_all(request.output_dir)?;

    for (index, carrier_path) in carrier_frames.iter().enumerate().take(frame_count) {
        let carrier = load_image_f32(carrier_path)?;
        let field = uniform_displacement_field(
            carrier.width,
            carrier.height,
            request.shift_x,
            request.shift_y,
        )?;
        let time_seconds = index as f64 / request.fps;
        let amount = request.amount * envelope.gain_at(time_seconds);
        let rendered = render_audio_video_route_frame(&carrier, &field, amount, request.backend)?;
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    println!(
        "rendered audio→video route sequence with {} frame(s) (amount {}, shift [{}, {}], {:?}) from {} modulating {} to {}",
        frame_count,
        request.amount,
        request.shift_x,
        request.shift_y,
        request.backend,
        request.modulator_wav.display(),
        request.carrier_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) fn render_audio_video_route_frame(
    carrier: &ImageBufferF32,
    field: &FlowField,
    amount: f32,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(flow_displace_cpu(carrier, field, amount)?),
        RenderBackend::Metal => render_audio_video_route_frame_metal(carrier, field, amount),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_audio_video_route_frame_metal(
    carrier: &ImageBufferF32,
    field: &FlowField,
    amount: f32,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::flow_displace_metal(carrier, field, amount)?;
    let cpu = flow_displace_cpu(carrier, field, amount)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU displace outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal audio-route render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_audio_video_route_frame_metal(
    _carrier: &ImageBufferF32,
    _field: &FlowField,
    _amount: f32,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

pub(crate) struct DatamoshSequenceRequest<'a> {
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) flow_cache_dir: Option<&'a Path>,
    pub(crate) keyframe_interval: u32,
    pub(crate) amount: f32,
    pub(crate) block_size: u32,
    pub(crate) residual_gain: f32,
    pub(crate) residual_decay: f32,
    pub(crate) refresh_threshold: f32,
    pub(crate) vector_remix: VectorRemixMode,
    pub(crate) remix_seed: u64,
    pub(crate) preset: DatamoshPreset,
    pub(crate) backend: RenderBackend,
    pub(crate) max_frames: Option<usize>,
    pub(crate) job_id: &'a str,
    pub(crate) provenance: Option<&'a RenderJobProvenance>,
    pub(crate) stop_after_frame: bool,
}

pub(crate) const DATAMOSH_RENDER_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct DatamoshSequenceContract {
    version: u32,
    flow_algorithm: String,
    modulator: FeedbackSequenceSourceFingerprint,
    carrier: FeedbackSequenceSourceFingerprint,
    settings: DatamoshSequenceSettings,
    backend: RenderBackend,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct DatamoshSequenceSettings {
    pub(crate) keyframe_interval: u32,
    pub(crate) amount: f32,
    pub(crate) block_size: u32,
    pub(crate) residual_gain: f32,
    pub(crate) residual_decay: f32,
    pub(crate) refresh_threshold: f32,
    pub(crate) vector_remix: String,
    pub(crate) remix_seed: u64,
    pub(crate) preset: DatamoshPreset,
    #[serde(default)]
    pub(crate) scanline_smear: bool,
    #[serde(default)]
    pub(crate) codec_engrave: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DatamoshSequenceCheckpoint {
    version: u32,
    task: String,
    job_id: String,
    status: String,
    next_frame_index: u32,
    previous_output_path: Option<String>,
    previous_output_state: Option<FlowFeedbackStateDescriptor>,
    residual_flow_path: Option<String>,
    contract: DatamoshSequenceContract,
    provenance: RenderJobProvenance,
}

/// Render a controlled-datamosh ("bloom/melt") sequence. Source A's per-frame
/// optical flow (Lucas-Kanade between consecutive A frames) advects Source B's
/// *previous output*; keyframes (`is_datamosh_keyframe`) snap back to the carrier.
/// The recursion carries the previous output as RGBA32F in memory (the
/// unquantized internal state), never re-reading a display PNG.
pub(crate) fn render_datamosh_sequence(
    request: DatamoshSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    let settings = resolve_datamosh_settings(&request);
    if let Some(note) = datamosh_preset_resolution_note(&request, &settings) {
        println!("{note}");
    }
    if !settings.amount.is_finite() || settings.amount < 0.0 {
        return Err(CliError::Message(
            "amount must be finite and non-negative".to_string(),
        ));
    }
    if !settings.residual_gain.is_finite() || settings.residual_gain < 0.0 {
        return Err(CliError::Message(
            "residual-gain must be finite and non-negative".to_string(),
        ));
    }
    if !settings.residual_decay.is_finite() || settings.residual_decay < 0.0 {
        return Err(CliError::Message(
            "residual-decay must be finite and non-negative".to_string(),
        ));
    }
    if !settings.refresh_threshold.is_finite() || settings.refresh_threshold < 0.0 {
        return Err(CliError::Message(
            "block-refresh-threshold must be finite and non-negative".to_string(),
        ));
    }
    if matches!(request.max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let modulator_frames = collect_image_frames(request.modulator_dir)?;
    let carrier_frames = collect_image_frames(request.carrier_dir)?;
    if modulator_frames.is_empty() || carrier_frames.is_empty() {
        return Err(CliError::Message(
            "datamosh requires at least one PNG frame in both the modulator and carrier directories"
                .to_string(),
        ));
    }
    let available = modulator_frames.len().min(carrier_frames.len());
    let frame_count = request
        .max_frames
        .map(|limit| limit.min(available))
        .unwrap_or(available);
    let frame_count_u32 = u32::try_from(frame_count).map_err(|_| {
        CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
    })?;
    let contract = DatamoshSequenceContract {
        version: DATAMOSH_RENDER_CONTRACT_VERSION,
        flow_algorithm: OPTICAL_FLOW_ALGORITHM.to_string(),
        modulator: feedback_source_fingerprint(request.modulator_dir, &modulator_frames)?,
        carrier: feedback_source_fingerprint(request.carrier_dir, &carrier_frames)?,
        settings: settings.clone(),
        backend: request.backend,
    };
    let provenance = request.provenance.cloned().unwrap_or_else(|| {
        datamosh_sequence_provenance(
            request.modulator_dir,
            request.carrier_dir,
            request.flow_cache_dir,
        )
    });

    fs::create_dir_all(request.output_dir)?;
    if let Some(cache_root) = request.flow_cache_dir {
        fs::create_dir_all(cache_root)?;
    }

    // Residual accumulation is active only with a positive gain over coarse
    // blocks; otherwise the loop uses the plain block-quantize path (gain 0 ⇒
    // byte-identical block tier, by construction).
    let residual_active = settings.residual_gain > 0.0 && settings.block_size >= 2;
    // Per-block keep/drop refresh is active only with a positive threshold over
    // coarse blocks (threshold 0 ⇒ byte-identical to the block/residual path).
    let refresh_active = settings.refresh_threshold > 0.0 && settings.block_size >= 2;
    // Vector remix permutes the block-MV grid before advection; active only with a
    // non-None mode over coarse blocks (None ⇒ byte-identical to the block path). It
    // computes `effective` itself, so it takes precedence over residual.
    let remix_active =
        settings.vector_remix_mode() != VectorRemixMode::None && settings.block_size >= 2;

    let (start_frame, mut previous_output, mut accumulated_residual) = load_datamosh_resume_state(
        request.output_dir,
        request.job_id,
        &contract,
        &provenance,
        frame_count_u32,
    )?;
    if start_frame >= frame_count {
        println!(
            "datamosh sequence already complete in {}",
            request.output_dir.display()
        );
        return Ok(FrameSequenceRenderResult { frame_count });
    }
    let mut previous_modulator = if start_frame > 0 {
        Some(load_image_f32(&modulator_frames[start_frame - 1])?)
    } else {
        None
    };
    let mut latest_state_path: Option<String> = None;
    let mut latest_residual_path: Option<String> = None;
    let mut reused_optical_flow_cache_count = 0usize;
    let mut generated_optical_flow_cache_count = 0usize;
    let mut metal_flow_validated = false;
    let flow_cache_algorithm = optical_flow_cache_algorithm(request.backend);
    for index in start_frame..frame_count {
        let carrier = load_image_f32(&carrier_frames[index])?;
        let modulator = load_image_f32(&modulator_frames[index])?;
        let is_keyframe = is_datamosh_keyframe(index, settings.keyframe_interval);

        let rendered = match previous_output.as_ref() {
            // P-frame delta: advect the held previous output by A's flow. The
            // carrier is frozen from the last keyframe and is not sampled here.
            Some(previous) if !is_keyframe => {
                let previous_modulator = previous_modulator.as_ref().ok_or_else(|| {
                    CliError::Message(
                        "internal error: missing previous modulator frame for datamosh flow"
                            .to_string(),
                    )
                })?;
                let cache_directory = request
                    .flow_cache_dir
                    .map(|root| root.join(format!("frame_{index:06}")));
                let (flow, generated_temporal_flow_cache, reused_temporal_flow_cache) =
                    if let Some(flow) = cache_directory
                        .as_deref()
                        .map(|directory| {
                            read_cached_temporal_flow(
                                directory,
                                flow_cache_algorithm,
                                &contract.modulator.checksum,
                                carrier.width,
                                carrier.height,
                            )
                        })
                        .transpose()?
                        .flatten()
                    {
                        (flow, false, true)
                    } else {
                        (
                            compute_optical_flow_backend(
                                previous_modulator,
                                &modulator,
                                carrier.width,
                                carrier.height,
                                LUCAS_KANADE_WINDOW_RADIUS,
                                request.backend,
                                &mut metal_flow_validated,
                            )?,
                            cache_directory.is_some(),
                            false,
                        )
                    };
                reused_optical_flow_cache_count += usize::from(reused_temporal_flow_cache);
                generated_optical_flow_cache_count += usize::from(generated_temporal_flow_cache);
                if generated_temporal_flow_cache {
                    if let Some(frame_cache_dir) = cache_directory.as_deref() {
                        write_flow_cache_with_source_fingerprint(
                            frame_cache_dir,
                            &flow,
                            flow_cache_algorithm,
                            Some(&contract.modulator.checksum),
                        )?;
                    }
                }
                // Codec-simulated mosh: quantize A's flow to a coarse block grid
                // (CPU flow transform) so whole macroblocks slide; block_size <= 1
                // returns the flow unchanged (the smooth bloom path). With residual
                // active, the discarded intra-block motion is accumulated and
                // re-injected (also a pure CPU flow transform). The displace that
                // follows is the existing parity-gated kernel on either backend, so
                // Metal stays free.
                let effective = if remix_active {
                    // FFglitch-style MV remix: permute the block-mean grid (sort by
                    // magnitude / seeded shuffle) before the parity-gated displace.
                    // A pure flow→flow transform like quantize, so Metal stays free.
                    remix_block_vectors(
                        &flow,
                        settings.block_size,
                        settings.vector_remix_mode(),
                        settings.remix_seed,
                    )?
                } else if residual_active {
                    let accum = accumulated_residual
                        .take()
                        .unwrap_or(zero_flow(carrier.width, carrier.height)?);
                    let (effective, new_accum) = datamosh_residual_flow(
                        &flow,
                        &accum,
                        settings.block_size,
                        settings.residual_gain,
                        settings.residual_decay,
                    )?;
                    accumulated_residual = Some(new_accum);
                    effective
                } else {
                    quantize_flow_to_blocks(&flow, settings.block_size)?
                };
                let advected = render_datamosh_advect_frame(
                    previous,
                    &effective,
                    settings.amount,
                    request.backend,
                )?;
                // Per-block keep/drop: macroblocks whose mean motion is below the
                // threshold snap back to the carrier B[i] (intra-block refresh)
                // while busier blocks keep rotting. A pure CPU composite over the
                // gated displace output, so Metal stays free; refreshed blocks also
                // clear their residual accumulator (matching the keyframe reset).
                let refreshed = if refresh_active {
                    let block_means = quantize_flow_to_blocks(&flow, settings.block_size)?;
                    let composed = datamosh_block_refresh_composite(
                        &advected,
                        &carrier,
                        &block_means,
                        settings.refresh_threshold,
                    )?;
                    if let Some(accum) = accumulated_residual.take() {
                        accumulated_residual = Some(reset_residual_in_refreshed_blocks(
                            &accum,
                            &block_means,
                            settings.refresh_threshold,
                        )?);
                    }
                    composed
                } else {
                    advected
                };
                let post_scanline = if settings.scanline_smear {
                    let frame_index = u32::try_from(index).map_err(|_| {
                        CliError::Message(
                            "frame sequence contains more than u32::MAX frames".to_string(),
                        )
                    })?;
                    datamosh_scanline_smear_frame_cpu(
                        &refreshed,
                        &effective,
                        frame_index,
                        if settings.codec_engrave {
                            datamosh_codec_scanline_smear_settings(settings.remix_seed)
                        } else {
                            datamosh_scanline_smear_settings(settings.remix_seed)
                        },
                    )?
                } else {
                    refreshed
                };
                if settings.codec_engrave {
                    let frame_index = u32::try_from(index).map_err(|_| {
                        CliError::Message(
                            "frame sequence contains more than u32::MAX frames".to_string(),
                        )
                    })?;
                    datamosh_codec_engrave_frame_cpu(
                        &post_scanline,
                        &carrier,
                        &effective,
                        frame_index,
                        datamosh_codec_engrave_settings(settings.remix_seed),
                    )?
                } else {
                    post_scanline
                }
            }
            // Frame zero or keyframe refresh: the carrier is the output verbatim,
            // and the residual accumulator is cleared (I-frame refresh).
            _ => {
                accumulated_residual = None;
                carrier.clone()
            }
        };

        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
        previous_output = Some(rendered);
        previous_modulator = Some(modulator);

        let frame_index = u32::try_from(index).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let state_path = datamosh_output_state_path(request.output_dir, frame_index);
        let state_relative_path = datamosh_output_state_relative_path(frame_index);
        let output_state = previous_output.as_ref().ok_or_else(|| {
            CliError::Message("internal error: missing datamosh output state".to_string())
        })?;
        let descriptor = write_flow_feedback_state(&state_path, output_state)?;
        let residual_relative_path = if let Some(residual) = accumulated_residual.as_ref() {
            let relative_path = datamosh_residual_state_relative_path(frame_index);
            write_flow_cache(
                request.output_dir.join(&relative_path),
                residual,
                "datamosh_residual_state_v1",
            )?;
            Some(relative_path)
        } else {
            None
        };
        write_datamosh_checkpoint(
            request.output_dir,
            DatamoshCheckpointWrite {
                job_id: request.job_id,
                status: "running",
                next_frame_index: frame_index.checked_add(1).ok_or_else(|| {
                    CliError::Message(
                        "frame sequence contains more than u32::MAX frames".to_string(),
                    )
                })?,
                previous_output_path: Some(&state_relative_path),
                previous_output_state: Some(descriptor),
                residual_flow_path: residual_relative_path.as_deref(),
                contract: &contract,
                provenance: &provenance,
            },
        )?;
        latest_state_path = Some(state_relative_path);
        latest_residual_path = residual_relative_path;

        if request.stop_after_frame {
            println!(
                "checkpointed datamosh sequence after frame {} in {}",
                index,
                request.output_dir.display()
            );
            return Ok(FrameSequenceRenderResult {
                frame_count: index + 1,
            });
        }
    }

    let final_state_path = latest_state_path.as_deref().ok_or_else(|| {
        CliError::Message("datamosh render completed without a float state checkpoint".to_string())
    })?;
    let state_path = datamosh_state_path_from_checkpoint(request.output_dir, final_state_path)?;
    let (final_state, _) = read_flow_feedback_state(&state_path)?;
    write_datamosh_checkpoint(
        request.output_dir,
        DatamoshCheckpointWrite {
            job_id: request.job_id,
            status: "complete",
            next_frame_index: frame_count_u32,
            previous_output_path: Some(final_state_path),
            previous_output_state: Some(final_state),
            residual_flow_path: latest_residual_path.as_deref(),
            contract: &contract,
            provenance: &provenance,
        },
    )?;

    println!(
        "rendered datamosh sequence with {} frame(s) (keyframe-interval {}, amount {}, block-size {}, residual-gain {}, residual-decay {}, block-refresh-threshold {}, vector-remix {:?} seed {}, scanline-smear {}, codec-engrave {}, {:?}) from {} moshing {} to {}",
        frame_count,
        settings.keyframe_interval,
        settings.amount,
        settings.block_size,
        settings.residual_gain,
        settings.residual_decay,
        settings.refresh_threshold,
        settings.vector_remix_mode(),
        settings.remix_seed,
        settings.scanline_smear,
        settings.codec_engrave,
        request.backend,
        request.modulator_dir.display(),
        request.carrier_dir.display(),
        request.output_dir.display()
    );
    if let Some(cache_root) = request.flow_cache_dir {
        println!(
            "reused {reused_optical_flow_cache_count} and generated {generated_optical_flow_cache_count} datamosh optical-flow cache frame(s) in {}",
            cache_root.display()
        );
    }
    Ok(FrameSequenceRenderResult { frame_count })
}

impl DatamoshSequenceSettings {
    pub(crate) fn vector_remix_mode(&self) -> VectorRemixMode {
        match self.vector_remix.as_str() {
            "sort" => VectorRemixMode::Sort,
            "shuffle" => VectorRemixMode::Shuffle,
            _ => VectorRemixMode::None,
        }
    }
}

pub(crate) fn resolve_datamosh_settings(
    request: &DatamoshSequenceRequest<'_>,
) -> DatamoshSequenceSettings {
    let custom = datamosh_custom_settings(request);
    match request.preset {
        DatamoshPreset::Custom => custom,
        DatamoshPreset::CodecBloom => DatamoshSequenceSettings {
            keyframe_interval: 0,
            amount: 1.0,
            block_size: 1,
            residual_gain: 0.0,
            residual_decay: 0.9,
            refresh_threshold: 0.0,
            vector_remix: "none".to_string(),
            remix_seed: 0,
            preset: request.preset,
            scanline_smear: false,
            codec_engrave: false,
        },
        DatamoshPreset::StructuredMelt => DatamoshSequenceSettings {
            keyframe_interval: 0,
            amount: 1.5,
            block_size: 8,
            residual_gain: 0.8,
            residual_decay: 0.95,
            refresh_threshold: 0.0,
            vector_remix: "none".to_string(),
            remix_seed: 0,
            preset: request.preset,
            scanline_smear: false,
            codec_engrave: false,
        },
        DatamoshPreset::MacroblockRot => DatamoshSequenceSettings {
            keyframe_interval: 0,
            amount: 1.25,
            block_size: 16,
            residual_gain: 1.0,
            residual_decay: 0.9,
            refresh_threshold: 1.0,
            vector_remix: "none".to_string(),
            remix_seed: 0,
            preset: request.preset,
            scanline_smear: false,
            codec_engrave: false,
        },
        DatamoshPreset::VectorShuffle => DatamoshSequenceSettings {
            keyframe_interval: 0,
            amount: 1.0,
            block_size: 16,
            residual_gain: 0.0,
            residual_decay: 0.9,
            refresh_threshold: 0.0,
            vector_remix: "shuffle".to_string(),
            remix_seed: request.remix_seed,
            preset: request.preset,
            scanline_smear: false,
            codec_engrave: false,
        },
        DatamoshPreset::ScanlineSmear => DatamoshSequenceSettings {
            keyframe_interval: 0,
            amount: 6.0,
            block_size: 8,
            residual_gain: 2.5,
            residual_decay: 0.99,
            refresh_threshold: 0.08,
            vector_remix: "sort".to_string(),
            remix_seed: request.remix_seed,
            preset: request.preset,
            scanline_smear: true,
            codec_engrave: false,
        },
        DatamoshPreset::CodecEngrave => DatamoshSequenceSettings {
            keyframe_interval: 0,
            amount: 7.0,
            block_size: 8,
            residual_gain: 2.5,
            residual_decay: 0.99,
            refresh_threshold: 0.15,
            vector_remix: "sort".to_string(),
            remix_seed: request.remix_seed,
            preset: request.preset,
            scanline_smear: true,
            codec_engrave: true,
        },
    }
}

pub(crate) fn datamosh_custom_settings(
    request: &DatamoshSequenceRequest<'_>,
) -> DatamoshSequenceSettings {
    DatamoshSequenceSettings {
        keyframe_interval: request.keyframe_interval,
        amount: request.amount,
        block_size: request.block_size,
        residual_gain: request.residual_gain,
        residual_decay: request.residual_decay,
        refresh_threshold: request.refresh_threshold,
        vector_remix: vector_remix_name(request.vector_remix).to_string(),
        remix_seed: request.remix_seed,
        preset: request.preset,
        scanline_smear: false,
        codec_engrave: false,
    }
}

pub(crate) fn datamosh_scanline_smear_settings(seed: u64) -> ScanlineSmearSettings {
    ScanlineSmearSettings {
        line_height: 3,
        max_shift: 360.0,
        motion_gain: 140.0,
        wave_amplitude: 80.0,
        wave_frequency: 0.37,
        smear_mix: 0.90,
        structure_protect: 0.68,
        chroma_burst_rate: 0.03,
        chroma_burst_size: 18,
        seed,
    }
}

pub(crate) fn datamosh_codec_scanline_smear_settings(seed: u64) -> ScanlineSmearSettings {
    ScanlineSmearSettings {
        line_height: 2,
        max_shift: 400.0,
        motion_gain: 160.0,
        wave_amplitude: 90.0,
        wave_frequency: 0.55,
        smear_mix: 0.88,
        structure_protect: 0.72,
        chroma_burst_rate: 0.03,
        chroma_burst_size: 18,
        seed,
    }
}

pub(crate) fn datamosh_codec_engrave_settings(seed: u64) -> CodecEngraveSettings {
    CodecEngraveSettings {
        block_size: 4,
        edge_gain: 14.0,
        hatch_strength: 0.9,
        hatch_frequency: 1.2,
        chroma_offset: 3.5,
        block_step: 0.35,
        foreground_boost: 0.18,
        micro_contrast: 1.3,
        seed,
    }
}

pub(crate) fn datamosh_preset_resolution_note(
    request: &DatamoshSequenceRequest<'_>,
    resolved: &DatamoshSequenceSettings,
) -> Option<String> {
    if matches!(request.preset, DatamoshPreset::Custom) {
        return None;
    }
    let explicit = datamosh_custom_settings(request);
    let overridden = explicit.keyframe_interval != resolved.keyframe_interval
        || explicit.amount != resolved.amount
        || explicit.block_size != resolved.block_size
        || explicit.residual_gain != resolved.residual_gain
        || explicit.residual_decay != resolved.residual_decay
        || explicit.refresh_threshold != resolved.refresh_threshold
        || explicit.vector_remix != resolved.vector_remix
        || explicit.remix_seed != resolved.remix_seed
        || explicit.scanline_smear != resolved.scanline_smear
        || explicit.codec_engrave != resolved.codec_engrave;
    overridden.then(|| {
        format!(
            "datamosh preset '{}' resolved to keyframe-interval {}, amount {}, block-size {}, residual-gain {}, residual-decay {}, block-refresh-threshold {}, vector-remix {} seed {}, scanline-smear {}, codec-engrave {}; explicit datamosh knobs are ignored unless the preset uses them. Use --preset custom for manual control.",
            datamosh_preset_label(request.preset),
            resolved.keyframe_interval,
            resolved.amount,
            resolved.block_size,
            resolved.residual_gain,
            resolved.residual_decay,
            resolved.refresh_threshold,
            resolved.vector_remix,
            resolved.remix_seed,
            resolved.scanline_smear,
            resolved.codec_engrave
        )
    })
}

pub(crate) fn vector_remix_name(mode: VectorRemixMode) -> &'static str {
    match mode {
        VectorRemixMode::None => "none",
        VectorRemixMode::Sort => "sort",
        VectorRemixMode::Shuffle => "shuffle",
    }
}

pub(crate) fn datamosh_preset_label(preset: DatamoshPreset) -> &'static str {
    match preset {
        DatamoshPreset::Custom => "custom",
        DatamoshPreset::CodecBloom => "codec_bloom",
        DatamoshPreset::StructuredMelt => "structured_melt",
        DatamoshPreset::MacroblockRot => "macroblock_rot",
        DatamoshPreset::VectorShuffle => "vector_shuffle",
        DatamoshPreset::ScanlineSmear => "scanline_smear",
        DatamoshPreset::CodecEngrave => "codec_engrave",
    }
}

pub(crate) fn bitstream_preset_label(preset: DatamoshBitstreamPreset) -> &'static str {
    match preset {
        DatamoshBitstreamPreset::Custom => "custom",
        DatamoshBitstreamPreset::Bloom => "bloom",
        DatamoshBitstreamPreset::HeavyMelt => "heavy_melt",
        DatamoshBitstreamPreset::VoidMosh => "void_mosh",
        DatamoshBitstreamPreset::MotionGraft => "motion_graft",
    }
}

pub(crate) fn datamosh_sequence_algorithm(settings: &DatamoshSequenceSettings) -> &'static str {
    if settings.codec_engrave {
        DATAMOSH_CODEC_ENGRAVE_ALGORITHM
    } else if settings.scanline_smear {
        DATAMOSH_SCANLINE_SMEAR_ALGORITHM
    } else {
        morphogen_render::datamosh_algorithm(
            settings.block_size,
            settings.residual_gain,
            settings.refresh_threshold,
            settings.vector_remix_mode(),
        )
    }
}

pub(crate) fn datamosh_sequence_provenance(
    modulator_dir: &Path,
    carrier_dir: &Path,
    flow_cache_dir: Option<&Path>,
) -> RenderJobProvenance {
    RenderJobProvenance {
        sources: vec![
            RenderJobSourceProvenance {
                source_id: "source-a-frames".to_string(),
                role: SourceRole::Modulator,
                path: modulator_dir.to_string_lossy().to_string(),
            },
            RenderJobSourceProvenance {
                source_id: "source-b-frames".to_string(),
                role: SourceRole::Carrier,
                path: carrier_dir.to_string_lossy().to_string(),
            },
        ],
        analysis_caches: flow_cache_dir
            .map(|path| {
                vec![RenderJobAnalysisCacheProvenance {
                    kind: AnalysisKind::OpticalFlow,
                    path: path.to_string_lossy().to_string(),
                    producer: OPTICAL_FLOW_ALGORITHM.to_string(),
                }]
            })
            .unwrap_or_default(),
    }
}

pub(crate) fn load_datamosh_resume_state(
    output_dir: &Path,
    job_id: &str,
    expected_contract: &DatamoshSequenceContract,
    expected_provenance: &RenderJobProvenance,
    frame_count: u32,
) -> Result<(usize, Option<ImageBufferF32>, Option<FlowField>), CliError> {
    let checkpoint_path = output_dir.join("checkpoint.json");
    if !checkpoint_path.exists() {
        return Ok((0, None, None));
    }

    let checkpoint: DatamoshSequenceCheckpoint =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path)?)?;
    if checkpoint.version != DATAMOSH_RENDER_CONTRACT_VERSION
        || checkpoint.task != "frame_sequence_datamosh"
        || checkpoint.job_id != job_id
    {
        return Err(CliError::Message(format!(
            "datamosh checkpoint at {} is incompatible with this render",
            checkpoint_path.display()
        )));
    }
    if checkpoint.contract != *expected_contract || checkpoint.provenance != *expected_provenance {
        return Err(CliError::Message(
            "datamosh checkpoint input provenance or settings changed; start with a new output directory"
                .to_string(),
        ));
    }
    if checkpoint.next_frame_index > frame_count {
        return Err(CliError::Message(
            "datamosh checkpoint advances beyond the current frame sequence".to_string(),
        ));
    }
    let start_frame = checkpoint.next_frame_index as usize;
    if start_frame == 0 {
        return Ok((0, None, None));
    }

    let expected_output = checkpoint.previous_output_state.ok_or_else(|| {
        CliError::Message(
            "datamosh checkpoint is missing its previous float output state".to_string(),
        )
    })?;
    let relative_output_path = checkpoint.previous_output_path.ok_or_else(|| {
        CliError::Message("datamosh checkpoint is missing its output state path".to_string())
    })?;
    let output_state_path = datamosh_state_path_from_checkpoint(output_dir, &relative_output_path)?;
    let (actual_output, previous_output) = read_flow_feedback_state(&output_state_path)?;
    if actual_output != expected_output {
        return Err(CliError::Message(format!(
            "datamosh output state at {} does not match its checkpoint",
            output_state_path.display()
        )));
    }

    let residual = checkpoint
        .residual_flow_path
        .as_deref()
        .map(|relative_path| {
            let path = datamosh_state_path_from_checkpoint(output_dir, relative_path)?;
            let (_, flow) = read_flow_cache(&path)?;
            Ok::<FlowField, CliError>(flow)
        })
        .transpose()?;
    let previous_frame_path = output_dir.join(format!("frame_{:06}.png", start_frame - 1));
    if !previous_frame_path.exists() {
        return Err(CliError::Message(format!(
            "datamosh checkpoint is missing exported frame {}",
            previous_frame_path.display()
        )));
    }
    Ok((start_frame, Some(previous_output), residual))
}

pub(crate) struct DatamoshCheckpointWrite<'a> {
    job_id: &'a str,
    status: &'a str,
    next_frame_index: u32,
    previous_output_path: Option<&'a str>,
    previous_output_state: Option<FlowFeedbackStateDescriptor>,
    residual_flow_path: Option<&'a str>,
    contract: &'a DatamoshSequenceContract,
    provenance: &'a RenderJobProvenance,
}

pub(crate) fn write_datamosh_checkpoint(
    output_dir: &Path,
    checkpoint: DatamoshCheckpointWrite<'_>,
) -> Result<(), CliError> {
    let checkpoint = DatamoshSequenceCheckpoint {
        version: DATAMOSH_RENDER_CONTRACT_VERSION,
        task: "frame_sequence_datamosh".to_string(),
        job_id: checkpoint.job_id.to_string(),
        status: checkpoint.status.to_string(),
        next_frame_index: checkpoint.next_frame_index,
        previous_output_path: checkpoint.previous_output_path.map(str::to_string),
        previous_output_state: checkpoint.previous_output_state,
        residual_flow_path: checkpoint.residual_flow_path.map(str::to_string),
        contract: checkpoint.contract.clone(),
        provenance: checkpoint.provenance.clone(),
    };
    write_feedback_json_atomically(
        &output_dir.join("checkpoint.json"),
        &serde_json::to_string_pretty(&checkpoint)?,
    )?;
    Ok(())
}

pub(crate) fn datamosh_output_state_relative_path(frame_index: u32) -> String {
    format!("state/datamosh_output_frame_{frame_index:06}.rgba32f")
}

pub(crate) fn datamosh_residual_state_relative_path(frame_index: u32) -> String {
    format!("state/datamosh_residual_frame_{frame_index:06}")
}

pub(crate) fn datamosh_output_state_path(output_dir: &Path, frame_index: u32) -> PathBuf {
    output_dir.join(datamosh_output_state_relative_path(frame_index))
}

pub(crate) fn datamosh_state_path_from_checkpoint(
    output_dir: &Path,
    relative_path: &str,
) -> Result<PathBuf, CliError> {
    let relative_path = Path::new(relative_path);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(CliError::Message(
            "datamosh checkpoint state path must be relative to its output directory".to_string(),
        ));
    }
    Ok(output_dir.join(relative_path))
}

/// The flow-cache algorithm id used for datamosh optical flow, segregated by backend.
/// The GPU and CPU estimators agree within tolerance but not bit-for-bit, and the
/// determinism contract treats the backend as a setting — so a Metal-produced cache is
/// never reused by a CPU render (or vice versa); toggling the backend recomputes.
pub(crate) const OPTICAL_FLOW_METAL_ALGORITHM: &str = "pyramidal_lucas_kanade_metal_v1";

/// Validate-then-trust parity epsilon for the Metal optical-flow backend, in output
/// pixels of displacement. The CPU reference is ground truth; on a representative
/// textured-translation fixture the Metal refinement diverges by ~7e-5 px, so this
/// 0.01 px gate cleanly separates correct GPU flow (sub-pixel rounding) from a broken
/// kernel (which diverges by whole pixels) while tolerating real-footage float rounding.
pub(crate) const OPTICAL_FLOW_METAL_PARITY_EPSILON: f32 = 0.01;

pub(crate) fn optical_flow_cache_algorithm(backend: RenderBackend) -> &'static str {
    match backend {
        RenderBackend::Cpu => OPTICAL_FLOW_ALGORITHM,
        RenderBackend::Metal => OPTICAL_FLOW_METAL_ALGORITHM,
    }
}

/// Compute temporal optical flow on the selected backend (shared by the datamosh and
/// flow-feedback sequences). The CPU backend runs the reference estimator directly. The
/// Metal backend runs the GPU refinement, but the first time it computes a frame in a
/// render it also runs the CPU reference and gates the result — validate-then-trust.
/// Once frame parity is confirmed (`metal_validated`) the GPU is trusted for the rest of
/// the sequence, so the expensive CPU flow runs at most once per render instead of every
/// frame.
fn compute_optical_flow_backend(
    previous_modulator: &ImageBufferF32,
    modulator: &ImageBufferF32,
    width: u32,
    height: u32,
    radius: i32,
    backend: RenderBackend,
    metal_validated: &mut bool,
) -> Result<FlowField, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(pyramidal_lucas_kanade_flow_cpu(
            previous_modulator,
            modulator,
            width,
            height,
            radius,
        )?
        .flow),
        RenderBackend::Metal => compute_optical_flow_metal(
            previous_modulator,
            modulator,
            width,
            height,
            radius,
            metal_validated,
        ),
    }
}

#[cfg(target_os = "macos")]
fn compute_optical_flow_metal(
    previous_modulator: &ImageBufferF32,
    modulator: &ImageBufferF32,
    width: u32,
    height: u32,
    radius: i32,
    metal_validated: &mut bool,
) -> Result<FlowField, CliError> {
    let gpu = morphogen_metal::pyramidal_lucas_kanade_flow_metal(
        previous_modulator,
        modulator,
        width,
        height,
        radius,
    )?;
    if !*metal_validated {
        let cpu =
            pyramidal_lucas_kanade_flow_cpu(previous_modulator, modulator, width, height, radius)?;
        let difference = max_flow_vector_difference(&gpu.flow, &cpu.flow).ok_or_else(|| {
            CliError::Message(
                "Metal and CPU optical-flow fields have mismatched dimensions; cannot verify parity"
                    .to_string(),
            )
        })?;
        if difference > OPTICAL_FLOW_METAL_PARITY_EPSILON {
            return Err(CliError::Message(format!(
                "Metal optical flow diverged from the CPU reference by {difference} px (tolerance {OPTICAL_FLOW_METAL_PARITY_EPSILON}) on the first validated frame"
            )));
        }
        *metal_validated = true;
        eprintln!(
            "validated Metal optical flow against the CPU reference (max flow difference {difference} px); trusting the GPU for the remaining frames"
        );
    }
    Ok(gpu.flow)
}

#[cfg(not(target_os = "macos"))]
fn compute_optical_flow_metal(
    _previous_modulator: &ImageBufferF32,
    _modulator: &ImageBufferF32,
    _width: u32,
    _height: u32,
    _radius: i32,
    _metal_validated: &mut bool,
) -> Result<FlowField, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

fn max_flow_vector_difference(a: &FlowField, b: &FlowField) -> Option<f32> {
    if a.width != b.width || a.height != b.height || a.vectors.len() != b.vectors.len() {
        return None;
    }
    let mut max = 0.0_f32;
    for (va, vb) in a.vectors.iter().zip(b.vectors.iter()) {
        max = max.max((va[0] - vb[0]).abs()).max((va[1] - vb[1]).abs());
    }
    Some(max)
}

/// The advection ("P-frame") step of datamosh: displace `previous_output` by
/// A's flow. Delegates to the parity-gated flow displace; this is the only
/// pixel work in the non-keyframe branch.
pub(crate) fn render_datamosh_advect_frame(
    previous_output: &ImageBufferF32,
    flow: &FlowField,
    amount: f32,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(flow_displace_cpu(previous_output, flow, amount)?),
        RenderBackend::Metal => render_datamosh_advect_frame_metal(previous_output, flow, amount),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_datamosh_advect_frame_metal(
    previous_output: &ImageBufferF32,
    flow: &FlowField,
    amount: f32,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::flow_displace_metal(previous_output, flow, amount)?;
    let cpu = flow_displace_cpu(previous_output, flow, amount)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU displace outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal datamosh render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_datamosh_advect_frame_metal(
    _previous_output: &ImageBufferF32,
    _flow: &FlowField,
    _amount: f32,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

pub(crate) struct DatamoshBitstreamRequest<'a> {
    pub(crate) input: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) fps: f64,
    pub(crate) operation: CliDatamoshBitstreamOperation,
    pub(crate) p_frame_index: u32,
    pub(crate) duplicate_count: u32,
    /// `motion-transfer` only: the carrier (Source B) whose I-frame seeds the output.
    pub(crate) carrier: Option<&'a Path>,
    /// `motion-transfer` only: leading carrier frames kept before the modulator's motion.
    pub(crate) carrier_keyframes: u32,
}

pub(crate) const DATAMOSH_BITSTREAM_PFRAME_DUP_ALGORITHM: &str =
    "datamosh_bitstream_pframe_dup_experimental_v1";
pub(crate) const DATAMOSH_BITSTREAM_REMOVE_KEYFRAME_ALGORITHM: &str =
    "datamosh_bitstream_remove_keyframe_experimental_v1";
pub(crate) const DATAMOSH_BITSTREAM_MOTION_TRANSFER_ALGORITHM: &str =
    "datamosh_bitstream_motion_transfer_experimental_v1";

#[derive(Serialize)]
struct DatamoshBitstreamSidecar {
    algorithm: String,
    /// Always false: ffmpeg's MPEG-4 codec makes this output non-reproducible.
    deterministic: bool,
    input: String,
    /// The carrier (Source B) clip for `motion-transfer`; absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    carrier: Option<String>,
    fps: f64,
    codec: String,
    operation: String,
    p_frame_index: u32,
    duplicate_count: u32,
    /// Leading carrier frames kept before the modulator's motion (`motion-transfer`).
    carrier_keyframes: u32,
    p_frames_available: u32,
    ffmpeg_version: String,
    note: String,
}

/// EXPERIMENTAL, NON-DETERMINISTIC real bitstream datamosh. Encodes `input` to a
/// P-frame-only AVI/MPEG-4 via external ffmpeg, performs explicit compressed-stream
/// surgery (`morphogen_media::duplicate_p_frame` or `remove_leading_keyframe`),
/// then decodes the mangled stream to a PNG sequence. This path lives OUTSIDE the
/// deterministic render graph by design — there is no parity gate and the output is
/// not bit-reproducible (it depends on ffmpeg's codec).
pub(crate) fn datamosh_bitstream(request: DatamoshBitstreamRequest<'_>) -> Result<(), CliError> {
    fs::create_dir_all(request.output_dir)?;
    let moshed = request.output_dir.join("moshed.avi");

    // Encode the substrate(s), perform the chunk surgery, and report how many
    // P-frames the operation had to work with. For motion-transfer the carrier
    // (Source B) seeds the I-frame and the modulator (`input`, Source A), scaled to
    // the carrier's grid, supplies the P-frame motion.
    let (moshed_bytes, p_frames_available, carrier_label) = match request.operation {
        CliDatamoshBitstreamOperation::PframeDuplicate
        | CliDatamoshBitstreamOperation::RemoveKeyframe => {
            let encoded = request.output_dir.join("encoded.avi");
            morphogen_media::encode_datamosh_avi(request.input, &encoded, request.fps)?;
            let encoded_bytes = fs::read(&encoded)?;
            let available = morphogen_media::count_p_frames(&encoded_bytes)?;
            let bytes = match request.operation {
                CliDatamoshBitstreamOperation::PframeDuplicate => {
                    morphogen_media::duplicate_p_frame(
                        &encoded_bytes,
                        request.p_frame_index,
                        request.duplicate_count,
                    )?
                }
                _ => morphogen_media::remove_leading_keyframe(&encoded_bytes)?,
            };
            let _ = fs::remove_file(&encoded);
            (bytes, available, None)
        }
        CliDatamoshBitstreamOperation::MotionTransfer => {
            let carrier = request.carrier.ok_or_else(|| {
                CliError::Message(
                    "motion-transfer requires --carrier <Source B> (the clip whose appearance is kept)"
                        .to_string(),
                )
            })?;
            let carrier_avi = request.output_dir.join("carrier.avi");
            let modulator_avi = request.output_dir.join("modulator.avi");
            morphogen_media::encode_datamosh_avi(carrier, &carrier_avi, request.fps)?;
            let carrier_bytes = fs::read(&carrier_avi)?;
            let (w, h) = morphogen_media::avi_dimensions(&carrier_bytes)?;
            // Scale the modulator to the carrier's macroblock grid before splicing.
            morphogen_media::encode_datamosh_avi_scaled(
                request.input,
                &modulator_avi,
                request.fps,
                w,
                h,
            )?;
            let modulator_bytes = fs::read(&modulator_avi)?;
            let available = morphogen_media::count_p_frames(&modulator_bytes)?;
            let bytes = morphogen_media::transfer_motion(
                &carrier_bytes,
                &modulator_bytes,
                request.carrier_keyframes,
            )?;
            let _ = fs::remove_file(&carrier_avi);
            let _ = fs::remove_file(&modulator_avi);
            (
                bytes,
                available,
                Some(carrier.to_string_lossy().to_string()),
            )
        }
    };
    fs::write(&moshed, &moshed_bytes)?;
    morphogen_media::decode_avi_frames(&moshed, request.output_dir)?;

    let algorithm = datamosh_bitstream_algorithm(request.operation);
    let operation = datamosh_bitstream_operation_name(request.operation);
    let sidecar = DatamoshBitstreamSidecar {
        algorithm: algorithm.to_string(),
        deterministic: false,
        input: request.input.to_string_lossy().to_string(),
        carrier: carrier_label,
        fps: request.fps,
        codec: "mpeg4".to_string(),
        operation: operation.to_string(),
        p_frame_index: request.p_frame_index,
        duplicate_count: request.duplicate_count,
        carrier_keyframes: request.carrier_keyframes,
        p_frames_available,
        ffmpeg_version: morphogen_media::ffmpeg_version().unwrap_or_default(),
        note: "Experimental real bitstream datamosh: output is NOT bit-reproducible \
               (depends on the external ffmpeg MPEG-4 codec) and lives outside the \
               deterministic render graph."
            .to_string(),
    };
    let sidecar_path = request.output_dir.join("datamosh_bitstream.json");
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar)?)?;

    match request.operation {
        CliDatamoshBitstreamOperation::PframeDuplicate => {
            println!(
                "datamosh-bitstream (EXPERIMENTAL, non-deterministic): bloomed P-frame {} x{} of {} P-frames -> {}",
                request.p_frame_index,
                request.duplicate_count,
                p_frames_available,
                request.output_dir.display()
            );
        }
        CliDatamoshBitstreamOperation::RemoveKeyframe => {
            println!(
                "datamosh-bitstream (EXPERIMENTAL, non-deterministic): removed leading keyframe from {} P-frame substrate -> {}",
                p_frames_available,
                request.output_dir.display()
            );
        }
        CliDatamoshBitstreamOperation::MotionTransfer => {
            println!(
                "datamosh-bitstream (EXPERIMENTAL, non-deterministic): transferred {} P-frames of modulator motion onto the carrier (kept {} carrier frame(s)) -> {}",
                p_frames_available,
                request.carrier_keyframes,
                request.output_dir.display()
            );
        }
    }
    Ok(())
}

pub(crate) fn datamosh_bitstream_algorithm(
    operation: CliDatamoshBitstreamOperation,
) -> &'static str {
    match operation {
        CliDatamoshBitstreamOperation::PframeDuplicate => DATAMOSH_BITSTREAM_PFRAME_DUP_ALGORITHM,
        CliDatamoshBitstreamOperation::RemoveKeyframe => {
            DATAMOSH_BITSTREAM_REMOVE_KEYFRAME_ALGORITHM
        }
        CliDatamoshBitstreamOperation::MotionTransfer => {
            DATAMOSH_BITSTREAM_MOTION_TRANSFER_ALGORITHM
        }
    }
}

pub(crate) fn datamosh_bitstream_operation_name(
    operation: CliDatamoshBitstreamOperation,
) -> &'static str {
    match operation {
        CliDatamoshBitstreamOperation::PframeDuplicate => "pframe_duplicate",
        CliDatamoshBitstreamOperation::RemoveKeyframe => "remove_keyframe",
        CliDatamoshBitstreamOperation::MotionTransfer => "motion_transfer",
    }
}

pub(crate) struct ConvolutionalBlendSequenceRequest<'a> {
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: ConvolutionBlendSettings,
    pub(crate) kernel_mode: KernelMode,
    pub(crate) backend: RenderBackend,
    pub(crate) max_frames: Option<usize>,
}

pub(crate) fn render_convolutional_blend_sequence(
    request: ConvolutionalBlendSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if matches!(request.max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let modulator_frames = collect_image_frames(request.modulator_dir)?;
    let carrier_frames = collect_image_frames(request.carrier_dir)?;
    if modulator_frames.is_empty() || carrier_frames.is_empty() {
        return Err(CliError::Message(
            "convolutional blend requires at least one PNG frame in each source directory"
                .to_string(),
        ));
    }

    let paired_count = modulator_frames.len().min(carrier_frames.len());
    let frame_count = request
        .max_frames
        .map(|limit| limit.min(paired_count))
        .unwrap_or(paired_count);
    fs::create_dir_all(request.output_dir)?;

    for index in 0..frame_count {
        let modulator = load_image_f32(&modulator_frames[index])?;
        let carrier = load_image_f32(&carrier_frames[index])?;
        let rendered = match request.kernel_mode {
            KernelMode::Luma => {
                let kernel =
                    analyze_convolution_kernel_cpu(&modulator, request.settings.kernel_size)?;
                render_convolutional_blend_frame(
                    &carrier,
                    &kernel,
                    request.settings.amount,
                    request.backend,
                )?
            }
            KernelMode::Color => {
                let kernels = analyze_convolution_kernels_color_cpu(
                    &modulator,
                    request.settings.kernel_size,
                )?;
                render_convolutional_blend_color_frame(
                    &carrier,
                    &kernels,
                    request.settings.amount,
                    request.backend,
                )?
            }
        };
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    if modulator_frames.len() != carrier_frames.len() {
        println!(
            "source frame counts differ: {} modulator frame(s), {} carrier frame(s); rendered common prefix",
            modulator_frames.len(),
            carrier_frames.len()
        );
    }
    println!(
        "rendered convolutional blend sequence with {} frame(s) (kernel {}, {} mode, amount {}, {:?}) from {} convolving {} to {}",
        frame_count,
        request.settings.kernel_size,
        kernel_mode_label(request.kernel_mode),
        request.settings.amount,
        request.backend,
        request.modulator_dir.display(),
        request.carrier_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) struct DispersionBlendSequenceRequest<'a> {
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: DispersionSettings,
    pub(crate) ownership_refresh: f32,
    pub(crate) dispersion_ramp: u32,
    pub(crate) smear: f32,
    pub(crate) smear_decay: f32,
    pub(crate) max_frames: Option<usize>,
}

/// Render the colour-group dispersion blend over a paired PNG sequence. Carries two
/// stateful fields frame-to-frame: the colour-grouped A/B ownership field (advected
/// by the source current) and the per-block content-offset field (which accumulates
/// the current plus a ramping random walk). Content is sampled at the displaced
/// coordinate, so tiles of both sources physically flow, shatter, and intermix.
pub(crate) fn render_dispersion_blend_sequence(
    request: DispersionBlendSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if matches!(request.max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let source_a_frames = collect_image_frames(request.source_a_dir)?;
    let source_b_frames = collect_image_frames(request.source_b_dir)?;
    if source_a_frames.is_empty() || source_b_frames.is_empty() {
        return Err(CliError::Message(
            "dispersion blend requires at least one PNG frame in each source directory".to_string(),
        ));
    }

    let paired_count = source_a_frames.len().min(source_b_frames.len());
    let frame_count = request
        .max_frames
        .map(|limit| limit.min(paired_count))
        .unwrap_or(paired_count);
    fs::create_dir_all(request.output_dir)?;

    let block = request.settings.block_size;
    let ownership_settings = request.settings.ownership_settings();
    let mut previous_ownership: Option<CoagulationField> = None;
    let mut previous_dispersion: Option<DispersionField> = None;
    let mut previous_a: Option<ImageBufferF32> = None;
    let mut previous_output: Option<ImageBufferF32> = None;

    for index in 0..frame_count {
        let source_a = load_image_f32(&source_a_frames[index])?;
        let source_b = load_image_f32(&source_b_frames[index])?;
        let cols = source_a.width.div_ceil(block);
        let rows = source_a.height.div_ceil(block);

        // The directional current = Source A's optical flow, downsampled to the tile
        // grid (cell units). Drives both the ownership advection and the content offset.
        let cell_flow = if index == 0 {
            None
        } else {
            let previous = previous_a.as_ref().ok_or_else(|| {
                CliError::Message(
                    "internal error: missing previous frame for dispersion flow".to_string(),
                )
            })?;
            let flow = pyramidal_lucas_kanade_flow_cpu(
                previous,
                &source_a,
                source_a.width,
                source_a.height,
                LUCAS_KANADE_WINDOW_RADIUS,
            )?
            .flow;
            Some(downsample_flow_to_cells(&flow, block)?)
        };

        let ownership = advance_coagulation_field(
            &source_a,
            &source_b,
            cell_flow.as_ref(),
            previous_ownership.as_ref(),
            ownership_settings,
            request.settings.coherent_amount,
            request.ownership_refresh,
        )?;

        let dispersion = if request.dispersion_ramp == 0 {
            1.0
        } else {
            (index as f32 / request.dispersion_ramp as f32).min(1.0)
        };
        let dispersion_field = advance_dispersion_field(
            previous_dispersion.as_ref(),
            cell_flow.as_ref(),
            cols,
            rows,
            dispersion,
            request.settings,
            index as u32,
        )?;

        let composite =
            disperse_composite_cpu(&source_a, &source_b, &ownership, &dispersion_field, block)?;
        // Optional directional smear: hold a decayed fraction of the previous output,
        // leaving streaks as tiles flow (RGB only; alpha from the composite).
        let rendered = if request.smear != 0.0 {
            let smeared = apply_history_smear(
                &composite,
                previous_output.as_ref(),
                request.smear,
                request.smear_decay,
            )?;
            previous_output = Some(smeared.clone());
            smeared
        } else {
            composite
        };
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;

        previous_ownership = Some(ownership);
        previous_dispersion = Some(dispersion_field);
        previous_a = Some(source_a);
    }

    if source_a_frames.len() != source_b_frames.len() {
        println!(
            "source frame counts differ: {} A frame(s), {} B frame(s); rendered common prefix",
            source_a_frames.len(),
            source_b_frames.len()
        );
    }
    println!(
        "rendered dispersion blend sequence with {} frame(s) (block {}, coherent {}, scatter {}, damping {}, ramp {}) from {} dispersed with {} to {}",
        frame_count,
        block,
        request.settings.coherent_amount,
        request.settings.scatter_amount,
        request.settings.damping,
        request.dispersion_ramp,
        request.source_a_dir.display(),
        request.source_b_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) struct FluidAdvectSequenceRequest<'a> {
    pub(crate) source_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: FluidAdvectSettings,
    pub(crate) frames: usize,
    pub(crate) backend: RenderBackend,
}

/// Render the faux-fluid dye advection. The source is treated as a continuous dye:
/// frame zero is the source verbatim, then each frame the held dye (RGBA32F in memory —
/// the stateful internal buffer, never a re-read PNG) is advected along a procedural
/// turbulence field and the current source frame is bled back in. Source frames cycle
/// if the render outlasts the clip.
pub(crate) fn render_fluid_advect_sequence(
    request: FluidAdvectSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let source_frames = collect_image_frames(request.source_dir)?;
    if source_frames.is_empty() {
        return Err(CliError::Message(
            "fluid advect requires at least one PNG frame in the source directory".to_string(),
        ));
    }

    fs::create_dir_all(request.output_dir)?;

    let mut previous_output: Option<ImageBufferF32> = None;
    for index in 0..request.frames {
        let source = load_image_f32(&source_frames[index % source_frames.len()])?;
        let rendered = render_fluid_advect_frame(
            &source,
            previous_output.as_ref(),
            index as u32,
            request.settings,
            request.backend,
        )?;
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
        previous_output = Some(rendered);
    }

    println!(
        "rendered fluid advect sequence with {} frame(s) (advect {}, turbulence-scale {}, turbulence-speed {}, reinject {}, {:?}) from {} to {}",
        request.frames,
        request.settings.advect,
        request.settings.turbulence_scale,
        request.settings.turbulence_speed,
        request.settings.reinject,
        request.backend,
        request.source_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult {
        frame_count: request.frames,
    })
}

pub(crate) struct FluidAdvectTwoSourceSequenceRequest<'a> {
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: FluidAdvectTwoSourceSettings,
    pub(crate) frames: usize,
    pub(crate) backend: RenderBackend,
}

/// Render the mutual two-source faux-fluid advection: Source A (modulator) supplies the
/// optical-flow motion that advects Source B (carrier) as a continuous dye. Frame zero is B
/// verbatim (no prior A frame to derive motion from); thereafter A's Lucas-Kanade flow between
/// consecutive A frames advects the held dye (RGBA32F state — never a re-read PNG) and a little
/// of the current B frame is bled back in. Bounded by the shorter of the two clips (no cyclic
/// wrap, so the flow never jumps across a clip boundary).
pub(crate) fn render_fluid_advect_two_source_sequence(
    request: FluidAdvectTwoSourceSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let a_frames = collect_image_frames(request.source_a_dir)?;
    let b_frames = collect_image_frames(request.source_b_dir)?;
    if a_frames.is_empty() || b_frames.is_empty() {
        return Err(CliError::Message(
            "two-source fluid advect requires at least one PNG frame in both the A and B directories"
                .to_string(),
        ));
    }
    let available = a_frames.len().min(b_frames.len());
    let frame_count = request.frames.min(available);

    fs::create_dir_all(request.output_dir)?;

    let mut previous_output: Option<ImageBufferF32> = None;
    let mut previous_a: Option<ImageBufferF32> = None;
    for index in 0..frame_count {
        let carrier_b = load_image_f32(&b_frames[index])?;
        let modulator_a = load_image_f32(&a_frames[index])?;

        let rendered = match (previous_output.as_ref(), previous_a.as_ref()) {
            (Some(previous), Some(previous_a)) => {
                // A's per-frame motion, sized to B's dimensions and in B's pixel units.
                let flow = pyramidal_lucas_kanade_flow_cpu(
                    previous_a,
                    &modulator_a,
                    carrier_b.width,
                    carrier_b.height,
                    LUCAS_KANADE_WINDOW_RADIUS,
                )?
                .flow;
                render_two_source_advect_frame(
                    &carrier_b,
                    previous,
                    &flow,
                    request.settings,
                    request.backend,
                )?
            }
            // Frame zero (or a missing prior A frame): B is the output verbatim.
            _ => carrier_b.clone(),
        };

        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
        previous_output = Some(rendered);
        previous_a = Some(modulator_a);
    }

    println!(
        "rendered two-source fluid advect sequence with {} frame(s) (advect {}, reinject {}, {:?}) from A {} advecting B {} to {}",
        frame_count,
        request.settings.advect,
        request.settings.reinject,
        request.backend,
        request.source_a_dir.display(),
        request.source_b_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

/// Advance the two-source dye one frame on the chosen backend (the per-frame core shared by the
/// two-source and single-source optical-flow sequences). `previous` is always present here —
/// frame zero (B verbatim) is handled by the callers. CPU runs the reference; Metal runs the
/// parity-gated kernel and is checked against the CPU per frame.
pub(crate) fn render_two_source_advect_frame(
    carrier_b: &ImageBufferF32,
    previous: &ImageBufferF32,
    flow: &FlowField,
    settings: FluidAdvectTwoSourceSettings,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(fluid_advect_two_source_frame_cpu(
            carrier_b,
            Some(previous),
            flow,
            settings,
        )?),
        RenderBackend::Metal => {
            render_two_source_advect_frame_metal(carrier_b, previous, flow, settings)
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_two_source_advect_frame_metal(
    carrier_b: &ImageBufferF32,
    previous: &ImageBufferF32,
    flow: &FlowField,
    settings: FluidAdvectTwoSourceSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::fluid_advect_two_source_metal(carrier_b, previous, flow, settings)?;
    let cpu = fluid_advect_two_source_frame_cpu(carrier_b, Some(previous), flow, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU two-source fluid advect outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal two-source fluid advect render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_two_source_advect_frame_metal(
    _carrier_b: &ImageBufferF32,
    _previous: &ImageBufferF32,
    _flow: &FlowField,
    _settings: FluidAdvectTwoSourceSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

pub(crate) struct OpticalFlowAdvectSequenceRequest<'a> {
    pub(crate) source_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: FluidAdvectTwoSourceSettings,
    pub(crate) frames: usize,
    pub(crate) backend: RenderBackend,
}

/// Render the single-source optical-flow-driven advection: the video is advected by its own
/// motion. The self-driven case of the two-source advection — the source is both the
/// modulator (its Lucas-Kanade flow between consecutive frames) and the carrier (the dye and
/// the reinjected frame) — so it reuses `fluid_advect_two_source_frame_cpu`. Frame zero is the
/// source verbatim (no prior frame to derive motion from); thereafter the held dye (RGBA32F
/// state) flows along the source's measured motion. Bounded by the available source frames.
pub(crate) fn render_optical_flow_advect_sequence(
    request: OpticalFlowAdvectSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let source_frames = collect_image_frames(request.source_dir)?;
    if source_frames.is_empty() {
        return Err(CliError::Message(
            "optical-flow advect requires at least one PNG frame in the source directory"
                .to_string(),
        ));
    }
    let frame_count = request.frames.min(source_frames.len());

    fs::create_dir_all(request.output_dir)?;

    let mut previous_output: Option<ImageBufferF32> = None;
    let mut previous_source: Option<ImageBufferF32> = None;
    for (index, source_path) in source_frames.iter().enumerate().take(frame_count) {
        let source = load_image_f32(source_path)?;

        let rendered = match (previous_output.as_ref(), previous_source.as_ref()) {
            (Some(previous), Some(previous_source)) => {
                // The source's own per-frame motion drives the field that advects its dye.
                let flow = pyramidal_lucas_kanade_flow_cpu(
                    previous_source,
                    &source,
                    source.width,
                    source.height,
                    LUCAS_KANADE_WINDOW_RADIUS,
                )?
                .flow;
                render_two_source_advect_frame(
                    &source,
                    previous,
                    &flow,
                    request.settings,
                    request.backend,
                )?
            }
            // Frame zero (or a missing prior frame): the source is the output verbatim.
            _ => source.clone(),
        };

        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
        previous_output = Some(rendered);
        previous_source = Some(source);
    }

    println!(
        "rendered optical-flow advect sequence with {} frame(s) (advect {}, reinject {}, {:?}) from {} to {}",
        frame_count,
        request.settings.advect,
        request.settings.reinject,
        request.backend,
        request.source_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) struct FieldParticlesSequenceRequest<'a> {
    pub(crate) source_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: FieldParticleSettings,
    pub(crate) frames: usize,
    pub(crate) backend: RenderBackend,
}

/// Render the discrete-carrier particle advection: a grid of coloured particles seeded from
/// the source's first frame rides the shared steady-vortex field. Frame zero is the initial
/// grid (a posterised source); each later frame integrates the particle positions along the
/// field and splats them onto a black canvas. The particle state (positions + colours) is the
/// stateful carrier, carried in memory (never a re-read PNG). With live colour disabled,
/// colours are fixed at seed time; enabled live colour re-samples each particle's source cell.
pub(crate) fn render_field_particles_sequence(
    request: FieldParticlesSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let source_frames = collect_image_frames(request.source_dir)?;
    if source_frames.is_empty() {
        return Err(CliError::Message(
            "field particles requires at least one PNG frame in the source directory".to_string(),
        ));
    }

    let seed_frame = load_image_f32(&source_frames[0])?;
    fs::create_dir_all(request.output_dir)?;

    let mut field = initialize_field_particles(&seed_frame, request.settings)?;
    for index in 0..request.frames {
        if index > 0 {
            advance_field_particles(&mut field, index as u32, request.settings)?;
        }
        // Live colour: each particle re-samples its origin cell from the current source frame
        // (frames cycle if the render outlasts the clip) so the video plays through the flow.
        if request.settings.live_color {
            let current = load_image_f32(&source_frames[index % source_frames.len()])?;
            refresh_field_particle_colors(&mut field, &current);
        }
        let rendered = render_field_particles_frame(&field, request.settings, request.backend)?;
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    println!(
        "rendered field particles sequence with {} frame(s) ({} particles, spacing {}, size {}, advect {}, {:?}) from {} to {}",
        request.frames,
        field.particle_count(),
        request.settings.spacing,
        request.settings.particle_size,
        request.settings.advect,
        request.backend,
        request.source_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult {
        frame_count: request.frames,
    })
}

/// Splat the particle carrier on the chosen backend. CPU runs the reference scatter; Metal runs
/// the parity-gated gather kernel and is checked against the CPU per frame.
pub(crate) fn render_field_particles_frame(
    field: &ParticleField,
    settings: FieldParticleSettings,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(render_field_particles(field, settings)?),
        RenderBackend::Metal => render_field_particles_frame_metal(field, settings),
    }
}

pub(crate) struct CascadeTrailsSequenceRequest<'a> {
    pub(crate) source_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: CascadeTrailSettings,
    pub(crate) frames: usize,
}

/// Render the persistent-trail vector-field cascade: a grid of source-image tiles is advected
/// along the shared steady-vortex field and stamped every frame onto a never-cleared canvas, so
/// the image smears into ribbons that trace the streamlines. The cascade state (tile positions +
/// patches + the accumulator) is the stateful carrier, carried in memory. With `live_refresh`,
/// each tile's patch is re-sampled from its origin cell in the current source frame. CPU-only.
pub(crate) fn render_cascade_trails_sequence(
    request: CascadeTrailsSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let source_frames = collect_image_frames(request.source_dir)?;
    if source_frames.is_empty() {
        return Err(CliError::Message(
            "cascade trails requires at least one PNG frame in the source directory".to_string(),
        ));
    }

    let seed_frame = load_image_f32(&source_frames[0])?;
    fs::create_dir_all(request.output_dir)?;

    let mut state = initialize_cascade_trails(&seed_frame, request.settings)?;

    if request.settings.temporal_tiles {
        // Load all frames upfront and spread them across tiles — each tile captures a distinct
        // temporal slice of the clip and holds it frozen for the entire render.
        let all_frames: Vec<_> = source_frames
            .iter()
            .map(|p| load_image_f32(p))
            .collect::<Result<Vec<_>, _>>()?;
        assign_temporal_patches(&mut state, &all_frames);
    }

    for index in 0..request.frames {
        if index > 0 {
            // With temporal_tiles the patches are frozen — advance positions only (live_refresh
            // is implicitly off).  Without it, live_refresh is controlled by the setting.
            let current = load_image_f32(&source_frames[index % source_frames.len()])?;
            advance_cascade_trails(&mut state, &current, request.settings, index as u32)?;
        }
        let rendered = render_cascade_trails(&state);
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    let (width, height) = state.dimensions();
    println!(
        "rendered cascade trails sequence with {} frame(s) ({} tiles, tile {}, spacing {}, advect {}) {}x{} from {} to {}",
        request.frames,
        state.tile_count(),
        request.settings.tile_size,
        request.settings.grid_spacing,
        request.settings.advect,
        width,
        height,
        request.source_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult {
        frame_count: request.frames,
    })
}

pub(crate) struct CascadeCollageSequenceRequest<'a> {
    pub(crate) source_dir: Option<&'a Path>,
    pub(crate) output_dir: &'a Path,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) frames: u32,
    pub(crate) settings: CascadeCollageSettings,
}

/// Apply the high-level generative knobs to the default composition: tile size
/// (`tile_scale`), tile amount (`detail_tiles` extra tiles on top of the 4 coverage
/// tiles), and overall colour (`hue_rotate` shifts every tile's hue).
pub(crate) fn apply_cascade_generative(
    settings: &mut CascadeCollageSettings,
    tile_scale: f32,
    detail_tiles: u32,
    hue_rotate: f32,
) {
    let base = 4usize.min(settings.shapes.len());
    let keep = (base + detail_tiles as usize).min(settings.shapes.len());
    settings.shapes.truncate(keep);
    for s in &mut settings.shapes {
        s.hw *= tile_scale;
        s.hh *= tile_scale;
        s.base_hue = (s.base_hue + hue_rotate).rem_euclid(1.0);
        s.edge_hue = (s.edge_hue + hue_rotate).rem_euclid(1.0);
    }
}

pub(crate) fn render_cascade_collage_sequence(
    request: CascadeCollageSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }
    fs::create_dir_all(request.output_dir)?;

    let frame_count = if let Some(src_dir) = request.source_dir {
        // texture mode: tiles carry crops of the source video (texture + colour)
        let source_frames = collect_image_frames(src_dir)?;
        if source_frames.is_empty() {
            return Err(CliError::Message(
                "cascade collage source directory contains no PNG frames".to_string(),
            ));
        }
        let count = (request.frames as usize).min(source_frames.len());
        for (index, frame_path) in source_frames.iter().enumerate().take(count) {
            let source = load_image_f32(frame_path)?;
            let rendered = render_cascade_collage_frame(
                source.width,
                source.height,
                Some(&source),
                &request.settings,
                index as u32,
            )?;
            save_png(
                &rendered,
                &request.output_dir.join(format!("frame_{index:06}.png")),
            )?;
        }
        println!(
            "rendered cascade collage sequence with {} frame(s) (texture from {}, scrib_amp_scale {:.2}, morph_rate {:.3}) to {}",
            count,
            src_dir.display(),
            request.settings.scrib_amp_scale,
            request.settings.morph_rate,
            request.output_dir.display()
        );
        count
    } else {
        // palette mode: source-less procedural generator
        if request.width == 0 || request.height == 0 {
            return Err(CliError::Message(
                "width and height must be greater than zero".to_string(),
            ));
        }
        for index in 0..request.frames {
            let rendered = render_cascade_collage_frame(
                request.width,
                request.height,
                None,
                &request.settings,
                index,
            )?;
            save_png(
                &rendered,
                &request.output_dir.join(format!("frame_{index:06}.png")),
            )?;
        }
        println!(
            "rendered cascade collage sequence with {} frame(s) ({}x{} palette, scrib_amp_scale {:.2}, morph_rate {:.3}, frame_hue_rate {:.3}) to {}",
            request.frames,
            request.width,
            request.height,
            request.settings.scrib_amp_scale,
            request.settings.morph_rate,
            request.settings.frame_hue_rate,
            request.output_dir.display()
        );
        request.frames as usize
    };

    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) struct BlockCollageSequenceRequest<'a> {
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: BlockCollageSettings,
    pub(crate) frames: u32,
}

pub(crate) fn render_block_collage_sequence(
    request: BlockCollageSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let source_a_frames = collect_image_frames(request.source_a_dir)?;
    let source_b_frames = collect_image_frames(request.source_b_dir)?;
    if source_a_frames.is_empty() || source_b_frames.is_empty() {
        return Err(CliError::Message(
            "block collage requires at least one PNG frame in each source directory".to_string(),
        ));
    }

    let paired_count = source_a_frames.len().min(source_b_frames.len());
    let frame_count = (request.frames as usize).min(paired_count);
    fs::create_dir_all(request.output_dir)?;

    for index in 0..frame_count {
        let source_a = load_image_f32(&source_a_frames[index])?;
        let source_b = load_image_f32(&source_b_frames[index])?;
        let rendered =
            render_block_collage_frame(&source_a, &source_b, &request.settings, index as u32)?;
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    if source_a_frames.len() != source_b_frames.len() {
        println!(
            "source frame counts differ: {} A, {} B; rendered common prefix",
            source_a_frames.len(),
            source_b_frames.len()
        );
    }
    println!(
        "rendered block collage sequence with {} frame(s) (tile {}, threshold {:.2}, cluster_scale {:.3}) from A:{} + B:{} to {}",
        frame_count,
        request.settings.tile_size,
        request.settings.threshold,
        request.settings.cluster_scale,
        request.source_a_dir.display(),
        request.source_b_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) struct PixelSortSequenceRequest<'a> {
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: PixelSortSettings,
    pub(crate) frames: u32,
    pub(crate) backend: RenderBackend,
    /// LK window radius for a-flow mask mode.
    pub(crate) flow_radius: i32,
    pub(crate) modulation: ModulationCliArgs<'a>,
}

pub(crate) fn render_pixel_sort_sequence(
    request: PixelSortSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let mask_source = request.settings.mask_source;
    if mask_source != MaskSource::SelfMask && request.backend == RenderBackend::Metal {
        return Err(CliError::Message(
            "cross-synth mask modes (a-luma, a-edge, a-flow) are CPU-only; use --backend cpu"
                .to_string(),
        ));
    }

    let source_a_frames = collect_image_frames(request.source_a_dir)?;
    let source_b_frames = collect_image_frames(request.source_b_dir)?;
    if source_b_frames.is_empty() {
        return Err(CliError::Message(
            "pixel sort requires at least one PNG frame in the source B directory".to_string(),
        ));
    }
    if mask_source.needs_source_a() && source_a_frames.is_empty() {
        return Err(CliError::Message(format!(
            "mask-source {:?} requires source A frames; source-a-dir has no PNG files",
            mask_source
        )));
    }

    let b_count = source_b_frames.len();
    let frame_count = (request.frames as usize).min(b_count);
    fs::create_dir_all(request.output_dir)?;

    let modulation = request.modulation.build_plan()?;
    if let Some(plan) = &modulation {
        // Dry-run at frame 0 so an unknown target fails before any frame renders.
        let mut probe = request.settings;
        for (target, value) in plan.frame_values(0) {
            apply_pixel_sort_modulation(&mut probe, target, value)?;
        }
        println!("modulation routes: {}", plan.describe());
    }

    let mut prev_a: Option<ImageBufferF32> = None;
    let mut metal_flow_validated = false;

    for (index, frame_path) in source_b_frames.iter().enumerate().take(frame_count) {
        let mut frame_settings = request.settings;
        if let Some(plan) = &modulation {
            for (target, value) in plan.frame_values(index) {
                apply_pixel_sort_modulation(&mut frame_settings, target, value)?;
            }
        }
        let source_b = load_image_f32(frame_path)?;
        let bw = source_b.width;
        let bh = source_b.height;

        // Compute the per-pixel mask for cross-synth modes.
        let a_mask: Vec<f32> = match mask_source {
            MaskSource::SelfMask => vec![],
            MaskSource::ALuma | MaskSource::AEdge => {
                let a_idx = index % source_a_frames.len();
                let source_a = load_image_f32(&source_a_frames[a_idx])?;
                match mask_source {
                    MaskSource::ALuma => compute_a_luma_mask(&source_a, bw, bh),
                    _ => compute_a_edge_mask(&source_a, bw, bh),
                }
            }
            MaskSource::AFlow => {
                let a_idx = index % source_a_frames.len();
                let source_a = load_image_f32(&source_a_frames[a_idx])?;
                if let Some(pa) = &prev_a {
                    let flow = compute_optical_flow_backend(
                        pa,
                        &source_a,
                        source_a.width,
                        source_a.height,
                        request.flow_radius,
                        RenderBackend::Cpu,
                        &mut metal_flow_validated,
                    )?;
                    let mask = compute_a_flow_mask(&flow, bw, bh);
                    prev_a = Some(source_a);
                    mask
                } else {
                    // Frame 0: no prev → zero magnitudes → nothing sortable (passthrough)
                    prev_a = Some(source_a);
                    vec![0.0f32; (bw * bh) as usize]
                }
            }
        };

        // For non-flow modes update prev_a is not needed; for ALuma/AEdge prev_a stays None.
        let rendered = match request.backend {
            RenderBackend::Cpu => render_pixel_sort_frame(&source_b, &frame_settings, &a_mask)?,
            RenderBackend::Metal => render_pixel_sort_frame_metal(&source_b, &frame_settings)?,
        };
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    let algo = if mask_source == MaskSource::SelfMask {
        "self"
    } else {
        PIXEL_SORT_CROSS_SYNTH_ALGORITHM
    };
    println!(
        "rendered pixel sort sequence with {} frame(s) \
         (axis {:?}, key {:?}, mask {:?}, threshold [{:.2},{:.2}], max_span {}, algo {}, backend {:?}) \
         from B:{} to {}",
        frame_count,
        request.settings.axis,
        request.settings.key,
        mask_source,
        request.settings.threshold_low,
        request.settings.threshold_high,
        request.settings.max_span,
        algo,
        request.backend,
        request.source_b_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

#[cfg(target_os = "macos")]
pub(crate) fn render_pixel_sort_frame_metal(
    source: &ImageBufferF32,
    settings: &PixelSortSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::pixel_sort_metal(source, settings)?;
    let cpu = render_pixel_sort_frame(source, settings, &[])?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU pixel sort outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal pixel sort diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_pixel_sort_frame_metal(
    _source: &ImageBufferF32,
    _settings: &PixelSortSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

#[cfg(target_os = "macos")]
pub(crate) fn render_field_particles_frame_metal(
    field: &ParticleField,
    settings: FieldParticleSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::field_particles_splat_metal(field, settings)?;
    let cpu = render_field_particles(field, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU field particle outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal field particles render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_field_particles_frame_metal(
    _field: &ParticleField,
    _settings: FieldParticleSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

/// Advance the dye one frame on the chosen backend. Frame zero (`previous == None`) is the
/// source verbatim on either backend; otherwise CPU runs the reference and Metal runs the
/// parity-gated kernel.
pub(crate) fn render_fluid_advect_frame(
    source: &ImageBufferF32,
    previous: Option<&ImageBufferF32>,
    frame_index: u32,
    settings: FluidAdvectSettings,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(fluid_advect_frame_cpu(
            source,
            previous,
            frame_index,
            settings,
        )?),
        RenderBackend::Metal => {
            render_fluid_advect_frame_metal(source, previous, frame_index, settings)
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_fluid_advect_frame_metal(
    source: &ImageBufferF32,
    previous: Option<&ImageBufferF32>,
    frame_index: u32,
    settings: FluidAdvectSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::fluid_advect_metal(source, previous, frame_index, settings)?;
    let cpu = fluid_advect_frame_cpu(source, previous, frame_index, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU fluid advect outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal fluid advect render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_fluid_advect_frame_metal(
    _source: &ImageBufferF32,
    _previous: Option<&ImageBufferF32>,
    _frame_index: u32,
    _settings: FluidAdvectSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

pub(crate) struct FluidMosaicSequenceRequest<'a> {
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: FluidMosaicSettings,
    pub(crate) frames: usize,
}

/// Render the fluid colour-sort mosaic. Tiles of both sources are seeded from each
/// source's first frame, settled into colour groups, then advected by a fluid field
/// frame-to-frame so the grouped colours flow and intermix. With `--live-refresh` each
/// tile re-samples its painted colour/patch from the current source frame so the videos
/// play through the mosaic (render-only); otherwise it is a self-contained particle
/// simulation seeded from each source's first frame.
pub(crate) fn render_fluid_mosaic_sequence(
    request: FluidMosaicSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let source_a_frames = collect_image_frames(request.source_a_dir)?;
    let source_b_frames = collect_image_frames(request.source_b_dir)?;
    if source_a_frames.is_empty() || source_b_frames.is_empty() {
        return Err(CliError::Message(
            "fluid mosaic requires at least one PNG frame in each source directory".to_string(),
        ));
    }

    let source_a = load_image_f32(&source_a_frames[0])?;
    let source_b = load_image_f32(&source_b_frames[0])?;
    fs::create_dir_all(request.output_dir)?;

    let mut state = initialize_fluid_mosaic(&source_a, &source_b, request.settings)?;
    for index in 0..request.frames {
        if index > 0 {
            state = advance_fluid_mosaic(&state, request.settings, index as u32)?;
        }
        // Live colour refresh: re-sample each tile's painted colour/patch from the
        // current source frame so the videos play through the mosaic. Frame 0 already
        // carries the seed colours; later frames cycle if the render outlasts a clip.
        // With --live-resort the re-sample also re-bins each tile, so the cohesion force
        // follows the live colour and domains migrate to track the video (sim-driving);
        // plain --live-refresh leaves the bins frozen (render-only).
        if (request.settings.live_refresh || request.settings.live_resort) && index > 0 {
            let frame_a = load_image_f32(&source_a_frames[index % source_a_frames.len()])?;
            let frame_b = load_image_f32(&source_b_frames[index % source_b_frames.len()])?;
            if request.settings.live_resort {
                resort_fluid_mosaic_colors(&mut state, &frame_a, &frame_b)?;
            } else {
                refresh_fluid_mosaic_colors(&mut state, &frame_a, &frame_b)?;
            }
        }
        let frame = render_fluid_mosaic(&state, request.settings)?;
        save_png(
            &frame,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    println!(
        "rendered fluid mosaic sequence with {} frame(s) (tile {}, bins {}, cohesion {}, repulsion {}, fluid {}, settle {}) seeded from {} + {} to {}",
        request.frames,
        request.settings.tile_size,
        request.settings.color_bins,
        request.settings.cohesion,
        request.settings.repulsion,
        request.settings.fluid_strength,
        request.settings.settle_iterations,
        request.source_a_dir.display(),
        request.source_b_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult {
        frame_count: request.frames,
    })
}

pub(crate) struct CoagulatedBlendSequenceRequest<'a> {
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: CoagulationSettings,
    pub(crate) flow_source: CoagulationFlowSource,
    pub(crate) advect_amount: f32,
    pub(crate) refresh: f32,
    pub(crate) turbulence: f32,
    pub(crate) smear: f32,
    pub(crate) smear_decay: f32,
    pub(crate) backend: RenderBackend,
    pub(crate) max_frames: Option<usize>,
}

/// Render the descriptor-coagulated flow blend over a paired PNG sequence. Slice 1
/// (stateless) when `advect_amount == 0` and `refresh == 1` — each frame is the
/// per-cell descriptor blend in isolation. Otherwise Slice 2 (temporal/stateful):
/// the A/B ownership field is carried frame-to-frame, advected by the chosen flow,
/// and re-seeded from fresh descriptors by `refresh`, so coagulated patches drift,
/// smear, and collide over time.
pub(crate) fn render_coagulated_blend_sequence(
    request: CoagulatedBlendSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if matches!(request.max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let source_a_frames = collect_image_frames(request.source_a_dir)?;
    let source_b_frames = collect_image_frames(request.source_b_dir)?;
    if source_a_frames.is_empty() || source_b_frames.is_empty() {
        return Err(CliError::Message(
            "coagulated blend requires at least one PNG frame in each source directory".to_string(),
        ));
    }

    let paired_count = source_a_frames.len().min(source_b_frames.len());
    let frame_count = request
        .max_frames
        .map(|limit| limit.min(paired_count))
        .unwrap_or(paired_count);
    fs::create_dir_all(request.output_dir)?;

    // Stateless when advection is off, the field re-seeds fully every frame, and
    // there is no history smear; this keeps the Slice-1 path byte-identical to its
    // dedicated frame function.
    let temporal = request.advect_amount != 0.0 || request.refresh != 1.0 || request.smear != 0.0;
    let patch = request.settings.patch_size;

    let mut previous_field: Option<CoagulationField> = None;
    let mut previous_output: Option<ImageBufferF32> = None;
    let mut previous_a: Option<ImageBufferF32> = None;
    let mut previous_b: Option<ImageBufferF32> = None;

    for index in 0..frame_count {
        let source_a = load_image_f32(&source_a_frames[index])?;
        let source_b = load_image_f32(&source_b_frames[index])?;

        // Build (or advance) the ownership field on the CPU, then composite it on
        // the selected backend.
        let field = if !temporal {
            coagulation_field(&source_a, &source_b, request.settings)?
        } else {
            let cols = source_a.width.div_ceil(patch);
            let rows = source_a.height.div_ceil(patch);
            // Skip flow estimation entirely when advection is off (e.g. smear-only).
            let cell_flow = if index == 0 || request.advect_amount == 0.0 {
                None
            } else {
                Some(coagulation_cell_flow(
                    request.flow_source,
                    previous_a.as_ref(),
                    &source_a,
                    previous_b.as_ref(),
                    &source_b,
                    patch,
                    cols,
                    rows,
                    index as u32,
                    request.turbulence,
                    request.settings.seed,
                )?)
            };
            advance_coagulation_field(
                &source_a,
                &source_b,
                cell_flow.as_ref(),
                previous_field.as_ref(),
                request.settings,
                request.advect_amount,
                request.refresh,
            )?
        };

        let composite = render_coagulated_composite_frame(
            &source_a,
            &source_b,
            &field,
            request.settings,
            request.backend,
        )?;
        if temporal {
            previous_field = Some(field);
        }

        // Output feedback smear: hold a decayed fraction of the previous output into
        // this frame, leaving trails as patches move (RGB only; alpha stays from the
        // composite). smear == 0 ⇒ the composite unchanged.
        let rendered = if request.smear != 0.0 {
            let smeared = apply_history_smear(
                &composite,
                previous_output.as_ref(),
                request.smear,
                request.smear_decay,
            )?;
            previous_output = Some(smeared.clone());
            smeared
        } else {
            composite
        };

        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
        previous_a = Some(source_a);
        previous_b = Some(source_b);
    }

    if source_a_frames.len() != source_b_frames.len() {
        println!(
            "source frame counts differ: {} A frame(s), {} B frame(s); rendered common prefix",
            source_a_frames.len(),
            source_b_frames.len()
        );
    }
    println!(
        "rendered coagulated blend sequence with {} frame(s) (patch {}, strength {}, coherence {}x{}, edge {}; {}; {:?}) from {} blended with {} to {}",
        frame_count,
        request.settings.patch_size,
        request.settings.coagulation_strength,
        request.settings.coherence_passes,
        request.settings.coherence_strength,
        request.settings.edge_hardness,
        if temporal {
            format!(
                "temporal: {:?} flow, advect {}, refresh {}",
                request.flow_source, request.advect_amount, request.refresh
            )
        } else {
            "stateless (Slice 1)".to_string()
        },
        request.backend,
        request.source_a_dir.display(),
        request.source_b_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

/// Build the cell-resolution advection flow for one temporal frame from the chosen
/// source. Optical-flow sources estimate Lucas-Kanade motion between consecutive
/// source frames and downsample it to the cell grid; turbulence is synthesized.
#[allow(clippy::too_many_arguments)]
fn coagulation_cell_flow(
    source: CoagulationFlowSource,
    previous_a: Option<&ImageBufferF32>,
    source_a: &ImageBufferF32,
    previous_b: Option<&ImageBufferF32>,
    source_b: &ImageBufferF32,
    patch: u32,
    cols: u32,
    rows: u32,
    frame_index: u32,
    turbulence: f32,
    seed: u64,
) -> Result<FlowField, CliError> {
    let cell_flow_between = |previous: Option<&ImageBufferF32>,
                             current: &ImageBufferF32|
     -> Result<FlowField, CliError> {
        let previous = previous.ok_or_else(|| {
            CliError::Message(
                "internal error: missing previous frame for coagulation flow".to_string(),
            )
        })?;
        let flow = pyramidal_lucas_kanade_flow_cpu(
            previous,
            current,
            current.width,
            current.height,
            LUCAS_KANADE_WINDOW_RADIUS,
        )?
        .flow;
        Ok(downsample_flow_to_cells(&flow, patch)?)
    };

    Ok(match source {
        CoagulationFlowSource::AFlow => cell_flow_between(previous_a, source_a)?,
        CoagulationFlowSource::BFlow => cell_flow_between(previous_b, source_b)?,
        CoagulationFlowSource::Mixed => {
            let a = cell_flow_between(previous_a, source_a)?;
            let b = cell_flow_between(previous_b, source_b)?;
            average_cell_flows(&a, &b)?
        }
        CoagulationFlowSource::Turbulence => {
            synthesize_turbulence_flow(cols, rows, frame_index, turbulence, seed)?
        }
    })
}

/// Composite one coagulated-blend frame from a prebuilt ownership field on the
/// selected backend. The Metal path is gated against the CPU `composite_with_field`
/// reference per frame.
fn render_coagulated_composite_frame(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    field: &CoagulationField,
    settings: CoagulationSettings,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(composite_with_field(source_a, source_b, field, settings)?),
        RenderBackend::Metal => {
            render_coagulated_composite_frame_metal(source_a, source_b, field, settings)
        }
    }
}

#[cfg(target_os = "macos")]
fn render_coagulated_composite_frame_metal(
    source_a: &ImageBufferF32,
    source_b: &ImageBufferF32,
    field: &CoagulationField,
    settings: CoagulationSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::coagulated_composite_metal(
        source_a,
        source_b,
        &field.weights,
        field.cols,
        field.rows,
        field.patch_size,
        settings.edge_hardness,
        settings.edge_dither,
        settings.block_jitter,
        settings.seed,
    )?;
    let cpu = composite_with_field(source_a, source_b, field, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU coagulated-composite outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal coagulated-composite render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
fn render_coagulated_composite_frame_metal(
    _source_a: &ImageBufferF32,
    _source_b: &ImageBufferF32,
    _field: &CoagulationField,
    _settings: CoagulationSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

pub(crate) fn render_convolutional_blend_frame(
    carrier: &ImageBufferF32,
    kernel: &ConvolutionKernel,
    amount: f32,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(convolution_blend_cpu(carrier, kernel, amount)?),
        RenderBackend::Metal => render_convolutional_blend_frame_metal(carrier, kernel, amount),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_convolutional_blend_frame_metal(
    carrier: &ImageBufferF32,
    kernel: &ConvolutionKernel,
    amount: f32,
) -> Result<ImageBufferF32, CliError> {
    let gpu =
        morphogen_metal::convolution_blend_metal(carrier, &kernel.weights, kernel.size, amount)?;
    let cpu = convolution_blend_cpu(carrier, kernel, amount)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU convolution outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal convolution-blend render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_convolutional_blend_frame_metal(
    _carrier: &ImageBufferF32,
    _kernel: &ConvolutionKernel,
    _amount: f32,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

pub(crate) fn render_convolutional_blend_color_frame(
    carrier: &ImageBufferF32,
    kernels: &[ConvolutionKernel; 3],
    amount: f32,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(convolution_blend_color_cpu(carrier, kernels, amount)?),
        RenderBackend::Metal => {
            render_convolutional_blend_color_frame_metal(carrier, kernels, amount)
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_convolutional_blend_color_frame_metal(
    carrier: &ImageBufferF32,
    kernels: &[ConvolutionKernel; 3],
    amount: f32,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::convolution_blend_color_metal(
        carrier,
        &kernels[0].weights,
        &kernels[1].weights,
        &kernels[2].weights,
        kernels[0].size,
        amount,
    )?;
    let cpu = convolution_blend_color_cpu(carrier, kernels, amount)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU colour convolution outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal colour convolution-blend render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_convolutional_blend_color_frame_metal(
    _carrier: &ImageBufferF32,
    _kernels: &[ConvolutionKernel; 3],
    _amount: f32,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

pub(crate) fn granular_audio_modulation_from_cli(
    rms_cache: Option<&Path>,
    onset_cache: Option<&Path>,
    stft_cache: Option<&Path>,
    rms_variation_scale: f32,
    onset_rearrangement_scale: f32,
    centroid_grain_size_scale: f32,
) -> Option<GranularAudioModulation> {
    if rms_cache.is_none()
        && onset_cache.is_none()
        && stft_cache.is_none()
        && rms_variation_scale == 0.0
        && onset_rearrangement_scale == 0.0
        && centroid_grain_size_scale == 0.0
    {
        return None;
    }

    Some(GranularAudioModulation {
        rms_cache_path: rms_cache.map(|path| path.to_string_lossy().to_string()),
        onset_cache_path: onset_cache.map(|path| path.to_string_lossy().to_string()),
        stft_cache_path: stft_cache.map(|path| path.to_string_lossy().to_string()),
        rms_variation_scale,
        onset_rearrangement_scale,
        centroid_grain_size_scale,
    })
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TimedScalarControl {
    time_seconds: f64,
    value: f32,
}

pub(crate) struct GranularAudioControls {
    frame_rate: f64,
    rms: Option<Vec<TimedScalarControl>>,
    onset: Option<Vec<TimedScalarControl>>,
    centroid: Option<Vec<TimedScalarControl>>,
    rms_variation_scale: f32,
    onset_rearrangement_scale: f32,
    centroid_grain_size_scale: f32,
}

impl GranularAudioControls {
    fn settings_for_frame(
        &self,
        frame_index: usize,
        base: GranularMosaicSettings,
    ) -> GranularMosaicSettings {
        if self.rms_variation_scale == 0.0
            && self.onset_rearrangement_scale == 0.0
            && self.centroid_grain_size_scale == 0.0
        {
            return base;
        }

        let time_seconds = frame_index as f64 / self.frame_rate;
        let rms = self
            .rms
            .as_deref()
            .map(|frames| scalar_at_frame_time(frames, time_seconds))
            .unwrap_or(0.0);
        let onset = self
            .onset
            .as_deref()
            .map(|frames| scalar_at_frame_time(frames, time_seconds))
            .unwrap_or(0.0);
        let centroid = self
            .centroid
            .as_deref()
            .map(|frames| scalar_at_frame_time(frames, time_seconds))
            .unwrap_or(0.0);

        let grain_size = (base.grain_size as f64
            + centroid as f64 * self.centroid_grain_size_scale as f64)
            .round()
            .clamp(1.0, u32::MAX as f64) as u32;
        GranularMosaicSettings {
            grain_size,
            rearrangement: (base.rearrangement + onset * self.onset_rearrangement_scale)
                .clamp(0.0, 1.0),
            variation: (base.variation + rms * self.rms_variation_scale).clamp(0.0, 1.0),
            ..base
        }
    }

    fn rms_frame_count(&self) -> usize {
        self.rms.as_ref().map_or(0, Vec::len)
    }

    fn onset_frame_count(&self) -> usize {
        self.onset.as_ref().map_or(0, Vec::len)
    }

    fn centroid_frame_count(&self) -> usize {
        self.centroid.as_ref().map_or(0, Vec::len)
    }
}

pub(crate) fn load_granular_audio_controls(
    modulation: Option<&GranularAudioModulation>,
    frame_rate: f64,
) -> Result<Option<GranularAudioControls>, CliError> {
    let Some(modulation) = modulation else {
        return Ok(None);
    };
    if !frame_rate.is_finite() || frame_rate <= 0.0 {
        return Err(CliError::Message(
            "frame-rate must be a positive finite number".to_string(),
        ));
    }
    for (name, scale) in [
        ("rms-variation-scale", modulation.rms_variation_scale),
        (
            "onset-rearrangement-scale",
            modulation.onset_rearrangement_scale,
        ),
        (
            "centroid-grain-size-scale",
            modulation.centroid_grain_size_scale,
        ),
    ] {
        if !scale.is_finite() {
            return Err(CliError::Message(format!("{name} must be finite")));
        }
    }
    if modulation.rms_variation_scale != 0.0 && modulation.rms_cache_path.is_none() {
        return Err(CliError::Message(
            "rms-variation-scale requires --rms-cache".to_string(),
        ));
    }
    if modulation.onset_rearrangement_scale != 0.0 && modulation.onset_cache_path.is_none() {
        return Err(CliError::Message(
            "onset-rearrangement-scale requires --onset-cache".to_string(),
        ));
    }
    if modulation.centroid_grain_size_scale != 0.0 && modulation.stft_cache_path.is_none() {
        return Err(CliError::Message(
            "centroid-grain-size-scale requires --stft-cache".to_string(),
        ));
    }

    let rms = modulation
        .rms_cache_path
        .as_deref()
        .map(load_rms_controls)
        .transpose()?;
    let onset = modulation
        .onset_cache_path
        .as_deref()
        .map(load_onset_controls)
        .transpose()?;
    let centroid = modulation
        .stft_cache_path
        .as_deref()
        .map(load_centroid_controls)
        .transpose()?;

    Ok(Some(GranularAudioControls {
        frame_rate,
        rms,
        onset,
        centroid,
        rms_variation_scale: modulation.rms_variation_scale,
        onset_rearrangement_scale: modulation.onset_rearrangement_scale,
        centroid_grain_size_scale: modulation.centroid_grain_size_scale,
    }))
}

pub(crate) fn load_rms_controls(path: &str) -> Result<Vec<TimedScalarControl>, CliError> {
    let cache: AudioAnalysisCache = serde_json::from_str(&fs::read_to_string(path)?)?;
    if cache.cache_format != "rms_envelope_v1" {
        return Err(CliError::Message(format!(
            "RMS cache at {path} has unsupported format {}",
            cache.cache_format
        )));
    }
    if cache.sample_rate == 0 || cache.frame_size == 0 || cache.hop_size == 0 {
        return Err(CliError::Message(format!(
            "RMS cache at {path} has invalid timing metadata"
        )));
    }
    timed_scalar_controls(
        cache
            .frames
            .into_iter()
            .map(|frame| Ok((frame.time_seconds, frame.rms))),
        "RMS",
        true,
    )
}

pub(crate) fn load_onset_controls(path: &str) -> Result<Vec<TimedScalarControl>, CliError> {
    let cache: OnsetStrengthCache = serde_json::from_str(&fs::read_to_string(path)?)?;
    if cache.cache_format != "onset_strength_v1" {
        return Err(CliError::Message(format!(
            "onset cache at {path} has unsupported format {}",
            cache.cache_format
        )));
    }
    if cache.source_cache_format != "stft_magnitude_v1"
        || cache.sample_rate == 0
        || cache.hop_size == 0
    {
        return Err(CliError::Message(format!(
            "onset cache at {path} has invalid source or timing metadata"
        )));
    }
    let controls = timed_scalar_controls(
        cache
            .frames
            .into_iter()
            .map(|frame| Ok((frame.time_seconds, frame.strength))),
        "onset",
        true,
    )?;
    let peak = controls.iter().map(|frame| frame.value).fold(0.0, f32::max);
    if peak == 0.0 {
        return Ok(controls);
    }
    Ok(controls
        .into_iter()
        .map(|frame| TimedScalarControl {
            time_seconds: frame.time_seconds,
            value: frame.value / peak,
        })
        .collect())
}

pub(crate) fn load_centroid_controls(path: &str) -> Result<Vec<TimedScalarControl>, CliError> {
    let cache: StftAnalysisCache = serde_json::from_str(&fs::read_to_string(path)?)?;
    if cache.cache_format != "stft_magnitude_v1" {
        return Err(CliError::Message(format!(
            "STFT cache at {path} has unsupported format {}",
            cache.cache_format
        )));
    }
    if cache.sample_rate == 0 || cache.fft_size == 0 || cache.hop_size == 0 {
        return Err(CliError::Message(format!(
            "STFT cache at {path} has invalid timing metadata"
        )));
    }
    let nyquist = cache.sample_rate as f32 * 0.5;
    let fft_size = cache.fft_size;
    let sample_rate = cache.sample_rate;
    timed_scalar_controls(
        cache.frames.into_iter().map(|frame| {
            spectral_centroid_from_magnitudes(&frame.magnitudes, fft_size, sample_rate)
                .map(|centroid| (frame.time_seconds, (centroid / nyquist).clamp(0.0, 1.0)))
        }),
        "spectral centroid",
        true,
    )
}

pub(crate) fn timed_scalar_controls(
    values: impl IntoIterator<Item = Result<(f64, f32), morphogen_audio::AudioError>>,
    label: &str,
    non_negative: bool,
) -> Result<Vec<TimedScalarControl>, CliError> {
    let mut controls = Vec::new();
    let mut previous_time = None;
    for value in values {
        let (time_seconds, value) = value?;
        if !time_seconds.is_finite() || time_seconds < 0.0 || !value.is_finite() {
            return Err(CliError::Message(format!(
                "{label} cache contains non-finite or negative-time descriptor data"
            )));
        }
        if non_negative && value < 0.0 {
            return Err(CliError::Message(format!(
                "{label} cache contains a negative control value"
            )));
        }
        if previous_time.is_some_and(|previous_time| time_seconds < previous_time) {
            return Err(CliError::Message(format!(
                "{label} cache descriptor times must be sorted"
            )));
        }
        previous_time = Some(time_seconds);
        controls.push(TimedScalarControl {
            time_seconds,
            value,
        });
    }
    if controls.is_empty() {
        return Err(CliError::Message(format!(
            "{label} cache contains no descriptor frames"
        )));
    }
    Ok(controls)
}

pub(crate) fn scalar_at_frame_time(frames: &[TimedScalarControl], time_seconds: f64) -> f32 {
    let descriptor_count = frames.partition_point(|frame| frame.time_seconds <= time_seconds);
    descriptor_count
        .checked_sub(1)
        .and_then(|index| frames.get(index))
        .map(|frame| frame.value)
        .unwrap_or(0.0)
}

pub(crate) struct GranularMosaicSequenceRenderRequest<'a> {
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: GranularMosaicSettings,
    pub(crate) frame_rate: f64,
    pub(crate) max_frames: Option<usize>,
    pub(crate) grain_cache_dir: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
    pub(crate) audio_modulation: Option<GranularAudioModulation>,
    pub(crate) selection_mode: GrainSelectionMode,
}

pub(crate) fn render_granular_mosaic_sequence(
    request: GranularMosaicSequenceRenderRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    let GranularMosaicSequenceRenderRequest {
        modulator_dir,
        carrier_dir,
        output_dir,
        settings,
        frame_rate,
        max_frames,
        grain_cache_dir,
        backend,
        audio_modulation,
        selection_mode,
    } = request;
    settings.validate()?;
    if !frame_rate.is_finite() || frame_rate <= 0.0 {
        return Err(CliError::Message(
            "frame-rate must be a positive finite number".to_string(),
        ));
    }
    if matches!(max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let modulator_frames = collect_image_frames(modulator_dir)?;
    let carrier_frames = collect_image_frames(carrier_dir)?;
    if modulator_frames.is_empty() || carrier_frames.is_empty() {
        return Err(CliError::Message(
            "granular mosaic requires at least one PNG frame in each source directory".to_string(),
        ));
    }

    let paired_count = modulator_frames.len().min(carrier_frames.len());
    let frame_count = max_frames
        .map(|limit| limit.min(paired_count))
        .unwrap_or(paired_count);
    fs::create_dir_all(output_dir)?;
    if let Some(cache_root) = grain_cache_dir {
        fs::create_dir_all(cache_root)?;
    }
    let audio_controls = load_granular_audio_controls(audio_modulation.as_ref(), frame_rate)?;

    let mut reused_descriptor_count = 0usize;
    let mut reused_selection_count = 0usize;

    for index in 0..frame_count {
        let modulator = load_image_f32(&modulator_frames[index])?;
        let carrier = load_image_f32(&carrier_frames[index])?;
        let frame_settings = audio_controls
            .as_ref()
            .map(|controls| controls.settings_for_frame(index, settings))
            .unwrap_or(settings);
        let rendered = if let Some(cache_root) = grain_cache_dir {
            let modulator_fingerprint = image_file_fingerprint(&modulator_frames[index])?;
            let carrier_fingerprint = image_file_fingerprint(&carrier_frames[index])?;
            render_granular_mosaic_frame(
                &modulator,
                &carrier,
                frame_settings,
                Some(GranularMosaicCacheContext {
                    directory: &cache_root.join(format!("frame_{index:06}")),
                    modulator_fingerprint: &modulator_fingerprint,
                    carrier_fingerprint: &carrier_fingerprint,
                }),
                backend,
                selection_mode,
            )?
        } else {
            render_granular_mosaic_frame(
                &modulator,
                &carrier,
                frame_settings,
                None,
                backend,
                selection_mode,
            )?
        };
        reused_descriptor_count += usize::from(rendered.reused_descriptor_cache);
        reused_selection_count += usize::from(rendered.reused_selection_cache);
        save_png(
            &rendered.image,
            &output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    if modulator_frames.len() != carrier_frames.len() {
        println!(
            "source frame counts differ: {} modulator frame(s), {} carrier frame(s); rendered common prefix",
            modulator_frames.len(),
            carrier_frames.len()
        );
    }
    if let Some(cache_root) = grain_cache_dir {
        println!(
            "reused {reused_descriptor_count} descriptor and {reused_selection_count} selection cache frame(s) from {}",
            cache_root.display()
        );
    }
    if let Some(controls) = audio_controls {
        println!(
            "applied Source A audio granular modulation from {} RMS, {} onset, and {} centroid descriptor frame(s)",
            controls.rms_frame_count(),
            controls.onset_frame_count(),
            controls.centroid_frame_count()
        );
    }
    println!(
        "rendered granular mosaic sequence with {} frame(s) on the {} backend from {} modulating {} to {}",
        frame_count,
        render_backend_label(backend),
        modulator_dir.display(),
        carrier_dir.display(),
        output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) struct GranularMosaicPoolSequenceRequest<'a> {
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: GranularMosaicSettings,
    pub(crate) audio_weight: f32,
    pub(crate) texture_weight: f32,
    pub(crate) modulator_rms_cache: Option<&'a Path>,
    pub(crate) carrier_rms_cache: Option<&'a Path>,
    pub(crate) modulator_centroid_cache: Option<&'a Path>,
    pub(crate) carrier_centroid_cache: Option<&'a Path>,
    pub(crate) pool_window: u32,
    pub(crate) anti_repeat_weight: f32,
    pub(crate) anti_repeat_cooldown: u32,
    pub(crate) coherence_weight: f32,
    pub(crate) coherence_reach: u32,
    pub(crate) spatial_coherence_weight: f32,
    pub(crate) frame_rate: f64,
    pub(crate) max_frames: Option<usize>,
    pub(crate) grain_cache_dir: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
}

/// Build a pool/query audio vector in fixed order `[rms?, centroid?]`, sampling
/// each supplied descriptor at `time_seconds`. Absent descriptors contribute no
/// dimension (so k ranges 0..=2); supplying the descriptors symmetrically on the
/// modulator and carrier sides keeps both indexing the same audio dimensions.
pub(crate) fn pool_audio_vector(
    rms: Option<&[TimedScalarControl]>,
    centroid: Option<&[TimedScalarControl]>,
    time_seconds: f64,
) -> Vec<f32> {
    let mut audio = Vec::with_capacity(2);
    if let Some(controls) = rms {
        audio.push(scalar_at_frame_time(controls, time_seconds));
    }
    if let Some(controls) = centroid {
        audio.push(scalar_at_frame_time(controls, time_seconds));
    }
    audio
}

/// Render a temporal-grain-pool mosaic sequence (step 6b). The whole-clip pool is
/// built once from every carrier frame; each grain carries its frame's audio
/// descriptor vector (`[rms?, centroid?]`), and selection matches that against
/// Source A's frame-time query vector.
pub(crate) fn render_granular_mosaic_pool_sequence(
    request: GranularMosaicPoolSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    let GranularMosaicPoolSequenceRequest {
        modulator_dir,
        carrier_dir,
        output_dir,
        settings,
        audio_weight,
        texture_weight,
        modulator_rms_cache,
        carrier_rms_cache,
        modulator_centroid_cache,
        carrier_centroid_cache,
        pool_window,
        anti_repeat_weight,
        anti_repeat_cooldown,
        coherence_weight,
        coherence_reach,
        spatial_coherence_weight,
        frame_rate,
        max_frames,
        grain_cache_dir,
        backend,
    } = request;
    settings.validate()?;
    if !anti_repeat_weight.is_finite() || anti_repeat_weight < 0.0 {
        return Err(CliError::Message(
            "anti-repeat-weight must be a finite, non-negative number".to_string(),
        ));
    }
    if !coherence_weight.is_finite() || coherence_weight < 0.0 {
        return Err(CliError::Message(
            "coherence-weight must be a finite, non-negative number".to_string(),
        ));
    }
    if !spatial_coherence_weight.is_finite() || spatial_coherence_weight < 0.0 {
        return Err(CliError::Message(
            "spatial-coherence-weight must be a finite, non-negative number".to_string(),
        ));
    }
    if !frame_rate.is_finite() || frame_rate <= 0.0 {
        return Err(CliError::Message(
            "frame-rate must be a positive finite number".to_string(),
        ));
    }
    if matches!(max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }
    if !audio_weight.is_finite() || audio_weight < 0.0 {
        return Err(CliError::Message(
            "audio-weight must be a finite, non-negative number".to_string(),
        ));
    }
    if !texture_weight.is_finite() || texture_weight < 0.0 {
        return Err(CliError::Message(
            "texture-weight must be a finite, non-negative number".to_string(),
        ));
    }
    // Audio matching needs the pool grains and the query to share a descriptor
    // space, so each descriptor type is required on both sides or neither. The
    // audio vector is built in fixed order [rms?, centroid?] so the modulator
    // query and carrier pool grains index the same dimensions.
    if modulator_rms_cache.is_some() != carrier_rms_cache.is_some() {
        return Err(CliError::Message(
            "pool audio matching needs both --modulator-rms-cache and --carrier-rms-cache (or neither)"
                .to_string(),
        ));
    }
    if modulator_centroid_cache.is_some() != carrier_centroid_cache.is_some() {
        return Err(CliError::Message(
            "pool centroid matching needs both --modulator-centroid-cache and --carrier-centroid-cache (or neither)"
                .to_string(),
        ));
    }

    let modulator_frames = collect_image_frames(modulator_dir)?;
    let carrier_frame_paths = collect_image_frames(carrier_dir)?;
    if modulator_frames.is_empty() || carrier_frame_paths.is_empty() {
        return Err(CliError::Message(
            "granular mosaic requires at least one PNG frame in each source directory".to_string(),
        ));
    }

    fs::create_dir_all(output_dir)?;
    if let Some(cache_root) = grain_cache_dir {
        fs::create_dir_all(cache_root)?;
    }

    // The whole-clip pool material: every carrier frame, held in memory so the
    // cross-frame render can sample any selected grain's source frame.
    let pool_frames = carrier_frame_paths
        .iter()
        .map(|path| load_image_f32(path))
        .collect::<Result<Vec<_>, _>>()?;

    let carrier_rms_controls = carrier_rms_cache
        .map(|path| load_rms_controls(&path.to_string_lossy()))
        .transpose()?;
    let carrier_centroid_controls = carrier_centroid_cache
        .map(|path| load_centroid_controls(&path.to_string_lossy()))
        .transpose()?;
    let frame_audio: Vec<Vec<f32>> = (0..pool_frames.len())
        .map(|frame_index| {
            pool_audio_vector(
                carrier_rms_controls.as_deref(),
                carrier_centroid_controls.as_deref(),
                frame_index as f64 / frame_rate,
            )
        })
        .collect();

    let carrier_set_fingerprint = pool_set_fingerprint(&carrier_frame_paths, &frame_audio)?;
    let (pool, reused_pool) = resolve_grain_pool(
        grain_cache_dir,
        &pool_frames,
        &frame_audio,
        settings.grain_size,
        &carrier_set_fingerprint,
    )?;

    let modulator_rms_controls = modulator_rms_cache
        .map(|path| load_rms_controls(&path.to_string_lossy()))
        .transpose()?;
    let modulator_centroid_controls = modulator_centroid_cache
        .map(|path| load_centroid_controls(&path.to_string_lossy()))
        .transpose()?;

    let paired_count = modulator_frames.len().min(pool_frames.len());
    let frame_count = max_frames
        .map(|limit| limit.min(paired_count))
        .unwrap_or(paired_count);

    // Anti-repeat scheduling state: the most recent output frame at which each
    // global grain index was selected. Only tracked when enabled so the default
    // path stays allocation-free and byte-identical.
    let anti_repeat_enabled = anti_repeat_weight > 0.0 && anti_repeat_cooldown > 0;
    let mut last_used_frame: Vec<Option<u32>> = if anti_repeat_enabled {
        vec![None; pool.grains.len()]
    } else {
        Vec::new()
    };

    // Temporal-coherence scheduling state: the global grain index each output tile
    // selected on the previous frame (one entry per tile, row-major). Only tracked
    // when enabled so the default path stays allocation-free and byte-identical.
    let coherence_enabled =
        (coherence_weight > 0.0 || spatial_coherence_weight > 0.0) && coherence_reach > 0;
    let mut prev_selection: Vec<Option<u32>> = Vec::new();

    for index in 0..frame_count {
        let modulator = load_image_f32(&modulator_frames[index])?;
        let carrier = &pool_frames[index];
        let query_audio = pool_audio_vector(
            modulator_rms_controls.as_deref(),
            modulator_centroid_controls.as_deref(),
            index as f64 / frame_rate,
        );
        let window = if pool_window == 0 {
            PoolSelectionWindow::WholeClip
        } else {
            PoolSelectionWindow::Trailing {
                current_frame: index as u32,
                frames: pool_window,
            }
        };
        let anti_repeat = anti_repeat_enabled.then(|| AntiRepeat {
            last_used_frame: &last_used_frame,
            current_frame: index as u32,
            cooldown: anti_repeat_cooldown,
            weight: anti_repeat_weight,
        });
        // Frame zero has an empty `prev_selection`, so coherence is a no-op there
        // (byte-identical to the non-scheduled selection).
        let coherence = coherence_enabled.then(|| TemporalCoherence {
            prev_selection: &prev_selection,
            reach: coherence_reach,
            weight: coherence_weight,
            spatial_weight: spatial_coherence_weight,
        });
        let selection = select_grains_from_pool_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &query_audio,
            &pool,
            settings,
            audio_weight,
            texture_weight,
            window,
            anti_repeat,
            coherence,
        )?;
        if coherence_enabled {
            prev_selection = selection.indices.iter().map(|&index| Some(index)).collect();
        }
        if anti_repeat_enabled {
            for &grain_index in &selection.indices {
                last_used_frame[grain_index as usize] = Some(index as u32);
            }
        }
        let image = render_granular_mosaic_pool_output(
            &pool_frames,
            &pool,
            carrier,
            &selection,
            settings,
            backend,
        )?;
        save_png(&image, &output_dir.join(format!("frame_{index:06}.png")))?;
    }

    if modulator_frames.len() != pool_frames.len() {
        println!(
            "source frame counts differ: {} modulator frame(s), {} carrier frame(s); rendered common prefix, pooled over all carrier frames",
            modulator_frames.len(),
            pool_frames.len()
        );
    }
    if let Some(cache_root) = grain_cache_dir {
        println!(
            "{} grain pool sidecar at {}",
            if reused_pool { "reused" } else { "wrote" },
            cache_root.display()
        );
    }
    println!(
        "rendered granular mosaic pool sequence with {} frame(s) ({}, {} pool frame(s), audio_dims={}, audio_weight={}, texture_weight={}) from {} modulating {} to {}",
        frame_count,
        POOLED_GRAIN_ALGORITHM,
        pool_frames.len(),
        pool.audio_dims,
        audio_weight,
        texture_weight,
        modulator_dir.display(),
        carrier_dir.display(),
        output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

/// Resolve the temporal grain pool, reusing a matching sidecar or assembling it
/// from the carrier frames and writing it back. Returns whether it was reused.
pub(crate) fn resolve_grain_pool(
    grain_cache_dir: Option<&Path>,
    pool_frames: &[ImageBufferF32],
    frame_audio: &[Vec<f32>],
    grain_size: u32,
    carrier_set_fingerprint: &str,
) -> Result<(GrainPool, bool), CliError> {
    let audio_dims = frame_audio.first().map_or(0, Vec::len);
    if let Some(cache_root) = grain_cache_dir {
        if cache_root
            .join(GRAIN_POOL_DESCRIPTOR_CACHE_FILE_NAME)
            .is_file()
        {
            let cached = read_grain_pool_descriptor_cache(cache_root)?;
            let is_current = cached.algorithm == POOLED_GRAIN_ALGORITHM
                && cached.grain_size == grain_size
                && cached.frame_count as usize == pool_frames.len()
                && cached.audio_dims == audio_dims
                && cached.carrier_set_fingerprint == carrier_set_fingerprint;
            if is_current {
                return Ok((cached.pool, true));
            }
        }
    }
    let pool = analyze_grain_pool_cpu(pool_frames, frame_audio, grain_size)?;
    if let Some(cache_root) = grain_cache_dir {
        write_grain_pool_descriptor_cache(
            cache_root,
            pool_frames.len() as u32,
            carrier_set_fingerprint,
            &pool,
        )?;
    }
    Ok((pool, false))
}

/// Combined fingerprint over every carrier frame and its per-frame audio, so any
/// change to a pool frame or its descriptor invalidates a cached pool.
pub(crate) fn pool_set_fingerprint(
    carrier_frame_paths: &[PathBuf],
    frame_audio: &[Vec<f32>],
) -> Result<String, CliError> {
    let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
    for (path, audio) in carrier_frame_paths.iter().zip(frame_audio.iter()) {
        update_fnv1a(&mut checksum, image_file_fingerprint(path)?.as_bytes());
        update_fnv1a(&mut checksum, &[0]);
        for value in audio {
            update_fnv1a(&mut checksum, &value.to_bits().to_le_bytes());
        }
        update_fnv1a(&mut checksum, &[0]);
    }
    Ok(format!("fnv1a64:{checksum:016x}"))
}

pub(crate) struct GranularMosaicCacheContext<'a> {
    directory: &'a Path,
    modulator_fingerprint: &'a str,
    carrier_fingerprint: &'a str,
}

pub(crate) struct GranularMosaicFrameRenderResult {
    image: ImageBufferF32,
    reused_descriptor_cache: bool,
    reused_selection_cache: bool,
}

pub(crate) fn render_granular_mosaic_frame(
    modulator: &ImageBufferF32,
    carrier: &ImageBufferF32,
    settings: GranularMosaicSettings,
    cache: Option<GranularMosaicCacheContext<'_>>,
    backend: RenderBackend,
    selection_mode: GrainSelectionMode,
) -> Result<GranularMosaicFrameRenderResult, CliError> {
    if let Some(cache) = cache {
        fs::create_dir_all(cache.directory)?;
        let (selection, reused_descriptor_cache, reused_selection_cache) = match selection_mode {
            GrainSelectionMode::Luma => {
                let (descriptors, reused_descriptor) =
                    resolve_luma_grain_descriptors(&cache, carrier, settings)?;
                let (selection, reused_selection) = resolve_grain_selection_cache(
                    &cache,
                    GRANULAR_MOSAIC_ALGORITHM,
                    carrier,
                    settings,
                    || {
                        Ok(select_grains_cpu(
                            modulator,
                            carrier.width,
                            carrier.height,
                            &descriptors,
                            settings,
                        )?)
                    },
                )?;
                (selection, reused_descriptor, reused_selection)
            }
            GrainSelectionMode::MultimodalRgb => {
                let (descriptors, reused_descriptor) =
                    resolve_color_grain_descriptors(&cache, carrier, settings)?;
                let (selection, reused_selection) = resolve_grain_selection_cache(
                    &cache,
                    MULTIMODAL_GRAIN_ALGORITHM,
                    carrier,
                    settings,
                    || {
                        Ok(select_grains_multimodal_cpu(
                            modulator,
                            carrier.width,
                            carrier.height,
                            &descriptors,
                            settings,
                        )?)
                    },
                )?;
                (selection, reused_descriptor, reused_selection)
            }
        };
        let image = render_granular_mosaic_output(carrier, &selection, settings, backend)?;
        return Ok(GranularMosaicFrameRenderResult {
            image,
            reused_descriptor_cache,
            reused_selection_cache,
        });
    }

    let selection = match selection_mode {
        GrainSelectionMode::Luma => {
            let descriptors = analyze_grains_cpu(carrier, settings.grain_size)?;
            select_grains_cpu(
                modulator,
                carrier.width,
                carrier.height,
                &descriptors,
                settings,
            )?
        }
        GrainSelectionMode::MultimodalRgb => {
            let descriptors = analyze_grain_colors_cpu(carrier, settings.grain_size)?;
            select_grains_multimodal_cpu(
                modulator,
                carrier.width,
                carrier.height,
                &descriptors,
                settings,
            )?
        }
    };
    Ok(GranularMosaicFrameRenderResult {
        image: render_granular_mosaic_output(carrier, &selection, settings, backend)?,
        reused_descriptor_cache: false,
        reused_selection_cache: false,
    })
}

/// Resolve the luma grain descriptors for a frame, reusing a matching sidecar or
/// recomputing and rewriting it. Returns whether the cache was reused.
pub(crate) fn resolve_luma_grain_descriptors(
    cache: &GranularMosaicCacheContext<'_>,
    carrier: &ImageBufferF32,
    settings: GranularMosaicSettings,
) -> Result<(Vec<GrainDescriptor>, bool), CliError> {
    let descriptor_cache = if cache
        .directory
        .join(GRAIN_DESCRIPTOR_CACHE_FILE_NAME)
        .is_file()
    {
        Some(read_grain_descriptor_cache(cache.directory)?)
    } else {
        None
    };
    let is_current = descriptor_cache.as_ref().is_some_and(|descriptor_cache| {
        descriptor_cache.algorithm == GRANULAR_MOSAIC_ALGORITHM
            && descriptor_cache.carrier_width == carrier.width
            && descriptor_cache.carrier_height == carrier.height
            && descriptor_cache.grain_size == settings.grain_size
            && descriptor_cache.carrier_fingerprint == cache.carrier_fingerprint
    });
    match descriptor_cache {
        Some(descriptor_cache) if is_current => Ok((descriptor_cache.descriptors, true)),
        _ => {
            let descriptors = analyze_grains_cpu(carrier, settings.grain_size)?;
            write_grain_descriptor_cache(
                cache.directory,
                carrier.width,
                carrier.height,
                settings.grain_size,
                cache.carrier_fingerprint,
                &descriptors,
            )?;
            Ok((descriptors, false))
        }
    }
}

/// Resolve the multimodal RGB grain descriptors for a frame. Mirrors
/// [`resolve_luma_grain_descriptors`] over the color sidecar and algorithm id.
pub(crate) fn resolve_color_grain_descriptors(
    cache: &GranularMosaicCacheContext<'_>,
    carrier: &ImageBufferF32,
    settings: GranularMosaicSettings,
) -> Result<(Vec<GrainColorDescriptor>, bool), CliError> {
    let descriptor_cache = if cache
        .directory
        .join(GRAIN_COLOR_DESCRIPTOR_CACHE_FILE_NAME)
        .is_file()
    {
        Some(read_grain_color_descriptor_cache(cache.directory)?)
    } else {
        None
    };
    let is_current = descriptor_cache.as_ref().is_some_and(|descriptor_cache| {
        descriptor_cache.algorithm == MULTIMODAL_GRAIN_ALGORITHM
            && descriptor_cache.carrier_width == carrier.width
            && descriptor_cache.carrier_height == carrier.height
            && descriptor_cache.grain_size == settings.grain_size
            && descriptor_cache.carrier_fingerprint == cache.carrier_fingerprint
    });
    match descriptor_cache {
        Some(descriptor_cache) if is_current => Ok((descriptor_cache.descriptors, true)),
        _ => {
            let descriptors = analyze_grain_colors_cpu(carrier, settings.grain_size)?;
            write_grain_color_descriptor_cache(
                cache.directory,
                carrier.width,
                carrier.height,
                settings.grain_size,
                cache.carrier_fingerprint,
                &descriptors,
            )?;
            Ok((descriptors, false))
        }
    }
}

/// Resolve the grain selection for a frame, reusing a sidecar that matches the
/// active selection algorithm and settings, or computing it via `compute` and
/// writing it back. Shared across selection modes since the selection sidecar is
/// algorithm-tagged but otherwise identical in shape.
pub(crate) fn resolve_grain_selection_cache(
    cache: &GranularMosaicCacheContext<'_>,
    algorithm: &str,
    carrier: &ImageBufferF32,
    settings: GranularMosaicSettings,
    compute: impl FnOnce() -> Result<GrainSelection, CliError>,
) -> Result<(GrainSelection, bool), CliError> {
    let selection_cache = if cache
        .directory
        .join(GRAIN_SELECTION_CACHE_FILE_NAME)
        .is_file()
    {
        Some(read_grain_selection_cache(cache.directory)?)
    } else {
        None
    };
    let is_current = selection_cache.as_ref().is_some_and(|selection_cache| {
        selection_cache.algorithm == algorithm
            && selection_cache.modulator_fingerprint == cache.modulator_fingerprint
            && selection_cache.carrier_fingerprint == cache.carrier_fingerprint
            && selection_cache.carrier_width == carrier.width
            && selection_cache.carrier_height == carrier.height
            && selection_cache.grain_size == settings.grain_size
            && selection_cache.variation == settings.variation
            && selection_cache.seed == settings.seed
    });
    match selection_cache {
        Some(selection_cache) if is_current => Ok((selection_cache.selection, true)),
        _ => {
            let selection = compute()?;
            write_grain_selection_cache(
                cache.directory,
                algorithm,
                cache.modulator_fingerprint,
                cache.carrier_fingerprint,
                carrier.width,
                carrier.height,
                settings,
                &selection,
            )?;
            Ok((selection, false))
        }
    }
}

pub(crate) fn render_granular_mosaic_output(
    carrier: &ImageBufferF32,
    selection: &morphogen_render::GrainSelection,
    settings: GranularMosaicSettings,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(granular_mosaic_with_selection_cpu(
            carrier, selection, settings,
        )?),
        RenderBackend::Metal => render_granular_mosaic_output_metal(carrier, selection, settings),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_granular_mosaic_output_metal(
    carrier: &ImageBufferF32,
    selection: &morphogen_render::GrainSelection,
    settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::granular_mosaic_metal(carrier, selection, settings)?;
    let cpu = granular_mosaic_with_selection_cpu(carrier, selection, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU granular outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal granular render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(target_os = "macos")]
pub(crate) fn render_video_vocoder_match_metal(
    carrier: &ImageBufferF32,
    tone: &[f32],
    amount: f32,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::video_vocoder_match_metal(carrier, tone, amount)?;
    let cpu = apply_tone_map_cpu(carrier, tone, amount)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU video vocoder outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal video vocoder render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_video_vocoder_match_metal(
    _carrier: &ImageBufferF32,
    _tone: &[f32],
    _amount: f32,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal render backend is only available on macOS".to_string(),
    ))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_granular_mosaic_output_metal(
    _carrier: &ImageBufferF32,
    _selection: &morphogen_render::GrainSelection,
    _settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal render backend is only available on macOS".to_string(),
    ))
}

pub(crate) fn render_granular_mosaic_pool_output(
    pool_frames: &[ImageBufferF32],
    pool: &morphogen_render::GrainPool,
    carrier: &ImageBufferF32,
    selection: &morphogen_render::GrainSelection,
    settings: GranularMosaicSettings,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(granular_mosaic_with_pool_selection_cpu(
            pool_frames,
            pool,
            carrier,
            selection,
            settings,
        )?),
        RenderBackend::Metal => render_granular_mosaic_pool_output_metal(
            pool_frames,
            pool,
            carrier,
            selection,
            settings,
        ),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_granular_mosaic_pool_output_metal(
    pool_frames: &[ImageBufferF32],
    pool: &morphogen_render::GrainPool,
    carrier: &ImageBufferF32,
    selection: &morphogen_render::GrainSelection,
    settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::granular_mosaic_pool_metal(
        pool_frames,
        pool,
        carrier,
        selection,
        settings,
    )?;
    let cpu =
        granular_mosaic_with_pool_selection_cpu(pool_frames, pool, carrier, selection, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU granular pool outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal granular pool render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_granular_mosaic_pool_output_metal(
    _pool_frames: &[ImageBufferF32],
    _pool: &morphogen_render::GrainPool,
    _carrier: &ImageBufferF32,
    _selection: &morphogen_render::GrainSelection,
    _settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal render backend is only available on macOS".to_string(),
    ))
}

pub(crate) fn print_granular_cache_summary(
    grain_cache_dir: Option<&Path>,
    rendered: GranularMosaicFrameRenderResult,
) {
    if let Some(cache_directory) = grain_cache_dir {
        println!(
            "{} granular descriptor cache and {} selection cache at {}",
            if rendered.reused_descriptor_cache {
                "reused"
            } else {
                "generated"
            },
            if rendered.reused_selection_cache {
                "reused"
            } else {
                "generated"
            },
            cache_directory.display()
        );
    }
}

pub(crate) struct FrameSequenceRenderRequest<'a> {
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) amount: f32,
    pub(crate) flow_cache_dir: Option<&'a Path>,
    pub(crate) max_frames: Option<usize>,
    pub(crate) backend: RenderBackend,
    pub(crate) rms: RmsAmountConfig<'a>,
}

/// Maximum per-channel difference tolerated between the Metal render output and the
/// CPU reference before a frame-sequence render is rejected. Float32 image values are
/// in roughly [0, 1]; one output LSB keeps the exported 8-bit PNGs visually equivalent,
/// although samples exactly on a quantization boundary may differ by one encoded value.
pub(crate) const METAL_CPU_PARITY_EPSILON: f32 = 1.0 / 255.0;

pub(crate) struct RmsAmountConfig<'a> {
    pub(crate) wav_path: Option<&'a Path>,
    pub(crate) frame_rate: f64,
    pub(crate) window_size: usize,
    pub(crate) hop_size: usize,
    pub(crate) amount_scale: f32,
}

pub(crate) fn render_frame_sequence(
    request: FrameSequenceRenderRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    let FrameSequenceRenderRequest {
        modulator_dir,
        carrier_dir,
        output_dir,
        amount,
        flow_cache_dir,
        max_frames,
        backend,
        rms,
    } = request;

    if !amount.is_finite() {
        return Err(CliError::Message("amount must be finite".to_string()));
    }
    if !rms.amount_scale.is_finite() {
        return Err(CliError::Message(
            "rms-amount-scale must be finite".to_string(),
        ));
    }
    if !rms.frame_rate.is_finite() || rms.frame_rate <= 0.0 {
        return Err(CliError::Message(
            "frame-rate must be a positive finite number".to_string(),
        ));
    }
    if matches!(max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let rms_modulation = load_rms_amount_modulation(rms)?;

    let modulator_frames = collect_image_frames(modulator_dir)?;
    let carrier_frames = collect_image_frames(carrier_dir)?;
    if modulator_frames.is_empty() {
        return Err(CliError::Message(format!(
            "no supported image frames found in {}",
            modulator_dir.display()
        )));
    }
    if carrier_frames.is_empty() {
        return Err(CliError::Message(format!(
            "no supported image frames found in {}",
            carrier_dir.display()
        )));
    }

    let paired_count = modulator_frames.len().min(carrier_frames.len());
    let frame_count = max_frames
        .map(|limit| limit.min(paired_count))
        .unwrap_or(paired_count);

    fs::create_dir_all(output_dir)?;
    if let Some(cache_root) = flow_cache_dir {
        fs::create_dir_all(cache_root)?;
    }

    for index in 0..frame_count {
        let modulator = load_image_f32(&modulator_frames[index])?;
        let carrier = load_image_f32(&carrier_frames[index])?;
        let flow = luminance_gradient_flow_cpu(&modulator, carrier.width, carrier.height)?;
        let frame_amount = rms_modulation
            .as_ref()
            .map(|modulation| modulation.amount_for_frame(index, amount))
            .unwrap_or(amount);
        let displaced = render_displacement_frame(&carrier, &flow, frame_amount, backend)?;
        let output_path = output_dir.join(format!("frame_{index:06}.png"));
        save_png(&displaced, &output_path)?;

        if let Some(cache_root) = flow_cache_dir {
            let frame_cache_dir = cache_root.join(format!("frame_{index:06}"));
            write_flow_cache(frame_cache_dir, &flow, "luminance_gradient_cpu_v1")?;
        }
    }

    if modulator_frames.len() != carrier_frames.len() {
        println!(
            "source frame counts differ: {} modulator frame(s), {} carrier frame(s); rendered common prefix",
            modulator_frames.len(),
            carrier_frames.len()
        );
    }
    if let Some(cache_root) = flow_cache_dir {
        println!(
            "wrote per-frame luminance flow caches to {}",
            cache_root.display()
        );
    }
    if let Some(modulation) = rms_modulation {
        println!(
            "applied RMS amount modulation from {} descriptor frame(s)",
            modulation.descriptors.len()
        );
    }
    println!(
        "rendered frame sequence with {} frame(s) on the {} backend from {} modulating {} to {}",
        frame_count,
        render_backend_label(backend),
        modulator_dir.display(),
        carrier_dir.display(),
        output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) fn render_backend_label(backend: RenderBackend) -> &'static str {
    match backend {
        RenderBackend::Cpu => "CPU",
        RenderBackend::Metal => "Metal",
    }
}

/// Render one displacement frame on the requested backend. The Metal path is gated by a
/// per-frame parity check against the CPU reference so a divergent GPU result fails the
/// render rather than silently writing wrong pixels.
pub(crate) fn render_displacement_frame(
    carrier: &ImageBufferF32,
    flow: &FlowField,
    amount: f32,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(flow_displace_cpu(carrier, flow, amount)?),
        RenderBackend::Metal => render_displacement_frame_metal(carrier, flow, amount),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_displacement_frame_metal(
    carrier: &ImageBufferF32,
    flow: &FlowField,
    amount: f32,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::flow_displace_metal(carrier, flow, amount)?;
    let cpu = flow_displace_cpu(carrier, flow, amount)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU outputs have mismatched dimensions; cannot verify parity".to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_displacement_frame_metal(
    _carrier: &ImageBufferF32,
    _flow: &FlowField,
    _amount: f32,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal render backend is only available on macOS".to_string(),
    ))
}

pub(crate) struct FrameSequenceRenderResult {
    pub(crate) frame_count: usize,
}

pub(crate) const FLOW_FEEDBACK_RENDER_CONTRACT_VERSION: u32 = 2;
pub(crate) const LUMINANCE_FLOW_ALGORITHM: &str = "luminance_gradient_cpu_v1";
pub(crate) const OPTICAL_FLOW_ALGORITHM: &str = "pyramidal_lucas_kanade_cpu_v1";

/// The recorded analysis-algorithm identifier for a flow source. This string is
/// part of the feedback render contract, so changing the flow source invalidates
/// an existing checkpoint.
pub(crate) fn flow_source_algorithm(flow_source: FlowSource) -> &'static str {
    match flow_source {
        FlowSource::Luminance => LUMINANCE_FLOW_ALGORITHM,
        FlowSource::OpticalFlow => OPTICAL_FLOW_ALGORITHM,
    }
}

pub(crate) fn read_cached_temporal_flow(
    cache_directory: &Path,
    algorithm: &str,
    source_fingerprint: &str,
    width: u32,
    height: u32,
) -> Result<Option<FlowField>, CliError> {
    if !cache_directory.join("manifest.json").is_file() {
        return Ok(None);
    }

    let (manifest, flow) = read_flow_cache(cache_directory)?;
    let is_current = manifest.algorithm == algorithm
        && manifest.width == width
        && manifest.height == height
        && manifest.vector_convention == FLOW_VECTOR_CONVENTION
        && manifest.source_fingerprint.as_deref() == Some(source_fingerprint);

    Ok(is_current.then_some(flow))
}

pub(crate) struct FeedbackSequenceRenderRequest<'a> {
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) flow_cache_dir: Option<&'a Path>,
    pub(crate) max_frames: Option<usize>,
    pub(crate) reset_at_frame: Option<usize>,
    pub(crate) frame_rate: f64,
    pub(crate) settings: FlowFeedbackSettings,
    pub(crate) output_bit_depth: u8,
    pub(crate) temporal_supersampling: u32,
    pub(crate) backend: RenderBackend,
    pub(crate) flow_source: FlowSource,
    pub(crate) job_id: &'a str,
    pub(crate) provenance: Option<&'a RenderJobProvenance>,
    pub(crate) stop_after_frame: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FeedbackSequenceSourceFingerprint {
    directory: String,
    frame_count: u32,
    checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct FeedbackSequenceContract {
    version: u32,
    flow_algorithm: String,
    modulator: FeedbackSequenceSourceFingerprint,
    carrier: FeedbackSequenceSourceFingerprint,
    settings: FlowFeedbackSettings,
    output_bit_depth: u8,
    temporal_supersampling: u32,
    backend: RenderBackend,
    reset_at_frame: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FeedbackSequenceCheckpoint {
    version: u32,
    task: String,
    job_id: String,
    status: String,
    next_frame_index: u32,
    state_path: Option<String>,
    state: Option<FlowFeedbackStateDescriptor>,
    contract: FeedbackSequenceContract,
    provenance: RenderJobProvenance,
}

pub(crate) fn render_feedback_sequence(
    request: FeedbackSequenceRenderRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    let FeedbackSequenceRenderRequest {
        modulator_dir,
        carrier_dir,
        output_dir,
        flow_cache_dir,
        max_frames,
        reset_at_frame,
        frame_rate,
        settings,
        output_bit_depth,
        temporal_supersampling,
        backend,
        flow_source,
        job_id,
        provenance,
        stop_after_frame,
    } = request;

    let flow_algorithm = flow_source_algorithm(flow_source);
    settings.validate()?;
    validate_feedback_export_settings(output_bit_depth, temporal_supersampling)?;
    if !frame_rate.is_finite() || frame_rate <= 0.0 {
        return Err(CliError::Message(
            "frame-rate must be a positive finite number".to_string(),
        ));
    }
    if matches!(max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let modulator_frames = collect_image_frames(modulator_dir)?;
    let carrier_frames = collect_image_frames(carrier_dir)?;
    if modulator_frames.is_empty() {
        return Err(CliError::Message(format!(
            "no supported image frames found in {}",
            modulator_dir.display()
        )));
    }
    if carrier_frames.is_empty() {
        return Err(CliError::Message(format!(
            "no supported image frames found in {}",
            carrier_dir.display()
        )));
    }

    let paired_count = modulator_frames.len().min(carrier_frames.len());
    let frame_count = max_frames
        .map(|limit| limit.min(paired_count))
        .unwrap_or(paired_count);
    let frame_count_u32 = u32::try_from(frame_count).map_err(|_| {
        CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
    })?;
    let reset_at_frame = reset_at_frame
        .map(|frame| {
            u32::try_from(frame).map_err(|_| {
                CliError::Message("reset-at-frame must be less than u32::MAX".to_string())
            })
        })
        .transpose()?;
    if matches!(reset_at_frame, Some(frame) if frame >= frame_count_u32) {
        return Err(CliError::Message(
            "reset-at-frame must refer to a rendered frame".to_string(),
        ));
    }
    let contract = FeedbackSequenceContract {
        version: FLOW_FEEDBACK_RENDER_CONTRACT_VERSION,
        flow_algorithm: flow_algorithm.to_string(),
        modulator: feedback_source_fingerprint(modulator_dir, &modulator_frames)?,
        carrier: feedback_source_fingerprint(carrier_dir, &carrier_frames)?,
        settings,
        output_bit_depth,
        temporal_supersampling,
        backend,
        reset_at_frame,
    };
    let provenance = provenance.cloned().unwrap_or_else(|| {
        feedback_sequence_provenance(modulator_dir, carrier_dir, flow_cache_dir, flow_algorithm)
    });

    let frame_dir = output_dir.join("frames");
    fs::create_dir_all(&frame_dir)?;
    if let Some(cache_root) = flow_cache_dir {
        fs::create_dir_all(cache_root)?;
    }

    let (start_frame, mut previous_output, mut latest_state_path) =
        load_feedback_resume_state(output_dir, job_id, &contract, &provenance, frame_count_u32)?;
    let mut reused_optical_flow_cache_count = 0usize;
    let mut generated_optical_flow_cache_count = 0usize;
    let mut metal_flow_validated = false;
    // The flow cache is segregated by backend for the optical-flow source so a
    // GPU-produced cache is never reused by a CPU render (the two agree within
    // tolerance, not bit-for-bit). Luminance flow is CPU-only and keeps its id.
    let flow_cache_algorithm: &str = match flow_source {
        FlowSource::OpticalFlow => optical_flow_cache_algorithm(backend),
        FlowSource::Luminance => flow_algorithm,
    };
    for index in start_frame..frame_count {
        let modulator = load_image_f32(&modulator_frames[index])?;
        let carrier = load_image_f32(&carrier_frames[index])?;
        let is_reset_frame = Some(index as u32) == reset_at_frame;
        let (flow, generated_temporal_flow_cache, reused_temporal_flow_cache) = match flow_source {
            FlowSource::Luminance => (
                luminance_gradient_flow_cpu(&modulator, carrier.width, carrier.height)?,
                false,
                false,
            ),
            FlowSource::OpticalFlow => {
                // Frame zero and explicit resets have no temporal history, so
                // both the history and field are reset together.
                if index == 0 || is_reset_frame {
                    (
                        FlowField::from_fn(carrier.width, carrier.height, |_, _| [0.0, 0.0])?,
                        false,
                        false,
                    )
                } else {
                    let cache_directory =
                        flow_cache_dir.map(|root| root.join(format!("frame_{index:06}")));
                    if let Some(flow) = cache_directory
                        .as_deref()
                        .map(|directory| {
                            read_cached_temporal_flow(
                                directory,
                                flow_cache_algorithm,
                                &contract.modulator.checksum,
                                carrier.width,
                                carrier.height,
                            )
                        })
                        .transpose()?
                        .flatten()
                    {
                        (flow, false, true)
                    } else {
                        let previous_modulator = load_image_f32(&modulator_frames[index - 1])?;
                        (
                            compute_optical_flow_backend(
                                &previous_modulator,
                                &modulator,
                                carrier.width,
                                carrier.height,
                                LUCAS_KANADE_WINDOW_RADIUS,
                                backend,
                                &mut metal_flow_validated,
                            )?,
                            cache_directory.is_some(),
                            false,
                        )
                    }
                }
            }
        };
        reused_optical_flow_cache_count += usize::from(reused_temporal_flow_cache);
        generated_optical_flow_cache_count += usize::from(generated_temporal_flow_cache);
        let history = (!is_reset_frame)
            .then_some(previous_output.as_ref())
            .flatten();
        let output = render_feedback_frame(&carrier, history, &flow, settings, backend)?;
        let export_frame = flow_temporal_supersample_cpu(
            &output,
            &flow,
            settings.feedback_amount,
            temporal_supersampling,
        )?;
        let output_path = frame_dir.join(format!("frame_{index:06}.png"));
        save_png_with_bit_depth(&export_frame, &output_path, output_bit_depth)?;
        if let Some(cache_root) = flow_cache_dir {
            let frame_cache_dir = cache_root.join(format!("frame_{index:06}"));
            match flow_source {
                FlowSource::Luminance => {
                    write_flow_cache(frame_cache_dir, &flow, flow_algorithm)?;
                }
                FlowSource::OpticalFlow if generated_temporal_flow_cache => {
                    write_flow_cache_with_source_fingerprint(
                        frame_cache_dir,
                        &flow,
                        flow_cache_algorithm,
                        Some(&contract.modulator.checksum),
                    )?;
                }
                FlowSource::OpticalFlow => {}
            }
        }

        let frame_index = u32::try_from(index).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let state_path = feedback_state_path(output_dir, frame_index);
        let state_path_relative = feedback_state_relative_path(frame_index);
        let descriptor = write_flow_feedback_state(&state_path, &output)?;
        write_feedback_checkpoint(
            output_dir,
            FeedbackCheckpointWrite {
                job_id,
                status: "running",
                next_frame_index: frame_index.checked_add(1).ok_or_else(|| {
                    CliError::Message(
                        "frame sequence contains more than u32::MAX frames".to_string(),
                    )
                })?,
                state_path: Some(&state_path_relative),
                state: Some(descriptor),
                contract: &contract,
                provenance: &provenance,
            },
        )?;
        previous_output = Some(output);
        latest_state_path = Some(state_path_relative);

        if stop_after_frame {
            println!(
                "checkpointed flow-feedback sequence after frame {} in {}",
                index,
                output_dir.display()
            );
            return Ok(FrameSequenceRenderResult {
                frame_count: index + 1,
            });
        }
    }

    let final_state_path = latest_state_path.as_deref().ok_or_else(|| {
        CliError::Message("feedback render completed without a float state checkpoint".to_string())
    })?;
    let state_path = feedback_state_path_from_checkpoint(output_dir, final_state_path)?;
    let (final_state, _) = read_flow_feedback_state(&state_path)?;
    write_feedback_checkpoint(
        output_dir,
        FeedbackCheckpointWrite {
            job_id,
            status: "complete",
            next_frame_index: frame_count_u32,
            state_path: Some(final_state_path),
            state: Some(final_state),
            contract: &contract,
            provenance: &provenance,
        },
    )?;

    let frame_paths = (0..frame_count_u32)
        .map(|index| format!("frames/frame_{index:06}.png"))
        .collect::<Vec<_>>();
    let timing = RenderTimingMetadata {
        frame_rate,
        frame_count: frame_count_u32,
        start_seconds: 0.0,
        duration_seconds: frame_count as f64 / frame_rate,
        sample_rate: 48_000,
        audio_sample_count: 0,
    };
    write_feedback_sequence_manifest(FeedbackSequenceManifestWrite {
        job_id,
        output_dir,
        frame_paths: &frame_paths,
        timing: &timing,
        contract: &contract,
        provenance: &provenance,
        state_path: final_state_path,
        output_bit_depth,
        temporal_supersampling,
    })?;

    if modulator_frames.len() != carrier_frames.len() {
        println!(
            "source frame counts differ: {} modulator frame(s), {} carrier frame(s); rendered common prefix",
            modulator_frames.len(),
            carrier_frames.len()
        );
    }
    if let Some(cache_root) = flow_cache_dir {
        println!(
            "wrote per-frame {flow_algorithm} flow caches to {}",
            cache_root.display()
        );
        if matches!(flow_source, FlowSource::OpticalFlow) {
            println!(
                "reused {reused_optical_flow_cache_count} and generated {generated_optical_flow_cache_count} temporal optical-flow cache frame(s)"
            );
        }
    }
    println!(
        "rendered flow-feedback sequence with {} frame(s) on the {} backend from {} modulating {} to {}",
        frame_count,
        render_backend_label(backend),
        modulator_dir.display(),
        carrier_dir.display(),
        output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) fn render_feedback_frame(
    carrier: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    flow: &FlowField,
    settings: FlowFeedbackSettings,
    backend: RenderBackend,
) -> Result<ImageBufferF32, CliError> {
    match backend {
        RenderBackend::Cpu => Ok(flow_feedback_frame_cpu(
            carrier,
            previous_output,
            flow,
            settings,
        )?),
        RenderBackend::Metal => {
            render_feedback_frame_metal(carrier, previous_output, flow, settings)
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn render_feedback_frame_metal(
    carrier: &ImageBufferF32,
    previous_output: Option<&ImageBufferF32>,
    flow: &FlowField,
    settings: FlowFeedbackSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::flow_feedback_metal(carrier, previous_output, flow, settings)?;
    let cpu = flow_feedback_frame_cpu(carrier, previous_output, flow, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU outputs have mismatched dimensions; cannot verify parity".to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal feedback render diverged from CPU reference by {difference} (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_feedback_frame_metal(
    _carrier: &ImageBufferF32,
    _previous_output: Option<&ImageBufferF32>,
    _flow: &FlowField,
    _settings: FlowFeedbackSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal render backend is only available on macOS".to_string(),
    ))
}

pub(crate) fn feedback_sequence_provenance(
    modulator_dir: &Path,
    carrier_dir: &Path,
    flow_cache_dir: Option<&Path>,
    flow_algorithm: &str,
) -> RenderJobProvenance {
    RenderJobProvenance {
        sources: vec![
            RenderJobSourceProvenance {
                source_id: "source-a-frames".to_string(),
                role: SourceRole::Modulator,
                path: modulator_dir.to_string_lossy().to_string(),
            },
            RenderJobSourceProvenance {
                source_id: "source-b-frames".to_string(),
                role: SourceRole::Carrier,
                path: carrier_dir.to_string_lossy().to_string(),
            },
        ],
        analysis_caches: flow_cache_dir
            .map(|path| {
                vec![RenderJobAnalysisCacheProvenance {
                    kind: AnalysisKind::OpticalFlow,
                    path: path.to_string_lossy().to_string(),
                    producer: flow_algorithm.to_string(),
                }]
            })
            .unwrap_or_default(),
    }
}

pub(crate) fn granular_mosaic_provenance(
    modulator_dir: &Path,
    carrier_dir: &Path,
    grain_cache_dir: Option<&Path>,
    audio_modulation: Option<&GranularAudioModulation>,
    selection_mode: GrainSelectionMode,
) -> RenderJobProvenance {
    let sources = vec![
        RenderJobSourceProvenance {
            source_id: "source-a-frames".to_string(),
            role: SourceRole::Modulator,
            path: modulator_dir.to_string_lossy().to_string(),
        },
        RenderJobSourceProvenance {
            source_id: "source-b-frames".to_string(),
            role: SourceRole::Carrier,
            path: carrier_dir.to_string_lossy().to_string(),
        },
    ];
    let mut analysis_caches = grain_cache_dir
        .map(|path| {
            vec![RenderJobAnalysisCacheProvenance {
                kind: AnalysisKind::GrainDescriptors,
                path: path.to_string_lossy().to_string(),
                producer: grain_selection_algorithm(selection_mode).to_string(),
            }]
        })
        .unwrap_or_default();

    if let Some(audio_modulation) = audio_modulation {
        if let Some(path) = audio_modulation.rms_cache_path.as_deref() {
            analysis_caches.push(RenderJobAnalysisCacheProvenance {
                kind: AnalysisKind::AudioRms,
                path: path.to_string(),
                producer: "rms_envelope_v1".to_string(),
            });
        }
        if let Some(path) = audio_modulation.onset_cache_path.as_deref() {
            analysis_caches.push(RenderJobAnalysisCacheProvenance {
                kind: AnalysisKind::OnsetStrength,
                path: path.to_string(),
                producer: "onset_strength_v1".to_string(),
            });
        }
        if let Some(path) = audio_modulation.stft_cache_path.as_deref() {
            analysis_caches.push(RenderJobAnalysisCacheProvenance {
                kind: AnalysisKind::SpectralCentroid,
                path: path.to_string(),
                producer: "stft_magnitude_v1".to_string(),
            });
        }
    }

    RenderJobProvenance {
        sources,
        analysis_caches,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn granular_mosaic_pool_provenance(
    modulator_dir: &Path,
    carrier_dir: &Path,
    grain_cache_dir: Option<&Path>,
    modulator_rms_cache: Option<&str>,
    carrier_rms_cache: Option<&str>,
    modulator_centroid_cache: Option<&str>,
    carrier_centroid_cache: Option<&str>,
) -> RenderJobProvenance {
    let sources = vec![
        RenderJobSourceProvenance {
            source_id: "source-a-frames".to_string(),
            role: SourceRole::Modulator,
            path: modulator_dir.to_string_lossy().to_string(),
        },
        RenderJobSourceProvenance {
            source_id: "source-b-frames".to_string(),
            role: SourceRole::Carrier,
            path: carrier_dir.to_string_lossy().to_string(),
        },
    ];
    let mut analysis_caches = grain_cache_dir
        .map(|path| {
            vec![RenderJobAnalysisCacheProvenance {
                kind: AnalysisKind::GrainDescriptors,
                path: path.to_string_lossy().to_string(),
                producer: POOLED_GRAIN_ALGORITHM.to_string(),
            }]
        })
        .unwrap_or_default();
    for cache in [modulator_rms_cache, carrier_rms_cache]
        .into_iter()
        .flatten()
    {
        analysis_caches.push(RenderJobAnalysisCacheProvenance {
            kind: AnalysisKind::AudioRms,
            path: cache.to_string(),
            producer: "rms_envelope_v1".to_string(),
        });
    }
    for cache in [modulator_centroid_cache, carrier_centroid_cache]
        .into_iter()
        .flatten()
    {
        analysis_caches.push(RenderJobAnalysisCacheProvenance {
            kind: AnalysisKind::Stft,
            path: cache.to_string(),
            producer: "stft_magnitude_v1".to_string(),
        });
    }

    RenderJobProvenance {
        sources,
        analysis_caches,
    }
}

pub(crate) fn feedback_source_fingerprint(
    directory: &Path,
    frames: &[PathBuf],
) -> Result<FeedbackSequenceSourceFingerprint, CliError> {
    let frame_count = u32::try_from(frames.len()).map_err(|_| {
        CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
    })?;
    let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
    for frame in frames {
        update_fnv1a(
            &mut checksum,
            frame.file_name().unwrap_or_default().as_encoded_bytes(),
        );
        update_fnv1a(&mut checksum, &[0]);
        update_fnv1a(&mut checksum, &fs::read(frame)?);
    }
    Ok(FeedbackSequenceSourceFingerprint {
        directory: directory.to_string_lossy().to_string(),
        frame_count,
        checksum: format!("fnv1a64:{checksum:016x}"),
    })
}

pub(crate) fn load_feedback_resume_state(
    output_dir: &Path,
    job_id: &str,
    expected_contract: &FeedbackSequenceContract,
    expected_provenance: &RenderJobProvenance,
    frame_count: u32,
) -> Result<(usize, Option<ImageBufferF32>, Option<String>), CliError> {
    let checkpoint_path = output_dir.join("checkpoint.json");
    if !checkpoint_path.exists() {
        return Ok((0, None, None));
    }

    let checkpoint: FeedbackSequenceCheckpoint =
        serde_json::from_str(&fs::read_to_string(&checkpoint_path)?)?;
    if checkpoint.version != FLOW_FEEDBACK_RENDER_CONTRACT_VERSION
        || checkpoint.task != "frame_sequence_flow_feedback"
        || checkpoint.job_id != job_id
    {
        return Err(CliError::Message(format!(
            "feedback checkpoint at {} is incompatible with this render",
            checkpoint_path.display()
        )));
    }
    if checkpoint.contract != *expected_contract || checkpoint.provenance != *expected_provenance {
        return Err(CliError::Message(
            "feedback checkpoint input provenance or settings changed; start with a new output directory"
                .to_string(),
        ));
    }
    if checkpoint.next_frame_index > frame_count {
        return Err(CliError::Message(
            "feedback checkpoint advances beyond the current frame sequence".to_string(),
        ));
    }
    let start_frame = checkpoint.next_frame_index as usize;
    if start_frame == 0 {
        return Ok((0, None, None));
    }

    let expected_state = checkpoint.state.ok_or_else(|| {
        CliError::Message(
            "feedback checkpoint is missing its previous float output state".to_string(),
        )
    })?;
    let relative_state_path = checkpoint.state_path.ok_or_else(|| {
        CliError::Message("feedback checkpoint is missing its state path".to_string())
    })?;
    let state_path = feedback_state_path_from_checkpoint(output_dir, &relative_state_path)?;
    let (actual_state, state) = read_flow_feedback_state(&state_path)?;
    if actual_state != expected_state {
        return Err(CliError::Message(format!(
            "feedback state at {} does not match its checkpoint",
            state_path.display()
        )));
    }
    let previous_frame_path = output_dir
        .join("frames")
        .join(format!("frame_{:06}.png", start_frame - 1));
    if !previous_frame_path.exists() {
        return Err(CliError::Message(format!(
            "feedback checkpoint is missing exported frame {}",
            previous_frame_path.display()
        )));
    }
    Ok((start_frame, Some(state), Some(relative_state_path)))
}

pub(crate) struct FeedbackCheckpointWrite<'a> {
    job_id: &'a str,
    status: &'a str,
    next_frame_index: u32,
    state_path: Option<&'a str>,
    state: Option<FlowFeedbackStateDescriptor>,
    contract: &'a FeedbackSequenceContract,
    provenance: &'a RenderJobProvenance,
}

pub(crate) fn write_feedback_checkpoint(
    output_dir: &Path,
    checkpoint: FeedbackCheckpointWrite<'_>,
) -> Result<(), CliError> {
    let checkpoint = FeedbackSequenceCheckpoint {
        version: FLOW_FEEDBACK_RENDER_CONTRACT_VERSION,
        task: "frame_sequence_flow_feedback".to_string(),
        job_id: checkpoint.job_id.to_string(),
        status: checkpoint.status.to_string(),
        next_frame_index: checkpoint.next_frame_index,
        state_path: checkpoint.state_path.map(str::to_string),
        state: checkpoint.state,
        contract: checkpoint.contract.clone(),
        provenance: checkpoint.provenance.clone(),
    };
    write_feedback_json_atomically(
        &output_dir.join("checkpoint.json"),
        &serde_json::to_string_pretty(&checkpoint)?,
    )?;
    Ok(())
}

pub(crate) struct FeedbackSequenceManifestWrite<'a> {
    job_id: &'a str,
    output_dir: &'a Path,
    frame_paths: &'a [String],
    timing: &'a RenderTimingMetadata,
    contract: &'a FeedbackSequenceContract,
    provenance: &'a RenderJobProvenance,
    state_path: &'a str,
    output_bit_depth: u8,
    temporal_supersampling: u32,
}

pub(crate) fn write_feedback_sequence_manifest(
    request: FeedbackSequenceManifestWrite<'_>,
) -> Result<(), CliError> {
    let FeedbackSequenceManifestWrite {
        job_id,
        output_dir,
        frame_paths,
        timing,
        contract,
        provenance,
        state_path,
        output_bit_depth,
        temporal_supersampling,
    } = request;
    let manifest = serde_json::json!({
        "job_id": job_id,
        "status": "complete",
        "task": "frame_sequence_flow_feedback",
        "frames": frame_paths,
        "audio_stems": [],
        "timing": {
            "frame_rate": timing.frame_rate,
            "frame_count": timing.frame_count,
            "start_seconds": timing.start_seconds,
            "duration_seconds": timing.duration_seconds,
            "sample_rate": timing.sample_rate,
            "audio_sample_count": timing.audio_sample_count
        },
        "feedback_contract": contract,
        "feedback_state_path": state_path,
        "export": {
            "format": "png",
            "bit_depth": output_bit_depth,
            "temporal_supersampling": temporal_supersampling
        },
        "provenance": provenance,
        "deterministic": true
    });
    write_feedback_json_atomically(
        &output_dir.join("manifest.json"),
        &serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

pub(crate) fn validate_feedback_export_settings(
    output_bit_depth: u8,
    temporal_supersampling: u32,
) -> Result<(), CliError> {
    if !matches!(output_bit_depth, 8 | 16) {
        return Err(CliError::Message(
            "output-bit-depth must be either 8 or 16 for PNG feedback exports".to_string(),
        ));
    }
    if temporal_supersampling == 0 {
        return Err(CliError::Message(
            "temporal-supersampling must be at least one".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn feedback_state_relative_path(frame_index: u32) -> String {
    format!("state/feedback_frame_{frame_index:06}.rgba32f")
}

pub(crate) fn feedback_state_path_from_checkpoint(
    output_dir: &Path,
    relative_path: &str,
) -> Result<PathBuf, CliError> {
    let relative_path = Path::new(relative_path);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(CliError::Message(
            "feedback checkpoint state path must be relative to its output directory".to_string(),
        ));
    }
    Ok(output_dir.join(relative_path))
}

pub(crate) fn write_feedback_json_atomically(path: &Path, content: &str) -> Result<(), CliError> {
    let temporary_path = path.with_extension("json.tmp");
    fs::write(&temporary_path, content)?;
    fs::rename(temporary_path, path)?;
    Ok(())
}

pub(crate) struct RmsAmountModulation {
    descriptors: Vec<AudioDescriptorFrame>,
    frame_rate: f64,
    amount_scale: f32,
}

impl RmsAmountModulation {
    fn amount_for_frame(&self, frame_index: usize, base_amount: f32) -> f32 {
        let time_seconds = frame_index as f64 / self.frame_rate;
        let descriptor_count = self
            .descriptors
            .partition_point(|descriptor| descriptor.time_seconds <= time_seconds);
        let rms = descriptor_count
            .checked_sub(1)
            .and_then(|index| self.descriptors.get(index))
            .map(|descriptor| descriptor.rms)
            .unwrap_or(0.0);
        base_amount + rms.max(0.0) * self.amount_scale
    }
}

pub(crate) fn load_rms_amount_modulation(
    config: RmsAmountConfig<'_>,
) -> Result<Option<RmsAmountModulation>, CliError> {
    let Some(wav_path) = config.wav_path else {
        return Ok(None);
    };

    let buffer = load_wav_f32(wav_path)?;
    let descriptors = rms_envelope(&buffer, config.window_size, config.hop_size)?;
    if descriptors.is_empty() {
        return Err(CliError::Message(format!(
            "RMS modulator WAV contains no descriptor frames: {}",
            wav_path.display()
        )));
    }

    Ok(Some(RmsAmountModulation {
        descriptors,
        frame_rate: config.frame_rate,
        amount_scale: config.amount_scale,
    }))
}

// ─── Channel Shift ────────────────────────────────────────────────────────────

pub(crate) struct ChannelShiftSequenceRequest<'a> {
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: ChannelShiftSettings,
    pub(crate) frames: u32,
    pub(crate) backend: RenderBackend,
    /// Source A frames for A-flow-driven per-row shift (Slice 3). None = constant-offset only.
    pub(crate) source_a_dir: Option<&'a Path>,
    /// Per-row shift gain: row_shift_x[y] = mean_x_flow[y] × flow_gain. 0 = off.
    pub(crate) flow_gain: f32,
    /// Lucas-Kanade window radius for optical-flow in A-flow mode.
    pub(crate) radius: i32,
    pub(crate) modulation: ModulationCliArgs<'a>,
}

pub(crate) fn render_channel_shift_sequence(
    request: ChannelShiftSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let flow_active = request.flow_gain != 0.0;

    if flow_active && request.source_a_dir.is_none() {
        return Err(CliError::Message(
            "A-flow-driven mode requires --source-a-dir".to_string(),
        ));
    }
    if flow_active && request.backend == RenderBackend::Metal {
        return Err(CliError::Message(
            "A-flow-driven channel shift is CPU-only; use --backend cpu".to_string(),
        ));
    }

    let source_b_frames = collect_image_frames(request.source_b_dir)?;
    if source_b_frames.is_empty() {
        return Err(CliError::Message(
            "channel shift requires at least one PNG frame in the source B directory".to_string(),
        ));
    }

    let source_a_frames: Option<Vec<_>> =
        request.source_a_dir.map(collect_image_frames).transpose()?;

    let frame_count = (request.frames as usize).min(source_b_frames.len());
    fs::create_dir_all(request.output_dir)?;

    let modulation = request.modulation.build_plan()?;
    if let Some(plan) = &modulation {
        // Dry-run at frame 0 so an unknown target fails before any frame renders.
        let mut probe = request.settings;
        for (target, value) in plan.frame_values(0) {
            apply_channel_shift_modulation(&mut probe, target, value)?;
        }
        println!("modulation routes: {}", plan.describe());
    }

    let mut prev_a: Option<ImageBufferF32> = None;
    let mut metal_flow_validated = false;

    for (index, frame_path) in source_b_frames.iter().enumerate().take(frame_count) {
        let mut frame_settings = request.settings;
        if let Some(plan) = &modulation {
            for (target, value) in plan.frame_values(index) {
                apply_channel_shift_modulation(&mut frame_settings, target, value)?;
            }
        }
        let source_b = load_image_f32(frame_path)?;

        let per_row_shifts: Vec<f32> = if flow_active {
            let a_frames = source_a_frames.as_ref().unwrap();
            let a_idx = index.min(a_frames.len().saturating_sub(1));
            let source_a = load_image_f32(&a_frames[a_idx])?;
            let shifts = if let Some(ref previous_a) = prev_a {
                let flow = compute_optical_flow_backend(
                    previous_a,
                    &source_a,
                    source_b.width,
                    source_b.height,
                    request.radius,
                    request.backend,
                    &mut metal_flow_validated,
                )?;
                compute_per_row_shifts(&flow, request.flow_gain)
            } else {
                // Frame 0: no previous frame → zero per-row shifts.
                vec![0.0f32; source_b.height as usize]
            };
            prev_a = Some(source_a);
            shifts
        } else {
            vec![]
        };

        let rendered = if !flow_active && request.backend == RenderBackend::Metal {
            render_channel_shift_frame_metal(&source_b, &frame_settings)?
        } else {
            render_channel_shift_frame(&source_b, &frame_settings, &per_row_shifts)?
        };
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    let mode_label = if flow_active {
        format!("flow-driven gain {:.2}", request.flow_gain)
    } else {
        format!(
            "R:{:+.1},{:+.1} G:{:+.1},{:+.1} B:{:+.1},{:+.1} px",
            request.settings.shift_r_x,
            request.settings.shift_r_y,
            request.settings.shift_g_x,
            request.settings.shift_g_y,
            request.settings.shift_b_x,
            request.settings.shift_b_y,
        )
    };
    println!(
        "rendered channel shift sequence with {} frame(s) ({}, backend {:?}) from {} to {}",
        frame_count,
        mode_label,
        request.backend,
        request.source_b_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

pub(crate) struct RetroStaticSequenceRequest<'a> {
    pub(crate) source_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: RetroStaticSettings,
    pub(crate) frames: u32,
    pub(crate) backend: RenderBackend,
    pub(crate) modulation: ModulationCliArgs<'a>,
}

pub(crate) fn render_retro_static_sequence(
    request: RetroStaticSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    request.settings.validate()?;
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }
    let source_frames = collect_image_frames(request.source_dir)?;
    if source_frames.is_empty() {
        return Err(CliError::Message(
            "retro static requires at least one PNG frame in the source directory".to_string(),
        ));
    }
    let frame_count = (request.frames as usize).min(source_frames.len());
    fs::create_dir_all(request.output_dir)?;

    let modulation = request.modulation.build_plan()?;
    if let Some(plan) = &modulation {
        // Dry-run at frame 0 so an unknown target fails before any frame renders.
        let mut probe = request.settings;
        for (target, value) in plan.frame_values(0) {
            apply_retro_static_modulation(&mut probe, target, value)?;
        }
        println!("modulation routes: {}", plan.describe());
    }

    for (index, frame_path) in source_frames.iter().enumerate().take(frame_count) {
        let mut frame_settings = request.settings;
        if let Some(plan) = &modulation {
            for (target, value) in plan.frame_values(index) {
                apply_retro_static_modulation(&mut frame_settings, target, value)?;
            }
        }
        let source = load_image_f32(frame_path)?;
        let rendered = if request.backend == RenderBackend::Metal {
            render_retro_static_frame_metal(&source, &frame_settings)?
        } else {
            render_retro_static_frame(&source, &frame_settings)?
        };
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    println!(
        "rendered retro static sequence with {} frame(s) (real_bpp {}, assumed_bpp {}, strength {:.2}, backend {:?}) from {} to {}",
        frame_count,
        request.settings.real_bpp,
        request.settings.assumed_bpp,
        request.settings.strength,
        request.backend,
        request.source_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

#[cfg(target_os = "macos")]
pub(crate) fn render_retro_static_frame_metal(
    source: &ImageBufferF32,
    settings: &RetroStaticSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::retro_static_metal(source, settings)?;
    let cpu = render_retro_static_frame(source, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU retro static outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal retro static diverged from CPU reference by {difference} \
             (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_retro_static_frame_metal(
    _source: &ImageBufferF32,
    _settings: &RetroStaticSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

#[cfg(target_os = "macos")]
pub(crate) fn render_channel_shift_frame_metal(
    source_b: &ImageBufferF32,
    settings: &ChannelShiftSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::channel_shift_metal(source_b, settings)?;
    let cpu = render_channel_shift_frame(source_b, settings, &[])?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU channel shift outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal channel shift diverged from CPU reference by {difference} \
             (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_channel_shift_frame_metal(
    _source_b: &ImageBufferF32,
    _settings: &ChannelShiftSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Palette Quantize — Slice 2 (posterize + neon palette, CPU + Metal)
// ---------------------------------------------------------------------------

pub(crate) struct PaletteQuantizeSequenceRequest<'a> {
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: PaletteQuantizeSettings,
    pub(crate) frames: u32,
    pub(crate) backend: RenderBackend,
}

pub(crate) fn render_palette_quantize_sequence(
    request: PaletteQuantizeSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    if request.frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }

    let source_b_frames = collect_image_frames(request.source_b_dir)?;
    if source_b_frames.is_empty() {
        return Err(CliError::Message(
            "palette quantize requires at least one PNG frame in the source B directory"
                .to_string(),
        ));
    }

    let frame_count = (request.frames as usize).min(source_b_frames.len());
    fs::create_dir_all(request.output_dir)?;

    let mode_label = match request.settings.mode {
        QuantizeMode::Posterize => format!("posterize levels={}", request.settings.levels),
        QuantizeMode::Palette => "neon-palette".to_string(),
        QuantizeMode::Kmeans => "kmeans".to_string(),
    };

    for (index, frame_path) in source_b_frames.iter().enumerate().take(frame_count) {
        let source_b = load_image_f32(frame_path)?;
        let rendered = match request.backend {
            RenderBackend::Cpu => render_palette_quantize_frame(&source_b, &request.settings)?,
            RenderBackend::Metal => {
                render_palette_quantize_frame_metal(&source_b, &request.settings)?
            }
        };
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    println!(
        "rendered palette quantize sequence with {} frame(s) \
         (mode {}, backend {:?}) from {} to {}",
        frame_count,
        mode_label,
        request.backend,
        request.source_b_dir.display(),
        request.output_dir.display(),
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

#[cfg(target_os = "macos")]
pub(crate) fn render_palette_quantize_frame_metal(
    source_b: &ImageBufferF32,
    settings: &PaletteQuantizeSettings,
) -> Result<ImageBufferF32, CliError> {
    let gpu = morphogen_metal::palette_quantize_metal(source_b, settings)?;
    let cpu = render_palette_quantize_frame(source_b, settings)?;
    let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
        CliError::Message(
            "Metal and CPU palette quantize outputs have mismatched dimensions; cannot verify parity"
                .to_string(),
        )
    })?;
    if difference > METAL_CPU_PARITY_EPSILON {
        return Err(CliError::Message(format!(
            "Metal palette quantize diverged from CPU reference by {difference} \
             (tolerance {METAL_CPU_PARITY_EPSILON})"
        )));
    }
    Ok(gpu)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_amount_modulation_uses_latest_descriptor_at_or_before_frame_time() {
        let modulation = RmsAmountModulation {
            descriptors: vec![
                AudioDescriptorFrame {
                    time_seconds: 0.0,
                    rms: 0.25,
                    spectral_centroid_hz: None,
                },
                AudioDescriptorFrame {
                    time_seconds: 0.5,
                    rms: 0.75,
                    spectral_centroid_hz: None,
                },
            ],
            frame_rate: 4.0,
            amount_scale: 8.0,
        };

        assert_eq!(modulation.amount_for_frame(0, 10.0), 12.0);
        assert_eq!(modulation.amount_for_frame(1, 10.0), 12.0);
        assert_eq!(modulation.amount_for_frame(2, 10.0), 16.0);
    }

    #[test]
    fn granular_audio_controls_route_cached_scalars_to_grain_settings() {
        let controls = GranularAudioControls {
            frame_rate: 4.0,
            rms: Some(vec![TimedScalarControl {
                time_seconds: 0.0,
                value: 0.5,
            }]),
            onset: Some(vec![
                TimedScalarControl {
                    time_seconds: 0.0,
                    value: 0.0,
                },
                TimedScalarControl {
                    time_seconds: 0.5,
                    value: 1.0,
                },
            ]),
            centroid: Some(vec![TimedScalarControl {
                time_seconds: 0.0,
                value: 0.5,
            }]),
            rms_variation_scale: 0.6,
            onset_rearrangement_scale: 0.4,
            centroid_grain_size_scale: 8.0,
        };
        let base = GranularMosaicSettings {
            grain_size: 16,
            rearrangement: 0.2,
            variation: 0.1,
            seed: 42,
        };

        let first = controls.settings_for_frame(0, base);
        let second = controls.settings_for_frame(2, base);

        assert_eq!(first.grain_size, 20);
        assert_eq!(first.variation, 0.4);
        assert_eq!(first.rearrangement, 0.2);
        assert_eq!(second.rearrangement, 0.6);
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn render_palette_quantize_frame_metal(
    _source_b: &ImageBufferF32,
    _settings: &PaletteQuantizeSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal backend is only available on macOS; use --backend cpu".to_string(),
    ))
}
