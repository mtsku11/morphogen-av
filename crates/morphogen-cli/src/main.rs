use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use clap::{Parser, Subcommand, ValueEnum};
use image::{ImageBuffer, ImageReader, Rgba};
use morphogen_audio::{
    load_wav_f32, onset_strength_from_stft, rms_envelope, save_wav_f32, stft_magnitude_cache,
    AudioAnalysisCache, AudioBufferF32, AudioDescriptorFrame, StftConfig, WindowFunction,
};
use morphogen_core::{
    AnalysisCacheEntry, AnalysisKind, ExportFormat, MediaProxy, Project, RenderBackend, RenderJob,
    RenderJobAnalysisCacheProvenance, RenderJobFailure, RenderJobOutputMetadata,
    RenderJobProvenance, RenderJobSourceProvenance, RenderJobStatus, RenderJobTask, RenderQuality,
    RenderQueue, RenderSettings, RenderTimingMetadata, SourceRole,
};
use morphogen_media::{
    extract_audio_wav_with_max_duration, extract_video_frames, probe_media, MediaError,
};
use morphogen_render::{
    feedback_state_path, flow_displace_cpu, flow_feedback_frame_cpu, luminance_gradient_flow_cpu,
    read_flow_feedback_state, write_flow_cache, write_flow_feedback_state, FlowFeedbackSettings,
    FlowFeedbackStateDescriptor, FlowField, ImageBufferF32, RenderError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Parser)]
#[command(name = "morphogen")]
#[command(about = "Morphogen AV engine validation CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    InitExample {
        output_path: PathBuf,
    },
    Probe {
        media_path: PathBuf,
    },
    ExtractFrames {
        input: PathBuf,
        output_dir: PathBuf,
        #[arg(long, default_value_t = 12.0)]
        fps: f64,
        #[arg(long)]
        max_frames: Option<u32>,
    },
    ExtractAudio {
        input: PathBuf,
        output_wav: PathBuf,
        #[arg(long, default_value_t = 48_000)]
        sample_rate: u32,
        #[arg(long)]
        max_duration_seconds: Option<f64>,
    },
    ExportAudioStem {
        input_wav: PathBuf,
        output_wav: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        gain: f32,
    },
    CacheStft {
        input_wav: PathBuf,
        output_json: PathBuf,
        #[arg(long, default_value_t = 1024)]
        fft_size: usize,
        #[arg(long, default_value_t = 256)]
        hop_size: usize,
        #[arg(long, value_enum, default_value_t = CliWindowFunction::Hann)]
        window: CliWindowFunction,
    },
    CacheOnsets {
        input_wav: PathBuf,
        output_json: PathBuf,
        #[arg(long, default_value_t = 1024)]
        fft_size: usize,
        #[arg(long, default_value_t = 256)]
        hop_size: usize,
        #[arg(long, value_enum, default_value_t = CliWindowFunction::Hann)]
        window: CliWindowFunction,
    },
    CacheRms {
        input_wav: PathBuf,
        output_json: PathBuf,
        #[arg(long, default_value_t = 2048)]
        window_size: usize,
        #[arg(long, default_value_t = 512)]
        hop_size: usize,
    },
    RenderTest {
        output_path: PathBuf,
    },
    MetalRenderTest {
        output_path: PathBuf,
    },
    RenderTwoSource {
        modulator_image: PathBuf,
        carrier_image: PathBuf,
        output_path: PathBuf,
        #[arg(long, default_value_t = 16.0)]
        amount: f32,
        #[arg(long)]
        flow_cache_dir: Option<PathBuf>,
    },
    RenderFrameSequence {
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_dir: PathBuf,
        #[arg(long, default_value_t = 16.0)]
        amount: f32,
        #[arg(long)]
        flow_cache_dir: Option<PathBuf>,
        #[arg(long)]
        max_frames: Option<usize>,
        #[arg(long)]
        rms_modulator_wav: Option<PathBuf>,
        #[arg(long, default_value_t = 12.0)]
        frame_rate: f64,
        #[arg(long, default_value_t = 2048)]
        rms_window_size: usize,
        #[arg(long, default_value_t = 512)]
        rms_hop_size: usize,
        #[arg(long, default_value_t = 16.0)]
        rms_amount_scale: f32,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    RenderFeedbackSequence {
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_dir: PathBuf,
        #[arg(long, default_value_t = 12.0)]
        carrier_amount: f32,
        #[arg(long, default_value_t = 24.0)]
        feedback_amount: f32,
        #[arg(long, default_value_t = 0.72)]
        feedback_mix: f32,
        #[arg(long, default_value_t = 0.995)]
        decay: f32,
        #[arg(long, default_value_t = 1)]
        iterations: u32,
        #[arg(long)]
        flow_cache_dir: Option<PathBuf>,
        #[arg(long)]
        max_frames: Option<usize>,
        #[arg(long)]
        reset_at_frame: Option<usize>,
        #[arg(long, default_value_t = 12.0)]
        frame_rate: f64,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        stop_after_frame: bool,
    },
    CacheSyntheticFlow {
        output_dir: PathBuf,
        #[arg(long, default_value_t = 64)]
        width: u32,
        #[arg(long, default_value_t = 64)]
        height: u32,
    },
    CacheLuminanceFlow {
        modulator_image: PathBuf,
        output_dir: PathBuf,
        #[arg(long)]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
    },
    QueueInit {
        queue_path: PathBuf,
    },
    QueueAddTest {
        queue_path: PathBuf,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddFrameSequence {
        queue_path: PathBuf,
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 16.0)]
        amount: f32,
        #[arg(long)]
        max_frames: Option<u32>,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long)]
        no_flow_cache: bool,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddFeedbackSequence {
        queue_path: PathBuf,
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 12.0)]
        carrier_amount: f32,
        #[arg(long, default_value_t = 24.0)]
        feedback_amount: f32,
        #[arg(long, default_value_t = 0.72)]
        feedback_mix: f32,
        #[arg(long, default_value_t = 0.995)]
        decay: f32,
        #[arg(long, default_value_t = 1)]
        iterations: u32,
        #[arg(long)]
        max_frames: Option<u32>,
        #[arg(long)]
        reset_at_frame: Option<u32>,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long)]
        no_flow_cache: bool,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueRunTest {
        queue_path: PathBuf,
        output_dir: PathBuf,
        #[arg(long)]
        stop_after_frame: bool,
    },
    QueueRunFrameSequence {
        queue_path: PathBuf,
    },
    QueueRunFeedbackSequence {
        queue_path: PathBuf,
    },
    QueueCancel {
        queue_path: PathBuf,
        job_id: String,
    },
    QueueInspect {
        queue_path: PathBuf,
    },
    InspectProject {
        project_path: PathBuf,
    },
    ProjectRegisterProxy {
        project_path: PathBuf,
        #[arg(
            long,
            conflicts_with = "source_role",
            required_unless_present = "source_role"
        )]
        source_id: Option<String>,
        #[arg(
            long,
            value_enum,
            conflicts_with = "source_id",
            required_unless_present = "source_id"
        )]
        source_role: Option<CliSourceRole>,
        #[arg(long)]
        frame_dir: PathBuf,
        #[arg(long)]
        audio: Option<PathBuf>,
        /// Analysis-cache reference to record, as `kind=path` (repeatable). Kind is the
        /// snake_case analysis name, e.g. `audio_rms=cache/source-a/rms.json`.
        #[arg(long = "analysis-cache")]
        analysis_cache: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliWindowFunction {
    Hann,
    Hamming,
    Rectangular,
}

