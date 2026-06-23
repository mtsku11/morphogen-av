use std::{
    fs,
    path::{Path, PathBuf},
};

use morphogen_audio::{
    save_wav_f32,
    AudioBufferF32,
};
use morphogen_core::{
    AnalysisKind, ExportFormat, FlowSource, GrainSelectionMode, GranularAudioModulation, KernelMode,
    RenderBackend, RenderJob, RenderJobAnalysisCacheProvenance, RenderJobFailure,
    RenderJobOutputMetadata, RenderJobProvenance, RenderJobSourceProvenance, RenderJobStatus,
    RenderJobTask, RenderQuality, RenderQueue, RenderSettings, RenderTimingMetadata, SourceRole,
    VideoVocoderMode,
};
use morphogen_render::{
    ConvolutionBlendSettings,
    flow_displace_cpu, VideoVocoderSettings, FlowFeedbackSettings, GranularMosaicSettings, StructureMode, DATAMOSH_BLOOM_ALGORITHM, RMS_DISPLACEMENT_ROUTE_ALGORITHM, POOLED_GRAIN_ALGORITHM,
};

use crate::args::*;
use crate::error::CliError;
use crate::imaging::*;
use crate::render::*;
pub(crate) fn queue_init(queue_path: &Path) -> Result<(), CliError> {
    let queue = RenderQueue::default();
    queue.save_json(queue_path)?;
    println!("wrote empty render queue to {}", queue_path.display());
    Ok(())
}

pub(crate) fn queue_add_test(queue_path: &Path, project_path: Option<&Path>) -> Result<(), CliError> {
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

pub(crate) fn queue_add_frame_sequence(request: QueueAddFrameSequenceRequest<'_>) -> Result<(), CliError> {
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
            CliError::Message("render queue has no queued or running video-vocoder jobs".to_string())
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
    pub(crate) max_frames: Option<u32>,
    pub(crate) project_path: Option<&'a Path>,
    pub(crate) backend: RenderBackend,
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
        max_frames,
        project_path,
        backend,
    } = request;
    if !amount.is_finite() || amount < 0.0 {
        return Err(CliError::Message(
            "amount must be finite and non-negative".to_string(),
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
        task: RenderJobTask::FrameSequenceDatamosh {
            modulator_frame_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_frame_directory: carrier_dir.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            keyframe_interval,
            amount,
            max_frames,
            backend,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!("queued datamosh render job {job_id} in {}", queue_path.display());
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
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a datamosh render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let render_result = render_datamosh_sequence(DatamoshSequenceRequest {
            modulator_dir: Path::new(&modulator_frame_directory),
            carrier_dir: Path::new(&carrier_frame_directory),
            output_dir: &output_dir.join("frames"),
            keyframe_interval,
            amount,
            backend,
            max_frames: max_frames.map(|value| value as usize),
        })?;
        let frame_count = u32::try_from(render_result.frame_count).map_err(|_| {
            CliError::Message("frame sequence contains more than u32::MAX frames".to_string())
        })?;
        let frame_rate = 30.0;
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
            "datamosh": {
                "algorithm": DATAMOSH_BLOOM_ALGORITHM,
                "keyframe_interval": keyframe_interval,
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
            println!("rendered queued datamosh job {} to {}", job_id, output_dir.display());
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
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a flow-feedback render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
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

pub(crate) fn synthetic_stereo_stem(sample_rate: u32, frames: usize) -> Result<AudioBufferF32, CliError> {
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
            RenderJobTask::FrameSequenceGranularMosaic { .. } => "frame_sequence_granular_mosaic",
            RenderJobTask::FrameSequenceGranularMosaicPool { .. } => {
                "frame_sequence_granular_mosaic_pool"
            }
            RenderJobTask::FrameSequenceVideoVocoder { .. } => "frame_sequence_video_vocoder",
            RenderJobTask::FrameSequenceAudioVideoRoute { .. } => {
                "frame_sequence_audio_video_route"
            }
            RenderJobTask::FrameSequenceDatamosh { .. } => "frame_sequence_datamosh",
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
