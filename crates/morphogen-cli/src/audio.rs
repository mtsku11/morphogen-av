use std::fs;
use std::path::{Path, PathBuf};

use crate::args::*;
use crate::error::CliError;
use crate::imaging::{collect_image_frames, load_image_f32, write_parent_dirs};
use morphogen_audio::{
    centroid_filter_cross_synth, descriptor_filter_route, descriptor_gain_route,
    descriptor_pan_route, impulse_convolution_blend, load_wav_f32, phase_vocoder_cross_synth,
    rms_gain_cross_synth, save_wav_f32, ConvolutionMethod as AudioConvolutionMethod, FilterType,
    StftConfig, CENTROID_FILTER_CROSS_SYNTH_ALGORITHM, PHASE_VOCODER_CROSS_SYNTH_ALGORITHM,
    RMS_GAIN_CROSS_SYNTH_ALGORITHM,
};
use morphogen_core::{
    video_audio_route_algorithm_id, ConvolutionMethod, CrossSynthFilterType, CrossSynthMode,
    CrossSynthWindow, ExportFormat, IrMode, RenderJob, RenderJobFailure, RenderJobOutputMetadata,
    RenderJobProvenance, RenderJobSourceProvenance, RenderJobStatus, RenderJobTask, RenderQuality,
    RenderQueue, RenderSettings, RenderTimingMetadata, SourceRole, VideoAudioRouteDescriptor,
    VideoAudioRouteFilterType, VideoAudioRouteMode, VideoAudioRouteSampling,
};
use morphogen_render::{
    lucas_kanade_flow_cpu, FlowField, ImageBufferF32, LUCAS_KANADE_WINDOW_RADIUS,
};

/// Mean Rec.709 luma of a frame, in `[0,1]`. An empty image yields `0.0`.
pub(crate) fn frame_mean_luma(image: &ImageBufferF32) -> f32 {
    if image.pixels.is_empty() {
        return 0.0;
    }
    let sum: f64 = image
        .pixels
        .iter()
        .map(|p| (p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722) as f64)
        .sum();
    (sum / image.pixels.len() as f64) as f32
}

/// Read Source A's PNG sequence into `(time_seconds, mean_luma)` samples, where
/// `time = frame_index / fps`. Shared by the direct render and the queue run.
pub(crate) fn build_luma_samples(
    modulator_dir: &Path,
    fps: f64,
    max_frames: Option<usize>,
) -> Result<Vec<(f64, f32)>, CliError> {
    let mut frames = collect_image_frames(modulator_dir)?;
    if let Some(cap) = max_frames {
        frames.truncate(cap);
    }
    if frames.is_empty() {
        return Err(CliError::Message(format!(
            "no image frames found in {}",
            modulator_dir.display()
        )));
    }
    let mut samples = Vec::with_capacity(frames.len());
    for (index, path) in frames.iter().enumerate() {
        let image = load_image_f32(path)?;
        samples.push((index as f64 / fps, frame_mean_luma(&image)));
    }
    Ok(samples)
}

/// Mean optical-flow magnitude (in pixels) over a dense temporal flow field.
/// An empty field yields `0.0`.
fn mean_flow_magnitude(flow: &FlowField) -> f32 {
    if flow.vectors.is_empty() {
        return 0.0;
    }
    let sum: f64 = flow
        .vectors
        .iter()
        .map(|v| ((v[0] * v[0] + v[1] * v[1]) as f64).sqrt())
        .sum();
    (sum / flow.vectors.len() as f64) as f32
}

