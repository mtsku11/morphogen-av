use std::{
    fs,
    path::{Path, PathBuf},
};

use morphogen_audio::{
    load_wav_f32, rms_envelope, spectral_centroid_from_magnitudes, AudioAnalysisCache, AudioDescriptorFrame, OnsetStrengthCache, StftAnalysisCache,
};
use morphogen_core::{
    AnalysisKind, FlowSource, GrainSelectionMode, GranularAudioModulation, KernelMode,
    RenderBackend, RenderJobAnalysisCacheProvenance, RenderJobProvenance, RenderJobSourceProvenance, RenderTimingMetadata, SourceRole,
};
use morphogen_render::{
    coagulated_blend_frame_cpu, CoagulationSettings,
    analyze_convolution_kernel_cpu, analyze_convolution_kernels_color_cpu,
    convolution_blend_color_cpu, convolution_blend_cpu, ConvolutionBlendSettings, ConvolutionKernel,
    analyze_grain_colors_cpu, analyze_grain_pool_cpu, analyze_grains_cpu, feedback_state_path,
    is_datamosh_keyframe, flow_displace_cpu, flow_feedback_frame_cpu, flow_temporal_supersample_cpu,
    datamosh_block_refresh_composite, datamosh_residual_flow, quantize_flow_to_blocks,
    reset_residual_in_refreshed_blocks, zero_flow,
    granular_mosaic_with_pool_selection_cpu, granular_mosaic_with_selection_cpu,
    luminance_gradient_flow_cpu, pyramidal_lucas_kanade_flow_cpu, read_flow_cache,
    read_flow_feedback_state, read_grain_color_descriptor_cache, read_grain_descriptor_cache,
    read_grain_pool_descriptor_cache, read_grain_selection_cache, select_grains_cpu,
    select_grains_from_pool_cpu, select_grains_multimodal_cpu, video_vocoder_cpu,
    analyze_luma_band_envelope_cpu, apply_tone_map_cpu, luma_specification_tone_map, AntiRepeat,
    TemporalCoherence, VideoVocoderSettings, write_flow_cache,
    write_flow_cache_with_source_fingerprint, write_flow_feedback_state,
    write_grain_color_descriptor_cache, write_grain_descriptor_cache,
    write_grain_pool_descriptor_cache, write_grain_selection_cache, FlowFeedbackSettings,
    FlowFeedbackStateDescriptor, FlowField, GrainColorDescriptor, GrainDescriptor, GrainPool,
    GrainSelection, GranularMosaicSettings, ImageBufferF32, PoolSelectionWindow,
    RmsDisplacementEnvelope, uniform_displacement_field,
    FLOW_VECTOR_CONVENTION, GRAIN_COLOR_DESCRIPTOR_CACHE_FILE_NAME, GRAIN_DESCRIPTOR_CACHE_FILE_NAME,
    GRAIN_POOL_DESCRIPTOR_CACHE_FILE_NAME, GRAIN_SELECTION_CACHE_FILE_NAME, GRANULAR_MOSAIC_ALGORITHM,
    LUCAS_KANADE_WINDOW_RADIUS, MULTIMODAL_GRAIN_ALGORITHM, POOLED_GRAIN_ALGORITHM,
};
use serde::{Deserialize, Serialize};

use crate::args::*;
use crate::error::CliError;
use crate::imaging::*;
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
        render_granular_mosaic_frame(&modulator, &carrier, settings, None, backend, selection_mode)?
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
        let rendered =
            render_video_vocoder_frame(&modulator, &carrier, settings, mode, backend)?;
        save_png(
            &rendered,
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
    let rms_frames = rms_envelope(&buffer, request.rms_window as usize, request.rms_hop as usize)?;
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
        let field =
            uniform_displacement_field(carrier.width, carrier.height, request.shift_x, request.shift_y)?;
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
    pub(crate) keyframe_interval: u32,
    pub(crate) amount: f32,
    pub(crate) block_size: u32,
    pub(crate) residual_gain: f32,
    pub(crate) residual_decay: f32,
    pub(crate) refresh_threshold: f32,
    pub(crate) backend: RenderBackend,
    pub(crate) max_frames: Option<usize>,
}

