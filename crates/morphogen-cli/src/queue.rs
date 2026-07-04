use std::{
    fs,
    path::{Path, PathBuf},
};

use morphogen_audio::{save_wav_f32, AudioBufferF32};
use morphogen_core::{
    AnalysisKind, DatamoshBitstreamOperation, DatamoshBitstreamPreset, DatamoshPreset,
    ExportFormat, FlowSource, GrainSelectionMode, GranularAudioModulation, KernelMode,
    LfoShape as CoreLfoShape, ModulationSampling as CoreModulationSampling,
    ModulationSource as CoreModulationSource, NamedModulatorMedia, PixelSortAxis,
    PixelSortDirection, PixelSortKey, PixelSortMaskSource, RenderBackend, RenderJob,
    RenderJobAnalysisCacheProvenance, RenderJobFailure, RenderJobModulationRoute,
    RenderJobOutputMetadata, RenderJobProvenance, RenderJobSourceProvenance, RenderJobStatus,
    RenderJobTask, RenderQuality, RenderQueue, RenderSettings, RenderTimingMetadata, SourceRole,
    VectorRemixMode, VideoVocoderMode,
};
use morphogen_render::{
    apply_channel_shift_modulation, apply_flow_feedback_modulation, apply_fluid_advect_modulation,
    apply_fluid_advect_two_source_modulation, apply_palette_quantize_modulation,
    apply_pixel_sort_modulation, apply_retro_static_modulation, apply_rutt_etra_modulation,
    flow_displace_cpu, parse_modulation_route, validate_route_targets, BlendMode,
    BlockCollageSettings, CascadeCollageSettings, CascadeFieldType, CascadeTrailSettings,
    ChannelShiftSettings, ConvolutionBlendSettings, FieldParticleSettings, FlowFeedbackSettings,
    FluidAdvectSettings, FluidAdvectTwoSourceSettings, GranularMosaicSettings, LfoShape,
    MaskSource, ModulationSampling, ModulationSource, PaletteQuantizeSettings, PixelSortSettings,
    QuantizeMode, RetroStaticSettings, RuttEtraSettings, ScanlineFilter, SortAxis, SortDirection,
    SortKey, StructureMode, VideoVocoderSettings, BLOCK_COLLAGE_ALGORITHM,
    CASCADE_COLLAGE_ALGORITHM, CASCADE_TRAIL_ALGORITHM, CHANNEL_SHIFT_ALGORITHM,
    CHANNEL_SHIFT_FLOW_ALGORITHM, FIELD_PARTICLES_ALGORITHM, FLUID_ADVECT_ALGORITHM,
    FLUID_ADVECT_TWO_SOURCE_ALGORITHM, PALETTE_QUANTIZE_ALGORITHM, PIXEL_SORT_ALGORITHM,
    PIXEL_SORT_CROSS_SYNTH_ALGORITHM, POOLED_GRAIN_ALGORITHM, RETRO_STATIC_ALGORITHM,
    RMS_DISPLACEMENT_ROUTE_ALGORITHM, RUTT_ETRA_ALGORITHM, RUTT_ETRA_METAL_ALGORITHM,
};

use crate::args::*;
use crate::error::CliError;
use crate::imaging::*;
use crate::modulate::{
    apply_datamosh_modulation, parse_named_modulator_specs, resolve_modulator_media,
};
use crate::render::*;
pub(crate) fn queue_init(queue_path: &Path) -> Result<(), CliError> {
    let queue = RenderQueue::default();
    queue.save_json(queue_path)?;
    println!("wrote empty render queue to {}", queue_path.display());
    Ok(())
}

pub(crate) fn queue_add_test(
    queue_path: &Path,
    project_path: Option<&Path>,
) -> Result<(), CliError> {
    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 16,
            },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::TestRender,
        provenance: None,
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!("queued render job {job_id} in {}", queue_path.display());
    Ok(())
}

fn load_or_default_queue(queue_path: &Path) -> Result<RenderQueue, CliError> {
    if queue_path.exists() {
        Ok(RenderQueue::load_json(queue_path)?)
    } else {
        Ok(RenderQueue::default())
    }
}

fn png_sequence_settings(_frame_rate: f64) -> RenderSettings {
    RenderSettings {
        width: 1920,
        height: 1080,
        quality: RenderQuality::HighQualityOffline,
        export_format: ExportFormat::ImageSequence {
            extension: "png".to_string(),
            bit_depth: 8,
        },
        temporal_supersampling: 1,
        deterministic: true,
    }
}

fn validate_queued_sequence_timing(frames: u32, frame_rate: f64) -> Result<(), CliError> {
    if frames == 0 {
        return Err(CliError::Message(
            "frames must be greater than zero".to_string(),
        ));
    }
    if !frame_rate.is_finite() || frame_rate <= 0.0 {
        return Err(CliError::Message(
            "frame-rate must be a positive finite number".to_string(),
        ));
    }
    Ok(())
}

fn single_source_provenance(
    source_id: &str,
    role: SourceRole,
    source_dir: &Path,
) -> RenderJobProvenance {
    RenderJobProvenance {
        sources: vec![RenderJobSourceProvenance {
            source_id: source_id.to_string(),
            role,
            path: source_dir.to_string_lossy().to_string(),
        }],
        analysis_caches: Vec::new(),
    }
}

fn two_source_provenance(modulator_dir: &Path, carrier_dir: &Path) -> RenderJobProvenance {
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
        analysis_caches: Vec::new(),
    }
}

pub(crate) struct QueueAddFrameSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) amount: f32,
    pub(crate) max_frames: Option<u32>,
    pub(crate) frame_rate: f64,
    pub(crate) write_flow_cache: bool,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
}