/// Read Source A's PNG sequence into `(time_seconds, flow_magnitude)` samples,
/// where each frame's value is the mean temporal optical-flow magnitude against
/// the previous frame (Lucas-Kanade). Frame zero has no prior frame ⇒ `0.0`.
pub(crate) fn build_flow_magnitude_samples(
    modulator_dir: &Path,
    fps: f64,
    max_frames: Option<usize>,
) -> Result<Vec<(f64, f32)>, CliError> {
    let mut frames = collect_image_frames(modulator_dir)?;
    if let Some(cap) = max_frames {
        frames.truncate(cap);
    }
    if frames.is_empty() {
        return Err(CliError::Message(format!(
            "no image frames found in {}",
            modulator_dir.display()
        )));
    }
    let mut samples = Vec::with_capacity(frames.len());
    let mut previous: Option<ImageBufferF32> = None;
    for (index, path) in frames.iter().enumerate() {
        let image = load_image_f32(path)?;
        let magnitude = match &previous {
            None => 0.0,
            Some(prev) => {
                let flow = lucas_kanade_flow_cpu(
                    prev,
                    &image,
                    image.width,
                    image.height,
                    LUCAS_KANADE_WINDOW_RADIUS,
                )?;
                mean_flow_magnitude(&flow)
            }
        };
        samples.push((index as f64 / fps, magnitude));
        previous = Some(image);
    }
    Ok(samples)
}

/// Per-frame mean Sobel gradient magnitude (edge density). Measures how much
/// edge content a frame has, independent of its mean brightness. The 3×3
/// Sobel kernel is applied to the Rec.709 luma channel; the magnitude is
/// averaged over all valid interior pixels. Border pixels are skipped to
/// avoid clamp-bias on the Sobel window.
pub(crate) fn frame_mean_edge_density(image: &ImageBufferF32) -> f32 {
    let w = image.width as usize;
    let h = image.height as usize;
    if w < 3 || h < 3 {
        return 0.0;
    }
    let luma: Vec<f32> = image
        .pixels
        .iter()
        .map(|p| p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722)
        .collect();
    let mut sum = 0.0_f64;
    let mut count = 0usize;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let at = |dy: isize, dx: isize| luma[(y as isize + dy) as usize * w + (x as isize + dx) as usize];
            let gx = at(-1, 1) + 2.0 * at(0, 1) + at(1, 1)
                   - at(-1, -1) - 2.0 * at(0, -1) - at(1, -1);
            let gy = at(1, -1) + 2.0 * at(1, 0) + at(1, 1)
                   - at(-1, -1) - 2.0 * at(-1, 0) - at(-1, 1);
            sum += ((gx * gx + gy * gy) as f64).sqrt();
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        (sum / count as f64) as f32
    }
}

/// Read Source A's PNG sequence into `(time_seconds, edge_density)` samples.
/// Edge density is the mean Sobel gradient magnitude (peak-normalized by the
/// caller in modulate.rs, same convention as the flow source).
pub(crate) fn build_edge_density_samples(
    modulator_dir: &Path,
    fps: f64,
    max_frames: Option<usize>,
) -> Result<Vec<(f64, f32)>, CliError> {
    let mut frames = collect_image_frames(modulator_dir)?;
    if let Some(cap) = max_frames {
        frames.truncate(cap);
    }
    if frames.is_empty() {
        return Err(CliError::Message(format!(
            "no image frames found in {}",
            modulator_dir.display()
        )));
    }
    let mut samples = Vec::with_capacity(frames.len());
    for (index, path) in frames.iter().enumerate() {
        let image = load_image_f32(path)?;
        samples.push((index as f64 / fps, frame_mean_edge_density(&image)));
    }
    Ok(samples)
}

/// Build the modulator descriptor samples Source A drives the route with.
pub(crate) fn build_descriptor_samples(
    modulator_dir: &Path,
    descriptor: VideoAudioRouteDescriptor,
    fps: f64,
    max_frames: Option<usize>,
) -> Result<Vec<(f64, f32)>, CliError> {
    match descriptor {
        VideoAudioRouteDescriptor::Luma => build_luma_samples(modulator_dir, fps, max_frames),
        VideoAudioRouteDescriptor::Flow => {
            build_flow_magnitude_samples(modulator_dir, fps, max_frames)
        }
    }
}

