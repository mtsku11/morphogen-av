use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use clap::{Parser, Subcommand, ValueEnum};
use image::{ImageBuffer, ImageReader, Rgba};
use morphogen_audio::{
    load_wav_f32, onset_strength_from_stft, rms_envelope, save_wav_f32,
    spectral_centroid_from_magnitudes, stft_magnitude_cache, AudioAnalysisCache, AudioBufferF32,
    AudioDescriptorFrame, OnsetStrengthCache, StftAnalysisCache, StftConfig, WindowFunction,
};
use morphogen_core::{
    AnalysisCacheEntry, AnalysisKind, ExportFormat, FlowSource, GrainSelectionMode,
    GranularAudioModulation, MediaProxy, Project, RenderBackend, RenderJob,
    RenderJobAnalysisCacheProvenance,
    RenderJobFailure, RenderJobOutputMetadata, RenderJobProvenance, RenderJobSourceProvenance,
    RenderJobStatus, RenderJobTask, RenderQuality, RenderQueue, RenderSettings,
    RenderTimingMetadata, SourceRole,
};
use morphogen_media::{
    extract_audio_wav_with_max_duration, extract_video_frames, probe_media, MediaError,
};
use morphogen_render::{
    analyze_grain_colors_cpu, analyze_grain_pool_cpu, analyze_grains_cpu, feedback_state_path,
    flow_displace_cpu, flow_feedback_frame_cpu, flow_temporal_supersample_cpu,
    granular_mosaic_with_pool_selection_cpu, granular_mosaic_with_selection_cpu,
    luminance_gradient_flow_cpu, pyramidal_lucas_kanade_flow_cpu, read_flow_cache,
    read_flow_feedback_state, read_grain_color_descriptor_cache, read_grain_descriptor_cache,
    read_grain_pool_descriptor_cache, read_grain_selection_cache, select_grains_cpu,
    select_grains_from_pool_cpu, select_grains_multimodal_cpu, write_flow_cache,
    write_flow_cache_with_source_fingerprint, write_flow_feedback_state,
    write_grain_color_descriptor_cache, write_grain_descriptor_cache,
    write_grain_pool_descriptor_cache, write_grain_selection_cache, FlowFeedbackSettings,
    FlowFeedbackStateDescriptor, FlowField, GrainColorDescriptor, GrainDescriptor, GrainPool,
    GrainSelection, GranularMosaicSettings, ImageBufferF32, PoolSelectionWindow, RenderError,
    StructureMode,
    FLOW_VECTOR_CONVENTION, GRAIN_COLOR_DESCRIPTOR_CACHE_FILE_NAME, GRAIN_DESCRIPTOR_CACHE_FILE_NAME,
    GRAIN_POOL_DESCRIPTOR_CACHE_FILE_NAME, GRAIN_SELECTION_CACHE_FILE_NAME, GRANULAR_MOSAIC_ALGORITHM,
    LUCAS_KANADE_WINDOW_RADIUS, MULTIMODAL_GRAIN_ALGORITHM, POOLED_GRAIN_ALGORITHM,
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
    RenderGranularMosaic {
        modulator_image: PathBuf,
        carrier_image: PathBuf,
        output_path: PathBuf,
        #[arg(long, default_value_t = 32)]
        grain_size: u32,
        #[arg(long, default_value_t = 1.0)]
        rearrangement: f32,
        #[arg(long, default_value_t = 0.25)]
        variation: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long)]
        grain_cache_dir: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long, value_enum, default_value_t = CliGrainSelection::Luma)]
        selection: CliGrainSelection,
    },
    RenderGranularMosaicSequence {
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_dir: PathBuf,
        #[arg(long, default_value_t = 32)]
        grain_size: u32,
        #[arg(long, default_value_t = 1.0)]
        rearrangement: f32,
        #[arg(long, default_value_t = 0.25)]
        variation: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long)]
        rms_cache: Option<PathBuf>,
        #[arg(long)]
        onset_cache: Option<PathBuf>,
        #[arg(long)]
        stft_cache: Option<PathBuf>,
        #[arg(long, default_value_t = 0.0)]
        rms_variation_scale: f32,
        #[arg(long, default_value_t = 0.0)]
        onset_rearrangement_scale: f32,
        #[arg(long, default_value_t = 0.0)]
        centroid_grain_size_scale: f32,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long)]
        max_frames: Option<usize>,
        #[arg(long)]
        grain_cache_dir: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long, value_enum, default_value_t = CliGrainSelection::Luma)]
        selection: CliGrainSelection,
    },
    /// Render a granular mosaic sequence whose grains are drawn from a whole-clip
    /// temporal pool (step 6b). Per-grain carrier audio matches against Source A's
    /// frame-time audio, making audio a real selection dimension.
    RenderGranularMosaicPoolSequence {
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_dir: PathBuf,
        #[arg(long, default_value_t = 32)]
        grain_size: u32,
        #[arg(long, default_value_t = 1.0)]
        rearrangement: f32,
        #[arg(long, default_value_t = 0.25)]
        variation: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Scales every audio dimension in the selection distance.
        #[arg(long, default_value_t = 1.0)]
        audio_weight: f32,
        /// RMS cache for Source A; supplies the per-output-frame query audio.
        #[arg(long)]
        modulator_rms_cache: Option<PathBuf>,
        /// RMS cache for Source B; supplies each pool grain's carrier audio.
        #[arg(long)]
        carrier_rms_cache: Option<PathBuf>,
        /// STFT cache for Source A; appends a spectral-centroid query dimension.
        #[arg(long)]
        modulator_centroid_cache: Option<PathBuf>,
        /// STFT cache for Source B; appends a spectral-centroid dimension to each pool grain.
        #[arg(long)]
        carrier_centroid_cache: Option<PathBuf>,
        /// Trailing pool window in frames: each output frame may only draw grains
        /// from the last N carrier frames (0 = whole-clip, the default).
        #[arg(long, default_value_t = 0)]
        pool_window: u32,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long)]
        max_frames: Option<usize>,
        #[arg(long)]
        grain_cache_dir: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
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
        #[arg(long, default_value_t = 0.0)]
        structure_mix: f32,
        #[arg(long, value_enum, default_value_t = CliStructureMode::SingleScale)]
        structure_mode: CliStructureMode,
        #[arg(long, default_value_t = 8)]
        output_bit_depth: u8,
        #[arg(long, default_value_t = 1)]
        temporal_supersampling: u32,
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
        #[arg(long, value_enum, default_value_t = CliFlowSource::OpticalFlow)]
        flow_source: CliFlowSource,
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
        #[arg(long, default_value_t = 0.0)]
        structure_mix: f32,
        #[arg(long, default_value_t = 8)]
        output_bit_depth: u8,
        #[arg(long, default_value_t = 1)]
        temporal_supersampling: u32,
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
        #[arg(long, value_enum, default_value_t = CliFlowSource::OpticalFlow)]
        flow_source: CliFlowSource,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddGranularMosaicSequence {
        queue_path: PathBuf,
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 32)]
        grain_size: u32,
        #[arg(long, default_value_t = 1.0)]
        rearrangement: f32,
        #[arg(long, default_value_t = 0.25)]
        variation: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long)]
        rms_cache: Option<PathBuf>,
        #[arg(long)]
        onset_cache: Option<PathBuf>,
        #[arg(long)]
        stft_cache: Option<PathBuf>,
        #[arg(long, default_value_t = 0.0)]
        rms_variation_scale: f32,
        #[arg(long, default_value_t = 0.0)]
        onset_rearrangement_scale: f32,
        #[arg(long, default_value_t = 0.0)]
        centroid_grain_size_scale: f32,
        #[arg(long)]
        max_frames: Option<u32>,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long)]
        no_grain_cache: bool,
        #[arg(long)]
        project_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long, value_enum, default_value_t = CliGrainSelection::Luma)]
        selection: CliGrainSelection,
    },
    /// Persist a step-6b temporal-grain-pool (joint-AV) granular job to the queue.
    QueueAddGranularMosaicPoolSequence {
        queue_path: PathBuf,
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 32)]
        grain_size: u32,
        #[arg(long, default_value_t = 1.0)]
        rearrangement: f32,
        #[arg(long, default_value_t = 0.25)]
        variation: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long, default_value_t = 1.0)]
        audio_weight: f32,
        #[arg(long)]
        modulator_rms_cache: Option<PathBuf>,
        #[arg(long)]
        carrier_rms_cache: Option<PathBuf>,
        #[arg(long)]
        max_frames: Option<u32>,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long)]
        no_grain_cache: bool,
        #[arg(long)]
        project_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
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
    QueueRunGranularMosaicSequence {
        queue_path: PathBuf,
    },
    QueueRunGranularMosaicPoolSequence {
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

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum CliGrainSelection {
    /// 1-D nearest neighbor on mean luminance.
    #[default]
    Luma,
    /// Multimodal nearest neighbor on mean RGB.
    Rgb,
}

impl From<CliGrainSelection> for GrainSelectionMode {
    fn from(value: CliGrainSelection) -> Self {
        match value {
            CliGrainSelection::Luma => Self::Luma,
            CliGrainSelection::Rgb => Self::MultimodalRgb,
        }
    }
}

/// Algorithm identifier stamped on sidecars and provenance for a selection mode.
fn grain_selection_algorithm(mode: GrainSelectionMode) -> &'static str {
    match mode {
        GrainSelectionMode::Luma => GRANULAR_MOSAIC_ALGORITHM,
        GrainSelectionMode::MultimodalRgb => MULTIMODAL_GRAIN_ALGORITHM,
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum CliFlowSource {
    Luminance,
    #[default]
    OpticalFlow,
}

impl From<CliFlowSource> for FlowSource {
    fn from(value: CliFlowSource) -> Self {
        match value {
            CliFlowSource::Luminance => Self::Luminance,
            CliFlowSource::OpticalFlow => Self::OpticalFlow,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum CliStructureMode {
    #[default]
    SingleScale,
    Multiscale,
}

impl From<CliStructureMode> for StructureMode {
    fn from(value: CliStructureMode) -> Self {
        match value {
            CliStructureMode::SingleScale => Self::SingleScale,
            CliStructureMode::Multiscale => Self::Multiscale,
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
        Commands::RenderGranularMosaic {
            modulator_image,
            carrier_image,
            output_path,
            grain_size,
            rearrangement,
            variation,
            seed,
            grain_cache_dir,
            backend,
            selection,
        } => render_granular_mosaic(
            &modulator_image,
            &carrier_image,
            &output_path,
            GranularMosaicSettings {
                grain_size,
                rearrangement,
                variation,
                seed,
            },
            grain_cache_dir.as_deref(),
            backend.into(),
            selection.into(),
        ),
        Commands::RenderGranularMosaicSequence {
            modulator_dir,
            carrier_dir,
            output_dir,
            grain_size,
            rearrangement,
            variation,
            seed,
            rms_cache,
            onset_cache,
            stft_cache,
            rms_variation_scale,
            onset_rearrangement_scale,
            centroid_grain_size_scale,
            frame_rate,
            max_frames,
            grain_cache_dir,
            backend,
            selection,
        } => render_granular_mosaic_sequence(GranularMosaicSequenceRenderRequest {
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_dir: &output_dir,
            settings: GranularMosaicSettings {
                grain_size,
                rearrangement,
                variation,
                seed,
            },
            frame_rate,
            max_frames,
            grain_cache_dir: grain_cache_dir.as_deref(),
            backend: backend.into(),
            audio_modulation: granular_audio_modulation_from_cli(
                rms_cache.as_deref(),
                onset_cache.as_deref(),
                stft_cache.as_deref(),
                rms_variation_scale,
                onset_rearrangement_scale,
                centroid_grain_size_scale,
            ),
            selection_mode: selection.into(),
        })
        .map(|_| ()),
        Commands::RenderGranularMosaicPoolSequence {
            modulator_dir,
            carrier_dir,
            output_dir,
            grain_size,
            rearrangement,
            variation,
            seed,
            audio_weight,
            modulator_rms_cache,
            carrier_rms_cache,
            modulator_centroid_cache,
            carrier_centroid_cache,
            pool_window,
            frame_rate,
            max_frames,
            grain_cache_dir,
            backend,
        } => render_granular_mosaic_pool_sequence(GranularMosaicPoolSequenceRequest {
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_dir: &output_dir,
            settings: GranularMosaicSettings {
                grain_size,
                rearrangement,
                variation,
                seed,
            },
            audio_weight,
            modulator_rms_cache: modulator_rms_cache.as_deref(),
            carrier_rms_cache: carrier_rms_cache.as_deref(),
            modulator_centroid_cache: modulator_centroid_cache.as_deref(),
            carrier_centroid_cache: carrier_centroid_cache.as_deref(),
            pool_window,
            frame_rate,
            max_frames,
            grain_cache_dir: grain_cache_dir.as_deref(),
            backend: backend.into(),
        })
        .map(|_| ()),
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
            structure_mix,
            structure_mode,
            output_bit_depth,
            temporal_supersampling,
            flow_cache_dir,
            max_frames,
            reset_at_frame,
            frame_rate,
            backend,
            flow_source,
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
                structure_mix,
                structure_mode: structure_mode.into(),
            },
            output_bit_depth,
            temporal_supersampling,
            backend: backend.into(),
            flow_source: flow_source.into(),
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
            structure_mix,
            output_bit_depth,
            temporal_supersampling,
            max_frames,
            reset_at_frame,
            frame_rate,
            no_flow_cache,
            backend,
            flow_source,
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
                structure_mix,
                // Multiscale structure mode is exposed only on the direct CPU
                // render path for now; the persisted queue keeps single-scale
                // (backlog: Structure-Preserving Morph task 5 follow-up).
                structure_mode: StructureMode::SingleScale,
            },
            output_bit_depth,
            temporal_supersampling,
            max_frames,
            reset_at_frame,
            frame_rate,
            write_flow_cache: !no_flow_cache,
            backend: backend.into(),
            flow_source: flow_source.into(),
            project_path: project_path.as_deref(),
        }),
        Commands::QueueAddGranularMosaicSequence {
            queue_path,
            modulator_dir,
            carrier_dir,
            output_root_dir,
            grain_size,
            rearrangement,
            variation,
            seed,
            rms_cache,
            onset_cache,
            stft_cache,
            rms_variation_scale,
            onset_rearrangement_scale,
            centroid_grain_size_scale,
            max_frames,
            frame_rate,
            no_grain_cache,
            project_path,
            backend,
            selection,
        } => queue_add_granular_mosaic_sequence(QueueAddGranularMosaicSequenceRequest {
            queue_path: &queue_path,
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_root_dir: &output_root_dir,
            settings: GranularMosaicSettings {
                grain_size,
                rearrangement,
                variation,
                seed,
            },
            audio_modulation: granular_audio_modulation_from_cli(
                rms_cache.as_deref(),
                onset_cache.as_deref(),
                stft_cache.as_deref(),
                rms_variation_scale,
                onset_rearrangement_scale,
                centroid_grain_size_scale,
            ),
            max_frames,
            frame_rate,
            write_grain_cache: !no_grain_cache,
            project_path: project_path.as_deref(),
            backend: backend.into(),
            selection_mode: selection.into(),
        }),
        Commands::QueueAddGranularMosaicPoolSequence {
            queue_path,
            modulator_dir,
            carrier_dir,
            output_root_dir,
            grain_size,
            rearrangement,
            variation,
            seed,
            audio_weight,
            modulator_rms_cache,
            carrier_rms_cache,
            max_frames,
            frame_rate,
            no_grain_cache,
            project_path,
            backend,
        } => queue_add_granular_mosaic_pool_sequence(QueueAddGranularMosaicPoolSequenceRequest {
            queue_path: &queue_path,
            modulator_dir: &modulator_dir,
            carrier_dir: &carrier_dir,
            output_root_dir: &output_root_dir,
            settings: GranularMosaicSettings {
                grain_size,
                rearrangement,
                variation,
                seed,
            },
            audio_weight,
            modulator_rms_cache: modulator_rms_cache.as_deref(),
            carrier_rms_cache: carrier_rms_cache.as_deref(),
            max_frames,
            frame_rate,
            write_grain_cache: !no_grain_cache,
            project_path: project_path.as_deref(),
            backend: backend.into(),
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
        Commands::QueueRunGranularMosaicSequence { queue_path } => {
            queue_run_granular_mosaic_sequence(&queue_path)
        }
        Commands::QueueRunGranularMosaicPoolSequence { queue_path } => {
            queue_run_granular_mosaic_pool_sequence(&queue_path)
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

fn render_granular_mosaic(
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

fn granular_audio_modulation_from_cli(
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
struct TimedScalarControl {
    time_seconds: f64,
    value: f32,
}

struct GranularAudioControls {
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

fn load_granular_audio_controls(
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

fn load_rms_controls(path: &str) -> Result<Vec<TimedScalarControl>, CliError> {
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

fn load_onset_controls(path: &str) -> Result<Vec<TimedScalarControl>, CliError> {
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

fn load_centroid_controls(path: &str) -> Result<Vec<TimedScalarControl>, CliError> {
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

fn timed_scalar_controls(
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

fn scalar_at_frame_time(frames: &[TimedScalarControl], time_seconds: f64) -> f32 {
    let descriptor_count = frames.partition_point(|frame| frame.time_seconds <= time_seconds);
    descriptor_count
        .checked_sub(1)
        .and_then(|index| frames.get(index))
        .map(|frame| frame.value)
        .unwrap_or(0.0)
}

struct GranularMosaicSequenceRenderRequest<'a> {
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_dir: &'a Path,
    settings: GranularMosaicSettings,
    frame_rate: f64,
    max_frames: Option<usize>,
    grain_cache_dir: Option<&'a Path>,
    backend: RenderBackend,
    audio_modulation: Option<GranularAudioModulation>,
    selection_mode: GrainSelectionMode,
}

fn render_granular_mosaic_sequence(
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

struct GranularMosaicPoolSequenceRequest<'a> {
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_dir: &'a Path,
    settings: GranularMosaicSettings,
    audio_weight: f32,
    modulator_rms_cache: Option<&'a Path>,
    carrier_rms_cache: Option<&'a Path>,
    modulator_centroid_cache: Option<&'a Path>,
    carrier_centroid_cache: Option<&'a Path>,
    pool_window: u32,
    frame_rate: f64,
    max_frames: Option<usize>,
    grain_cache_dir: Option<&'a Path>,
    backend: RenderBackend,
}

/// Build a pool/query audio vector in fixed order `[rms?, centroid?]`, sampling
/// each supplied descriptor at `time_seconds`. Absent descriptors contribute no
/// dimension (so k ranges 0..=2); supplying the descriptors symmetrically on the
/// modulator and carrier sides keeps both indexing the same audio dimensions.
fn pool_audio_vector(
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
fn render_granular_mosaic_pool_sequence(
    request: GranularMosaicPoolSequenceRequest<'_>,
) -> Result<FrameSequenceRenderResult, CliError> {
    let GranularMosaicPoolSequenceRequest {
        modulator_dir,
        carrier_dir,
        output_dir,
        settings,
        audio_weight,
        modulator_rms_cache,
        carrier_rms_cache,
        modulator_centroid_cache,
        carrier_centroid_cache,
        pool_window,
        frame_rate,
        max_frames,
        grain_cache_dir,
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
        let selection = select_grains_from_pool_cpu(
            &modulator,
            carrier.width,
            carrier.height,
            &query_audio,
            &pool,
            settings,
            audio_weight,
            window,
        )?;
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
        "rendered granular mosaic pool sequence with {} frame(s) ({}, {} pool frame(s), audio_dims={}, audio_weight={}) from {} modulating {} to {}",
        frame_count,
        POOLED_GRAIN_ALGORITHM,
        pool_frames.len(),
        pool.audio_dims,
        audio_weight,
        modulator_dir.display(),
        carrier_dir.display(),
        output_dir.display()
    );
    Ok(FrameSequenceRenderResult { frame_count })
}

/// Resolve the temporal grain pool, reusing a matching sidecar or assembling it
/// from the carrier frames and writing it back. Returns whether it was reused.
fn resolve_grain_pool(
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
fn pool_set_fingerprint(
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

struct GranularMosaicCacheContext<'a> {
    directory: &'a Path,
    modulator_fingerprint: &'a str,
    carrier_fingerprint: &'a str,
}

struct GranularMosaicFrameRenderResult {
    image: ImageBufferF32,
    reused_descriptor_cache: bool,
    reused_selection_cache: bool,
}

fn render_granular_mosaic_frame(
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
fn resolve_luma_grain_descriptors(
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
fn resolve_color_grain_descriptors(
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
fn resolve_grain_selection_cache(
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

fn render_granular_mosaic_output(
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
fn render_granular_mosaic_output_metal(
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

#[cfg(not(target_os = "macos"))]
fn render_granular_mosaic_output_metal(
    _carrier: &ImageBufferF32,
    _selection: &morphogen_render::GrainSelection,
    _settings: GranularMosaicSettings,
) -> Result<ImageBufferF32, CliError> {
    Err(CliError::Message(
        "the Metal render backend is only available on macOS".to_string(),
    ))
}

fn render_granular_mosaic_pool_output(
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
fn render_granular_mosaic_pool_output_metal(
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
fn render_granular_mosaic_pool_output_metal(
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

fn print_granular_cache_summary(
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

const FLOW_FEEDBACK_RENDER_CONTRACT_VERSION: u32 = 2;
const LUMINANCE_FLOW_ALGORITHM: &str = "luminance_gradient_cpu_v1";
const OPTICAL_FLOW_ALGORITHM: &str = "pyramidal_lucas_kanade_cpu_v1";

/// The recorded analysis-algorithm identifier for a flow source. This string is
/// part of the feedback render contract, so changing the flow source invalidates
/// an existing checkpoint.
fn flow_source_algorithm(flow_source: FlowSource) -> &'static str {
    match flow_source {
        FlowSource::Luminance => LUMINANCE_FLOW_ALGORITHM,
        FlowSource::OpticalFlow => OPTICAL_FLOW_ALGORITHM,
    }
}

fn read_cached_temporal_flow(
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

struct FeedbackSequenceRenderRequest<'a> {
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_dir: &'a Path,
    flow_cache_dir: Option<&'a Path>,
    max_frames: Option<usize>,
    reset_at_frame: Option<usize>,
    frame_rate: f64,
    settings: FlowFeedbackSettings,
    output_bit_depth: u8,
    temporal_supersampling: u32,
    backend: RenderBackend,
    flow_source: FlowSource,
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
    output_bit_depth: u8,
    temporal_supersampling: u32,
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

fn granular_mosaic_provenance(
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

fn granular_mosaic_pool_provenance(
    modulator_dir: &Path,
    carrier_dir: &Path,
    grain_cache_dir: Option<&Path>,
    modulator_rms_cache: Option<&str>,
    carrier_rms_cache: Option<&str>,
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

    RenderJobProvenance {
        sources,
        analysis_caches,
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

fn image_file_fingerprint(path: &Path) -> Result<String, CliError> {
    let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
    update_fnv1a(
        &mut checksum,
        path.file_name().unwrap_or_default().as_encoded_bytes(),
    );
    update_fnv1a(&mut checksum, &[0]);
    update_fnv1a(&mut checksum, &fs::read(path)?);
    Ok(format!("fnv1a64:{checksum:016x}"))
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

struct FeedbackSequenceManifestWrite<'a> {
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

fn write_feedback_sequence_manifest(
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

fn validate_feedback_export_settings(
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

struct QueueAddGranularMosaicSequenceRequest<'a> {
    queue_path: &'a Path,
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_root_dir: &'a Path,
    settings: GranularMosaicSettings,
    audio_modulation: Option<GranularAudioModulation>,
    max_frames: Option<u32>,
    frame_rate: f64,
    write_grain_cache: bool,
    project_path: Option<&'a Path>,
    backend: RenderBackend,
    selection_mode: GrainSelectionMode,
}

fn queue_add_granular_mosaic_sequence(
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

struct QueueAddGranularMosaicPoolSequenceRequest<'a> {
    queue_path: &'a Path,
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_root_dir: &'a Path,
    settings: GranularMosaicSettings,
    audio_weight: f32,
    modulator_rms_cache: Option<&'a Path>,
    carrier_rms_cache: Option<&'a Path>,
    max_frames: Option<u32>,
    frame_rate: f64,
    write_grain_cache: bool,
    project_path: Option<&'a Path>,
    backend: RenderBackend,
}

fn queue_add_granular_mosaic_pool_sequence(
    request: QueueAddGranularMosaicPoolSequenceRequest<'_>,
) -> Result<(), CliError> {
    let QueueAddGranularMosaicPoolSequenceRequest {
        queue_path,
        modulator_dir,
        carrier_dir,
        output_root_dir,
        settings,
        audio_weight,
        modulator_rms_cache,
        carrier_rms_cache,
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
    let provenance = granular_mosaic_pool_provenance(
        modulator_dir,
        carrier_dir,
        grain_cache_directory.as_deref().map(Path::new),
        modulator_rms_cache.as_deref(),
        carrier_rms_cache.as_deref(),
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
            modulator_rms_cache,
            carrier_rms_cache,
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

struct QueueAddFeedbackSequenceRequest<'a> {
    queue_path: &'a Path,
    modulator_dir: &'a Path,
    carrier_dir: &'a Path,
    output_root_dir: &'a Path,
    settings: FlowFeedbackSettings,
    output_bit_depth: u8,
    temporal_supersampling: u32,
    max_frames: Option<u32>,
    reset_at_frame: Option<u32>,
    frame_rate: f64,
    write_flow_cache: bool,
    backend: RenderBackend,
    flow_source: FlowSource,
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

fn queue_run_granular_mosaic_sequence(queue_path: &Path) -> Result<(), CliError> {
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

fn queue_run_granular_mosaic_pool_sequence(queue_path: &Path) -> Result<(), CliError> {
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
        modulator_rms_cache,
        carrier_rms_cache,
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
                modulator_rms_cache: modulator_rms_cache.as_deref().map(Path::new),
                carrier_rms_cache: carrier_rms_cache.as_deref().map(Path::new),
                // Queue jobs persist only RMS caches today; centroid (k=2) and the
                // trailing pool window are direct-render knobs until the queue task
                // carries them.
                modulator_centroid_cache: None,
                carrier_centroid_cache: None,
                pool_window: 0,
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
            modulator_rms_cache: modulator_rms_cache.as_deref(),
            carrier_rms_cache: carrier_rms_cache.as_deref(),
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

fn feedback_output_bit_depth(settings: &RenderSettings) -> Result<u8, CliError> {
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

#[allow(clippy::too_many_arguments)]
fn write_granular_mosaic_sequence_manifest(
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

struct GranularMosaicPoolManifest<'a> {
    job_id: &'a str,
    output_dir: &'a Path,
    frame_paths: &'a [String],
    timing: &'a RenderTimingMetadata,
    settings: &'a GranularMosaicSettings,
    audio_weight: f32,
    modulator_rms_cache: Option<&'a str>,
    carrier_rms_cache: Option<&'a str>,
    backend: RenderBackend,
    provenance: Option<&'a RenderJobProvenance>,
}

fn write_granular_mosaic_pool_sequence_manifest(
    manifest: GranularMosaicPoolManifest<'_>,
) -> Result<(), CliError> {
    let GranularMosaicPoolManifest {
        job_id,
        output_dir,
        frame_paths,
        timing,
        settings,
        audio_weight,
        modulator_rms_cache,
        carrier_rms_cache,
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
            "modulator_rms_cache": modulator_rms_cache,
            "carrier_rms_cache": carrier_rms_cache,
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
            RenderJobTask::FrameSequenceGranularMosaic { .. } => "frame_sequence_granular_mosaic",
            RenderJobTask::FrameSequenceGranularMosaicPool { .. } => {
                "frame_sequence_granular_mosaic_pool"
            }
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

fn save_png_with_bit_depth(
    image: &ImageBufferF32,
    output_path: &Path,
    bit_depth: u8,
) -> Result<(), CliError> {
    match bit_depth {
        8 => save_png(image, output_path),
        16 => save_png_16(image, output_path),
        _ => Err(CliError::Message(
            "PNG bit depth must be either 8 or 16".to_string(),
        )),
    }
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

fn save_png_16(image: &ImageBufferF32, output_path: &Path) -> Result<(), CliError> {
    let mut rgba: ImageBuffer<Rgba<u16>, Vec<u16>> = ImageBuffer::new(image.width, image.height);

    for y in 0..image.height {
        for x in 0..image.width {
            let pixel = image
                .pixel(x, y)
                .ok_or_else(|| CliError::Message(format!("missing pixel at {},{}", x, y)))?;
            rgba.put_pixel(
                x,
                y,
                Rgba([
                    float_to_u16(pixel[0]),
                    float_to_u16(pixel[1]),
                    float_to_u16(pixel[2]),
                    float_to_u16(pixel[3]),
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

fn float_to_u16(value: f32) -> u16 {
    (value.clamp(0.0, 1.0) * u16::MAX as f32).round() as u16
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