pub(crate) fn queue_add_frame_sequence(
    request: QueueAddFrameSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddFrameSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
        amount,
        max_frames,
        frame_rate,
        write_flow_cache,
        backend,
        project_path,
    } = request;

    if !amount.is_finite() {
        return Err(CliError::Message("amount must be finite".to_string()));
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

    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);
    let flow_cache_directory = write_flow_cache
        .then(|| job_output_dir.join("cache").join("flow"))
        .map(|path| path.to_string_lossy().to_string());

    let analysis_caches = flow_cache_directory
        .as_ref()
        .map(|path| {
            vec![RenderJobAnalysisCacheProvenance {
                kind: AnalysisKind::OpticalFlow,
                path: path.clone(),
                producer: "luminance_gradient_cpu_v1".to_string(),
            }]
        })
        .unwrap_or_default();

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 16,
            },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::FrameSequenceFlowDisplace {
            modulator_frame_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            flow_cache_directory,
            amount,
            max_frames,
            frame_rate,
            backend,
        },
        provenance: Some(RenderJobProvenance {
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
            analysis_caches,
        }),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued frame-sequence render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) struct QueueAddFluidAdvectSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: FluidAdvectSettings,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_fluid_advect_sequence(
    request: QueueAddFluidAdvectSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddFluidAdvectSequenceRequest {
        queue_path,
        source_dir,
        output_root_dir,
        settings,
        frames,
        frame_rate,
        backend,
        project_path,
        modulate,
        modulator_audio,
        modulator_frames,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = request;
    settings.validate()?;
    validate_queued_sequence_timing(frames, frame_rate)?;
    let modulation = parse_queue_modulation_routes(
        modulate,
        modulator_audio,
        modulator_frames,
        named_modulator_audio,
        named_modulator_frames,
        |target| {
            let mut probe = settings;
            apply_fluid_advect_modulation(&mut probe, target, 0.0).map_err(CliError::from)
        },
    )?;

    let mut queue = load_or_default_queue(queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: png_sequence_settings(frame_rate),
        task: RenderJobTask::FrameSequenceFluidAdvect {
            source_frame_directory: source_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames,
            frame_rate,
            advect: settings.advect,
            turbulence_scale: settings.turbulence_scale,
            turbulence_speed: settings.turbulence_speed,
            detail: settings.detail,
            reinject: settings.reinject,
            seed: settings.seed,
            backend,
            modulation_routes: modulation.routes,
            modulator_audio_path: modulator_audio.map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: modulator_frames.map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            source_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued fluid-advect render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) struct QueueAddFluidAdvectTwoSourceSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: FluidAdvectTwoSourceSettings,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_fluid_advect_two_source_sequence(
    request: QueueAddFluidAdvectTwoSourceSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddFluidAdvectTwoSourceSequenceRequest {
        queue_path,
        source_a_dir,
        source_b_dir,
        output_root_dir,
        settings,
        frames,
        frame_rate,
        backend,
        project_path,
        modulate,
        modulator_audio,
        modulator_frames,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = request;
    settings.validate()?;
    validate_queued_sequence_timing(frames, frame_rate)?;
    let modulation = parse_queue_modulation_routes(
        modulate,
        modulator_audio,
        modulator_frames,
        named_modulator_audio,
        named_modulator_frames,
        |target| {
            let mut probe = settings;
            apply_fluid_advect_two_source_modulation(&mut probe, target, 0.0)
                .map_err(CliError::from)
        },
    )?;

    let mut queue = load_or_default_queue(queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: png_sequence_settings(frame_rate),
        task: RenderJobTask::FrameSequenceFluidAdvectTwoSource {
            modulator_frame_directory: source_a_dir.to_string_lossy().to_string(),
            carrier_frame_directory: source_b_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames,
            frame_rate,
            advect: settings.advect,
            reinject: settings.reinject,
            backend,
            modulation_routes: modulation.routes,
            modulator_audio_path: modulator_audio.map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: modulator_frames.map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(two_source_provenance(source_a_dir, source_b_dir)),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued two-source fluid-advect render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) struct QueueAddOpticalFlowAdvectSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: FluidAdvectTwoSourceSettings,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_optical_flow_advect_sequence(
    request: QueueAddOpticalFlowAdvectSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddOpticalFlowAdvectSequenceRequest {
        queue_path,
        source_dir,
        output_root_dir,
        settings,
        frames,
        frame_rate,
        backend,
        project_path,
        modulate,
        modulator_audio,
        modulator_frames,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = request;
    settings.validate()?;
    validate_queued_sequence_timing(frames, frame_rate)?;
    let modulation = parse_queue_modulation_routes(
        modulate,
        modulator_audio,
        modulator_frames,
        named_modulator_audio,
        named_modulator_frames,
        |target| {
            let mut probe = settings;
            apply_fluid_advect_two_source_modulation(&mut probe, target, 0.0)
                .map_err(CliError::from)
        },
    )?;

    let mut queue = load_or_default_queue(queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: png_sequence_settings(frame_rate),
        task: RenderJobTask::FrameSequenceOpticalFlowAdvect {
            source_frame_directory: source_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames,
            frame_rate,
            advect: settings.advect,
            reinject: settings.reinject,
            backend,
            modulation_routes: modulation.routes,
            modulator_audio_path: modulator_audio.map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: modulator_frames.map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            source_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued optical-flow advect render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) struct QueueAddFieldParticlesSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: FieldParticleSettings,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
}

pub(crate) fn queue_add_field_particles_sequence(
    request: QueueAddFieldParticlesSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddFieldParticlesSequenceRequest {
        queue_path,
        source_dir,
        output_root_dir,
        settings,
        frames,
        frame_rate,
        backend,
        project_path,
    } = request;
    settings.validate()?;
    validate_queued_sequence_timing(frames, frame_rate)?;

    let mut queue = load_or_default_queue(queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: png_sequence_settings(frame_rate),
        task: RenderJobTask::FrameSequenceFieldParticles {
            source_frame_directory: source_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames,
            frame_rate,
            spacing: settings.spacing,
            particle_size: settings.particle_size,
            advect: settings.advect,
            turbulence_scale: settings.turbulence_scale,
            turbulence_speed: settings.turbulence_speed,
            detail: settings.detail,
            live_color: settings.live_color,
            seed: settings.seed,
            backend,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            source_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued field-particles render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) struct QueueAddCascadeTrailsSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: CascadeTrailSettings,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) project_path: Option<&'a Path>,
}

pub(crate) fn queue_add_cascade_trails_sequence(
    request: QueueAddCascadeTrailsSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddCascadeTrailsSequenceRequest {
        queue_path,
        source_dir,
        output_root_dir,
        settings,
        frames,
        frame_rate,
        project_path,
    } = request;
    settings.validate()?;
    validate_queued_sequence_timing(frames, frame_rate)?;

    let mut queue = load_or_default_queue(queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: png_sequence_settings(frame_rate),
        task: RenderJobTask::FrameSequenceCascadeTrails {
            source_frame_directory: source_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames,
            frame_rate,
            tile_size: settings.tile_size,
            grid_spacing: settings.grid_spacing,
            advect: settings.advect,
            turbulence_scale: settings.turbulence_scale,
            detail: settings.detail,
            live_refresh: settings.live_refresh,
            seed: settings.seed,
            field: cascade_field_type_label(settings.field),
            river_direction: settings.river_direction,
            river_speed: settings.river_speed,
            river_turbulence: settings.river_turbulence,
            temporal_tiles: settings.temporal_tiles,
            decay: settings.decay,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            source_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued cascade-trails render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) struct QueueAddCascadeCollageSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) scrib_amp_scale: f32,
    pub(crate) morph_rate: f32,
    pub(crate) frame_hue_rate: f32,
    pub(crate) bright_osc: f32,
    pub(crate) edge_width: f32,
    pub(crate) edge_strength: f32,
    pub(crate) face_strength: f32,
    pub(crate) face_sat: f32,
    pub(crate) hue_steps: u32,
    pub(crate) edge_detect: f32,
    pub(crate) tile_scale: f32,
    pub(crate) detail_tiles: u32,
    pub(crate) hue_rotate: f32,
    pub(crate) block_blend: BlendMode,
    pub(crate) block_opacity: f32,
    pub(crate) seed: u64,
    pub(crate) project_path: Option<&'a Path>,
}

pub(crate) fn queue_add_cascade_collage_sequence(
    request: QueueAddCascadeCollageSequenceRequest<'_>,
) -> Result<(), CliError> {
    validate_queued_sequence_timing(request.frames, request.frame_rate)?;
    // build-then-validate: catches invalid knobs before the job is persisted
    let mut probe_settings = CascadeCollageSettings {
        scrib_amp_scale: request.scrib_amp_scale,
        morph_rate: request.morph_rate,
        frame_hue_rate: request.frame_hue_rate,
        bright_osc: request.bright_osc,
        edge_width: request.edge_width,
        edge_strength: request.edge_strength,
        face_strength: request.face_strength,
        face_sat: request.face_sat,
        hue_steps: request.hue_steps,
        edge_detect: request.edge_detect,
        block_blend: request.block_blend,
        block_opacity: request.block_opacity,
        seed: request.seed,
        ..CascadeCollageSettings::default()
    };
    apply_cascade_generative(
        &mut probe_settings,
        request.tile_scale,
        request.detail_tiles,
        request.hue_rotate,
    );
    probe_settings.validate()?;

    let mut queue = load_or_default_queue(request.queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = request.output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: request
            .project_path
            .map(|p| p.to_string_lossy().to_string()),
        settings: png_sequence_settings(request.frame_rate),
        task: RenderJobTask::FrameSequenceCascadeCollage {
            source_frame_directory: request.source_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames: request.frames,
            frame_rate: request.frame_rate,
            scrib_amp_scale: request.scrib_amp_scale,
            morph_rate: request.morph_rate,
            frame_hue_rate: request.frame_hue_rate,
            bright_osc: request.bright_osc,
            edge_width: request.edge_width,
            edge_strength: request.edge_strength,
            face_strength: request.face_strength,
            face_sat: request.face_sat,
            hue_steps: request.hue_steps,
            edge_detect: request.edge_detect,
            tile_scale: request.tile_scale,
            detail_tiles: request.detail_tiles,
            hue_rotate: request.hue_rotate,
            block_blend: cascade_block_blend_label(request.block_blend),
            block_opacity: request.block_opacity,
            seed: request.seed,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            request.source_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(request.queue_path)?;
    println!(
        "queued cascade-collage render job {job_id} in {}",
        request.queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_cascade_collage_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceCascadeCollage { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running cascade-collage jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceCascadeCollage {
        source_frame_directory,
        output_directory,
        frames,
        frame_rate,
        scrib_amp_scale,
        morph_rate,
        frame_hue_rate,
        bright_osc,
        edge_width,
        edge_strength,
        face_strength,
        face_sat,
        hue_steps,
        edge_detect,
        tile_scale,
        detail_tiles,
        hue_rotate,
        block_blend,
        block_opacity,
        seed,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a cascade-collage render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let mut settings = CascadeCollageSettings {
        scrib_amp_scale,
        morph_rate,
        frame_hue_rate,
        bright_osc,
        edge_width,
        edge_strength,
        face_strength,
        face_sat,
        hue_steps,
        edge_detect,
        block_blend: parse_cascade_block_blend(&block_blend),
        block_opacity,
        seed,
        ..CascadeCollageSettings::default()
    };
    apply_cascade_generative(&mut settings, tile_scale, detail_tiles, hue_rotate);

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_cascade_collage_sequence(CascadeCollageSequenceRequest {
            source_dir: Some(Path::new(&source_frame_directory)),
            output_dir: &output_dir.join("frames"),
            width: 0,
            height: 0,
            frames,
            settings: settings.clone(),
        })?;
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_cascade_collage",
            effect_key: "cascade_collage",
            effect: serde_json::json!({
                "algorithm": CASCADE_COLLAGE_ALGORITHM,
                "settings": settings,
                "backend": "CPU"
            }),
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "cascade-collage",
    )
}

fn cascade_block_blend_label(mode: BlendMode) -> String {
    match mode {
        BlendMode::Normal => "normal".to_string(),
        BlendMode::Multiply => "multiply".to_string(),
        BlendMode::Screen => "screen".to_string(),
        BlendMode::Average => "average".to_string(),
        BlendMode::Lighten => "lighten".to_string(),
    }
}

fn parse_cascade_block_blend(s: &str) -> BlendMode {
    match s {
        "multiply" => BlendMode::Multiply,
        "screen" => BlendMode::Screen,
        "average" => BlendMode::Average,
        "lighten" => BlendMode::Lighten,
        _ => BlendMode::Normal,
    }
}

pub(crate) struct QueueAddGranularMosaicSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: GranularMosaicSettings,
    pub(crate) audio_modulation: Option<GranularAudioModulation>,
    pub(crate) max_frames: Option<u32>,
    pub(crate) frame_rate: f64,
    pub(crate) write_grain_cache: bool,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
    pub(crate) selection_mode: GrainSelectionMode,
}

pub(crate) fn queue_add_granular_mosaic_sequence(
    request: QueueAddGranularMosaicSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddGranularMosaicSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
        settings,
        audio_modulation,
        max_frames,
        frame_rate,
        write_grain_cache,
        project_path,
        backend,
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

    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);
    let grain_cache_directory = write_grain_cache
        .then(|| job_output_dir.join("cache").join("grains"))
        .map(|path| path.to_string_lossy().to_string());
    let provenance = granular_mosaic_provenance(
        modulator_dir,
        carrier_dir,
        grain_cache_directory.as_deref().map(Path::new),
        audio_modulation.as_ref(),
        selection_mode,
    );

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 8,
            },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::FrameSequenceGranularMosaic {
            modulator_frame_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            grain_cache_directory,
            grain_size: settings.grain_size,
            rearrangement: settings.rearrangement,
            variation: settings.variation,
            seed: settings.seed,
            max_frames,
            frame_rate,
            backend,
            audio_modulation,
            selection_mode,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued granular-mosaic render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) struct QueueAddGranularMosaicPoolSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
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
    pub(crate) max_frames: Option<u32>,
    pub(crate) frame_rate: f64,
    pub(crate) write_grain_cache: bool,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
}

pub(crate) fn queue_add_granular_mosaic_pool_sequence(
    request: QueueAddGranularMosaicPoolSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddGranularMosaicPoolSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
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
        max_frames,
        frame_rate,
        write_grain_cache,
        project_path,
        backend,
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
    if !audio_weight.is_finite() || audio_weight < 0.0 {
        return Err(CliError::Message(
            "audio-weight must be a finite, non-negative number".to_string(),
        ));
    }
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
    if !texture_weight.is_finite() || texture_weight < 0.0 {
        return Err(CliError::Message(
            "texture-weight must be a finite, non-negative number".to_string(),
        ));
    }

    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);
    let grain_cache_directory = write_grain_cache
        .then(|| job_output_dir.join("cache").join("pool"))
        .map(|path| path.to_string_lossy().to_string());
    let modulator_rms_cache = modulator_rms_cache.map(|path| path.to_string_lossy().to_string());
    let carrier_rms_cache = carrier_rms_cache.map(|path| path.to_string_lossy().to_string());
    let modulator_centroid_cache =
        modulator_centroid_cache.map(|path| path.to_string_lossy().to_string());
    let carrier_centroid_cache =
        carrier_centroid_cache.map(|path| path.to_string_lossy().to_string());
    let provenance = granular_mosaic_pool_provenance(
        modulator_dir,
        carrier_dir,
        grain_cache_directory.as_deref().map(Path::new),
        modulator_rms_cache.as_deref(),
        carrier_rms_cache.as_deref(),
        modulator_centroid_cache.as_deref(),
        carrier_centroid_cache.as_deref(),
    );

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 8,
            },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::FrameSequenceGranularMosaicPool {
            modulator_frame_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            grain_cache_directory,
            grain_size: settings.grain_size,
            rearrangement: settings.rearrangement,
            variation: settings.variation,
            seed: settings.seed,
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
            max_frames,
            frame_rate,
            backend,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued granular-mosaic pool render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) struct QueueAddVideoVocoderSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: VideoVocoderSettings,
    pub(crate) mode: VideoVocoderMode,
    pub(crate) max_frames: Option<u32>,
    pub(crate) frame_rate: f64,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
}

pub(crate) fn queue_add_video_vocoder_sequence(
    request: QueueAddVideoVocoderSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddVideoVocoderSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
        settings,
        mode,
        max_frames,
        frame_rate,
        project_path,
        backend,
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
    if backend == RenderBackend::Metal && mode == VideoVocoderMode::Gain {
        return Err(CliError::Message(
            "the Metal backend is only implemented for --mode match; use --backend cpu for gain mode"
                .to_string(),
        ));
    }

    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);
    let provenance = RenderJobProvenance {
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
        analysis_caches: Vec::new(),
    };

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 8,
            },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::FrameSequenceVideoVocoder {
            modulator_frame_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            bands: settings.bands,
            amount: settings.amount,
            mode,
            max_frames,
            frame_rate,
            backend,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued video-vocoder render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) struct QueueAddFeedbackSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: FlowFeedbackSettings,
    pub(crate) output_bit_depth: u8,
    pub(crate) temporal_supersampling: u32,
    pub(crate) max_frames: Option<u32>,
    pub(crate) reset_at_frame: Option<u32>,
    pub(crate) frame_rate: f64,
    pub(crate) write_flow_cache: bool,
    pub(crate) backend: RenderBackend,
    pub(crate) flow_source: FlowSource,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_feedback_sequence(
    request: QueueAddFeedbackSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddFeedbackSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
        settings,
        output_bit_depth,
        temporal_supersampling,
        max_frames,
        reset_at_frame,
        frame_rate,
        write_flow_cache,
        backend,
        flow_source,
        project_path,
        modulate,
        modulator_audio,
        modulator_frames,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = request;

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
    let modulation = parse_queue_modulation_routes(
        modulate,
        modulator_audio,
        modulator_frames,
        named_modulator_audio,
        named_modulator_frames,
        |target| {
            let mut probe = settings;
            apply_flow_feedback_modulation(&mut probe, target, 0.0).map_err(CliError::from)
        },
    )?;
    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);
    let flow_cache_directory = write_flow_cache
        .then(|| job_output_dir.join("cache").join("flow"))
        .map(|path| path.to_string_lossy().to_string());
    let provenance = feedback_sequence_provenance(
        modulator_dir,
        carrier_dir,
        flow_cache_directory.as_deref().map(Path::new),
        flow_source_algorithm(flow_source),
    );

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: output_bit_depth,
            },
            temporal_supersampling,
            deterministic: true,
        },
        task: RenderJobTask::FrameSequenceFlowFeedback {
            modulator_frame_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            flow_cache_directory,
            carrier_amount: settings.carrier_amount,
            feedback_amount: settings.feedback_amount,
            feedback_mix: settings.feedback_mix,
            decay: settings.decay,
            iterations: settings.iterations,
            max_frames,
            reset_at_frame,
            frame_rate,
            backend,
            flow_source,
            structure_mix: settings.structure_mix,
            modulation_routes: modulation.routes,
            modulator_audio_path: modulator_audio.map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: modulator_frames.map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued flow-feedback render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_test(
    queue_path: &Path,
    output_dir: &Path,
    stop_after_frame: bool,
) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                job.status,
                RenderJobStatus::Queued | RenderJobStatus::Running
            )
        })
        .ok_or_else(|| {
            CliError::Message("render queue has no queued or running jobs".to_string())
        })?;
    let job_id = queue.jobs[job_index].id.clone();
    let job_output_dir = output_dir.join(&job_id);

    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;
    let output = write_test_render_output_bundle(&job_id, &job_output_dir, stop_after_frame)?;
    queue.jobs[job_index].output = Some(output.metadata);

    if output.complete {
        queue.jobs[job_index].status = RenderJobStatus::Complete;
        queue.save_json(queue_path)?;

        println!(
            "rendered queued test job {} to {}",
            job_id,
            job_output_dir.display()
        );
    } else {
        queue.save_json(queue_path)?;
        println!(
            "checkpointed queued test job {} after frame output in {}",
            job_id,
            job_output_dir.display()
        );
    }
    Ok(())
}

pub(crate) fn queue_run_frame_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceFlowDisplace { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running frame-sequence jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceFlowDisplace {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        flow_cache_directory,
        amount,
        max_frames,
        frame_rate,
        backend,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a frame-sequence render".to_string(),
        ));
    };

    let output_dir = PathBuf::from(output_directory);
    let frame_dir = output_dir.join("frames");

    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    // Run the fallible work in one place so any failure is recorded durably as a
    // Failed status with a reason rather than leaving the job stuck in Running.
    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_frame_sequence(FrameSequenceRenderRequest {
            modulator_dir: Path::new(&modulator_frame_directory),
            carrier_dir: Path::new(&carrier_frame_directory),
            output_dir: &frame_dir,
            amount,
            flow_cache_dir: flow_cache_directory.as_deref().map(Path::new),
            max_frames: max_frames.map(|value| value as usize),
            backend,
            rms: RmsAmountConfig {
                wav_path: None,
                frame_rate,
                window_size: 2048,
                hop_size: 512,
                amount_scale: 16.0,
            },
        })?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: frame_count as f64 / frame_rate,
            sample_rate: 48_000,
            audio_sample_count: 0,
        };
        let frame_paths = (0..frame_count)
            .map(|index| format!("frames/frame_{index:06}.png"))
            .collect::<Vec<_>>();
        write_frame_sequence_manifest(
            &job_id,
            &output_dir,
            &frame_paths,
            &timing,
            provenance.as_ref(),
        )?;
        write_frame_sequence_checkpoint(&job_id, &output_dir, &frame_paths, frame_count)?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued frame-sequence job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("frame-sequence job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) fn queue_run_fluid_advect_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceFluidAdvect { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message("render queue has no queued or running fluid-advect jobs".to_string())
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceFluidAdvect {
        source_frame_directory,
        output_directory,
        frames,
        frame_rate,
        advect,
        turbulence_scale,
        turbulence_speed,
        detail,
        reinject,
        seed,
        backend,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a fluid-advect render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = FluidAdvectSettings {
        advect,
        turbulence_scale,
        turbulence_speed,
        detail,
        reinject,
        seed,
    };
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);
    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_fluid_advect_sequence(FluidAdvectSequenceRequest {
            source_dir: Path::new(&source_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames: frames as usize,
            backend,
            modulation: ModulationCliArgs {
                modulate: &modulation_specs,
                modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                sampling: render_modulation_sampling(modulation_sampling),
                // The job's frame_rate is the envelope time base (a direct
                // render matches with --modulation-fps <frame_rate>).
                fps: frame_rate,
                // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                cache_dir: None,
                named_modulator_audio: &named_modulator_audio_specs,
                named_modulator_frames: &named_modulator_frames_specs,
            },
        })?;
        let mut effect = serde_json::json!({
            "algorithm": FLUID_ADVECT_ALGORITHM,
            "settings": settings,
            "backend": render_backend_label(backend)
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            effect["modulation"] = modulation;
        }
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_fluid_advect",
            effect_key: "fluid_advect",
            effect,
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "fluid-advect",
    )
}

pub(crate) fn queue_run_fluid_advect_two_source_sequence(
    queue_path: &Path,
) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceFluidAdvectTwoSource { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running two-source fluid-advect jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceFluidAdvectTwoSource {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        frames,
        frame_rate,
        advect,
        reinject,
        backend,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a two-source fluid-advect render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = FluidAdvectTwoSourceSettings { advect, reinject };
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);
    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result =
            render_fluid_advect_two_source_sequence(FluidAdvectTwoSourceSequenceRequest {
                source_a_dir: Path::new(&modulator_frame_directory),
                source_b_dir: Path::new(&carrier_frame_directory),
                output_dir: &output_dir.join("frames"),
                settings,
                frames: frames as usize,
                backend,
                modulation: ModulationCliArgs {
                    modulate: &modulation_specs,
                    modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                    modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                    sampling: render_modulation_sampling(modulation_sampling),
                    // The job's frame_rate is the envelope time base.
                    fps: frame_rate,
                    // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                    cache_dir: None,
                    named_modulator_audio: &named_modulator_audio_specs,
                    named_modulator_frames: &named_modulator_frames_specs,
                },
            })?;
        let mut effect = serde_json::json!({
            "algorithm": FLUID_ADVECT_TWO_SOURCE_ALGORITHM,
            "settings": settings,
            "backend": render_backend_label(backend)
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            effect["modulation"] = modulation;
        }
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_fluid_advect_two_source",
            effect_key: "fluid_advect_two_source",
            effect,
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "two-source fluid-advect",
    )
}

pub(crate) fn queue_run_optical_flow_advect_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceOpticalFlowAdvect { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running optical-flow advect jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceOpticalFlowAdvect {
        source_frame_directory,
        output_directory,
        frames,
        frame_rate,
        advect,
        reinject,
        backend,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not an optical-flow advect render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = FluidAdvectTwoSourceSettings { advect, reinject };
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);
    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result =
            render_optical_flow_advect_sequence(OpticalFlowAdvectSequenceRequest {
                source_dir: Path::new(&source_frame_directory),
                output_dir: &output_dir.join("frames"),
                settings,
                frames: frames as usize,
                backend,
                modulation: ModulationCliArgs {
                    modulate: &modulation_specs,
                    modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                    modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                    sampling: render_modulation_sampling(modulation_sampling),
                    // The job's frame_rate is the envelope time base.
                    fps: frame_rate,
                    // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                    cache_dir: None,
                    named_modulator_audio: &named_modulator_audio_specs,
                    named_modulator_frames: &named_modulator_frames_specs,
                },
            })?;
        let mut effect = serde_json::json!({
            "algorithm": FLUID_ADVECT_TWO_SOURCE_ALGORITHM,
            "settings": settings,
            "flow_source": "self_optical_flow",
            "backend": render_backend_label(backend)
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            effect["modulation"] = modulation;
        }
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_optical_flow_advect",
            effect_key: "optical_flow_advect",
            effect,
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "optical-flow advect",
    )
}

pub(crate) fn queue_run_field_particles_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceFieldParticles { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running field-particles jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceFieldParticles {
        source_frame_directory,
        output_directory,
        frames,
        frame_rate,
        spacing,
        particle_size,
        advect,
        turbulence_scale,
        turbulence_speed,
        detail,
        live_color,
        seed,
        backend,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a field-particles render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = FieldParticleSettings {
        spacing,
        particle_size,
        advect,
        turbulence_scale,
        turbulence_speed,
        detail,
        live_color,
        seed,
    };
    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_field_particles_sequence(FieldParticlesSequenceRequest {
            source_dir: Path::new(&source_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames: frames as usize,
            backend,
        })?;
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_field_particles",
            effect_key: "field_particles",
            effect: serde_json::json!({
                "algorithm": FIELD_PARTICLES_ALGORITHM,
                "settings": settings,
                "backend": render_backend_label(backend)
            }),
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "field-particles",
    )
}

pub(crate) fn queue_run_cascade_trails_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceCascadeTrails { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running cascade-trails jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceCascadeTrails {
        source_frame_directory,
        output_directory,
        frames,
        frame_rate,
        tile_size,
        grid_spacing,
        advect,
        turbulence_scale,
        detail,
        live_refresh,
        seed,
        field,
        river_direction,
        river_speed,
        river_turbulence,
        temporal_tiles,
        decay,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a cascade-trails render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = CascadeTrailSettings {
        tile_size,
        grid_spacing,
        advect,
        turbulence_scale,
        detail,
        live_refresh,
        seed,
        field: parse_cascade_field_type(&field),
        river_direction,
        river_speed,
        river_turbulence,
        temporal_tiles,
        decay,
    };
    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_cascade_trails_sequence(CascadeTrailsSequenceRequest {
            source_dir: Path::new(&source_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames: frames as usize,
        })?;
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_cascade_trails",
            effect_key: "trail_cascade",
            effect: serde_json::json!({
                "algorithm": CASCADE_TRAIL_ALGORITHM,
                "settings": settings,
                "backend": "CPU"
            }),
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "cascade-trails",
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) struct QueueAddRetroStaticSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) real_bpp: u32,
    pub(crate) assumed_bpp: u32,
    pub(crate) filter: ScanlineFilter,
    pub(crate) strength: f32,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_retro_static_sequence(
    request: QueueAddRetroStaticSequenceRequest<'_>,
) -> Result<(), CliError> {
    validate_queued_sequence_timing(request.frames, request.frame_rate)?;
    let settings = RetroStaticSettings {
        real_bpp: request.real_bpp,
        assumed_bpp: request.assumed_bpp,
        filter: request.filter,
        strength: request.strength,
    };
    settings.validate()?;

    let modulation = parse_queue_modulation_routes(
        request.modulate,
        request.modulator_audio,
        request.modulator_frames,
        request.named_modulator_audio,
        request.named_modulator_frames,
        |target| {
            let mut probe = settings;
            apply_retro_static_modulation(&mut probe, target, 0.0).map_err(CliError::from)
        },
    )?;

    let mut queue = load_or_default_queue(request.queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = request.output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: request
            .project_path
            .map(|p| p.to_string_lossy().to_string()),
        settings: png_sequence_settings(request.frame_rate),
        task: RenderJobTask::FrameSequenceRetroStatic {
            source_frame_directory: request.source_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames: request.frames,
            frame_rate: request.frame_rate,
            real_bpp: request.real_bpp,
            assumed_bpp: request.assumed_bpp,
            filter: scanline_filter_label(request.filter),
            strength: request.strength,
            backend: request.backend,
            modulation_routes: modulation.routes,
            modulator_audio_path: request
                .modulator_audio
                .map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: request
                .modulator_frames
                .map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(request.modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            request.source_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(request.queue_path)?;
    println!(
        "queued retro-static render job {job_id} in {}",
        request.queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_retro_static_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceRetroStatic { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message("render queue has no queued or running retro-static jobs".to_string())
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceRetroStatic {
        source_frame_directory,
        output_directory,
        frames,
        frame_rate,
        real_bpp,
        assumed_bpp,
        filter,
        strength,
        backend,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a retro-static render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = RetroStaticSettings {
        real_bpp,
        assumed_bpp,
        filter: parse_scanline_filter(&filter),
        strength,
    };
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_retro_static_sequence(RetroStaticSequenceRequest {
            source_dir: Path::new(&source_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames,
            backend,
            modulation: ModulationCliArgs {
                modulate: &modulation_specs,
                modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                sampling: render_modulation_sampling(modulation_sampling),
                // The job's frame_rate is the sequence time base.
                fps: frame_rate,
                // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                cache_dir: None,
                named_modulator_audio: &named_modulator_audio_specs,
                named_modulator_frames: &named_modulator_frames_specs,
            },
        })?;
        let mut effect = serde_json::json!({
            "algorithm": RETRO_STATIC_ALGORITHM,
            "settings": settings,
            "backend": format!("{backend:?}")
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            effect["modulation"] = modulation;
        }
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_retro_static",
            effect_key: "retro_static",
            effect,
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "retro-static",
    )
}

fn scanline_filter_label(filter: ScanlineFilter) -> String {
    match filter {
        ScanlineFilter::None => "none".to_string(),
        ScanlineFilter::Sub => "sub".to_string(),
        ScanlineFilter::Up => "up".to_string(),
        ScanlineFilter::Average => "average".to_string(),
        ScanlineFilter::Paeth => "paeth".to_string(),
    }
}

fn parse_scanline_filter(s: &str) -> ScanlineFilter {
    match s {
        "sub" => ScanlineFilter::Sub,
        "up" => ScanlineFilter::Up,
        "average" => ScanlineFilter::Average,
        "paeth" => ScanlineFilter::Paeth,
        _ => ScanlineFilter::None,
    }
}

pub(crate) struct QueueAddChannelShiftSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) settings: ChannelShiftSettings,
    pub(crate) source_a_dir: Option<&'a Path>,
    pub(crate) flow_gain: f32,
    pub(crate) flow_radius: i32,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_channel_shift_sequence(
    request: QueueAddChannelShiftSequenceRequest<'_>,
) -> Result<(), CliError> {
    validate_queued_sequence_timing(request.frames, request.frame_rate)?;
    let flow_active = request.flow_gain != 0.0;
    if flow_active && request.source_a_dir.is_none() {
        return Err(CliError::Message(
            "flow-driven channel shift requires --source-a-dir".to_string(),
        ));
    }
    if flow_active && request.backend == RenderBackend::Metal {
        return Err(CliError::Message(
            "flow-driven channel shift is CPU-only; use --backend cpu".to_string(),
        ));
    }

    let modulation = parse_queue_modulation_routes(
        request.modulate,
        request.modulator_audio,
        request.modulator_frames,
        request.named_modulator_audio,
        request.named_modulator_frames,
        |target| {
            let mut probe = request.settings;
            apply_channel_shift_modulation(&mut probe, target, 0.0).map_err(CliError::from)
        },
    )?;

    let mut queue = load_or_default_queue(request.queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = request.output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: request
            .project_path
            .map(|p| p.to_string_lossy().to_string()),
        settings: png_sequence_settings(request.frame_rate),
        task: RenderJobTask::FrameSequenceChannelShift {
            carrier_frame_directory: request.source_b_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames: request.frames,
            frame_rate: request.frame_rate,
            shift_r_x: request.settings.shift_r_x,
            shift_r_y: request.settings.shift_r_y,
            shift_g_x: request.settings.shift_g_x,
            shift_g_y: request.settings.shift_g_y,
            shift_b_x: request.settings.shift_b_x,
            shift_b_y: request.settings.shift_b_y,
            flow_source_frame_directory: request
                .source_a_dir
                .map(|p| p.to_string_lossy().to_string()),
            flow_gain: request.flow_gain,
            flow_radius: request.flow_radius,
            backend: request.backend,
            modulation_routes: modulation.routes,
            modulator_audio_path: request
                .modulator_audio
                .map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: request
                .modulator_frames
                .map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(request.modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            request.source_b_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(request.queue_path)?;
    println!(
        "queued channel-shift render job {job_id} in {}",
        request.queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_channel_shift_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceChannelShift { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running channel-shift jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceChannelShift {
        carrier_frame_directory,
        output_directory,
        frames,
        frame_rate,
        shift_r_x,
        shift_r_y,
        shift_g_x,
        shift_g_y,
        shift_b_x,
        shift_b_y,
        flow_source_frame_directory,
        flow_gain,
        flow_radius,
        backend,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a channel-shift render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = ChannelShiftSettings {
        shift_r_x,
        shift_r_y,
        shift_g_x,
        shift_g_y,
        shift_b_x,
        shift_b_y,
    };
    let algorithm = if flow_gain != 0.0 {
        CHANNEL_SHIFT_FLOW_ALGORITHM
    } else {
        CHANNEL_SHIFT_ALGORITHM
    };
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_channel_shift_sequence(ChannelShiftSequenceRequest {
            source_b_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames,
            backend,
            source_a_dir: flow_source_frame_directory.as_deref().map(Path::new),
            flow_gain,
            radius: flow_radius,
            modulation: ModulationCliArgs {
                modulate: &modulation_specs,
                modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                sampling: render_modulation_sampling(modulation_sampling),
                // The job's frame_rate is the sequence time base.
                fps: frame_rate,
                // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                cache_dir: None,
                named_modulator_audio: &named_modulator_audio_specs,
                named_modulator_frames: &named_modulator_frames_specs,
            },
        })?;
        let mut effect = serde_json::json!({
            "algorithm": algorithm,
            "settings": settings,
            "flow_gain": flow_gain,
            "flow_radius": flow_radius,
            "backend": format!("{backend:?}")
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            effect["modulation"] = modulation;
        }
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_channel_shift",
            effect_key: "channel_shift",
            effect,
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "channel-shift",
    )
}

pub(crate) struct QueueAddPaletteQuantizeSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) settings: PaletteQuantizeSettings,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_palette_quantize_sequence(
    request: QueueAddPaletteQuantizeSequenceRequest<'_>,
) -> Result<(), CliError> {
    validate_queued_sequence_timing(request.frames, request.frame_rate)?;
    if matches!(request.settings.mode, QuantizeMode::Posterize) && request.settings.levels < 2 {
        return Err(CliError::Message(
            "levels must be >= 2 for posterize mode".to_string(),
        ));
    }

    let modulation = parse_queue_modulation_routes(
        request.modulate,
        request.modulator_audio,
        request.modulator_frames,
        request.named_modulator_audio,
        request.named_modulator_frames,
        |target| {
            let mut probe = request.settings;
            apply_palette_quantize_modulation(&mut probe, target, 0.0).map_err(CliError::from)
        },
    )?;

    let mut queue = load_or_default_queue(request.queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = request.output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: request
            .project_path
            .map(|p| p.to_string_lossy().to_string()),
        settings: png_sequence_settings(request.frame_rate),
        task: RenderJobTask::FrameSequencePaletteQuantize {
            carrier_frame_directory: request.source_b_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames: request.frames,
            frame_rate: request.frame_rate,
            mode: quantize_mode_label(request.settings.mode),
            levels: request.settings.levels,
            backend: request.backend,
            modulation_routes: modulation.routes,
            modulator_audio_path: request
                .modulator_audio
                .map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: request
                .modulator_frames
                .map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(request.modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            request.source_b_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(request.queue_path)?;
    println!(
        "queued palette-quantize render job {job_id} in {}",
        request.queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_palette_quantize_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequencePaletteQuantize { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running palette-quantize jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequencePaletteQuantize {
        carrier_frame_directory,
        output_directory,
        frames,
        frame_rate,
        mode,
        levels,
        backend,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a palette-quantize render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = PaletteQuantizeSettings {
        mode: parse_quantize_mode(&mode),
        levels,
    };
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_palette_quantize_sequence(PaletteQuantizeSequenceRequest {
            source_b_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames,
            backend,
            modulation: ModulationCliArgs {
                modulate: &modulation_specs,
                modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                sampling: render_modulation_sampling(modulation_sampling),
                // The job's frame_rate is the sequence time base.
                fps: frame_rate,
                // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                cache_dir: None,
                named_modulator_audio: &named_modulator_audio_specs,
                named_modulator_frames: &named_modulator_frames_specs,
            },
        })?;
        let mut effect = serde_json::json!({
            "algorithm": PALETTE_QUANTIZE_ALGORITHM,
            "settings": settings,
            "backend": format!("{backend:?}")
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            effect["modulation"] = modulation;
        }
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_palette_quantize",
            effect_key: "palette_quantize",
            effect,
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "palette-quantize",
    )
}

fn quantize_mode_label(mode: QuantizeMode) -> String {
    match mode {
        QuantizeMode::Posterize => "posterize".to_string(),
        QuantizeMode::Palette => "palette".to_string(),
        QuantizeMode::Kmeans => "kmeans".to_string(),
    }
}

fn parse_quantize_mode(s: &str) -> QuantizeMode {
    match s {
        "palette" => QuantizeMode::Palette,
        "kmeans" => QuantizeMode::Kmeans,
        _ => QuantizeMode::Posterize,
    }
}

pub(crate) struct QueueAddRuttEtraSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) settings: RuttEtraSettings,
    pub(crate) backend: RenderBackend,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_rutt_etra_sequence(
    request: QueueAddRuttEtraSequenceRequest<'_>,
) -> Result<(), CliError> {
    validate_queued_sequence_timing(request.frames, request.frame_rate)?;
    request.settings.validate()?;

    let modulation = parse_queue_modulation_routes(
        request.modulate,
        request.modulator_audio,
        request.modulator_frames,
        request.named_modulator_audio,
        request.named_modulator_frames,
        |target| {
            let mut probe = request.settings;
            apply_rutt_etra_modulation(&mut probe, target, 0.0).map_err(CliError::from)
        },
    )?;

    let mut queue = load_or_default_queue(request.queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = request.output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: request
            .project_path
            .map(|p| p.to_string_lossy().to_string()),
        settings: png_sequence_settings(request.frame_rate),
        task: RenderJobTask::FrameSequenceRuttEtra {
            carrier_frame_directory: request.source_b_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames: request.frames,
            frame_rate: request.frame_rate,
            line_pitch: request.settings.line_pitch,
            displacement_depth: request.settings.displacement_depth,
            line_thickness: request.settings.line_thickness,
            mono: request.settings.mono,
            backend: request.backend,
            modulation_routes: modulation.routes,
            modulator_audio_path: request
                .modulator_audio
                .map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: request
                .modulator_frames
                .map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(request.modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            request.source_b_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(request.queue_path)?;
    println!(
        "queued rutt-etra render job {job_id} in {}",
        request.queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_rutt_etra_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceRuttEtra { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message("render queue has no queued or running rutt-etra jobs".to_string())
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceRuttEtra {
        carrier_frame_directory,
        output_directory,
        frames,
        frame_rate,
        line_pitch,
        displacement_depth,
        line_thickness,
        mono,
        backend,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a rutt-etra render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = RuttEtraSettings {
        line_pitch,
        displacement_depth,
        line_thickness,
        mono,
    };
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_rutt_etra_sequence(RuttEtraSequenceRequest {
            source_b_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames,
            backend,
            modulation: ModulationCliArgs {
                modulate: &modulation_specs,
                modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                sampling: render_modulation_sampling(modulation_sampling),
                // The job's frame_rate is the sequence time base.
                fps: frame_rate,
                // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                cache_dir: None,
                named_modulator_audio: &named_modulator_audio_specs,
                named_modulator_frames: &named_modulator_frames_specs,
            },
        })?;
        let algorithm = match backend {
            RenderBackend::Cpu => RUTT_ETRA_ALGORITHM,
            RenderBackend::Metal => RUTT_ETRA_METAL_ALGORITHM,
        };
        let mut effect = serde_json::json!({
            "algorithm": algorithm,
            "settings": settings,
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            effect["modulation"] = modulation;
        }
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_rutt_etra",
            effect_key: "rutt_etra",
            effect,
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "rutt-etra",
    )
}

/// Queue an effect chain (`docs/EFFECT_CHAIN_MILESTONE.md` Slice 4). The
/// whole spec is parsed + validated at add time (grammar, knobs, modulation
/// routes/media — the same gate as the direct command); the job persists the
/// *resolved* spec document verbatim, so queue-run re-parses the identical
/// spec and shares the direct code path (add→run byte-identity).
pub(crate) fn queue_add_chain(
    queue_path: &Path,
    spec_path: &Path,
    input_dir: &Path,
    output_root_dir: &Path,
    project_path: Option<&Path>,
) -> Result<(), CliError> {
    let spec_text = fs::read_to_string(spec_path)?;
    let spec = crate::chain::parse_and_validate_chain_spec(&spec_text)?;
    let spec_value = crate::chain::resolved_chain_spec_value(&spec)?;

    let mut queue = load_or_default_queue(queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|p| p.to_string_lossy().to_string()),
        // The chain's envelope/frame time base (stateless stages default to
        // 12 fps; the feedback stage's frame rate is pinned to the same 12).
        settings: png_sequence_settings(12.0),
        task: RenderJobTask::RenderChain {
            input_frame_directory: input_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            spec: spec_value,
        },
        provenance: Some(single_source_provenance(
            "source-frames",
            SourceRole::Carrier,
            input_dir,
        )),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued chain render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_chain(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::RenderChain { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message("render queue has no queued or running chain jobs".to_string())
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::RenderChain {
        input_frame_directory,
        output_directory,
        spec,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a chain render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let spec = crate::chain::chain_spec_from_value(&spec)?;
        let summary =
            crate::chain::run_chain_spec(&spec, Path::new(&input_frame_directory), &output_dir)?;
        let frame_count = u32::try_from(summary.frame_count).map_err(|_| {
            CliError::Message("chain output contains more than u32::MAX frames".to_string())
        })?;
        let frames_prefix = summary
            .final_frames_dir
            .strip_prefix(&output_dir)
            .unwrap_or(&summary.final_frames_dir)
            .to_string_lossy()
            .to_string();
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths: (0..frame_count)
                .map(|index| format!("{frames_prefix}/frame_{index:06}.png"))
                .collect(),
            audio_stem_paths: Vec::new(),
            timing: RenderTimingMetadata {
                frame_rate: 12.0,
                frame_count,
                start_seconds: 0.0,
                duration_seconds: frame_count as f64 / 12.0,
                sample_rate: 48_000,
                audio_sample_count: 0,
            },
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "chain",
    )
}

pub(crate) struct QueueAddBlockCollageSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) source_a_dir: &'a Path,
    pub(crate) source_b_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: BlockCollageSettings,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) project_path: Option<&'a Path>,
}

pub(crate) fn queue_add_block_collage_sequence(
    request: QueueAddBlockCollageSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddBlockCollageSequenceRequest {
        queue_path,
        source_a_dir,
        source_b_dir,
        output_root_dir,
        settings,
        frames,
        frame_rate,
        project_path,
    } = request;
    settings.validate()?;
    validate_queued_sequence_timing(frames, frame_rate)?;

    let mut queue = load_or_default_queue(queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|p| p.to_string_lossy().to_string()),
        settings: png_sequence_settings(frame_rate),
        task: RenderJobTask::FrameSequenceBlockCollage {
            modulator_frame_directory: source_a_dir.to_string_lossy().to_string(),
            carrier_frame_directory: source_b_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames,
            frame_rate,
            tile_size: settings.tile_size,
            threshold: settings.threshold,
            cluster_scale: settings.cluster_scale,
            evolution_speed: settings.evolution_speed,
            seed: settings.seed,
        },
        provenance: Some(two_source_provenance(source_a_dir, source_b_dir)),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued block-collage render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_block_collage_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceBlockCollage { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running block-collage jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequenceBlockCollage {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        frames,
        frame_rate,
        tile_size,
        threshold,
        cluster_scale,
        evolution_speed,
        seed,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a block-collage render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = BlockCollageSettings {
        tile_size,
        threshold,
        cluster_scale,
        evolution_speed,
        seed,
    };
    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_block_collage_sequence(BlockCollageSequenceRequest {
            source_a_dir: Path::new(&modulator_frame_directory),
            source_b_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames,
        })?;
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_block_collage",
            effect_key: "block_collage",
            effect: serde_json::json!({
                "algorithm": BLOCK_COLLAGE_ALGORITHM,
                "settings": settings,
                "backend": "CPU"
            }),
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "block-collage",
    )
}

pub(crate) fn queue_run_granular_mosaic_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceGranularMosaic { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running granular-mosaic jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::FrameSequenceGranularMosaic {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        grain_cache_directory,
        grain_size,
        rearrangement,
        variation,
        seed,
        max_frames,
        frame_rate,
        backend,
        audio_modulation,
        selection_mode,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a granular-mosaic render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    let provenance = granular_mosaic_provenance(
        Path::new(&modulator_frame_directory),
        Path::new(&carrier_frame_directory),
        grain_cache_directory.as_deref().map(Path::new),
        audio_modulation.as_ref(),
        selection_mode,
    );
    queue.jobs[job_index].provenance = Some(provenance.clone());
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_granular_mosaic_sequence(GranularMosaicSequenceRenderRequest {
            modulator_dir: Path::new(&modulator_frame_directory),
            carrier_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings: GranularMosaicSettings {
                grain_size,
                rearrangement,
                variation,
                seed,
            },
            frame_rate,
            max_frames: max_frames.map(|value| value as usize),
            grain_cache_dir: grain_cache_directory.as_deref().map(Path::new),
            backend,
            audio_modulation: audio_modulation.clone(),
            selection_mode,
        })?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: frame_count as f64 / frame_rate,
            sample_rate: 48_000,
            audio_sample_count: 0,
        };
        let frame_paths = (0..frame_count)
            .map(|index| format!("frames/frame_{index:06}.png"))
            .collect::<Vec<_>>();
        let settings = GranularMosaicSettings {
            grain_size,
            rearrangement,
            variation,
            seed,
        };
        write_granular_mosaic_sequence_manifest(
            &job_id,
            &output_dir,
            &frame_paths,
            &timing,
            &settings,
            audio_modulation.as_ref(),
            Some(&provenance),
            selection_mode,
        )?;
        write_frame_sequence_checkpoint(&job_id, &output_dir, &frame_paths, frame_count)?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued granular-mosaic job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("granular-mosaic job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) fn queue_run_granular_mosaic_pool_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceGranularMosaicPool { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running granular-mosaic pool jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::FrameSequenceGranularMosaicPool {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        grain_cache_directory,
        grain_size,
        rearrangement,
        variation,
        seed,
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
        max_frames,
        frame_rate,
        backend,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a granular-mosaic pool render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    let provenance = granular_mosaic_pool_provenance(
        Path::new(&modulator_frame_directory),
        Path::new(&carrier_frame_directory),
        grain_cache_directory.as_deref().map(Path::new),
        modulator_rms_cache.as_deref(),
        carrier_rms_cache.as_deref(),
        modulator_centroid_cache.as_deref(),
        carrier_centroid_cache.as_deref(),
    );
    queue.jobs[job_index].provenance = Some(provenance.clone());
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let settings = GranularMosaicSettings {
            grain_size,
            rearrangement,
            variation,
            seed,
        };
        let render_result =
            render_granular_mosaic_pool_sequence(GranularMosaicPoolSequenceRequest {
                modulator_dir: Path::new(&modulator_frame_directory),
                carrier_dir: Path::new(&carrier_frame_directory),
                output_dir: &output_dir.join("frames"),
                settings,
                audio_weight,
                texture_weight,
                modulator_rms_cache: modulator_rms_cache.as_deref().map(Path::new),
                carrier_rms_cache: carrier_rms_cache.as_deref().map(Path::new),
                modulator_centroid_cache: modulator_centroid_cache.as_deref().map(Path::new),
                carrier_centroid_cache: carrier_centroid_cache.as_deref().map(Path::new),
                pool_window,
                anti_repeat_weight,
                anti_repeat_cooldown,
                coherence_weight,
                coherence_reach,
                spatial_coherence_weight,
                frame_rate,
                max_frames: max_frames.map(|value| value as usize),
                grain_cache_dir: grain_cache_directory.as_deref().map(Path::new),
                backend,
            })?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: frame_count as f64 / frame_rate,
            sample_rate: 48_000,
            audio_sample_count: 0,
        };
        let frame_paths = (0..frame_count)
            .map(|index| format!("frames/frame_{index:06}.png"))
            .collect::<Vec<_>>();
        write_granular_mosaic_pool_sequence_manifest(GranularMosaicPoolManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_paths: &frame_paths,
            timing: &timing,
            settings: &settings,
            audio_weight,
            texture_weight,
            modulator_rms_cache: modulator_rms_cache.as_deref(),
            carrier_rms_cache: carrier_rms_cache.as_deref(),
            modulator_centroid_cache: modulator_centroid_cache.as_deref(),
            carrier_centroid_cache: carrier_centroid_cache.as_deref(),
            pool_window,
            anti_repeat_weight,
            anti_repeat_cooldown,
            coherence_weight,
            coherence_reach,
            spatial_coherence_weight,
            backend,
            provenance: Some(&provenance),
        })?;
        write_frame_sequence_checkpoint(&job_id, &output_dir, &frame_paths, frame_count)?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued granular-mosaic pool job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("granular-mosaic pool job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) fn queue_run_video_vocoder_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceVideoVocoder { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running video-vocoder jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::FrameSequenceVideoVocoder {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        bands,
        amount,
        mode,
        max_frames,
        frame_rate,
        backend,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a video-vocoder render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let settings = VideoVocoderSettings { bands, amount };
        let render_result = render_video_vocoder_sequence(
            Path::new(&modulator_frame_directory),
            Path::new(&carrier_frame_directory),
            &output_dir.join("frames"),
            settings,
            mode.into(),
            backend,
            max_frames.map(|value| value as usize),
        )?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: frame_count as f64 / frame_rate,
            sample_rate: 48_000,
            audio_sample_count: 0,
        };
        let frame_paths = (0..frame_count)
            .map(|index| format!("frames/frame_{index:06}.png"))
            .collect::<Vec<_>>();
        let algorithm = match mode {
            VideoVocoderMode::Match => "luma_histogram_spec_vocoder_cpu_v1",
            VideoVocoderMode::Gain => "luma_band_gain_vocoder_cpu_v1",
        };
        let manifest = serde_json::json!({
            "job_id": job_id,
            "status": "complete",
            "task": "frame_sequence_video_vocoder",
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
            "video_vocoder": {
                "algorithm": algorithm,
                "mode": match mode { VideoVocoderMode::Match => "match", VideoVocoderMode::Gain => "gain" },
                "bands": bands,
                "amount": amount,
                "backend": render_backend_label(backend)
            },
            "provenance": queue.jobs[job_index].provenance,
            "deterministic": true
        });
        fs::write(
            output_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        write_frame_sequence_checkpoint(&job_id, &output_dir, &frame_paths, frame_count)?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued video-vocoder job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("video-vocoder job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) struct QueueAddAudioVideoRouteSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_wav: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) amount: f32,
    pub(crate) shift_x: f32,
    pub(crate) shift_y: f32,
    pub(crate) rms_window: u32,
    pub(crate) rms_hop: u32,
    pub(crate) frame_rate: f64,
    pub(crate) max_frames: Option<u32>,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
}

pub(crate) fn queue_add_audio_video_route_sequence(
    request: QueueAddAudioVideoRouteSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddAudioVideoRouteSequenceRequest {
        queue_path,
        modulator_wav,
        carrier_dir,
        output_root_dir,
        amount,
        shift_x,
        shift_y,
        rms_window,
        rms_hop,
        frame_rate,
        max_frames,
        project_path,
        backend,
    } = request;
    if !amount.is_finite() || amount < 0.0 {
        return Err(CliError::Message(
            "amount must be finite and non-negative".to_string(),
        ));
    }
    if !shift_x.is_finite() || !shift_y.is_finite() {
        return Err(CliError::Message(
            "shift-x and shift-y must be finite".to_string(),
        ));
    }
    if rms_window == 0 || rms_hop == 0 {
        return Err(CliError::Message(
            "rms-window and rms-hop must be greater than zero".to_string(),
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

    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);
    let provenance = RenderJobProvenance {
        sources: vec![
            RenderJobSourceProvenance {
                source_id: "source-a-audio".to_string(),
                role: SourceRole::Modulator,
                path: modulator_wav.to_string_lossy().to_string(),
            },
            RenderJobSourceProvenance {
                source_id: "source-b-frames".to_string(),
                role: SourceRole::Carrier,
                path: carrier_dir.to_string_lossy().to_string(),
            },
        ],
        analysis_caches: Vec::new(),
    };

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 8,
            },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::FrameSequenceAudioVideoRoute {
            modulator_wav: modulator_wav.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            amount,
            shift_x,
            shift_y,
            rms_window,
            rms_hop,
            frame_rate,
            max_frames,
            backend,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued audio→video route render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_audio_video_route_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceAudioVideoRoute { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running audio→video route jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::FrameSequenceAudioVideoRoute {
        modulator_wav,
        carrier_frame_directory,
        output_directory,
        amount,
        shift_x,
        shift_y,
        rms_window,
        rms_hop,
        frame_rate,
        max_frames,
        backend,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not an audio→video route render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_audio_video_route_sequence(AudioVideoRouteSequenceRequest {
            modulator_wav: Path::new(&modulator_wav),
            carrier_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            amount,
            shift_x,
            shift_y,
            rms_window,
            rms_hop,
            fps: frame_rate,
            backend,
            max_frames: max_frames.map(|value| value as usize),
        })?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: frame_count as f64 / frame_rate,
            sample_rate: 48_000,
            audio_sample_count: 0,
        };
        let frame_paths = (0..frame_count)
            .map(|index| format!("frames/frame_{index:06}.png"))
            .collect::<Vec<_>>();
        let manifest = serde_json::json!({
            "job_id": job_id,
            "status": "complete",
            "task": "frame_sequence_audio_video_route",
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
            "audio_video_route": {
                "algorithm": RMS_DISPLACEMENT_ROUTE_ALGORITHM,
                "amount": amount,
                "shift_x": shift_x,
                "shift_y": shift_y,
                "rms_window": rms_window,
                "rms_hop": rms_hop,
                "backend": render_backend_label(backend)
            },
            "provenance": queue.jobs[job_index].provenance,
            "deterministic": true
        });
        fs::write(
            output_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        write_frame_sequence_checkpoint(&job_id, &output_dir, &frame_paths, frame_count)?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued audio→video route job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("audio→video route job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) struct QueueAddDatamoshSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) keyframe_interval: u32,
    pub(crate) amount: f32,
    pub(crate) block_size: u32,
    pub(crate) residual_gain: f32,
    pub(crate) residual_decay: f32,
    pub(crate) refresh_threshold: f32,
    pub(crate) vector_remix: VectorRemixMode,
    pub(crate) remix_seed: u64,
    pub(crate) preset: DatamoshPreset,
    pub(crate) flow_cache_dir: Option<&'a Path>,
    pub(crate) max_frames: Option<u32>,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

/// Map the persisted (core) vector-remix mode to the render crate's enum. A free
/// function rather than a `From` impl because both types are foreign to this crate
/// (orphan rule); the same core↔render bridge pattern the other queued modes use.
fn render_vector_remix(mode: VectorRemixMode) -> morphogen_render::VectorRemixMode {
    match mode {
        VectorRemixMode::None => morphogen_render::VectorRemixMode::None,
        VectorRemixMode::Sort => morphogen_render::VectorRemixMode::Sort,
        VectorRemixMode::Shuffle => morphogen_render::VectorRemixMode::Shuffle,
    }
}

pub(crate) fn queue_add_datamosh_sequence(
    request: QueueAddDatamoshSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddDatamoshSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
        keyframe_interval,
        amount,
        block_size,
        residual_gain,
        residual_decay,
        refresh_threshold,
        vector_remix,
        remix_seed,
        preset,
        flow_cache_dir,
        max_frames,
        project_path,
        backend,
        modulate,
        modulator_audio,
        modulator_frames,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = request;
    if !amount.is_finite() || amount < 0.0 {
        return Err(CliError::Message(
            "amount must be finite and non-negative".to_string(),
        ));
    }
    if !residual_gain.is_finite() || residual_gain < 0.0 {
        return Err(CliError::Message(
            "residual-gain must be finite and non-negative".to_string(),
        ));
    }
    if !residual_decay.is_finite() || residual_decay < 0.0 {
        return Err(CliError::Message(
            "residual-decay must be finite and non-negative".to_string(),
        ));
    }
    if !refresh_threshold.is_finite() || refresh_threshold < 0.0 {
        return Err(CliError::Message(
            "block-refresh-threshold must be finite and non-negative".to_string(),
        ));
    }
    if matches!(max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }
    // The probe validates target names only; the settings values (and the
    // preset-resolved smear/engrave flags) are irrelevant to it.
    let modulation = parse_queue_modulation_routes(
        modulate,
        modulator_audio,
        modulator_frames,
        named_modulator_audio,
        named_modulator_frames,
        |target| {
            let mut probe = DatamoshSequenceSettings {
                keyframe_interval,
                amount,
                block_size,
                residual_gain,
                residual_decay,
                refresh_threshold,
                vector_remix: "none".to_string(),
                remix_seed,
                preset,
                scanline_smear: false,
                codec_engrave: false,
            };
            apply_datamosh_modulation(&mut probe, target, 0.0)
        },
    )?;

    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);
    let flow_cache_directory = flow_cache_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| job_output_dir.join("cache").join("datamosh-flow"));
    let provenance =
        datamosh_sequence_provenance(modulator_dir, carrier_dir, Some(&flow_cache_directory));

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 8,
            },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::FrameSequenceDatamosh {
            modulator_frame_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            keyframe_interval,
            amount,
            max_frames,
            backend,
            block_size,
            residual_gain,
            residual_decay,
            block_refresh_threshold: refresh_threshold,
            vector_remix,
            remix_seed,
            preset,
            flow_cache_directory: Some(flow_cache_directory.to_string_lossy().to_string()),
            modulation_routes: modulation.routes,
            modulator_audio_path: modulator_audio.map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: modulator_frames.map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued datamosh render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_datamosh_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceDatamosh { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message("render queue has no queued or running datamosh jobs".to_string())
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::FrameSequenceDatamosh {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        keyframe_interval,
        amount,
        max_frames,
        backend,
        block_size,
        residual_gain,
        residual_decay,
        block_refresh_threshold,
        vector_remix,
        remix_seed,
        preset,
        flow_cache_directory,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a datamosh render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    let flow_cache_path = flow_cache_directory.as_deref().map(PathBuf::from);
    let provenance = queue.jobs[job_index].provenance.clone();
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    // The datamosh task has no per-job frame rate; its manifests fix 30 fps,
    // which is therefore also the modulation-envelope time base (a direct
    // render matches with --modulation-fps 30).
    let frame_rate = 30.0;
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_request = DatamoshSequenceRequest {
            modulator_dir: Path::new(&modulator_frame_directory),
            carrier_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            flow_cache_dir: flow_cache_path.as_deref(),
            keyframe_interval,
            amount,
            block_size,
            residual_gain,
            residual_decay,
            refresh_threshold: block_refresh_threshold,
            vector_remix: render_vector_remix(vector_remix),
            remix_seed,
            preset,
            backend,
            max_frames: max_frames.map(|value| value as usize),
            job_id: &job_id,
            provenance: provenance.as_ref(),
            stop_after_frame: false,
            modulation: ModulationCliArgs {
                modulate: &modulation_specs,
                modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                sampling: render_modulation_sampling(modulation_sampling),
                fps: frame_rate,
                // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                cache_dir: None,
                named_modulator_audio: &named_modulator_audio_specs,
                named_modulator_frames: &named_modulator_frames_specs,
            },
        };
        let resolved_settings = resolve_datamosh_settings(&render_request);
        let render_result = render_datamosh_sequence(render_request)?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: frame_count as f64 / frame_rate,
            sample_rate: 48_000,
            audio_sample_count: 0,
        };
        let frame_paths = (0..frame_count)
            .map(|index| format!("frames/frame_{index:06}.png"))
            .collect::<Vec<_>>();
        let mut datamosh_block = serde_json::json!({
            "algorithm": datamosh_sequence_algorithm(&resolved_settings),
            "preset": datamosh_preset_label(preset),
            "keyframe_interval": resolved_settings.keyframe_interval,
            "amount": resolved_settings.amount,
            "block_size": resolved_settings.block_size,
            "residual_gain": resolved_settings.residual_gain,
            "residual_decay": resolved_settings.residual_decay,
            "block_refresh_threshold": resolved_settings.refresh_threshold,
            "vector_remix": resolved_settings.vector_remix,
            "remix_seed": resolved_settings.remix_seed,
            "scanline_smear": resolved_settings.scanline_smear,
            "codec_engrave": resolved_settings.codec_engrave,
            "flow_cache_directory": flow_cache_directory,
            "backend": render_backend_label(backend)
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            datamosh_block["modulation"] = modulation;
        }
        let manifest = serde_json::json!({
            "job_id": job_id,
            "status": "complete",
            "task": "frame_sequence_datamosh",
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
            "datamosh": datamosh_block,
            "provenance": provenance,
            "deterministic": true
        });
        fs::write(
            output_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        write_frame_sequence_checkpoint(&job_id, &output_dir, &frame_paths, frame_count)?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued datamosh job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("datamosh job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) struct QueueAddDatamoshBitstreamRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) input_video: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) fps: f64,
    pub(crate) operation: DatamoshBitstreamOperation,
    pub(crate) p_frame_index: u32,
    pub(crate) duplicate_count: u32,
    pub(crate) carrier_video: Option<&'a Path>,
    pub(crate) carrier_keyframes: u32,
    pub(crate) preset: DatamoshBitstreamPreset,
    pub(crate) project_path: Option<&'a Path>,
}

fn cascade_field_type_label(field: CascadeFieldType) -> String {
    match field {
        CascadeFieldType::Vortex => "vortex".to_string(),
        CascadeFieldType::River => "river".to_string(),
        CascadeFieldType::RiverRoot => "river-root".to_string(),
        CascadeFieldType::CenterSplit => "center-split".to_string(),
        CascadeFieldType::Oscillate => "oscillate".to_string(),
        CascadeFieldType::SquarePop => "square-pop".to_string(),
    }
}

fn parse_cascade_field_type(s: &str) -> CascadeFieldType {
    match s {
        "river" => CascadeFieldType::River,
        "river-root" => CascadeFieldType::RiverRoot,
        "center-split" => CascadeFieldType::CenterSplit,
        "oscillate" => CascadeFieldType::Oscillate,
        "square-pop" => CascadeFieldType::SquarePop,
        _ => CascadeFieldType::Vortex,
    }
}

fn cli_bitstream_operation(op: DatamoshBitstreamOperation) -> CliDatamoshBitstreamOperation {
    match op {
        DatamoshBitstreamOperation::PframeDuplicate => {
            CliDatamoshBitstreamOperation::PframeDuplicate
        }
        DatamoshBitstreamOperation::RemoveKeyframe => CliDatamoshBitstreamOperation::RemoveKeyframe,
        DatamoshBitstreamOperation::MotionTransfer => CliDatamoshBitstreamOperation::MotionTransfer,
    }
}

pub(crate) fn queue_add_datamosh_bitstream(
    request: QueueAddDatamoshBitstreamRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddDatamoshBitstreamRequest {
        queue_path,
        input_video,
        output_root_dir,
        fps,
        mut operation,
        mut p_frame_index,
        mut duplicate_count,
        carrier_video,
        mut carrier_keyframes,
        preset,
        project_path,
    } = request;

    if !fps.is_finite() || fps <= 0.0 {
        return Err(CliError::Message(
            "fps must be finite and positive".to_string(),
        ));
    }

    // Preset resolution: override operation + numeric knobs.
    match preset {
        DatamoshBitstreamPreset::Custom => {}
        DatamoshBitstreamPreset::Bloom => {
            operation = DatamoshBitstreamOperation::PframeDuplicate;
            p_frame_index = 0;
            duplicate_count = 8;
        }
        DatamoshBitstreamPreset::HeavyMelt => {
            operation = DatamoshBitstreamOperation::PframeDuplicate;
            p_frame_index = 0;
            duplicate_count = 60;
        }
        DatamoshBitstreamPreset::VoidMosh => {
            operation = DatamoshBitstreamOperation::RemoveKeyframe;
        }
        DatamoshBitstreamPreset::MotionGraft => {
            operation = DatamoshBitstreamOperation::MotionTransfer;
            carrier_keyframes = 1;
        }
    }

    if operation == DatamoshBitstreamOperation::MotionTransfer && carrier_video.is_none() {
        return Err(CliError::Message(
            "motion-transfer requires --carrier-video (Source B whose appearance is kept)"
                .to_string(),
        ));
    }

    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    let mut sources = vec![RenderJobSourceProvenance {
        source_id: "input-video".to_string(),
        role: SourceRole::Modulator,
        path: input_video.to_string_lossy().to_string(),
    }];
    if let Some(carrier) = carrier_video {
        sources.push(RenderJobSourceProvenance {
            source_id: "carrier-video".to_string(),
            role: SourceRole::Carrier,
            path: carrier.to_string_lossy().to_string(),
        });
    }
    let provenance = RenderJobProvenance {
        sources,
        analysis_caches: Vec::new(),
    };

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|p| p.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 8,
            },
            temporal_supersampling: 1,
            deterministic: false,
        },
        task: RenderJobTask::DatamoshBitstream {
            input_video: input_video.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            fps,
            operation,
            p_frame_index,
            duplicate_count,
            carrier_video: carrier_video.map(|p| p.to_string_lossy().to_string()),
            carrier_keyframes,
            preset,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued datamosh bitstream render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_datamosh_bitstream(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::DatamoshBitstream { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running datamosh bitstream jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::DatamoshBitstream {
        input_video,
        output_directory,
        fps,
        operation,
        p_frame_index,
        duplicate_count,
        carrier_video,
        carrier_keyframes,
        preset,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a datamosh bitstream render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(&output_directory);
    let frames_dir = output_dir.join("frames");
    let provenance = queue.jobs[job_index].provenance.clone();
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let cli_operation = cli_bitstream_operation(operation);
    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        datamosh_bitstream(DatamoshBitstreamRequest {
            input: Path::new(&input_video),
            output_dir: &frames_dir,
            fps,
            operation: cli_operation,
            p_frame_index,
            duplicate_count,
            carrier: carrier_video.as_deref().map(Path::new),
            carrier_keyframes,
        })?;

        // Count output frames written by the bitstream handler.
        let mut frame_paths: Vec<String> = Vec::new();
        if frames_dir.is_dir() {
            for entry in fs::read_dir(&frames_dir)? {
                let entry = entry?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with("frame_") && name_str.ends_with(".png") {
                    frame_paths.push(format!("frames/{name_str}"));
                }
            }
        }
        frame_paths.sort();
        let frame_count = u32::try_from(frame_paths.len()).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate: fps,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: if fps > 0.0 {
                frame_count as f64 / fps
            } else {
                0.0
            },
            sample_rate: 48_000,
            audio_sample_count: 0,
        };

        let manifest = serde_json::json!({
            "job_id": job_id,
            "status": "complete",
            "task": "datamosh_bitstream",
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
            "datamosh_bitstream": {
                "algorithm": datamosh_bitstream_algorithm(cli_operation),
                "deterministic": false,
                "operation": datamosh_bitstream_operation_name(cli_operation),
                "p_frame_index": p_frame_index,
                "duplicate_count": duplicate_count,
                "carrier_keyframes": carrier_keyframes,
                "fps": fps,
                "codec": "mpeg4",
                "ffmpeg_version": morphogen_media::ffmpeg_version().unwrap_or_default(),
                "preset": bitstream_preset_label(preset),
                "note": "Real bitstream datamosh: output is NOT bit-reproducible (depends on external ffmpeg MPEG-4 codec)."
            },
            "provenance": provenance,
            "deterministic": false
        });
        fs::write(
            output_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        write_frame_sequence_checkpoint(&job_id, &output_dir, &frame_paths, frame_count)?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued datamosh bitstream job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("datamosh bitstream job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) struct QueueAddConvolutionalBlendSequenceRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) settings: ConvolutionBlendSettings,
    pub(crate) kernel_mode: KernelMode,
    pub(crate) max_frames: Option<u32>,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
}

pub(crate) fn queue_add_convolutional_blend_sequence(
    request: QueueAddConvolutionalBlendSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddConvolutionalBlendSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
        settings,
        kernel_mode,
        max_frames,
        project_path,
        backend,
    } = request;
    settings.validate()?;
    if matches!(max_frames, Some(0)) {
        return Err(CliError::Message(
            "max-frames must be greater than zero".to_string(),
        ));
    }

    let mut queue = if queue_path.exists() {
        RenderQueue::load_json(queue_path)?
    } else {
        RenderQueue::default()
    };
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);
    let provenance = RenderJobProvenance {
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
        analysis_caches: Vec::new(),
    };

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 1920,
            height: 1080,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::ImageSequence {
                extension: "png".to_string(),
                bit_depth: 8,
            },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::FrameSequenceConvolutionBlend {
            modulator_frame_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            kernel_size: settings.kernel_size,
            amount: settings.amount,
            max_frames,
            backend,
            kernel_mode,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued convolutional blend render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_convolutional_blend_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceConvolutionBlend { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running convolutional blend jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::FrameSequenceConvolutionBlend {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        kernel_size,
        amount,
        max_frames,
        backend,
        kernel_mode,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a convolutional blend render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result =
            render_convolutional_blend_sequence(ConvolutionalBlendSequenceRequest {
                modulator_dir: Path::new(&modulator_frame_directory),
                carrier_dir: Path::new(&carrier_frame_directory),
                output_dir: &output_dir.join("frames"),
                settings: ConvolutionBlendSettings {
                    kernel_size,
                    amount,
                },
                kernel_mode,
                backend,
                max_frames: max_frames.map(|value| value as usize),
            })?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate: 30.0,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: frame_count as f64 / 30.0,
            sample_rate: 48_000,
            audio_sample_count: 0,
        };
        let frame_paths = (0..frame_count)
            .map(|index| format!("frames/frame_{index:06}.png"))
            .collect::<Vec<_>>();
        let manifest = serde_json::json!({
            "job_id": job_id,
            "status": "complete",
            "task": "frame_sequence_convolution_blend",
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
            "convolution_blend": {
                "algorithm": convolution_blend_algorithm(kernel_mode),
                "kernel_size": kernel_size,
                "amount": amount,
                "kernel_mode": kernel_mode_label(kernel_mode),
                "backend": render_backend_label(backend)
            },
            "provenance": queue.jobs[job_index].provenance,
            "deterministic": true
        });
        fs::write(
            output_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        write_frame_sequence_checkpoint(&job_id, &output_dir, &frame_paths, frame_count)?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued convolutional blend job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("convolutional blend job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) fn queue_run_feedback_sequence(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequenceFlowFeedback { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running flow-feedback jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let output_bit_depth = feedback_output_bit_depth(&queue.jobs[job_index].settings)?;
    let temporal_supersampling = queue.jobs[job_index].settings.temporal_supersampling;
    validate_feedback_export_settings(output_bit_depth, temporal_supersampling)?;
    let RenderJobTask::FrameSequenceFlowFeedback {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        flow_cache_directory,
        carrier_amount,
        feedback_amount,
        feedback_mix,
        decay,
        iterations,
        max_frames,
        reset_at_frame,
        frame_rate,
        backend,
        flow_source,
        structure_mix,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a flow-feedback render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);
    let provenance = feedback_sequence_provenance(
        Path::new(&modulator_frame_directory),
        Path::new(&carrier_frame_directory),
        flow_cache_directory.as_deref().map(Path::new),
        flow_source_algorithm(flow_source),
    );

    queue.jobs[job_index].provenance = Some(provenance.clone());
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_feedback_sequence(FeedbackSequenceRenderRequest {
            modulator_dir: Path::new(&modulator_frame_directory),
            carrier_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir,
            flow_cache_dir: flow_cache_directory.as_deref().map(Path::new),
            max_frames: max_frames.map(|value| value as usize),
            reset_at_frame: reset_at_frame.map(|value| value as usize),
            frame_rate,
            settings: FlowFeedbackSettings {
                carrier_amount,
                feedback_amount,
                feedback_mix,
                decay,
                iterations,
                structure_mix,
                // Persisted queue jobs render single-scale structure (backlog:
                // Structure-Preserving Morph task 5 follow-up exposes multiscale
                // once it has a Metal parity path).
                structure_mode: StructureMode::SingleScale,
            },
            output_bit_depth,
            temporal_supersampling,
            backend,
            flow_source,
            job_id: &job_id,
            provenance: Some(&provenance),
            stop_after_frame: false,
            modulation: ModulationCliArgs {
                modulate: &modulation_specs,
                modulator_audio: modulator_audio_path.as_deref().map(Path::new),
                modulator_frames: modulator_frames_directory.as_deref().map(Path::new),
                sampling: render_modulation_sampling(modulation_sampling),
                // The job's frame_rate is the envelope time base — the same
                // convention as the direct CLI, which samples against
                // --frame-rate.
                fps: frame_rate,
                // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                cache_dir: None,
                named_modulator_audio: &named_modulator_audio_specs,
                named_modulator_frames: &named_modulator_frames_specs,
            },
        })?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let timing = RenderTimingMetadata {
            frame_rate,
            frame_count,
            start_seconds: 0.0,
            duration_seconds: frame_count as f64 / frame_rate,
            sample_rate: 48_000,
            audio_sample_count: 0,
        };
        let frame_paths = (0..frame_count)
            .map(|index| format!("frames/frame_{index:06}.png"))
            .collect::<Vec<_>>();
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths,
            audio_stem_paths: Vec::new(),
            timing,
        })
    })();

    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued flow-feedback job {} to {}",
                job_id,
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("flow-feedback job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) fn feedback_output_bit_depth(settings: &RenderSettings) -> Result<u8, CliError> {
    match &settings.export_format {
        ExportFormat::ImageSequence {
            extension,
            bit_depth,
        } if extension.eq_ignore_ascii_case("png") => Ok(*bit_depth),
        _ => Err(CliError::Message(
            "flow-feedback queue jobs currently require a PNG image-sequence export format"
                .to_string(),
        )),
    }
}

pub(crate) fn write_test_render_output_bundle(
    job_id: &str,
    output_dir: &Path,
    stop_after_frame: bool,
) -> Result<TestRenderOutput, CliError> {
    const TEST_RENDER_FRAME_RATE: f64 = 24.0;
    const TEST_RENDER_SAMPLE_RATE: u32 = 48_000;
    const TEST_RENDER_FRAME_COUNT: u32 = 1;
    const TEST_RENDER_AUDIO_SAMPLE_COUNT: usize = 48_000;

    let frame_dir = output_dir.join("frames");
    let audio_dir = output_dir.join("audio");
    fs::create_dir_all(&frame_dir)?;
    fs::create_dir_all(&audio_dir)?;

    let frame_path = frame_dir.join("frame_000000.png");
    if !frame_path.exists() {
        let carrier = synthetic_carrier(256, 256)?;
        let flow = synthetic_flow(256, 256)?;
        let frame = flow_displace_cpu(&carrier, &flow, 1.0)?;
        save_png(&frame, &frame_path)?;
    }

    let timing = RenderTimingMetadata {
        frame_rate: TEST_RENDER_FRAME_RATE,
        frame_count: TEST_RENDER_FRAME_COUNT,
        start_seconds: 0.0,
        duration_seconds: TEST_RENDER_FRAME_COUNT as f64 / TEST_RENDER_FRAME_RATE,
        sample_rate: TEST_RENDER_SAMPLE_RATE,
        audio_sample_count: TEST_RENDER_AUDIO_SAMPLE_COUNT as u64,
    };

    if stop_after_frame {
        write_test_render_checkpoint(
            job_id,
            output_dir,
            "running",
            &["frames/frame_000000.png"],
            &[],
            1,
        )?;
        return Ok(TestRenderOutput {
            complete: false,
            metadata: RenderJobOutputMetadata {
                output_directory: output_dir.to_string_lossy().to_string(),
                frame_paths: vec!["frames/frame_000000.png".to_string()],
                audio_stem_paths: Vec::new(),
                timing,
            },
        });
    }

    let audio_path = audio_dir.join("main.wav");
    if !audio_path.exists() {
        let stem = synthetic_stereo_stem(TEST_RENDER_SAMPLE_RATE, TEST_RENDER_AUDIO_SAMPLE_COUNT)?;
        save_wav_f32(&audio_path, &stem)?;
    }

    let manifest = serde_json::json!({
        "job_id": job_id,
        "status": "complete",
        "frames": ["frames/frame_000000.png"],
        "audio_stems": ["audio/main.wav"],
        "timing": {
            "frame_rate": timing.frame_rate,
            "frame_count": timing.frame_count,
            "start_seconds": timing.start_seconds,
            "duration_seconds": timing.duration_seconds,
            "sample_rate": timing.sample_rate,
            "audio_sample_count": timing.audio_sample_count
        },
        "deterministic": true
    });
    fs::write(
        output_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    write_test_render_checkpoint(
        job_id,
        output_dir,
        "complete",
        &["frames/frame_000000.png"],
        &["audio/main.wav"],
        1,
    )?;
    Ok(TestRenderOutput {
        complete: true,
        metadata: RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths: vec!["frames/frame_000000.png".to_string()],
            audio_stem_paths: vec!["audio/main.wav".to_string()],
            timing,
        },
    })
}

pub(crate) struct TestRenderOutput {
    complete: bool,
    metadata: RenderJobOutputMetadata,
}

pub(crate) fn write_test_render_checkpoint(
    job_id: &str,
    output_dir: &Path,
    status: &str,
    frames: &[&str],
    audio_stems: &[&str],
    next_frame_index: u32,
) -> Result<(), CliError> {
    let checkpoint = serde_json::json!({
        "job_id": job_id,
        "status": status,
        "completed_frames": frames,
        "completed_audio_stems": audio_stems,
        "next_frame_index": next_frame_index
    });
    fs::write(
        output_dir.join("checkpoint.json"),
        serde_json::to_string_pretty(&checkpoint)?,
    )?;
    Ok(())
}

pub(crate) fn write_frame_sequence_manifest(
    job_id: &str,
    output_dir: &Path,
    frame_paths: &[String],
    timing: &RenderTimingMetadata,
    provenance: Option<&RenderJobProvenance>,
) -> Result<(), CliError> {
    let manifest = serde_json::json!({
        "job_id": job_id,
        "status": "complete",
        "task": "frame_sequence_flow_displace",
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
        "provenance": provenance,
        "deterministic": true
    });
    fs::write(
        output_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

pub(crate) struct ExperimentalFrameSequenceManifest<'a> {
    job_id: &'a str,
    output_dir: &'a Path,
    frame_count: usize,
    frame_rate: f64,
    task: &'a str,
    effect_key: &'a str,
    effect: serde_json::Value,
    provenance: Option<&'a RenderJobProvenance>,
}

pub(crate) fn complete_experimental_frame_sequence_job(
    manifest: ExperimentalFrameSequenceManifest<'_>,
) -> Result<RenderJobOutputMetadata, CliError> {
    let ExperimentalFrameSequenceManifest {
        job_id,
        output_dir,
        frame_count,
        frame_rate,
        task,
        effect_key,
        effect,
        provenance,
    } = manifest;
    let frame_count_u32 = u32::try_from(frame_count).map_err(|_| {
        CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
    })?;
    let timing = RenderTimingMetadata {
        frame_rate,
        frame_count: frame_count_u32,
        start_seconds: 0.0,
        duration_seconds: frame_count_u32 as f64 / frame_rate,
        sample_rate: 48_000,
        audio_sample_count: 0,
    };
    let frame_paths = (0..frame_count_u32)
        .map(|index| format!("frames/frame_{index:06}.png"))
        .collect::<Vec<_>>();
    write_experimental_frame_sequence_manifest(ExperimentalFrameSequenceManifestWrite {
        job_id,
        output_dir,
        frame_paths: &frame_paths,
        timing: &timing,
        task,
        effect_key,
        effect,
        provenance,
    })?;
    write_frame_sequence_checkpoint(job_id, output_dir, &frame_paths, frame_count_u32)?;
    Ok(RenderJobOutputMetadata {
        output_directory: output_dir.to_string_lossy().to_string(),
        frame_paths,
        audio_stem_paths: Vec::new(),
        timing,
    })
}

pub(crate) struct ExperimentalFrameSequenceManifestWrite<'a> {
    job_id: &'a str,
    output_dir: &'a Path,
    frame_paths: &'a [String],
    timing: &'a RenderTimingMetadata,
    task: &'a str,
    effect_key: &'a str,
    effect: serde_json::Value,
    provenance: Option<&'a RenderJobProvenance>,
}

pub(crate) fn write_experimental_frame_sequence_manifest(
    manifest: ExperimentalFrameSequenceManifestWrite<'_>,
) -> Result<(), CliError> {
    let ExperimentalFrameSequenceManifestWrite {
        job_id,
        output_dir,
        frame_paths,
        timing,
        task,
        effect_key,
        effect,
        provenance,
    } = manifest;
    let mut manifest = serde_json::json!({
        "job_id": job_id,
        "status": "complete",
        "task": task,
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
        "provenance": provenance,
        "deterministic": true
    });
    if let Some(object) = manifest.as_object_mut() {
        object.insert(effect_key.to_string(), effect);
    }
    fs::write(
        output_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

pub(crate) fn finish_frame_sequence_queue_job(
    queue: &mut RenderQueue,
    queue_path: &Path,
    job_index: usize,
    job_id: &str,
    output_dir: &Path,
    outcome: Result<RenderJobOutputMetadata, CliError>,
    effect_label: &str,
) -> Result<(), CliError> {
    match outcome {
        Ok(metadata) => {
            queue.jobs[job_index].status = RenderJobStatus::Complete;
            queue.jobs[job_index].output = Some(metadata);
            queue.jobs[job_index].failure = None;
            queue.save_json(queue_path)?;
            println!(
                "rendered queued {effect_label} job {job_id} to {}",
                output_dir.display()
            );
            Ok(())
        }
        Err(error) => {
            queue.jobs[job_index].status = RenderJobStatus::Failed;
            queue.jobs[job_index].failure = Some(RenderJobFailure {
                message: error.to_string(),
            });
            queue.save_json(queue_path)?;
            eprintln!("{effect_label} job {job_id} failed: {error}");
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn write_granular_mosaic_sequence_manifest(
    job_id: &str,
    output_dir: &Path,
    frame_paths: &[String],
    timing: &RenderTimingMetadata,
    settings: &GranularMosaicSettings,
    audio_modulation: Option<&GranularAudioModulation>,
    provenance: Option<&RenderJobProvenance>,
    selection_mode: GrainSelectionMode,
) -> Result<(), CliError> {
    let manifest = serde_json::json!({
        "job_id": job_id,
        "status": "complete",
        "task": "frame_sequence_granular_mosaic",
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
        "granular_mosaic": {
            "algorithm": grain_selection_algorithm(selection_mode),
            "settings": settings,
            "audio_modulation": audio_modulation
        },
        "provenance": provenance,
        "deterministic": true
    });
    fs::write(
        output_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

pub(crate) struct GranularMosaicPoolManifest<'a> {
    job_id: &'a str,
    output_dir: &'a Path,
    frame_paths: &'a [String],
    timing: &'a RenderTimingMetadata,
    settings: &'a GranularMosaicSettings,
    audio_weight: f32,
    texture_weight: f32,
    modulator_rms_cache: Option<&'a str>,
    carrier_rms_cache: Option<&'a str>,
    modulator_centroid_cache: Option<&'a str>,
    carrier_centroid_cache: Option<&'a str>,
    pool_window: u32,
    anti_repeat_weight: f32,
    anti_repeat_cooldown: u32,
    coherence_weight: f32,
    coherence_reach: u32,
    spatial_coherence_weight: f32,
    backend: RenderBackend,
    provenance: Option<&'a RenderJobProvenance>,
}

pub(crate) fn write_granular_mosaic_pool_sequence_manifest(
    manifest: GranularMosaicPoolManifest<'_>,
) -> Result<(), CliError> {
    let GranularMosaicPoolManifest {
        job_id,
        output_dir,
        frame_paths,
        timing,
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
        backend,
        provenance,
    } = manifest;
    let manifest = serde_json::json!({
        "job_id": job_id,
        "status": "complete",
        "task": "frame_sequence_granular_mosaic_pool",
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
        "granular_mosaic_pool": {
            "algorithm": POOLED_GRAIN_ALGORITHM,
            "settings": settings,
            "audio_weight": audio_weight,
            "texture_weight": texture_weight,
            "modulator_rms_cache": modulator_rms_cache,
            "carrier_rms_cache": carrier_rms_cache,
            "modulator_centroid_cache": modulator_centroid_cache,
            "carrier_centroid_cache": carrier_centroid_cache,
            "pool_window": pool_window,
            "anti_repeat_weight": anti_repeat_weight,
            "anti_repeat_cooldown": anti_repeat_cooldown,
            "coherence_weight": coherence_weight,
            "coherence_reach": coherence_reach,
            "spatial_coherence_weight": spatial_coherence_weight,
            "backend": render_backend_label(backend)
        },
        "provenance": provenance,
        "deterministic": true
    });
    fs::write(
        output_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

pub(crate) fn write_frame_sequence_checkpoint(
    job_id: &str,
    output_dir: &Path,
    frame_paths: &[String],
    next_frame_index: u32,
) -> Result<(), CliError> {
    let checkpoint = serde_json::json!({
        "job_id": job_id,
        "status": "complete",
        "completed_frames": frame_paths,
        "completed_audio_stems": [],
        "next_frame_index": next_frame_index
    });
    fs::write(
        output_dir.join("checkpoint.json"),
        serde_json::to_string_pretty(&checkpoint)?,
    )?;
    Ok(())
}

pub(crate) fn synthetic_stereo_stem(
    sample_rate: u32,
    frames: usize,
) -> Result<AudioBufferF32, CliError> {
    let mut samples = Vec::with_capacity(frames.saturating_mul(2));
    for frame in 0..frames {
        let phase = frame as f32 / sample_rate as f32;
        let left = (phase * 440.0 * std::f32::consts::TAU).sin() * 0.125;
        let right = (phase * 220.0 * std::f32::consts::TAU).sin() * 0.125;
        samples.push(left);
        samples.push(right);
    }

    AudioBufferF32::new(2, sample_rate, samples).map_err(CliError::from)
}

pub(crate) fn queue_cancel(queue_path: &Path, job_id: &str) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    queue.cancel_job(job_id)?;
    queue.save_json(queue_path)?;
    println!("cancelled job {job_id} in {}", queue_path.display());
    Ok(())
}

pub(crate) fn queue_inspect(queue_path: &Path) -> Result<(), CliError> {
    let queue = RenderQueue::load_json(queue_path)?;
    println!("render queue: {} job(s)", queue.jobs.len());
    for job in queue.jobs {
        let task_name = match job.task {
            RenderJobTask::TestRender => "test_render",
            RenderJobTask::FrameSequenceFlowDisplace { .. } => "frame_sequence_flow_displace",
            RenderJobTask::FrameSequenceFlowFeedback { .. } => "frame_sequence_flow_feedback",
            RenderJobTask::FrameSequenceFluidAdvect { .. } => "frame_sequence_fluid_advect",
            RenderJobTask::FrameSequenceFluidAdvectTwoSource { .. } => {
                "frame_sequence_fluid_advect_two_source"
            }
            RenderJobTask::FrameSequenceOpticalFlowAdvect { .. } => {
                "frame_sequence_optical_flow_advect"
            }
            RenderJobTask::FrameSequenceFieldParticles { .. } => "frame_sequence_field_particles",
            RenderJobTask::FrameSequenceCascadeTrails { .. } => "frame_sequence_cascade_trails",
            RenderJobTask::FrameSequenceCascadeCollage { .. } => "frame_sequence_cascade_collage",
            RenderJobTask::FrameSequenceRetroStatic { .. } => "frame_sequence_retro_static",
            RenderJobTask::FrameSequenceChannelShift { .. } => "frame_sequence_channel_shift",
            RenderJobTask::FrameSequencePaletteQuantize { .. } => "frame_sequence_palette_quantize",
            RenderJobTask::FrameSequenceRuttEtra { .. } => "frame_sequence_rutt_etra",
            RenderJobTask::RenderChain { .. } => "render_chain",
            RenderJobTask::FrameSequenceBlockCollage { .. } => "frame_sequence_block_collage",
            RenderJobTask::FrameSequencePixelSort { .. } => "frame_sequence_pixel_sort",
            RenderJobTask::FrameSequenceGranularMosaic { .. } => "frame_sequence_granular_mosaic",
            RenderJobTask::FrameSequenceGranularMosaicPool { .. } => {
                "frame_sequence_granular_mosaic_pool"
            }
            RenderJobTask::FrameSequenceVideoVocoder { .. } => "frame_sequence_video_vocoder",
            RenderJobTask::FrameSequenceAudioVideoRoute { .. } => {
                "frame_sequence_audio_video_route"
            }
            RenderJobTask::FrameSequenceDatamosh { .. } => "frame_sequence_datamosh",
            RenderJobTask::DatamoshBitstream { .. } => "datamosh_bitstream",
            RenderJobTask::FrameSequenceConvolutionBlend { .. } => {
                "frame_sequence_convolution_blend"
            }
            RenderJobTask::AudioSpectralCrossSynth { .. } => "audio_spectral_cross_synth",
            RenderJobTask::AudioImpulseConvolution { .. } => "audio_impulse_convolution",
            RenderJobTask::VideoAudioRoute { .. } => "video_audio_route",
        };
        let provenance_summary = job
            .provenance
            .as_ref()
            .map(|provenance| {
                format!(
                    " sources={} caches={}",
                    provenance.sources.len(),
                    provenance.analysis_caches.len()
                )
            })
            .unwrap_or_default();
        let output_summary = job
            .output
            .as_ref()
            .map(|output| {
                format!(
                    " output={} frames={} stems={} fps={:.3}",
                    output.output_directory,
                    output.frame_paths.len(),
                    output.audio_stem_paths.len(),
                    output.timing.frame_rate
                )
            })
            .unwrap_or_default();
        let failure_summary = job
            .failure
            .as_ref()
            .map(|failure| format!(" failure=\"{}\"", failure.message))
            .unwrap_or_default();
        println!(
            "  - {} task={} status={:?} size={}x{} project={}{}{}{}",
            job.id,
            task_name,
            job.status,
            job.settings.width,
            job.settings.height,
            job.project_path.as_deref().unwrap_or("<none>"),
            provenance_summary,
            output_summary,
            failure_summary
        );
    }
    Ok(())
}

// --- Modulation-route persistence bridges (core ↔ render are both foreign,
// so free helpers per the orphan-rule precedent) ---

fn core_lfo_shape(shape: LfoShape) -> CoreLfoShape {
    match shape {
        LfoShape::Sine => CoreLfoShape::Sine,
        LfoShape::Triangle => CoreLfoShape::Triangle,
        LfoShape::Square => CoreLfoShape::Square,
        LfoShape::Saw => CoreLfoShape::Saw,
    }
}

fn core_modulation_source(source: ModulationSource) -> CoreModulationSource {
    match source {
        ModulationSource::AudioRms => CoreModulationSource::AudioRms,
        ModulationSource::AudioOnset => CoreModulationSource::AudioOnset,
        ModulationSource::AudioCentroid => CoreModulationSource::AudioCentroid,
        ModulationSource::Luma => CoreModulationSource::Luma,
        ModulationSource::Flow => CoreModulationSource::Flow,
        ModulationSource::Lfo {
            shape,
            rate_hz,
            phase,
        } => CoreModulationSource::Lfo {
            shape: core_lfo_shape(shape),
            rate_hz,
            phase,
        },
    }
}

fn core_modulation_sampling(sampling: ModulationSampling) -> CoreModulationSampling {
    match sampling {
        ModulationSampling::Hold => CoreModulationSampling::Hold,
        ModulationSampling::Smooth => CoreModulationSampling::Smooth,
    }
}

fn render_modulation_sampling(sampling: CoreModulationSampling) -> ModulationSampling {
    match sampling {
        CoreModulationSampling::Hold => ModulationSampling::Hold,
        CoreModulationSampling::Smooth => ModulationSampling::Smooth,
    }
}

fn modulation_sampling_label(sampling: CoreModulationSampling) -> &'static str {
    match sampling {
        CoreModulationSampling::Hold => "hold",
        CoreModulationSampling::Smooth => "smooth",
    }
}

/// The three things a queue-add handler needs to persist on a modulatable
/// task: the resolved routes and the two named-modulator media vectors
/// (given order preserved, per `docs/MODULATION_MATRIX_MILESTONE.md`).
struct QueueModulationRoutes {
    routes: Vec<RenderJobModulationRoute>,
    named_audio: Vec<NamedModulatorMedia>,
    named_frames: Vec<NamedModulatorMedia>,
}

/// Parse and validate `--modulate` (+ `--named-modulator-*`) specs at
/// queue-add time — grammar, duplicate targets, modulator-flag requirements
/// (named and default), and (via `probe`, the effect's apply function on a
/// scratch settings copy) unknown targets all fail here, before the job
/// persists. Reuses `resolve_modulator_media` so a missing-media error is
/// worded identically to the direct-render path.
fn parse_queue_modulation_routes(
    specs: &[String],
    modulator_audio: Option<&Path>,
    modulator_frames: Option<&Path>,
    named_modulator_audio: &[String],
    named_modulator_frames: &[String],
    // CliError so CLI-side apply fns (datamosh) probe directly; render-side
    // apply fns map with `CliError::from`.
    mut probe: impl FnMut(&str) -> Result<(), CliError>,
) -> Result<QueueModulationRoutes, CliError> {
    let routes = specs
        .iter()
        .map(|spec| parse_modulation_route(spec))
        .collect::<Result<Vec<_>, _>>()?;
    validate_route_targets(&routes)?;
    let named_audio =
        parse_named_modulator_specs(named_modulator_audio, "--named-modulator-audio")?;
    let named_frames =
        parse_named_modulator_specs(named_modulator_frames, "--named-modulator-frames")?;
    for route in &routes {
        probe(&route.target)?;
        if route.source.needs_audio() {
            resolve_modulator_media(
                route,
                modulator_audio,
                &named_audio,
                "--modulator-audio <wav>",
                "--named-modulator-audio",
            )?;
        }
        if route.source.needs_frames() {
            resolve_modulator_media(
                route,
                modulator_frames,
                &named_frames,
                "--modulator-frames <dir>",
                "--named-modulator-frames",
            )?;
        }
    }
    let routes = routes
        .into_iter()
        .map(|route| RenderJobModulationRoute {
            target: route.target,
            source: core_modulation_source(route.source),
            scale: route.scale,
            offset: route.offset,
            sampling: route.sampling.map(core_modulation_sampling),
            modulator: route.modulator,
        })
        .collect();
    let to_named_media = |named: Vec<(String, PathBuf)>| -> Vec<NamedModulatorMedia> {
        named
            .into_iter()
            .map(|(name, path)| NamedModulatorMedia {
                name,
                path: path.to_string_lossy().to_string(),
            })
            .collect()
    };
    Ok(QueueModulationRoutes {
        routes,
        named_audio: to_named_media(named_audio),
        named_frames: to_named_media(named_frames),
    })
}

/// Reconstruct the CLI route specs from persisted routes so queue-run shares
/// the direct render's exact code path. `f32`'s `Display` prints the shortest
/// round-tripping decimal, so `parse(format(x)) == x` bit-for-bit. A named
/// route's `<name>.` prefix is restored ahead of the source name.
fn modulation_specs_from_routes(routes: &[RenderJobModulationRoute]) -> Vec<String> {
    routes
        .iter()
        .map(|route| {
            let suffix = match route.sampling {
                Some(sampling) => format!("@{}", modulation_sampling_label(sampling)),
                None => String::new(),
            };
            let modulator = route
                .modulator
                .as_deref()
                .map(|name| format!("{name}."))
                .unwrap_or_default();
            format!(
                "{}={modulator}{}:{},{}{suffix}",
                route.target,
                route.source.spec_text(),
                route.scale,
                route.offset
            )
        })
        .collect()
}

/// Reconstruct the repeatable `<name>=<path>` `--named-modulator-*` specs
/// from persisted media so queue-run shares the direct render's exact parse
/// path (given order preserved).
fn named_modulator_specs_from_media(media: &[NamedModulatorMedia]) -> Vec<String> {
    media
        .iter()
        .map(|entry| format!("{}={}", entry.name, entry.path))
        .collect()
}

/// The manifest's `modulation` block; `None` (key omitted) for unmodulated
/// jobs so their manifests stay byte-identical to the pre-slice format.
fn modulation_manifest_json(
    routes: &[RenderJobModulationRoute],
    modulator_audio: Option<&str>,
    modulator_frames: Option<&str>,
    sampling: CoreModulationSampling,
    frame_rate: f64,
) -> Option<serde_json::Value> {
    (!routes.is_empty()).then(|| {
        serde_json::json!({
            "routes": routes,
            "modulator_audio": modulator_audio,
            "modulator_frames": modulator_frames,
            "sampling": modulation_sampling_label(sampling),
            "fps": frame_rate,
        })
    })
}

// --- Core↔render enum bridges for pixel sort ---

fn render_pixel_sort_axis(axis: PixelSortAxis) -> SortAxis {
    match axis {
        PixelSortAxis::Row => SortAxis::Row,
        PixelSortAxis::Col => SortAxis::Col,
    }
}

fn render_pixel_sort_key(key: PixelSortKey) -> SortKey {
    match key {
        PixelSortKey::Luma => SortKey::Luma,
        PixelSortKey::Hue => SortKey::Hue,
        PixelSortKey::Sat => SortKey::Sat,
        PixelSortKey::Red => SortKey::Red,
        PixelSortKey::Green => SortKey::Green,
        PixelSortKey::Blue => SortKey::Blue,
    }
}

fn render_pixel_sort_direction(dir: PixelSortDirection) -> SortDirection {
    match dir {
        PixelSortDirection::Asc => SortDirection::Asc,
        PixelSortDirection::Desc => SortDirection::Desc,
    }
}

fn render_pixel_sort_mask_source(ms: PixelSortMaskSource) -> MaskSource {
    match ms {
        PixelSortMaskSource::SelfMask => MaskSource::SelfMask,
        PixelSortMaskSource::ALuma => MaskSource::ALuma,
        PixelSortMaskSource::AEdge => MaskSource::AEdge,
        PixelSortMaskSource::AFlow => MaskSource::AFlow,
    }
}

pub(crate) struct QueueAddPixelSortSequenceRequest<'a> {
    pub(crate) queue_path: &'a std::path::Path,
    pub(crate) source_a_dir: &'a std::path::Path,
    pub(crate) source_b_dir: &'a std::path::Path,
    pub(crate) output_root_dir: &'a std::path::Path,
    pub(crate) axis: PixelSortAxis,
    pub(crate) key: PixelSortKey,
    pub(crate) direction: PixelSortDirection,
    pub(crate) threshold_low: f32,
    pub(crate) threshold_high: f32,
    pub(crate) max_span: u32,
    pub(crate) mask_source: PixelSortMaskSource,
    pub(crate) flow_radius: i32,
    pub(crate) backend: RenderBackend,
    pub(crate) frames: u32,
    pub(crate) frame_rate: f64,
    pub(crate) project_path: Option<&'a std::path::Path>,
    pub(crate) modulate: &'a [String],
    pub(crate) modulator_audio: Option<&'a std::path::Path>,
    pub(crate) modulator_frames: Option<&'a std::path::Path>,
    pub(crate) modulation_sampling: ModulationSampling,
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
}

pub(crate) fn queue_add_pixel_sort_sequence(
    request: QueueAddPixelSortSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddPixelSortSequenceRequest {
        queue_path,
        source_a_dir,
        source_b_dir,
        output_root_dir,
        axis,
        key,
        direction,
        threshold_low,
        threshold_high,
        max_span,
        mask_source,
        flow_radius,
        backend,
        frames,
        frame_rate,
        project_path,
        modulate,
        modulator_audio,
        modulator_frames,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = request;
    validate_queued_sequence_timing(frames, frame_rate)?;

    let modulation = parse_queue_modulation_routes(
        modulate,
        modulator_audio,
        modulator_frames,
        named_modulator_audio,
        named_modulator_frames,
        |target| {
            let mut probe = PixelSortSettings::default();
            apply_pixel_sort_modulation(&mut probe, target, 0.0).map_err(CliError::from)
        },
    )?;

    let mut queue = load_or_default_queue(queue_path)?;
    let job_id = format!("job-{:04}", queue.jobs.len() + 1);
    let job_output_dir = output_root_dir.join(&job_id);

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|p| p.to_string_lossy().to_string()),
        settings: png_sequence_settings(frame_rate),
        task: RenderJobTask::FrameSequencePixelSort {
            modulator_frame_directory: source_a_dir.to_string_lossy().to_string(),
            carrier_frame_directory: source_b_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            frames,
            frame_rate,
            axis,
            key,
            direction,
            threshold_low,
            threshold_high,
            max_span,
            mask_source,
            flow_radius,
            backend,
            modulation_routes: modulation.routes,
            modulator_audio_path: modulator_audio.map(|p| p.to_string_lossy().to_string()),
            modulator_frames_directory: modulator_frames.map(|p| p.to_string_lossy().to_string()),
            modulation_sampling: core_modulation_sampling(modulation_sampling),
            named_modulator_audio: modulation.named_audio,
            named_modulator_frames: modulation.named_frames,
        },
        provenance: Some(two_source_provenance(source_a_dir, source_b_dir)),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued pixel-sort render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_pixel_sort_sequence(queue_path: &std::path::Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::FrameSequencePixelSort { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message("render queue has no queued or running pixel-sort jobs".to_string())
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let provenance = queue.jobs[job_index].provenance.clone();
    let RenderJobTask::FrameSequencePixelSort {
        modulator_frame_directory,
        carrier_frame_directory,
        output_directory,
        frames,
        frame_rate,
        axis,
        key,
        direction,
        threshold_low,
        threshold_high,
        max_span,
        mask_source,
        flow_radius,
        backend,
        modulation_routes,
        modulator_audio_path,
        modulator_frames_directory,
        modulation_sampling,
        named_modulator_audio,
        named_modulator_frames,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a pixel-sort render".to_string(),
        ));
    };
    let output_dir = std::path::PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let settings = PixelSortSettings {
        axis: render_pixel_sort_axis(axis),
        key: render_pixel_sort_key(key),
        direction: render_pixel_sort_direction(direction),
        threshold_low,
        threshold_high,
        max_span,
        mask_source: render_pixel_sort_mask_source(mask_source),
    };
    let algorithm = match mask_source {
        PixelSortMaskSource::SelfMask => PIXEL_SORT_ALGORITHM,
        _ => PIXEL_SORT_CROSS_SYNTH_ALGORITHM,
    };
    let backend_label = format!("{backend:?}");
    let modulation_specs = modulation_specs_from_routes(&modulation_routes);
    let named_modulator_audio_specs = named_modulator_specs_from_media(&named_modulator_audio);
    let named_modulator_frames_specs = named_modulator_specs_from_media(&named_modulator_frames);

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_pixel_sort_sequence(PixelSortSequenceRequest {
            source_a_dir: std::path::Path::new(&modulator_frame_directory),
            source_b_dir: std::path::Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            settings,
            frames,
            backend,
            flow_radius,
            modulation: ModulationCliArgs {
                modulate: &modulation_specs,
                modulator_audio: modulator_audio_path.as_deref().map(std::path::Path::new),
                modulator_frames: modulator_frames_directory
                    .as_deref()
                    .map(std::path::Path::new),
                sampling: render_modulation_sampling(modulation_sampling),
                // The job's frame_rate is the sequence time base.
                fps: frame_rate,
                // Queue jobs render uncached envelopes for now (the sidecar is a direct-CLI flag).
                cache_dir: None,
                named_modulator_audio: &named_modulator_audio_specs,
                named_modulator_frames: &named_modulator_frames_specs,
            },
        })?;
        let mut effect = serde_json::json!({
            "algorithm": algorithm,
            "settings": settings,
            "backend": backend_label,
        });
        if let Some(modulation) = modulation_manifest_json(
            &modulation_routes,
            modulator_audio_path.as_deref(),
            modulator_frames_directory.as_deref(),
            modulation_sampling,
            frame_rate,
        ) {
            effect["modulation"] = modulation;
        }
        complete_experimental_frame_sequence_job(ExperimentalFrameSequenceManifest {
            job_id: &job_id,
            output_dir: &output_dir,
            frame_count: render_result.frame_count,
            frame_rate,
            task: "frame_sequence_pixel_sort",
            effect_key: "pixel_sort",
            effect,
            provenance: provenance.as_ref(),
        })
    })();

    finish_frame_sequence_queue_job(
        &mut queue,
        queue_path,
        job_index,
        &job_id,
        &output_dir,
        outcome,
        "pixel-sort",
    )
}
