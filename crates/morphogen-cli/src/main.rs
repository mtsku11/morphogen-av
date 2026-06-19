use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand, ValueEnum};
use image::{ImageBuffer, ImageReader, Rgba};
use morphogen_audio::{
    load_wav_f32, onset_strength_from_stft, rms_envelope, save_wav_f32, stft_magnitude_cache,
    AudioBufferF32, AudioDescriptorFrame, StftConfig, WindowFunction,
};
use morphogen_core::{
    ExportFormat, Project, RenderJob, RenderJobOutputMetadata, RenderJobStatus, RenderQuality,
    RenderQueue, RenderSettings, RenderTimingMetadata,
};
use morphogen_media::{extract_audio_wav, extract_video_frames, probe_media, MediaError};
use morphogen_render::{
    flow_displace_cpu, luminance_gradient_flow_cpu, write_flow_cache, FlowField, ImageBufferF32,
    RenderError,
};
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
    QueueRunTest {
        queue_path: PathBuf,
        output_dir: PathBuf,
        #[arg(long)]
        stop_after_frame: bool,
    },
    QueueInspect {
        queue_path: PathBuf,
    },
    InspectProject {
        project_path: PathBuf,
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
        } => extract_audio(&input, &output_wav, sample_rate),
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
        } => render_frame_sequence(FrameSequenceRenderRequest {
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_dir: &output_dir,
            amount,
            flow_cache_dir: flow_cache_dir.as_deref(),
            max_frames,
            rms: RmsAmountConfig {
                wav_path: rms_modulator_wav.as_deref(),
                frame_rate,
                window_size: rms_window_size,
                hop_size: rms_hop_size,
                amount_scale: rms_amount_scale,
            },
        }),
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
        Commands::QueueRunTest {
            queue_path,
            output_dir,
            stop_after_frame,
        } => queue_run_test(&queue_path, &output_dir, stop_after_frame),
        Commands::QueueInspect { queue_path } => queue_inspect(&queue_path),
        Commands::InspectProject { project_path } => inspect_project(&project_path),
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

fn extract_audio(input: &Path, output_wav: &Path, sample_rate: u32) -> Result<(), CliError> {
    if sample_rate == 0 {
        return Err(CliError::Message(
            "sample-rate must be greater than zero".to_string(),
        ));
    }

    write_parent_dirs(output_wav)?;

    match extract_audio_wav(input, output_wav, sample_rate) {
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
    rms: RmsAmountConfig<'a>,
}

struct RmsAmountConfig<'a> {
    wav_path: Option<&'a Path>,
    frame_rate: f64,
    window_size: usize,
    hop_size: usize,
    amount_scale: f32,
}

fn render_frame_sequence(request: FrameSequenceRenderRequest<'_>) -> Result<(), CliError> {
    let FrameSequenceRenderRequest {
        modulator_dir,
        carrier_dir,
        output_dir,
        amount,
        flow_cache_dir,
        max_frames,
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
        let displaced = flow_displace_cpu(&carrier, &flow, frame_amount)?;
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
        "rendered frame sequence with {} frame(s) from {} modulating {} to {}",
        frame_count,
        modulator_dir.display(),
        carrier_dir.display(),
        output_dir.display()
    );
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
        status: RenderJobStatus::Queued,
        output: None,
    });
    queue.save_json(queue_path)?;
    println!("queued render job {job_id} in {}", queue_path.display());
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

fn queue_inspect(queue_path: &Path) -> Result<(), CliError> {
    let queue = RenderQueue::load_json(queue_path)?;
    println!("render queue: {} job(s)", queue.jobs.len());
    for job in queue.jobs {
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
        println!(
            "  - {} status={:?} size={}x{} project={}{}",
            job.id,
            job.status,
            job.settings.width,
            job.settings.height,
            job.project_path.as_deref().unwrap_or("<none>"),
            output_summary
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