/// Apply the selected video-to-audio route, returning `(output, algorithm_id)`.
#[allow(clippy::too_many_arguments)]
fn apply_video_audio_route(
    carrier: &morphogen_audio::AudioBufferF32,
    samples: &[(f64, f32)],
    descriptor: VideoAudioRouteDescriptor,
    mode: VideoAudioRouteMode,
    filter_type: VideoAudioRouteFilterType,
    sampling: VideoAudioRouteSampling,
    amount: f32,
) -> Result<(morphogen_audio::AudioBufferF32, &'static str), CliError> {
    let sampling = video_audio_route_sampling(sampling);
    let output = match mode {
        VideoAudioRouteMode::Gain => descriptor_gain_route(carrier, samples, sampling, amount)?,
        VideoAudioRouteMode::Pan => descriptor_pan_route(carrier, samples, sampling, amount)?,
        VideoAudioRouteMode::Filter => descriptor_filter_route(
            carrier,
            samples,
            video_audio_route_filter_type(filter_type),
            sampling,
            amount,
        )?,
    };
    Ok((output, video_audio_route_algorithm_id(descriptor, mode)))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_video_audio_route(
    modulator_dir: &Path,
    carrier_wav: &Path,
    output_wav: &Path,
    descriptor: CliVideoAudioRouteDescriptor,
    mode: CliVideoAudioRouteMode,
    filter_type: CliFilterType,
    sampling: CliVideoAudioRouteSampling,
    amount: f32,
    fps: f64,
    max_frames: Option<usize>,
) -> Result<(), CliError> {
    let carrier = load_wav_f32(carrier_wav)?;
    let descriptor: VideoAudioRouteDescriptor = descriptor.into();
    let samples = build_descriptor_samples(modulator_dir, descriptor, fps, max_frames)?;
    let (output, algorithm) = apply_video_audio_route(
        &carrier,
        &samples,
        descriptor,
        mode.into(),
        filter_type.into(),
        sampling.into(),
        amount,
    )?;

    write_parent_dirs(output_wav)?;
    save_wav_f32(output_wav, &output)?;
    println!(
        "rendered video-to-audio route ({algorithm}) from carrier {} to {}",
        carrier_wav.display(),
        output_wav.display()
    );
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_spectral_cross_synth(
    modulator_wav: &Path,
    carrier_wav: &Path,
    output_wav: &Path,
    mode: CliSpectralCrossSynthMode,
    amount: f32,
    filter_type: FilterType,
    rms_window: usize,
    rms_hop: usize,
    stft_config: StftConfig,
    vocode_bands: usize,
) -> Result<(), CliError> {
    let modulator = load_wav_f32(modulator_wav)?;
    let carrier = load_wav_f32(carrier_wav)?;
    let (output, algorithm) = match mode {
        CliSpectralCrossSynthMode::Gain => (
            rms_gain_cross_synth(&modulator, &carrier, rms_window, rms_hop, amount)?,
            RMS_GAIN_CROSS_SYNTH_ALGORITHM,
        ),
        CliSpectralCrossSynthMode::Filter => (
            centroid_filter_cross_synth(&modulator, &carrier, stft_config, filter_type, amount)?,
            CENTROID_FILTER_CROSS_SYNTH_ALGORITHM,
        ),
        CliSpectralCrossSynthMode::Vocode => (
            phase_vocoder_cross_synth(&modulator, &carrier, stft_config, vocode_bands, amount)?,
            PHASE_VOCODER_CROSS_SYNTH_ALGORITHM,
        ),
    };

    write_parent_dirs(output_wav)?;
    save_wav_f32(output_wav, &output)?;
    println!(
        "rendered spectral cross-synth ({algorithm}) from carrier {} to {}",
        carrier_wav.display(),
        output_wav.display()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_audio_impulse_convolution(
    modulator_wav: &Path,
    carrier_wav: &Path,
    output_wav: &Path,
    amount: f32,
    max_impulse_samples: Option<usize>,
    method: AudioConvolutionMethod,
    resample_impulse: bool,
    ir_mode: IrMode,
) -> Result<(), CliError> {
    let modulator = load_wav_f32(modulator_wav)?;
    let carrier = load_wav_f32(carrier_wav)?;
    let output = impulse_convolution_blend(
        &modulator,
        &carrier,
        amount,
        max_impulse_samples,
        method,
        resample_impulse,
        audio_ir_mode(ir_mode),
    )?;

    write_parent_dirs(output_wav)?;
    save_wav_f32(output_wav, &output)?;
    println!(
        "rendered audio impulse convolution ({}) from carrier {} to {}",
        impulse_convolution_algorithm(ir_mode),
        carrier_wav.display(),
        output_wav.display()
    );
    Ok(())
}
pub(crate) struct QueueAddSpectralCrossSynthRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_wav: &'a Path,
    pub(crate) carrier_wav: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) mode: CrossSynthMode,
    pub(crate) amount: f32,
    pub(crate) filter_type: CrossSynthFilterType,
    pub(crate) rms_window: usize,
    pub(crate) rms_hop: usize,
    pub(crate) fft_size: usize,
    pub(crate) stft_hop: usize,
    pub(crate) window: CrossSynthWindow,
    pub(crate) vocode_bands: usize,
    pub(crate) project_path: Option<&'a Path>,
}

pub(crate) fn queue_add_spectral_cross_synth(
    request: QueueAddSpectralCrossSynthRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddSpectralCrossSynthRequest {
        queue_path,
        modulator_wav,
        carrier_wav,
        output_root_dir,
        mode,
        amount,
        filter_type,
        rms_window,
        rms_hop,
        fft_size,
        stft_hop,
        window,
        vocode_bands,
        project_path,
    } = request;
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(CliError::Message(
            "amount must be finite and within [0, 1]".to_string(),
        ));
    }
    if rms_window == 0 || rms_hop == 0 {
        return Err(CliError::Message(
            "rms-window and rms-hop must be greater than zero".to_string(),
        ));
    }
    StftConfig {
        fft_size,
        hop_size: stft_hop,
        window: cross_synth_window(window),
    }
    .validate()?;
    if mode == CrossSynthMode::Vocode {
        // Mirror the render path's vocode checks (`phase_vocoder_cross_synth`
        // / `validate_complex_stft_config`) so rejection happens at add time.
        if stft_hop > fft_size / 2 {
            return Err(CliError::Message(format!(
                "stft-hop ({stft_hop}) must be <= fft-size / 2 ({}) for vocode mode",
                fft_size / 2
            )));
        }
        if vocode_bands == 0 || vocode_bands > fft_size / 2 {
            return Err(CliError::Message(format!(
                "vocode-bands must be between 1 and fft-size / 2 ({}), got {vocode_bands}",
                fft_size / 2
            )));
        }
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
                source_id: "source-b-audio".to_string(),
                role: SourceRole::Carrier,
                path: carrier_wav.to_string_lossy().to_string(),
            },
        ],
        analysis_caches: Vec::new(),
    };

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 0,
            height: 0,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::Wav { bit_depth: 32 },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::AudioSpectralCrossSynth {
            modulator_wav: modulator_wav.to_string_lossy().to_string(),
            carrier_wav: carrier_wav.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            mode,
            amount,
            filter_type,
            rms_window: rms_window as u32,
            rms_hop: rms_hop as u32,
            fft_size: fft_size as u32,
            stft_hop: stft_hop as u32,
            window,
            vocode_bands: vocode_bands as u32,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued spectral cross-synth render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_spectral_cross_synth(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::AudioSpectralCrossSynth { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running spectral cross-synth jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::AudioSpectralCrossSynth {
        modulator_wav,
        carrier_wav,
        output_directory,
        mode,
        amount,
        filter_type,
        rms_window,
        rms_hop,
        fft_size,
        stft_hop,
        window,
        vocode_bands,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a spectral cross-synth render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let modulator = load_wav_f32(&modulator_wav)?;
        let carrier = load_wav_f32(&carrier_wav)?;
        let stft_config = StftConfig {
            fft_size: fft_size as usize,
            hop_size: stft_hop as usize,
            window: cross_synth_window(window),
        };
        let (output, algorithm) = match mode {
            CrossSynthMode::Gain => (
                rms_gain_cross_synth(
                    &modulator,
                    &carrier,
                    rms_window as usize,
                    rms_hop as usize,
                    amount,
                )?,
                RMS_GAIN_CROSS_SYNTH_ALGORITHM,
            ),
            CrossSynthMode::Filter => (
                centroid_filter_cross_synth(
                    &modulator,
                    &carrier,
                    stft_config,
                    cross_synth_filter_type(filter_type),
                    amount,
                )?,
                CENTROID_FILTER_CROSS_SYNTH_ALGORITHM,
            ),
            CrossSynthMode::Vocode => (
                phase_vocoder_cross_synth(
                    &modulator,
                    &carrier,
                    stft_config,
                    vocode_bands as usize,
                    amount,
                )?,
                PHASE_VOCODER_CROSS_SYNTH_ALGORITHM,
            ),
        };

        let stem_rel = "audio/cross_synth.wav";
        let stem_path = output_dir.join(stem_rel);
        write_parent_dirs(&stem_path)?;
        save_wav_f32(&stem_path, &output)?;

        let sample_rate = output.sample_rate;
        let audio_sample_count = output.frames as u64;
        let duration_seconds = if sample_rate > 0 {
            output.frames as f64 / sample_rate as f64
        } else {
            0.0
        };
        let timing = RenderTimingMetadata {
            frame_rate: 0.0,
            frame_count: 0,
            start_seconds: 0.0,
            duration_seconds,
            sample_rate,
            audio_sample_count,
        };
        let mode_label = match mode {
            CrossSynthMode::Gain => "gain",
            CrossSynthMode::Filter => "filter",
            CrossSynthMode::Vocode => "vocode",
        };
        let filter_label = match filter_type {
            CrossSynthFilterType::Lowpass => "lowpass",
            CrossSynthFilterType::Highpass => "highpass",
        };
        let window_label = match window {
            CrossSynthWindow::Hann => "hann",
            CrossSynthWindow::Hamming => "hamming",
            CrossSynthWindow::Rectangular => "rectangular",
        };
        let mut manifest = serde_json::json!({
            "job_id": job_id,
            "status": "complete",
            "task": "audio_spectral_cross_synth",
            "frames": [],
            "audio_stems": [stem_rel],
            "timing": {
                "frame_rate": timing.frame_rate,
                "frame_count": timing.frame_count,
                "start_seconds": timing.start_seconds,
                "duration_seconds": timing.duration_seconds,
                "sample_rate": timing.sample_rate,
                "audio_sample_count": timing.audio_sample_count
            },
            "spectral_cross_synth": {
                "algorithm": algorithm,
                "mode": mode_label,
                "amount": amount,
                "filter_type": filter_label,
                "rms_window": rms_window,
                "rms_hop": rms_hop,
                "fft_size": fft_size,
                "stft_hop": stft_hop,
                "window": window_label
            },
            "provenance": queue.jobs[job_index].provenance,
            "deterministic": true
        });
        // Vocode-only knob: keyed in only for vocode jobs so gain/filter
        // manifests keep their pre-slice shape byte for byte.
        if mode == CrossSynthMode::Vocode {
            manifest["spectral_cross_synth"]["vocode_bands"] = serde_json::json!(vocode_bands);
        }
        fs::write(
            output_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths: Vec::new(),
            audio_stem_paths: vec![stem_rel.to_string()],
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
                "rendered queued spectral cross-synth job {} to {}",
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
            eprintln!("spectral cross-synth job {job_id} failed: {error}");
            Err(error)
        }
    }
}
pub(crate) struct QueueAddAudioImpulseConvolutionRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_wav: &'a Path,
    pub(crate) carrier_wav: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) amount: f32,
    pub(crate) max_impulse_samples: Option<u32>,
    pub(crate) method: ConvolutionMethod,
    pub(crate) resample_impulse: bool,
    pub(crate) ir_mode: IrMode,
    pub(crate) project_path: Option<&'a Path>,
}