impl From<CliWindowFunction> for WindowFunction {
    fn from(value: CliWindowFunction) -> Self {
        match value {
            CliWindowFunction::Hann => Self::Hann,
            CliWindowFunction::Hamming => Self::Hamming,
            CliWindowFunction::Rectangular => Self::Rectangular,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum CliRenderBackend {
    #[default]
    Cpu,
    Metal,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliSourceRole {
    Modulator,
    Carrier,
}

impl From<CliSourceRole> for SourceRole {
    fn from(value: CliSourceRole) -> Self {
        match value {
            CliSourceRole::Modulator => Self::Modulator,
            CliSourceRole::Carrier => Self::Carrier,
        }
    }
}

impl From<CliRenderBackend> for RenderBackend {
    fn from(value: CliRenderBackend) -> Self {
        match value {
            CliRenderBackend::Cpu => Self::Cpu,
            CliRenderBackend::Metal => Self::Metal,
        }
    }
}

#[derive(Debug, Error)]
enum CliError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Core(#[from] morphogen_core::CoreError),
    #[error(transparent)]
    Media(#[from] morphogen_media::MediaError),
    #[error(transparent)]
    Audio(#[from] morphogen_audio::AudioError),
    #[error(transparent)]
    Render(#[from] morphogen_render::RenderError),
    #[cfg(target_os = "macos")]
    #[error(transparent)]
    Metal(#[from] morphogen_metal::MetalDispatchError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), CliError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::InitExample { output_path } => init_example(&output_path),
        Commands::Probe { media_path } => probe(&media_path),
        Commands::ExtractFrames {
            input,
            output_dir,
            fps,
            max_frames,
        } => extract_frames(&input, &output_dir, fps, max_frames),
        Commands::ExtractAudio {
            input,
            output_wav,
            sample_rate,
            max_duration_seconds,
        } => extract_audio(&input, &output_wav, sample_rate, max_duration_seconds),
        Commands::ExportAudioStem {
            input_wav,
            output_wav,
            gain,
        } => export_audio_stem(&input_wav, &output_wav, gain),
        Commands::CacheStft {
            input_wav,
            output_json,
            fft_size,
            hop_size,
            window,
        } => cache_stft(&input_wav, &output_json, fft_size, hop_size, window.into()),
        Commands::CacheOnsets {
            input_wav,
            output_json,
            fft_size,
            hop_size,
            window,
        } => cache_onsets(&input_wav, &output_json, fft_size, hop_size, window.into()),
        Commands::CacheRms {
            input_wav,
            output_json,
            window_size,
            hop_size,
        } => cache_rms(&input_wav, &output_json, window_size, hop_size),
        Commands::RenderTest { output_path } => render_test(&output_path),
        Commands::MetalRenderTest { output_path } => metal_render_test(&output_path),
        Commands::RenderTwoSource {
            modulator_image,
            carrier_image,
            output_path,
            amount,
            flow_cache_dir,
        } => render_two_source(
            &modulator_image,
            &carrier_image,
            &output_path,
            amount,
            flow_cache_dir.as_deref(),
        ),
        Commands::RenderFrameSequence {
            modulator_dir,
            carrier_dir,
            output_dir,
            amount,
            flow_cache_dir,
            max_frames,
            rms_modulator_wav,
            frame_rate,
            rms_window_size,
            rms_hop_size,
            rms_amount_scale,
            backend,
        } => render_frame_sequence(FrameSequenceRenderRequest {
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_dir: &output_dir,
            amount,
            flow_cache_dir: flow_cache_dir.as_deref(),
            max_frames,
            backend: backend.into(),
            rms: RmsAmountConfig {
                wav_path: rms_modulator_wav.as_deref(),
                frame_rate,
                window_size: rms_window_size,
                hop_size: rms_hop_size,
                amount_scale: rms_amount_scale,
            },
        })
        .map(|_| ()),
        Commands::RenderFeedbackSequence {
            modulator_dir,
            carrier_dir,
            output_dir,
            carrier_amount,
            feedback_amount,
            feedback_mix,
            decay,
            iterations,
            flow_cache_dir,
            max_frames,
            reset_at_frame,
            frame_rate,
            backend,
            stop_after_frame,
        } => render_feedback_sequence(FeedbackSequenceRenderRequest {
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_dir: &output_dir,
            flow_cache_dir: flow_cache_dir.as_deref(),
            max_frames,
            reset_at_frame,
            frame_rate,
            settings: FlowFeedbackSettings {
                carrier_amount,
                feedback_amount,
                feedback_mix,
                decay,
                iterations,
            },
            backend: backend.into(),
            job_id: "direct-feedback-sequence",
            provenance: None,
            stop_after_frame,
        })
        .map(|_| ()),
        Commands::CacheSyntheticFlow {
            output_dir,
            width,
            height,
        } => cache_synthetic_flow(&output_dir, width, height),
        Commands::CacheLuminanceFlow {
            modulator_image,
            output_dir,
            width,
            height,
        } => cache_luminance_flow(&modulator_image, &output_dir, width, height),
        Commands::QueueInit { queue_path } => queue_init(&queue_path),
        Commands::QueueAddTest {
            queue_path,
            project_path,
        } => queue_add_test(&queue_path, project_path.as_deref()),
        Commands::QueueAddFrameSequence {
            queue_path,
            modulator_dir,
            carrier_dir,
            output_root_dir,
            amount,
            max_frames,
            frame_rate,
            no_flow_cache,
            backend,
            project_path,
        } => queue_add_frame_sequence(QueueAddFrameSequenceRequest {
            queue_path: &queue_path,
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_root_dir: &output_root_dir,
            amount,
            max_frames,
            frame_rate,
            write_flow_cache: !no_flow_cache,
            backend: backend.into(),
            project_path: project_path.as_deref(),
        }),
        Commands::QueueAddFeedbackSequence {
            queue_path,
            modulator_dir,
            carrier_dir,
            output_root_dir,
            carrier_amount,
            feedback_amount,
            feedback_mix,
            decay,
            iterations,
            max_frames,
            reset_at_frame,
            frame_rate,
            no_flow_cache,
            backend,
            project_path,
        } => queue_add_feedback_sequence(QueueAddFeedbackSequenceRequest {
            queue_path: &queue_path,
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_root_dir: &output_root_dir,
            settings: FlowFeedbackSettings {
                carrier_amount,
                feedback_amount,
                feedback_mix,
                decay,
                iterations,
            },
            max_frames,
            reset_at_frame,
            frame_rate,
            write_flow_cache: !no_flow_cache,
            backend: backend.into(),
            project_path: project_path.as_deref(),
        }),
        Commands::QueueRunTest {
            queue_path,
            output_dir,
            stop_after_frame,
        } => queue_run_test(&queue_path, &output_dir, stop_after_frame),
        Commands::QueueRunFrameSequence { queue_path } => queue_run_frame_sequence(&queue_path),
        Commands::QueueRunFeedbackSequence { queue_path } => {
            queue_run_feedback_sequence(&queue_path)
        }
        Commands::QueueCancel { queue_path, job_id } => queue_cancel(&queue_path, &job_id),
        Commands::QueueInspect { queue_path } => queue_inspect(&queue_path),
        Commands::InspectProject { project_path } => inspect_project(&project_path),
        Commands::ProjectRegisterProxy {
            project_path,
            source_id,
            source_role,
            frame_dir,
            audio,
            analysis_cache,
        } => project_register_proxy(ProjectRegisterProxyRequest {
            project_path: &project_path,
            source_id: source_id.as_deref(),
            source_role: source_role.map(Into::into),
            frame_dir: &frame_dir,
            audio: audio.as_deref(),
            analysis_cache: &analysis_cache,
        }),
    }
}

fn init_example(output_path: &Path) -> Result<(), CliError> {
    let project = Project::example_two_source_flow_displace();
    project.validate()?;
    write_parent_dirs(output_path)?;
    let json = serde_json::to_string_pretty(&project)?;
    fs::write(output_path, json)?;
    println!("wrote example project to {}", output_path.display());
    Ok(())
}

fn probe(media_path: &Path) -> Result<(), CliError> {
    match probe_media(media_path) {
        Ok(probe) => {
            println!("media: {}", probe.path);
            if let Some(format_name) = probe.format_name {
                println!("format: {format_name}");
            }
            if let Some(duration) = probe.duration_seconds {
                println!("duration_seconds: {duration:.3}");
            }
            for stream in probe.streams {
                println!(
                    "stream {}: type={:?} codec={:?} size={:?}x{:?} sample_rate={:?} channels={:?}",
                    stream.index,
                    stream.codec_type,
                    stream.codec_name,
                    stream.width,
                    stream.height,
                    stream.sample_rate,
                    stream.channels
                );
            }
            Ok(())
        }
        Err(MediaError::MissingBinary { binary }) => {
            Err(missing_media_binary(binary, "media probing"))
        }
        Err(error) => Err(error.into()),
    }
}

fn extract_frames(
    input: &Path,
    output_dir: &Path,
    fps: f64,
    max_frames: Option<u32>,
) -> Result<(), CliError> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(CliError::Message(
            "fps must be a positive finite number".to_string(),
        ));
    }

    match extract_video_frames(input, output_dir, fps, max_frames) {
        Ok(()) => {
            println!(
                "extracted video frames from {} to {}",
                input.display(),
                output_dir.display()
            );
            Ok(())
        }
        Err(MediaError::MissingBinary { binary }) => {
            Err(missing_media_binary(binary, "video frame extraction"))
        }
        Err(error) => Err(error.into()),
    }
}

fn extract_audio(
    input: &Path,
    output_wav: &Path,
    sample_rate: u32,
    max_duration_seconds: Option<f64>,
) -> Result<(), CliError> {
    if sample_rate == 0 {
        return Err(CliError::Message(
            "sample-rate must be greater than zero".to_string(),
        ));
    }
    let max_duration = max_duration_seconds
        .map(|duration| {
            if !duration.is_finite() || duration <= 0.0 {
                return Err(CliError::Message(
                    "max-duration-seconds must be a positive finite number".to_string(),
                ));
            }
            Ok(Duration::from_secs_f64(duration))
        })
        .transpose()?;

    write_parent_dirs(output_wav)?;

    match extract_audio_wav_with_max_duration(input, output_wav, sample_rate, max_duration) {
        Ok(()) => {
            println!(
                "extracted audio from {} to {}",
                input.display(),
                output_wav.display()
            );
            Ok(())
        }
        Err(MediaError::MissingBinary { binary }) => {
            Err(missing_media_binary(binary, "audio WAV extraction"))
        }
        Err(error) => Err(error.into()),
    }
}

fn export_audio_stem(input_wav: &Path, output_wav: &Path, gain: f32) -> Result<(), CliError> {
    if !gain.is_finite() {
        return Err(CliError::Message("gain must be finite".to_string()));
    }

    let input = load_wav_f32(input_wav)?;
    let output = apply_gain(&input, gain)?;
    write_parent_dirs(output_wav)?;
    save_wav_f32(output_wav, &output)?;
    println!(
        "exported WAV stem from {} to {}",
        input_wav.display(),
        output_wav.display()
    );
    Ok(())
}

fn cache_stft(
    input_wav: &Path,
    output_json: &Path,
    fft_size: usize,
    hop_size: usize,
    window: WindowFunction,
) -> Result<(), CliError> {
    let buffer = load_wav_f32(input_wav)?;
    let cache = stft_magnitude_cache(
        &buffer,
        StftConfig {
            fft_size,
            hop_size,
            window,
        },
    )?;

    write_parent_dirs(output_json)?;
    fs::write(output_json, serde_json::to_string_pretty(&cache)?)?;
    println!(
        "wrote STFT cache with {} frame(s) and {} bin(s) to {}",
        cache.frames.len(),
        cache.bin_count,
        output_json.display()
    );
    Ok(())
}

fn cache_onsets(
    input_wav: &Path,
    output_json: &Path,
    fft_size: usize,
    hop_size: usize,
    window: WindowFunction,
) -> Result<(), CliError> {
    let buffer = load_wav_f32(input_wav)?;
    let stft = stft_magnitude_cache(
        &buffer,
        StftConfig {
            fft_size,
            hop_size,
            window,
        },
    )?;
    let onsets = onset_strength_from_stft(&stft)?;

    write_parent_dirs(output_json)?;
    fs::write(output_json, serde_json::to_string_pretty(&onsets)?)?;
    println!(
        "wrote onset-strength cache with {} frame(s) to {}",
        onsets.frames.len(),
        output_json.display()
    );
    Ok(())
}

fn cache_rms(
    input_wav: &Path,
    output_json: &Path,
    window_size: usize,
    hop_size: usize,
) -> Result<(), CliError> {
    let buffer = load_wav_f32(input_wav)?;
    let frames = rms_envelope(&buffer, window_size, hop_size)?;
    let cache =
        AudioAnalysisCache::rms_envelope_cache(buffer.sample_rate, window_size, hop_size, frames);

    write_parent_dirs(output_json)?;
    fs::write(output_json, serde_json::to_string_pretty(&cache)?)?;
    println!(
        "wrote RMS envelope cache with {} frame(s) to {}",
        cache.frames.len(),
        output_json.display()
    );
    Ok(())
}

fn apply_gain(buffer: &AudioBufferF32, gain: f32) -> Result<AudioBufferF32, CliError> {
    let mut samples = Vec::with_capacity(buffer.samples.len());
    for sample in &buffer.samples {
        let scaled = *sample * gain;
        if !scaled.is_finite() {
            return Err(CliError::Message(
                "gain produced a non-finite sample".to_string(),
            ));
        }
        samples.push(scaled);
    }

    AudioBufferF32::new(buffer.channels, buffer.sample_rate, samples).map_err(CliError::from)
}

fn missing_media_binary(binary: String, operation: &str) -> CliError {
    CliError::Message(format!(
        "{binary} is not installed or not on PATH. Install FFmpeg tools to use {operation}, or use render-test without external media."
    ))
}

fn inspect_project(project_path: &Path) -> Result<(), CliError> {
    let json = fs::read_to_string(project_path)?;
    let project: Project = serde_json::from_str(&json)?;
    project.validate()?;

    println!("{}", project.summary());
    println!(
        "timeline: {} fps, {} Hz",
        project.timeline.frame_rate, project.timeline.sample_rate
    );
    println!("sources:");
    for source in &project.sources {
        println!("  - {} ({:?}) {}", source.label, source.role, source.uri);
    }
    println!("routes:");
    for route in &project.graph.routes {
        println!(
            "  - {}.{} -> {}.{} amount={}",
            route.from_node, route.from_output, route.to_node, route.to_parameter, route.amount
        );
    }

    Ok(())
}

struct ProjectRegisterProxyRequest<'a> {
    project_path: &'a Path,
    source_id: Option<&'a str>,
    source_role: Option<SourceRole>,
    frame_dir: &'a Path,
    audio: Option<&'a Path>,
    analysis_cache: &'a [String],
}

fn project_register_proxy(request: ProjectRegisterProxyRequest<'_>) -> Result<(), CliError> {
    let json = fs::read_to_string(request.project_path)?;
    let mut project: Project = serde_json::from_str(&json)?;
    let source_id = resolve_project_source_id(&project, request.source_id, request.source_role)?;

    let proxy = MediaProxy {
        frame_directory: request.frame_dir.to_string_lossy().to_string(),
        audio_path: request.audio.map(|path| path.to_string_lossy().to_string()),
    };

    let caches = request
        .analysis_cache
        .iter()
        .map(|spec| parse_analysis_cache_spec(spec, &source_id))
        .collect::<Result<Vec<_>, _>>()?;
    let cache_count = caches.len();

    project.register_source_proxy(&source_id, proxy, caches)?;
    project.validate()?;

    fs::write(
        request.project_path,
        serde_json::to_string_pretty(&project)?,
    )?;
    println!(
        "registered proxy for source '{}' with {} analysis-cache reference(s) in {}",
        source_id,
        cache_count,
        request.project_path.display()
    );
    Ok(())
}

fn resolve_project_source_id(
    project: &Project,
    source_id: Option<&str>,
    source_role: Option<SourceRole>,
) -> Result<String, CliError> {
    match (source_id, source_role) {
        (Some(source_id), None) => Ok(source_id.to_string()),
        (None, Some(source_role)) => {
            let mut matching_sources = project
                .sources
                .iter()
                .filter(|source| source.role == source_role);
            let source = matching_sources.next().ok_or_else(|| {
                CliError::Message(format!(
                    "project has no {:?} source to register a proxy for",
                    source_role
                ))
            })?;
            if matching_sources.next().is_some() {
                return Err(CliError::Message(format!(
                    "project has multiple {:?} sources; use --source-id",
                    source_role
                )));
            }
            Ok(source.id.clone())
        }
        _ => Err(CliError::Message(
            "provide exactly one of --source-id or --source-role".to_string(),
        )),
    }
}

fn parse_analysis_cache_spec(spec: &str, source_id: &str) -> Result<AnalysisCacheEntry, CliError> {
    let (kind_name, path) = spec.split_once('=').ok_or_else(|| {
        CliError::Message(format!(
            "analysis-cache '{spec}' must be in the form kind=path"
        ))
    })?;
    if path.trim().is_empty() {
        return Err(CliError::Message(format!(
            "analysis-cache '{spec}' has an empty path"
        )));
    }
    let kind = parse_analysis_kind(kind_name)?;

    Ok(AnalysisCacheEntry {
        id: format!("cache-{}-{}", kind_name.trim(), source_id),
        source_id: source_id.to_string(),
        kind,
        path: path.to_string(),
        frame_count: None,
        sample_count: None,
    })
}

fn parse_analysis_kind(name: &str) -> Result<AnalysisKind, CliError> {
    match name.trim() {
        "luminance" => Ok(AnalysisKind::Luminance),
        "edge_map" => Ok(AnalysisKind::EdgeMap),
        "optical_flow" => Ok(AnalysisKind::OpticalFlow),
        "depth_map" => Ok(AnalysisKind::DepthMap),
        "audio_rms" => Ok(AnalysisKind::AudioRms),
        "spectral_centroid" => Ok(AnalysisKind::SpectralCentroid),
        "onset_strength" => Ok(AnalysisKind::OnsetStrength),
        "stft" => Ok(AnalysisKind::Stft),
        "grain_descriptors" => Ok(AnalysisKind::GrainDescriptors),
        other => Err(CliError::Message(format!(
            "unknown analysis kind '{other}'"
        ))),
    }
}

fn render_test(output_path: &Path) -> Result<(), CliError> {
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

fn metal_render_test(output_path: &Path) -> Result<(), CliError> {
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

fn render_two_source(
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

struct FrameSequenceRenderRequest<'a> {
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_dir: &'a Path,
    amount: f32,
    flow_cache_dir: Option<&'a Path>,
    max_frames: Option<usize>,
    backend: RenderBackend,
    rms: RmsAmountConfig<'a>,
}

/// Maximum per-channel difference tolerated between the Metal render output and the
/// CPU reference before a frame-sequence render is rejected. Float32 image values are
/// in roughly [0, 1]; one output LSB keeps the exported 8-bit PNGs visually equivalent,
/// although samples exactly on a quantization boundary may differ by one encoded value.
const METAL_CPU_PARITY_EPSILON: f32 = 1.0 / 255.0;

struct RmsAmountConfig<'a> {
    wav_path: Option<&'a Path>,
    frame_rate: f64,
    window_size: usize,
    hop_size: usize,
    amount_scale: f32,
}

fn render_frame_sequence(
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

fn render_backend_label(backend: RenderBackend) -> &'static str {
    match backend {
        RenderBackend::Cpu => "CPU",
        RenderBackend::Metal => "Metal",
    }
}

/// Render one displacement frame on the requested backend. The Metal path is gated by a
/// per-frame parity check against the CPU reference so a divergent GPU result fails the
/// render rather than silently writing wrong pixels.
fn render_displacement_frame(
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
fn render_displacement_frame_metal(
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
fn render_displacement_frame_metal(
    _carrier: &ImageBufferF32,
    _flow: &FlowField,
    _amount: f32,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal render backend is only available on macOS".to_string(),
    ))
}

struct FrameSequenceRenderResult {
    frame_count: usize,
}

const FLOW_FEEDBACK_RENDER_CONTRACT_VERSION: u32 = 1;
const LUMINANCE_FLOW_ALGORITHM: &str = "luminance_gradient_cpu_v1";

struct FeedbackSequenceRenderRequest<'a> {
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_dir: &'a Path,
    flow_cache_dir: Option<&'a Path>,
    max_frames: Option<usize>,
    reset_at_frame: Option<usize>,
    frame_rate: f64,
    settings: FlowFeedbackSettings,
    backend: RenderBackend,
    job_id: &'a str,
    provenance: Option<&'a RenderJobProvenance>,
    stop_after_frame: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct FeedbackSequenceSourceFingerprint {
    directory: String,
    frame_count: u32,
    checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct FeedbackSequenceContract {
    version: u32,
    flow_algorithm: String,
    modulator: FeedbackSequenceSourceFingerprint,
    carrier: FeedbackSequenceSourceFingerprint,
    settings: FlowFeedbackSettings,
    backend: RenderBackend,
    reset_at_frame: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FeedbackSequenceCheckpoint {
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

fn render_feedback_sequence(
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
        backend,
        job_id,
        provenance,
        stop_after_frame,
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
        flow_algorithm: LUMINANCE_FLOW_ALGORITHM.to_string(),
        modulator: feedback_source_fingerprint(modulator_dir, &modulator_frames)?,
        carrier: feedback_source_fingerprint(carrier_dir, &carrier_frames)?,
        settings,
        backend,
        reset_at_frame,
    };
    let provenance = provenance.cloned().unwrap_or_else(|| {
        feedback_sequence_provenance(modulator_dir, carrier_dir, flow_cache_dir)
    });

    let frame_dir = output_dir.join("frames");
    fs::create_dir_all(&frame_dir)?;
    if let Some(cache_root) = flow_cache_dir {
        fs::create_dir_all(cache_root)?;
    }

    let (start_frame, mut previous_output, mut latest_state_path) =
        load_feedback_resume_state(output_dir, job_id, &contract, &provenance, frame_count_u32)?;
    for index in start_frame..frame_count {
        let modulator = load_image_f32(&modulator_frames[index])?;
        let carrier = load_image_f32(&carrier_frames[index])?;
        let flow = luminance_gradient_flow_cpu(&modulator, carrier.width, carrier.height)?;
        let history = (Some(index as u32) != reset_at_frame)
            .then_some(previous_output.as_ref())
            .flatten();
        let output = render_feedback_frame(&carrier, history, &flow, settings, backend)?;
        let output_path = frame_dir.join(format!("frame_{index:06}.png"));
        save_png(&output, &output_path)?;
        if let Some(cache_root) = flow_cache_dir {
            let frame_cache_dir = cache_root.join(format!("frame_{index:06}"));
            write_flow_cache(frame_cache_dir, &flow, LUMINANCE_FLOW_ALGORITHM)?;
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
    write_feedback_sequence_manifest(
        job_id,
        output_dir,
        &frame_paths,
        &timing,
        &contract,
        &provenance,
        final_state_path,
    )?;

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

fn render_feedback_frame(
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
fn render_feedback_frame_metal(
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
fn render_feedback_frame_metal(
    _carrier: &ImageBufferF32,
    _previous_output: Option<&ImageBufferF32>,
    _flow: &FlowField,
    _settings: FlowFeedbackSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal render backend is only available on macOS".to_string(),
    ))
}

fn feedback_sequence_provenance(
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
                    producer: LUMINANCE_FLOW_ALGORITHM.to_string(),
                }]
            })
            .unwrap_or_default(),
    }
}

fn feedback_source_fingerprint(
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

fn update_fnv1a(checksum: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *checksum ^= u64::from(*byte);
        *checksum = checksum.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn load_feedback_resume_state(
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

struct FeedbackCheckpointWrite<'a> {
    job_id: &'a str,
    status: &'a str,
    next_frame_index: u32,
    state_path: Option<&'a str>,
    state: Option<FlowFeedbackStateDescriptor>,
    contract: &'a FeedbackSequenceContract,
    provenance: &'a RenderJobProvenance,
}

fn write_feedback_checkpoint(
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

fn write_feedback_sequence_manifest(
    job_id: &str,
    output_dir: &Path,
    frame_paths: &[String],
    timing: &RenderTimingMetadata,
    contract: &FeedbackSequenceContract,
    provenance: &RenderJobProvenance,
    state_path: &str,
) -> Result<(), CliError> {
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
        "provenance": provenance,
        "deterministic": true
    });
    write_feedback_json_atomically(
        &output_dir.join("manifest.json"),
        &serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

fn feedback_state_relative_path(frame_index: u32) -> String {
    format!("state/feedback_frame_{frame_index:06}.rgba32f")
}

fn feedback_state_path_from_checkpoint(
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

fn write_feedback_json_atomically(path: &Path, content: &str) -> Result<(), CliError> {
    let temporary_path = path.with_extension("json.tmp");
    fs::write(&temporary_path, content)?;
    fs::rename(temporary_path, path)?;
    Ok(())
}

struct RmsAmountModulation {
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

fn load_rms_amount_modulation(
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

fn cache_synthetic_flow(output_dir: &Path, width: u32, height: u32) -> Result<(), CliError> {
    let flow = synthetic_flow(width, height)?;
    let manifest = write_flow_cache(output_dir, &flow, "synthetic_swirl_v1")?;
    println!(
        "wrote synthetic flow cache {}x{} with {} frame(s) to {}",
        manifest.width,
        manifest.height,
        manifest.frames.len(),
        output_dir.display()
    );
    Ok(())
}

fn cache_luminance_flow(
    modulator_image: &Path,
    output_dir: &Path,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(), CliError> {
    let modulator = load_image_f32(modulator_image)?;
    let width = width.unwrap_or(modulator.width);
    let height = height.unwrap_or(modulator.height);
    let flow = luminance_gradient_flow_cpu(&modulator, width, height)?;
    let manifest = write_flow_cache(output_dir, &flow, "luminance_gradient_cpu_v1")?;
    println!(
        "wrote luminance flow cache {}x{} with {} frame(s) to {}",
        manifest.width,
        manifest.height,
        manifest.frames.len(),
        output_dir.display()
    );
    Ok(())
}

fn queue_init(queue_path: &Path) -> Result<(), CliError> {
    let queue = RenderQueue::default();
    queue.save_json(queue_path)?;
    println!("wrote empty render queue to {}", queue_path.display());
    Ok(())
}

fn queue_add_test(queue_path: &Path, project_path: Option<&Path>) -> Result<(), CliError> {
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

struct QueueAddFrameSequenceRequest<'a> {
    queue_path: &'a Path,
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_root_dir: &'a Path,
    amount: f32,
    max_frames: Option<u32>,
    frame_rate: f64,
    write_flow_cache: bool,
    backend: RenderBackend,
    project_path: Option<&'a Path>,
}

fn queue_add_frame_sequence(request: QueueAddFrameSequenceRequest<'_>) -> Result<(), CliError> {
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

struct QueueAddFeedbackSequenceRequest<'a> {
    queue_path: &'a Path,
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_root_dir: &'a Path,
    settings: FlowFeedbackSettings,
    max_frames: Option<u32>,
    reset_at_frame: Option<u32>,
    frame_rate: f64,
    write_flow_cache: bool,
    backend: RenderBackend,
    project_path: Option<&'a Path>,
}

fn queue_add_feedback_sequence(
    request: QueueAddFeedbackSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddFeedbackSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
        settings,
        max_frames,
        reset_at_frame,
        frame_rate,
        write_flow_cache,
        backend,
        project_path,
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
    let flow_cache_directory = write_flow_cache
        .then(|| job_output_dir.join("cache").join("flow"))
        .map(|path| path.to_string_lossy().to_string());
    let provenance = feedback_sequence_provenance(
        modulator_dir,
        carrier_dir,
        flow_cache_directory.as_deref().map(Path::new),
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
                bit_depth: 16,
            },
            temporal_supersampling: 1,
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

fn queue_run_test(
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

fn queue_run_frame_sequence(queue_path: &Path) -> Result<(), CliError> {
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

fn queue_run_feedback_sequence(queue_path: &Path) -> Result<(), CliError> {
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
    let provenance = queue.jobs[job_index].provenance.clone().ok_or_else(|| {
        CliError::Message("flow-feedback queue job is missing source/cache provenance".to_string())
    })?;
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
    } = queue.jobs[job_index].task.clone()
    else {
        return Err(CliError::Message(
            "selected queue job is not a flow-feedback render".to_string(),
        ));
    };
    let output_dir = PathBuf::from(output_directory);

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
            },
            backend,
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

fn write_test_render_output_bundle(
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

struct TestRenderOutput {
    complete: bool,
    metadata: RenderJobOutputMetadata,
}

fn write_test_render_checkpoint(
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

fn write_frame_sequence_manifest(
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

fn write_frame_sequence_checkpoint(
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

fn synthetic_stereo_stem(sample_rate: u32, frames: usize) -> Result<AudioBufferF32, CliError> {
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

fn queue_cancel(queue_path: &Path, job_id: &str) -> Result<(), CliError> {
    let mut queue = RenderQueue::load_json(queue_path)?;
    queue.cancel_job(job_id)?;
    queue.save_json(queue_path)?;
    println!("cancelled job {job_id} in {}", queue_path.display());
    Ok(())
}

fn queue_inspect(queue_path: &Path) -> Result<(), CliError> {
    let queue = RenderQueue::load_json(queue_path)?;
    println!("render queue: {} job(s)", queue.jobs.len());
    for job in queue.jobs {
        let task_name = match job.task {
            RenderJobTask::TestRender => "test_render",
            RenderJobTask::FrameSequenceFlowDisplace { .. } => "frame_sequence_flow_displace",
            RenderJobTask::FrameSequenceFlowFeedback { .. } => "frame_sequence_flow_feedback",
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

fn load_image_f32(path: &Path) -> Result<ImageBufferF32, CliError> {
    let decoded = ImageReader::open(path)?.decode()?.to_rgba32f();
    let pixels = decoded.pixels().map(|pixel| pixel.0).collect();
    ImageBufferF32::new(decoded.width(), decoded.height(), pixels).map_err(CliError::from)
}

fn collect_image_frames(directory: &Path) -> Result<Vec<PathBuf>, CliError> {
    let mut frames = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_file() && is_supported_image_frame(&path) {
            frames.push(path);
        }
    }
    frames.sort();
    Ok(frames)
}

fn is_supported_image_frame(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return false;
    };

    ["png"]
        .iter()
        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
}

fn synthetic_carrier(width: u32, height: u32) -> Result<ImageBufferF32, RenderError> {
    ImageBufferF32::from_fn(width, height, |x, y| {
        let fx = normalized_coordinate(x, width);
        let fy = normalized_coordinate(y, height);
        let checker = if ((x / 16) + (y / 16)) % 2 == 0 {
            0.24
        } else {
            0.82
        };
        [fx, fy, checker, 1.0]
    })
}

fn synthetic_flow(width: u32, height: u32) -> Result<FlowField, RenderError> {
    FlowField::from_fn(width, height, |x, y| {
        let nx = normalized_coordinate(x, width) * 2.0 - 1.0;
        let ny = normalized_coordinate(y, height) * 2.0 - 1.0;
        let swirl_x = -ny * 7.5;
        let swirl_y = nx * 7.5;
        let ripple = (nx * std::f32::consts::PI * 4.0).sin() * 2.0;
        [swirl_x + ripple, swirl_y]
    })
}

fn normalized_coordinate(value: u32, extent: u32) -> f32 {
    if extent <= 1 {
        return 0.0;
    }
    value as f32 / (extent - 1) as f32
}

fn save_png(image: &ImageBufferF32, output_path: &Path) -> Result<(), CliError> {
    let mut rgba: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(image.width, image.height);

    for y in 0..image.height {
        for x in 0..image.width {
            let pixel = image
                .pixel(x, y)
                .ok_or_else(|| CliError::Message(format!("missing pixel at {},{}", x, y)))?;
            rgba.put_pixel(
                x,
                y,
                Rgba([
                    float_to_u8(pixel[0]),
                    float_to_u8(pixel[1]),
                    float_to_u8(pixel[2]),
                    float_to_u8(pixel[3]),
                ]),
            );
        }
    }

    rgba.save(output_path)?;
    Ok(())
}

fn float_to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn write_parent_dirs(path: &Path) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
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
}