/// Render a controlled-datamosh ("bloom/melt") sequence. Source A's per-frame
/// optical flow (Lucas-Kanade between consecutive A frames) advects Source B's
/// *previous output*; keyframes (`is_datamosh_keyframe`) snap back to the carrier.
/// The recursion carries the previous output as RGBA32F in memory (the
/// unquantized internal state), never re-reading a display PNG.
pub(crate) fn render_datamosh_sequence(
    request: DatamoshSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    if !request.amount.is_finite() || request.amount < 0.0 {
        return Err(CliError::Message(
            "amount must be finite and non-negative".to_string(),
        ));
    }
    if !request.residual_gain.is_finite() || request.residual_gain < 0.0 {
        return Err(CliError::Message(
            "residual-gain must be finite and non-negative".to_string(),
        ));
    }
    if !request.residual_decay.is_finite() || request.residual_decay < 0.0 {
        return Err(CliError::Message(
            "residual-decay must be finite and non-negative".to_string(),
        ));
    }
    if !request.refresh_threshold.is_finite() || request.refresh_threshold < 0.0 {
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

    fs::create_dir_all(request.output_dir)?;

    // Residual accumulation is active only with a positive gain over coarse
    // blocks; otherwise the loop uses the plain block-quantize path (gain 0 ⇒
    // byte-identical block tier, by construction).
    let residual_active = request.residual_gain > 0.0 && request.block_size >= 2;
    // Per-block keep/drop refresh is active only with a positive threshold over
    // coarse blocks (threshold 0 ⇒ byte-identical to the block/residual path).
    let refresh_active = request.refresh_threshold > 0.0 && request.block_size >= 2;

    let mut previous_output: Option<ImageBufferF32> = None;
    let mut previous_modulator: Option<ImageBufferF32> = None;
    // Per-pixel residual accumulator (the second stateful channel). Reset to zero
    // at frame zero and every keyframe (an I-frame clears accumulated residual).
    let mut accumulated_residual: Option<FlowField> = None;
    for index in 0..frame_count {
        let carrier = load_image_f32(&carrier_frames[index])?;
        let modulator = load_image_f32(&modulator_frames[index])?;
        let is_keyframe = is_datamosh_keyframe(index, request.keyframe_interval);

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
                let flow = pyramidal_lucas_kanade_flow_cpu(
                    previous_modulator,
                    &modulator,
                    carrier.width,
                    carrier.height,
                    LUCAS_KANADE_WINDOW_RADIUS,
                )?
                .flow;
                // Codec-simulated mosh: quantize A's flow to a coarse block grid
                // (CPU flow transform) so whole macroblocks slide; block_size <= 1
                // returns the flow unchanged (the smooth bloom path). With residual
                // active, the discarded intra-block motion is accumulated and
                // re-injected (also a pure CPU flow transform). The displace that
                // follows is the existing parity-gated kernel on either backend, so
                // Metal stays free.
                let effective = if residual_active {
                    let accum = accumulated_residual
                        .take()
                        .unwrap_or(zero_flow(carrier.width, carrier.height)?);
                    let (effective, new_accum) = datamosh_residual_flow(
                        &flow,
                        &accum,
                        request.block_size,
                        request.residual_gain,
                        request.residual_decay,
                    )?;
                    accumulated_residual = Some(new_accum);
                    effective
                } else {
                    quantize_flow_to_blocks(&flow, request.block_size)?
                };
                let advected = render_datamosh_advect_frame(
                    previous,
                    &effective,
                    request.amount,
                    request.backend,
                )?;
                // Per-block keep/drop: macroblocks whose mean motion is below the
                // threshold snap back to the carrier B[i] (intra-block refresh)
                // while busier blocks keep rotting. A pure CPU composite over the
                // gated displace output, so Metal stays free; refreshed blocks also
                // clear their residual accumulator (matching the keyframe reset).
                if refresh_active {
                    let block_means = quantize_flow_to_blocks(&flow, request.block_size)?;
                    let composed = datamosh_block_refresh_composite(
                        &advected,
                        &carrier,
                        &block_means,
                        request.refresh_threshold,
                    )?;
                    if let Some(accum) = accumulated_residual.take() {
                        accumulated_residual = Some(reset_residual_in_refreshed_blocks(
                            &accum,
                            &block_means,
                            request.refresh_threshold,
                        )?);
                    }
                    composed
                } else {
                    advected
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
    }

    println!(
        "rendered datamosh sequence with {} frame(s) (keyframe-interval {}, amount {}, block-size {}, residual-gain {}, residual-decay {}, block-refresh-threshold {}, {:?}) from {} moshing {} to {}",
        frame_count,
        request.keyframe_interval,
        request.amount,
        request.block_size,
        request.residual_gain,
        request.residual_decay,
        request.refresh_threshold,
        request.backend,
        request.modulator_dir.display(),
        request.carrier_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
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
    pub(crate) p_frame_index: u32,
    pub(crate) duplicate_count: u32,
}

const DATAMOSH_BITSTREAM_ALGORITHM: &str = "datamosh_bitstream_pframe_dup_experimental_v1";

#[derive(Serialize)]
struct DatamoshBitstreamSidecar {
    algorithm: String,
    /// Always false: ffmpeg's MPEG-4 codec makes this output non-reproducible.
    deterministic: bool,
    input: String,
    fps: f64,
    codec: String,
    p_frame_index: u32,
    duplicate_count: u32,
    p_frames_available: u32,
    ffmpeg_version: String,
    note: String,
}

/// EXPERIMENTAL, NON-DETERMINISTIC real bitstream datamosh. Encodes `input` to a
/// P-frame-only AVI/MPEG-4 via external ffmpeg, duplicates a chosen P-frame's
/// compressed chunk (`morphogen_media::duplicate_p_frame`) so its motion vectors
/// re-bloom on redecode, then decodes the mangled stream to a PNG sequence. This
/// path lives OUTSIDE the deterministic render graph by design — there is no parity
/// gate and the output is not bit-reproducible (it depends on ffmpeg's codec).
pub(crate) fn datamosh_bitstream(request: DatamoshBitstreamRequest<'_>) -> Result<(), CliError> {
    fs::create_dir_all(request.output_dir)?;
    let encoded = request.output_dir.join("encoded.avi");
    let moshed = request.output_dir.join("moshed.avi");

    morphogen_media::encode_datamosh_avi(request.input, &encoded, request.fps)?;
    let encoded_bytes = fs::read(&encoded)?;
    let p_frames_available = morphogen_media::count_p_frames(&encoded_bytes)?;
    let moshed_bytes = morphogen_media::duplicate_p_frame(
        &encoded_bytes,
        request.p_frame_index,
        request.duplicate_count,
    )?;
    fs::write(&moshed, &moshed_bytes)?;
    morphogen_media::decode_avi_frames(&moshed, request.output_dir)?;

    let sidecar = DatamoshBitstreamSidecar {
        algorithm: DATAMOSH_BITSTREAM_ALGORITHM.to_string(),
        deterministic: false,
        input: request.input.to_string_lossy().to_string(),
        fps: request.fps,
        codec: "mpeg4".to_string(),
        p_frame_index: request.p_frame_index,
        duplicate_count: request.duplicate_count,
        p_frames_available,
        ffmpeg_version: morphogen_media::ffmpeg_version().unwrap_or_default(),
        note: "Experimental real bitstream datamosh: output is NOT bit-reproducible \
               (depends on the external ffmpeg MPEG-4 codec) and lives outside the \
               deterministic render graph."
            .to_string(),
    };
    let sidecar_path = request.output_dir.join("datamosh_bitstream.json");
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar)?)?;

    // The encoded source AVI is a disposable intermediate; the moshed AVI is itself
    // a playable deliverable, so it is kept alongside the decoded frames.
    let _ = fs::remove_file(&encoded);

    println!(
        "datamosh-bitstream (EXPERIMENTAL, non-deterministic): bloomed P-frame {} x{} of {} P-frames -> {}",
        request.p_frame_index,
        request.duplicate_count,
        p_frames_available,
        request.output_dir.display()
    );
    Ok(())
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
                let kernels =
                    analyze_convolution_kernels_color_cpu(&modulator, request.settings.kernel_size)?;
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

pub(crate) struct CoagulatedBlendSequenceRequest<'a> {
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) settings: CoagulationSettings,
    pub(crate) max_frames: Option<usize>,
}