pub(crate) fn queue_add_audio_impulse_convolution(
    request: QueueAddAudioImpulseConvolutionRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddAudioImpulseConvolutionRequest {
        queue_path,
        modulator_wav,
        carrier_wav,
        output_root_dir,
        amount,
        max_impulse_samples,
        method,
        resample_impulse,
        ir_mode,
        project_path,
    } = request;
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(CliError::Message(
            "amount must be finite and within [0, 1]".to_string(),
        ));
    }
    if let Some(0) = max_impulse_samples {
        return Err(CliError::Message(
            "max-impulse-samples must be greater than zero".to_string(),
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
                source_id: "source-b-audio".to_string(),
                role: SourceRole::Carrier,
                path: carrier_wav.to_string_lossy().to_string(),
            },
        ],
        analysis_caches: Vec::new(),
    };

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 0,
            height: 0,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::Wav { bit_depth: 32 },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::AudioImpulseConvolution {
            modulator_wav: modulator_wav.to_string_lossy().to_string(),
            carrier_wav: carrier_wav.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            amount,
            max_impulse_samples,
            method,
            resample_impulse,
            ir_mode,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued audio impulse convolution render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_audio_impulse_convolution(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::AudioImpulseConvolution { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running audio impulse convolution jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::AudioImpulseConvolution {
        modulator_wav,
        carrier_wav,
        output_directory,
        amount,
        max_impulse_samples,
        method,
        resample_impulse,
        ir_mode,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not an audio impulse convolution render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let modulator = load_wav_f32(&modulator_wav)?;
        let carrier = load_wav_f32(&carrier_wav)?;
        let output = impulse_convolution_blend(
            &modulator,
            &carrier,
            amount,
            max_impulse_samples.map(|n| n as usize),
            audio_convolution_method(method),
            resample_impulse,
            audio_ir_mode(ir_mode),
        )?;

        let stem_rel = "audio/impulse_convolution.wav";
        let stem_path = output_dir.join(stem_rel);
        write_parent_dirs(&stem_path)?;
        save_wav_f32(&stem_path, &output)?;

        let sample_rate = output.sample_rate;
        let audio_sample_count = output.frames as u64;
        let duration_seconds = if sample_rate > 0 {
            output.frames as f64 / sample_rate as f64
        } else {
            0.0
        };
        let timing = RenderTimingMetadata {
            frame_rate: 0.0,
            frame_count: 0,
            start_seconds: 0.0,
            duration_seconds,
            sample_rate,
            audio_sample_count,
        };
        let manifest = serde_json::json!({
            "job_id": job_id,
            "status": "complete",
            "task": "audio_impulse_convolution",
            "frames": [],
            "audio_stems": [stem_rel],
            "timing": {
                "frame_rate": timing.frame_rate,
                "frame_count": timing.frame_count,
                "start_seconds": timing.start_seconds,
                "duration_seconds": timing.duration_seconds,
                "sample_rate": timing.sample_rate,
                "audio_sample_count": timing.audio_sample_count
            },
            "impulse_convolution": {
                "algorithm": impulse_convolution_algorithm(ir_mode),
                "amount": amount,
                "max_impulse_samples": max_impulse_samples,
                "method": convolution_method_label(method),
                "resample_impulse": resample_impulse,
                "ir_mode": ir_mode_label(ir_mode)
            },
            "provenance": queue.jobs[job_index].provenance,
            "deterministic": true
        });
        fs::write(
            output_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths: Vec::new(),
            audio_stem_paths: vec![stem_rel.to_string()],
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
                "rendered queued audio impulse convolution job {} to {}",
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
            eprintln!("audio impulse convolution job {job_id} failed: {error}");
            Err(error)
        }
    }
}

pub(crate) struct QueueAddVideoAudioRouteRequest<'a> {
    pub(crate) queue_path: &'a Path,
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_wav: &'a Path,
    pub(crate) output_root_dir: &'a Path,
    pub(crate) descriptor: VideoAudioRouteDescriptor,
    pub(crate) mode: VideoAudioRouteMode,
    pub(crate) filter_type: VideoAudioRouteFilterType,
    pub(crate) sampling: VideoAudioRouteSampling,
    pub(crate) amount: f32,
    pub(crate) fps: f64,
    pub(crate) project_path: Option<&'a Path>,
}

pub(crate) fn queue_add_video_audio_route(
    request: QueueAddVideoAudioRouteRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddVideoAudioRouteRequest {
        queue_path,
        modulator_dir,
        carrier_wav,
        output_root_dir,
        descriptor,
        mode,
        filter_type,
        sampling,
        amount,
        fps,
        project_path,
    } = request;
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(CliError::Message(
            "amount must be finite and within [0, 1]".to_string(),
        ));
    }
    if !fps.is_finite() || fps <= 0.0 {
        return Err(CliError::Message(
            "fps must be finite and greater than zero".to_string(),
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
                source_id: "source-a-video".to_string(),
                role: SourceRole::Modulator,
                path: modulator_dir.to_string_lossy().to_string(),
            },
            RenderJobSourceProvenance {
                source_id: "source-b-audio".to_string(),
                role: SourceRole::Carrier,
                path: carrier_wav.to_string_lossy().to_string(),
            },
        ],
        analysis_caches: Vec::new(),
    };

    queue.enqueue(RenderJob {
        id: job_id.clone(),
        project_path: project_path.map(|path| path.to_string_lossy().to_string()),
        settings: RenderSettings {
            width: 0,
            height: 0,
            quality: RenderQuality::HighQualityOffline,
            export_format: ExportFormat::Wav { bit_depth: 32 },
            temporal_supersampling: 1,
            deterministic: true,
        },
        task: RenderJobTask::VideoAudioRoute {
            modulator_directory: modulator_dir.to_string_lossy().to_string(),
            carrier_wav: carrier_wav.to_string_lossy().to_string(),
            output_directory: job_output_dir.to_string_lossy().to_string(),
            descriptor,
            mode,
            filter_type,
            sampling,
            amount,
            fps,
        },
        provenance: Some(provenance),
        status: RenderJobStatus::Queued,
        output: None,
        failure: None,
    });
    queue.save_json(queue_path)?;
    println!(
        "queued video-to-audio route render job {job_id} in {}",
        queue_path.display()
    );
    Ok(())
}

pub(crate) fn queue_run_video_audio_route(queue_path: &Path) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    let job_index = queue
        .jobs
        .iter()
        .position(|job| {
            matches!(
                (&job.status, &job.task),
                (
                    RenderJobStatus::Queued | RenderJobStatus::Running,
                    RenderJobTask::VideoAudioRoute { .. }
                )
            )
        })
        .ok_or_else(|| {
            CliError::Message(
                "render queue has no queued or running video-to-audio route jobs".to_string(),
            )
        })?;

    let job_id = queue.jobs[job_index].id.clone();
    let RenderJobTask::VideoAudioRoute {
        modulator_directory,
        carrier_wav,
        output_directory,
        descriptor,
        mode,
        filter_type,
        sampling,
        amount,
        fps,
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a video-to-audio route render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);
    queue.jobs[job_index].status = RenderJobStatus::Running;
    queue.save_json(queue_path)?;

    let outcome = (|| -> Result<RenderJobOutputMetadata, CliError> {
        let carrier = load_wav_f32(Path::new(&carrier_wav))?;
        let samples =
            build_descriptor_samples(Path::new(&modulator_directory), descriptor, fps, None)?;
        let (output, algorithm) = apply_video_audio_route(
            &carrier,
            &samples,
            descriptor,
            mode,
            filter_type,
            sampling,
            amount,
        )?;

        let stem_rel = "audio/video_audio_route.wav";
        let stem_path = output_dir.join(stem_rel);
        write_parent_dirs(&stem_path)?;
        save_wav_f32(&stem_path, &output)?;

        let sample_rate = output.sample_rate;
        let audio_sample_count = output.frames as u64;
        let duration_seconds = if sample_rate > 0 {
            output.frames as f64 / sample_rate as f64
        } else {
            0.0
        };
        let timing = RenderTimingMetadata {
            frame_rate: 0.0,
            frame_count: 0,
            start_seconds: 0.0,
            duration_seconds,
            sample_rate,
            audio_sample_count,
        };
        let mode_label = match mode {
            VideoAudioRouteMode::Gain => "gain",
            VideoAudioRouteMode::Pan => "pan",
            VideoAudioRouteMode::Filter => "filter",
        };
        let descriptor_label = match descriptor {
            VideoAudioRouteDescriptor::Luma => "luma",
            VideoAudioRouteDescriptor::Flow => "flow",
        };
        let filter_label = match filter_type {
            VideoAudioRouteFilterType::Lowpass => "lowpass",
            VideoAudioRouteFilterType::Highpass => "highpass",
        };
        let sampling_label = match sampling {
            VideoAudioRouteSampling::Hold => "hold",
            VideoAudioRouteSampling::Smooth => "smooth",
        };
        let manifest = serde_json::json!({
            "job_id": job_id,
            "status": "complete",
            "task": "video_audio_route",
            "frames": [],
            "audio_stems": [stem_rel],
            "timing": {
                "frame_rate": timing.frame_rate,
                "frame_count": timing.frame_count,
                "start_seconds": timing.start_seconds,
                "duration_seconds": timing.duration_seconds,
                "sample_rate": timing.sample_rate,
                "audio_sample_count": timing.audio_sample_count
            },
            "video_audio_route": {
                "algorithm": algorithm,
                "descriptor": descriptor_label,
                "mode": mode_label,
                "filter_type": filter_label,
                "sampling": sampling_label,
                "amount": amount,
                "fps": fps
            },
            "provenance": queue.jobs[job_index].provenance,
            "deterministic": true
        });
        fs::write(
            output_dir.join("manifest.json"),
            serde_json::to_string_pretty(&manifest)?,
        )?;
        Ok(RenderJobOutputMetadata {
            output_directory: output_dir.to_string_lossy().to_string(),
            frame_paths: Vec::new(),
            audio_stem_paths: vec![stem_rel.to_string()],
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
                "rendered queued video-to-audio route job {} to {}",
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
            eprintln!("video-to-audio route job {job_id} failed: {error}");
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use morphogen_render::ImageBufferF32;

    fn make_checkerboard(width: u32, height: u32, tile: u32) -> ImageBufferF32 {
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let v = if (x / tile + y / tile) % 2 == 0 { 1.0 } else { 0.0 };
                pixels.push([v, v, v, 1.0]);
            }
        }
        ImageBufferF32 { width, height, pixels }
    }

    fn make_uniform(width: u32, height: u32, level: f32) -> ImageBufferF32 {
        let pixels = vec![[level, level, level, 1.0]; (width * height) as usize];
        ImageBufferF32 { width, height, pixels }
    }

    #[test]
    fn edge_density_uniform_frame_is_zero() {
        // A flat grey frame has no edges.
        let img = make_uniform(16, 16, 0.5);
        assert_eq!(frame_mean_edge_density(&img), 0.0);
    }

    #[test]
    fn edge_density_increases_with_frequency() {
        // Fine checker has more edges per pixel than coarse checker,
        // even though both have identical mean luma (~0.5).
        let coarse = make_checkerboard(32, 32, 8);
        let fine = make_checkerboard(32, 32, 2);
        let coarse_luma = frame_mean_luma(&coarse);
        let fine_luma = frame_mean_luma(&fine);
        // Luma must be equal (within 5%) — proves this isn't a luma signal.
        assert!(
            (coarse_luma - fine_luma).abs() < 0.05,
            "mean luma should be near-equal: coarse={coarse_luma} fine={fine_luma}"
        );
        let coarse_edge = frame_mean_edge_density(&coarse);
        let fine_edge = frame_mean_edge_density(&fine);
        assert!(
            fine_edge > coarse_edge,
            "fine checker must have higher edge density: fine={fine_edge} coarse={coarse_edge}"
        );
    }

    #[test]
    fn edge_density_too_small_returns_zero() {
        // Images narrower than 3×3 can't support a 3×3 Sobel kernel.
        let tiny = make_checkerboard(2, 2, 1);
        assert_eq!(frame_mean_edge_density(&tiny), 0.0);
    }
}