/// Render the descriptor-coagulated flow blend over a paired PNG sequence (Slice 1:
/// CPU-only, single-frame — no advection/feedback yet). Each output frame blends the
/// paired A/B frame by an A/B ownership field grouped on per-cell descriptors.
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

    for index in 0..frame_count {
        let source_a = load_image_f32(&source_a_frames[index])?;
        let source_b = load_image_f32(&source_b_frames[index])?;
        let rendered = coagulated_blend_frame_cpu(&source_a, &source_b, request.settings)?;
        save_png(
            &rendered,
            &request.output_dir.join(format!("frame_{index:06}.png")),
        )?;
    }

    if source_a_frames.len() != source_b_frames.len() {
        println!(
            "source frame counts differ: {} A frame(s), {} B frame(s); rendered common prefix",
            source_a_frames.len(),
            source_b_frames.len()
        );
    }
    println!(
        "rendered coagulated blend sequence with {} frame(s) (patch {}, strength {}, coherence {}x{}, edge {}) from {} blended with {} to {}",
        frame_count,
        request.settings.patch_size,
        request.settings.coagulation_strength,
        request.settings.coherence_passes,
        request.settings.coherence_strength,
        request.settings.edge_hardness,
        request.source_a_dir.display(),
        request.source_b_dir.display(),
        request.output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
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
            select_grains_cpu(modulator, carrier.width, carrier.height, &descriptors, settings)?
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
        RenderBackend::Metal => {
            render_granular_mosaic_pool_output_metal(pool_frames, pool, carrier, selection, settings)
        }
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
    let gpu =
        morphogen_metal::granular_mosaic_pool_metal(pool_frames, pool, carrier, selection, settings)?;
    let cpu = granular_mosaic_with_pool_selection_cpu(pool_frames, pool, carrier, selection, settings)?;
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
                                flow_algorithm,
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
                            pyramidal_lucas_kanade_flow_cpu(
                                &previous_modulator,
                                &modulator,
                                carrier.width,
                                carrier.height,
                                LUCAS_KANADE_WINDOW_RADIUS,
                            )?
                            .flow,
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
                        flow_algorithm,
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
    for cache in [modulator_rms_cache, carrier_rms_cache].into_iter().flatten() {
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
