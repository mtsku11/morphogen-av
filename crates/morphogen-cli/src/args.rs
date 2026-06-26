use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use morphogen_audio::{
    ConvolutionMethod as AudioConvolutionMethod, EnvelopeSampling, FilterType,
    IrMode as AudioIrMode, WindowFunction, IMPULSE_CONVOLUTION_BLEND_ALGORITHM,
    PER_CHANNEL_IMPULSE_CONVOLUTION_BLEND_ALGORITHM,
};
use morphogen_core::{
    ConvolutionMethod, CrossSynthFilterType, CrossSynthMode, CrossSynthWindow, DatamoshPreset,
    FlowSource, GrainSelectionMode, IrMode, KernelMode, RenderBackend, SourceRole,
    VideoAudioRouteDescriptor, VideoAudioRouteFilterType, VideoAudioRouteMode,
    VideoAudioRouteSampling, VideoVocoderMode,
};
use morphogen_render::{
    CoagulationFlowSource, StructureMode, VectorRemixMode, CONVOLUTION_BLEND_ALGORITHM,
    CONVOLUTION_BLEND_COLOR_ALGORITHM, GRANULAR_MOSAIC_ALGORITHM, MULTIMODAL_GRAIN_ALGORITHM,
};
#[derive(Debug, Parser)]
#[command(name = "morphogen")]
#[command(about = "Morphogen AV engine validation CLI")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
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
    RenderSpectralCrossSynth {
        modulator_wav: PathBuf,
        carrier_wav: PathBuf,
        output_wav: PathBuf,
        #[arg(long, value_enum, default_value_t = CliCrossSynthMode::Gain)]
        mode: CliCrossSynthMode,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long, value_enum, default_value_t = CliFilterType::Lowpass)]
        filter_type: CliFilterType,
        #[arg(long, default_value_t = 2048)]
        rms_window: usize,
        #[arg(long, default_value_t = 512)]
        rms_hop: usize,
        #[arg(long, default_value_t = 1024)]
        fft_size: usize,
        #[arg(long, default_value_t = 256)]
        stft_hop: usize,
        #[arg(long, value_enum, default_value_t = CliWindowFunction::Hann)]
        window: CliWindowFunction,
    },
    /// Convolve carrier audio (Source B) with Source A's impulse response.
    RenderAudioImpulseConvolution {
        modulator_wav: PathBuf,
        carrier_wav: PathBuf,
        output_wav: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long)]
        max_impulse_samples: Option<usize>,
        #[arg(long, value_enum, default_value_t = CliConvolutionMethod::Direct)]
        method: CliConvolutionMethod,
        #[arg(long)]
        resample_impulse: bool,
        /// IR channel mapping: `mono` (one downmix IR) or `per-channel`
        /// (true-stereo, one IR per Source A channel).
        #[arg(long, value_enum, default_value_t = CliIrMode::Mono)]
        ir_mode: CliIrMode,
    },
    /// Render a video-to-audio descriptor-routing WAV: Source A's per-frame luma
    /// envelope (peak-normalized) drives Source B's audio gain or stereo pan.
    /// `--amount 0` is an exact Source B passthrough.
    RenderVideoAudioRoute {
        /// Source A video frames (PNG sequence); each frame's descriptor is the
        /// modulator.
        modulator_dir: PathBuf,
        /// Source B audio (WAV) to shape.
        carrier_wav: PathBuf,
        output_wav: PathBuf,
        /// Which Source A visual descriptor drives the envelope.
        #[arg(long, value_enum, default_value_t = CliVideoAudioRouteDescriptor::Luma)]
        descriptor: CliVideoAudioRouteDescriptor,
        #[arg(long, value_enum, default_value_t = CliVideoAudioRouteMode::Gain)]
        mode: CliVideoAudioRouteMode,
        /// Filter response for `--mode filter` (ignored otherwise).
        #[arg(long, value_enum, default_value_t = CliFilterType::Lowpass)]
        filter_type: CliFilterType,
        /// How the descriptor envelope is resampled onto B's audio grid.
        #[arg(long, value_enum, default_value_t = CliVideoAudioRouteSampling::Hold)]
        sampling: CliVideoAudioRouteSampling,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        /// Frame rate mapping A's frame index to time for the descriptor lookup.
        #[arg(long, default_value_t = 30.0)]
        fps: f64,
        #[arg(long)]
        max_frames: Option<usize>,
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
    /// Render a still video-vocoder frame: Source A's luma histogram becomes a
    /// per-band gain envelope reweighting Source B's tonal bands (luma-band gain
    /// routing). `--amount 0` is an exact Source B passthrough.
    RenderVideoVocoder {
        modulator_image: PathBuf,
        carrier_image: PathBuf,
        output_path: PathBuf,
        #[arg(long, default_value_t = 8)]
        bands: u32,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long, value_enum, default_value_t = CliVocoderMode::Match)]
        mode: CliVocoderMode,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    /// Render a video-vocoder PNG-frame sequence (per-frame luma-band gain
    /// routing). Source A's per-frame luma envelope reweights Source B's bands.
    RenderVideoVocoderSequence {
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_dir: PathBuf,
        #[arg(long, default_value_t = 8)]
        bands: u32,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long, value_enum, default_value_t = CliVocoderMode::Match)]
        mode: CliVocoderMode,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        max_frames: Option<usize>,
    },
    /// Render an audio-to-video descriptor-routing sequence: Source A's RMS
    /// envelope (peak-normalized) drives the per-frame displacement amount
    /// applied to Source B's frames via the parity-gated flow displace.
    RenderAudioVideoRouteSequence {
        /// Source A audio (WAV); its RMS envelope is the modulator.
        modulator_wav: PathBuf,
        /// Source B video frames (PNG sequence) to displace.
        carrier_dir: PathBuf,
        output_dir: PathBuf,
        /// Global displacement scale; multiplies the normalized RMS gain
        /// (0 = passthrough, the loudest A frame reaches this amount).
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        /// Uniform displacement field x-component in pixels at full amount.
        #[arg(long, default_value_t = 8.0)]
        shift_x: f32,
        /// Uniform displacement field y-component in pixels at full amount.
        #[arg(long, default_value_t = 0.0)]
        shift_y: f32,
        /// RMS analysis window (samples) for Source A.
        #[arg(long, default_value_t = 2048)]
        rms_window: u32,
        /// RMS analysis hop (samples) for Source A.
        #[arg(long, default_value_t = 512)]
        rms_hop: u32,
        /// Output frame rate; maps frame index → time for the envelope lookup.
        #[arg(long, default_value_t = 30.0)]
        fps: f64,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        max_frames: Option<usize>,
    },
    /// Render a controlled-datamosh sequence (flow-reuse "bloom/melt"): Source A's
    /// per-frame optical flow repeatedly advects Source B's previous output, so a
    /// held carrier frame smears under A's motion. `--keyframe-interval 1` snaps
    /// to Source B every frame (byte-identical passthrough); `0` melts from B[0].
    RenderDatamoshSequence {
        /// Source A video frames (PNG sequence); supplies the per-frame motion.
        modulator_dir: PathBuf,
        /// Source B video frames (PNG sequence) to mosh.
        carrier_dir: PathBuf,
        output_dir: PathBuf,
        /// Keyframe ("keep") interval: 1 = passthrough (snap to B every frame),
        /// N = snap every N frames (pulse), 0 = only frame 0 (full melt from B[0]).
        #[arg(long, default_value_t = 0)]
        keyframe_interval: u32,
        /// Per-step scale on A's flow (motion intensity); 0 freezes the held frame.
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        /// Macroblock size for codec-simulated mosh: `1` = smooth per-pixel bloom,
        /// `N >= 2` quantizes A's flow to NxN blocks so whole macroblocks slide.
        #[arg(long, default_value_t = 1)]
        block_size: u32,
        /// Block-residual gain: re-inject the intra-block motion discarded by
        /// quantization (a fine-motion haze). `0` = block path; needs block-size >= 2.
        #[arg(long, default_value_t = 0.0)]
        residual_gain: f32,
        /// Decay on the residual accumulator: `0` = one-frame kick, `->1` = drift.
        #[arg(long, default_value_t = 0.9)]
        residual_decay: f32,
        /// Per-block keep/drop threshold: macroblocks whose mean motion magnitude is
        /// below this snap back to the carrier (intra-block refresh) while busier
        /// blocks rot. `0` = no per-block refresh; needs block-size >= 2.
        #[arg(long, default_value_t = 0.0)]
        block_refresh_threshold: f32,
        /// FFglitch-style motion-vector remix on the block-MV grid (block-size 2 or
        /// more): `sort` reassigns block MVs by descending magnitude (motion pools),
        /// `shuffle` permutes them by `--remix-seed` (motion scrambles). `none` = off.
        #[arg(long, value_enum, default_value_t = CliVectorRemixMode::None)]
        vector_remix: CliVectorRemixMode,
        /// Seed for `--vector-remix shuffle` (deterministic permutation).
        #[arg(long, default_value_t = 0)]
        remix_seed: u64,
        /// Named deterministic destructive preset. `custom` keeps the explicit knobs.
        #[arg(long, value_enum, default_value_t = CliDatamoshPreset::Custom)]
        preset: CliDatamoshPreset,
        /// Reuse/write per-frame temporal optical-flow sidecars.
        #[arg(long)]
        flow_cache_dir: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        max_frames: Option<usize>,
        /// Stop after writing one frame and a resumable float-state checkpoint.
        #[arg(long)]
        stop_after_frame: bool,
    },
    /// EXPERIMENTAL, NON-DETERMINISTIC: real bitstream datamosh. Encodes a video to
    /// AVI/MPEG-4 (one I-frame, then P-frames) via external ffmpeg, performs
    /// controlled compressed-stream surgery, then decodes to a PNG sequence. Output
    /// is NOT bit-reproducible (depends on ffmpeg's codec); this path lives outside
    /// the deterministic render graph by design.
    DatamoshBitstream {
        /// Input video (any ffmpeg-decodable container). For `pframe-duplicate` /
        /// `remove-keyframe` this is the clip to mosh; for `motion-transfer` it is
        /// the MODULATOR (Source A, the motion donor) and `--carrier` is the carrier.
        input: PathBuf,
        /// Output directory for the decoded `frame_%06d.png` sequence.
        output_dir: PathBuf,
        /// Frame rate to encode/decode at.
        #[arg(long, default_value_t = 24.0)]
        fps: f64,
        /// Bitstream operation: duplicate a P-frame for bloom, remove the leading
        /// keyframe so the decoder starts from prediction data, or transfer the
        /// modulator's motion onto the carrier (needs `--carrier`).
        #[arg(long, value_enum, default_value_t = CliDatamoshBitstreamOperation::PframeDuplicate)]
        operation: CliDatamoshBitstreamOperation,
        /// Which P-frame to bloom (0-based among P-frames; 0 = the first P-frame).
        #[arg(long, default_value_t = 0)]
        p_frame_index: u32,
        /// Extra copies of that P-frame to insert; `0` = a plain transcode (off).
        #[arg(long, default_value_t = 0)]
        duplicate_count: u32,
        /// `motion-transfer` only: the CARRIER (Source B) whose appearance is kept.
        /// Its leading I-frame seeds the output; the modulator (`input`) supplies the
        /// motion. Scaled to the carrier's dimensions before splicing.
        #[arg(long)]
        carrier: Option<PathBuf>,
        /// `motion-transfer` only: how many leading carrier frames to keep before the
        /// modulator's motion takes over. `1` = just the I-frame (pure transfer).
        #[arg(long, default_value_t = 1)]
        carrier_keyframes: u32,
    },
    /// Render a convolutional AV blend sequence: each Source A frame supplies a
    /// normalized KxK luma kernel that Source B's matching frame is convolved
    /// with (parity-gated). `--amount 0` is an exact Source B passthrough.
    RenderConvolutionalBlendSequence {
        /// Source A video frames (PNG sequence); each supplies the kernel.
        modulator_dir: PathBuf,
        /// Source B video frames (PNG sequence) to convolve.
        carrier_dir: PathBuf,
        output_dir: PathBuf,
        /// Kernel edge length (odd, >= 1); larger spreads the blend wider.
        #[arg(long, default_value_t = 3)]
        kernel_size: u32,
        /// Wet/dry blend (0 = passthrough, 1 = fully convolved).
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        /// Kernel extraction: `luma` (one luminance kernel for all channels) or
        /// `color` (a separate kernel per R/G/B channel of Source A).
        #[arg(long, value_enum, default_value_t = CliKernelMode::Luma)]
        kernel_mode: CliKernelMode,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        max_frames: Option<usize>,
    },
    /// Render a colour-group dispersion blend (experimental, deterministic). Unlike
    /// the coagulated blend (which composites in place behind a moving mask), this
    /// advects the image *content* per block: colour-grouped tiles first flow along a
    /// directional current, then a growing random walk shatters and intermixes them
    /// (perpetual churn). CPU-only for now.
    RenderDispersionBlendSequence {
        /// Source A video frames (PNG sequence).
        source_a_dir: PathBuf,
        /// Source B video frames (PNG sequence).
        source_b_dir: PathBuf,
        output_dir: PathBuf,
        /// Tile edge length in pixels (fine ⇒ a dense glitch spray).
        #[arg(long, default_value_t = 8)]
        block_size: u32,
        /// Weight on per-tile mean-colour luminance in the A/B ownership preference.
        #[arg(long, default_value_t = 1.0)]
        color_weight: f32,
        /// Weight on per-tile texture energy in the ownership preference.
        #[arg(long, default_value_t = 0.4)]
        texture_weight: f32,
        /// Master ownership coagulation amount.
        #[arg(long, default_value_t = 1.6)]
        coagulation_strength: f32,
        /// Seeded per-tile scatter on the ownership preference.
        #[arg(long, default_value_t = 0.5)]
        randomness: f32,
        /// Spatial-coherence relaxation passes for the ownership field.
        #[arg(long, default_value_t = 2)]
        coherence_passes: u32,
        /// Per-pass neighbour pull for the ownership field, in [0, 1].
        #[arg(long, default_value_t = 0.5)]
        coherence_strength: f32,
        /// Baseline A ownership added to every tile.
        #[arg(long, default_value_t = 0.4)]
        bias: f32,
        /// How much each frame re-seeds the ownership field from fresh descriptors.
        #[arg(long, default_value_t = 0.4)]
        ownership_refresh: f32,
        /// Scales the coherent current (block-mean optical flow) per block.
        #[arg(long, default_value_t = 1.0)]
        coherent_amount: f32,
        /// Max per-frame random scatter step (pixels) at full dispersion.
        #[arg(long, default_value_t = 3.0)]
        scatter_amount: f32,
        /// Per-frame damping of accumulated offset in [0, 1) (keeps churn bounded).
        #[arg(long, default_value_t = 0.9)]
        damping: f32,
        /// Frames over which dispersion ramps from 0 to full (0 = full immediately).
        #[arg(long, default_value_t = 24)]
        dispersion_ramp: u32,
        /// Output feedback smear: fraction of the previous frame held into this one,
        /// leaving directional streaks as tiles flow (0 = none).
        #[arg(long, default_value_t = 0.0)]
        smear: f32,
        /// Per-frame decay of the held smear trail (1 = no fade).
        #[arg(long, default_value_t = 0.85)]
        smear_decay: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long)]
        max_frames: Option<usize>,
    },
    /// Render a faux-fluid dye advection (experimental, deterministic; CPU reference with
    /// an optional parity-gated Metal backend).
    /// A single source video is treated as a continuous "dye": each frame every pixel
    /// is pushed along a procedural divergence-free turbulence field (semi-Lagrangian
    /// advection) and a little of the current source frame is bled back in. The picture
    /// becomes liquid and marbles — no tiles or particles. `--reinject 1` shows the
    /// source verbatim (no fluid); `--advect 0 --reinject 0` holds frame zero.
    RenderFluidAdvectSequence {
        /// Source video frames (PNG sequence) — the dye that flows and is refreshed.
        source_dir: PathBuf,
        output_dir: PathBuf,
        /// Number of output frames to render.
        #[arg(long, default_value_t = 120)]
        frames: usize,
        /// Advection distance per frame (pixels) — how far the dye is pushed each step.
        /// Higher = the dye wraps around the vortices faster (tighter spirals).
        #[arg(long, default_value_t = 12.0)]
        advect: f32,
        /// Vortex scale (lattice cells per pixel). Smaller = larger coherent vortices.
        #[arg(long, default_value_t = 0.008)]
        turbulence_scale: f32,
        /// Drift/evolution rate of the fine detail per frame. The big vortices are steady
        /// (so the dye can spiral into them); this only stirs the fine texture.
        #[arg(long, default_value_t = 0.06)]
        turbulence_speed: f32,
        /// Fine-detail octave weight relative to the steady big vortices (shader uses 0.1).
        /// Higher = finer structure (and eventually wobble); 0 = pure large vortices.
        #[arg(long, default_value_t = 0.1)]
        detail: f32,
        /// Source bled back into the dye each frame, in [0, 1] (the "frame refresh").
        /// Lower = the dye persists longer and spirals more; 0 = pure smear, 1 = source
        /// verbatim. ~0.05 lets the swirls build while the video stays present.
        #[arg(long, default_value_t = 0.05)]
        reinject: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Render backend. `metal` is gated against the CPU reference per frame.
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    /// Render the mutual two-source faux-fluid advection (experimental, deterministic):
    /// Source A's optical-flow motion advects Source B's colour as a continuous dye. Frame
    /// zero is B verbatim; thereafter A's Lucas-Kanade flow between consecutive A frames flows
    /// B's dye and `--reinject` of the current B frame is bled back in. This is the cross-synth
    /// model (A reshapes B). `--reinject 1` = B verbatim (the off case).
    RenderFluidAdvectTwoSourceSequence {
        /// Source A video frames (PNG sequence) — the modulator whose motion drives the flow.
        source_a_dir: PathBuf,
        /// Source B video frames (PNG sequence) — the carrier dye that is advected.
        source_b_dir: PathBuf,
        output_dir: PathBuf,
        /// Number of output frames to render (capped to the shorter of the two clips).
        #[arg(long, default_value_t = 120)]
        frames: usize,
        /// Strength applied to A's flow when advecting B's dye (flow units; A's flow is in
        /// pixels/frame). 1.0 moves the dye exactly with A's motion; higher amplifies; 0 holds.
        #[arg(long, default_value_t = 1.0)]
        advect: f32,
        /// Source B bled back into the dye each frame, in [0, 1] (the "frame refresh"). Lower =
        /// B smears further along A's motion; 0 = pure smear, 1 = B verbatim. ~0.08 marbles.
        #[arg(long, default_value_t = 0.08)]
        reinject: f32,
        /// Render backend. `metal` is gated against the CPU reference per frame.
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    /// Render the single-source optical-flow-driven advection (experimental, deterministic):
    /// the video is advected by its OWN motion. Each frame the source's Lucas-Kanade flow
    /// (between consecutive frames) flows the held dye and `--reinject` of the current frame is
    /// bled back in — so the picture liquefies along where it is actually moving (vs the
    /// procedural vortices of render-fluid-advect-sequence). The self-driven case of the
    /// two-source advection. `--reinject 1` = source verbatim (the off case). A static clip has
    /// no motion ⇒ source verbatim.
    RenderOpticalFlowAdvectSequence {
        /// Source video frames (PNG sequence) — both the dye and the motion source.
        source_dir: PathBuf,
        output_dir: PathBuf,
        /// Number of output frames to render (capped to the available source frames).
        #[arg(long, default_value_t = 120)]
        frames: usize,
        /// Strength applied to the source's own flow when advecting the dye (flow units; the
        /// flow is in pixels/frame). 1.0 moves the dye with the measured motion; 0 holds.
        #[arg(long, default_value_t = 1.0)]
        advect: f32,
        /// Source bled back into the dye each frame, in [0, 1] (the "frame refresh"). Lower =
        /// the dye smears further along the motion; 0 = pure smear, 1 = source verbatim.
        #[arg(long, default_value_t = 0.08)]
        reinject: f32,
        /// Render backend. `metal` is gated against the CPU reference per frame.
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    /// Render the discrete-carrier particle advection (experimental, deterministic):
    /// a grid of coloured particles seeded from the source rides the shared steady-vortex
    /// field. Frame zero is the initial grid (a posterised source); each later frame flows the
    /// particles along the field and splats them onto black. `--advect 0` holds the static grid
    /// (the off case). Distinct from the fluid mosaic — no cohesion/repulsion, just flow.
    RenderFieldParticlesSequence {
        /// Source video frames (PNG sequence). The first frame seeds particle positions and
        /// colours; `--live-colour` additionally samples each current frame at particle origins.
        source_dir: PathBuf,
        output_dir: PathBuf,
        /// Number of output frames to render.
        #[arg(long, default_value_t = 120)]
        frames: usize,
        /// Grid spacing in pixels — one particle per spacing×spacing cell (smaller = denser).
        #[arg(long, default_value_t = 8)]
        spacing: u32,
        /// Edge length (pixels) of each particle's splat square (= spacing tiles the canvas).
        #[arg(long, default_value_t = 8)]
        particle_size: u32,
        /// Field strength per frame (pixels). 0 holds the static grid; higher = flows further.
        #[arg(long, default_value_t = 6.0)]
        advect: f32,
        /// Vortex scale (lattice cells per pixel). Smaller = larger coherent vortices.
        #[arg(long, default_value_t = 0.008)]
        turbulence_scale: f32,
        /// Drift rate of the field's fine detail per frame (the big vortices stay steady).
        #[arg(long, default_value_t = 0.06)]
        turbulence_speed: f32,
        /// Fine-detail octave weight relative to the steady big vortices (0 = pure vortices).
        #[arg(long, default_value_t = 0.1)]
        detail: f32,
        /// Re-sample each particle's colour from its origin cell in the current source frame
        /// every frame, so a video plays through the flowing carrier (off = frozen seed colour).
        #[arg(long, default_value_t = false)]
        live_colour: bool,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Render backend. `metal` is gated against the CPU reference per frame (a correctness-
        /// first gather kernel; for a dense grid the CPU scatter is faster).
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    /// Render a persistent-trail vector-field cascade (experimental, deterministic; CPU-only):
    /// a grid of source-image tiles is advected along the shared steady-vortex field and stamped
    /// every frame onto a canvas that is never cleared, so the image smears into ribbons that
    /// trace the streamlines. `--grid-spacing > --tile-size` gives sparse ribbons on black;
    /// `== --tile-size` smears the whole image. `--advect 0` holds the static grid (the off case).
    RenderCascadeTrailsSequence {
        /// Source video frames (PNG sequence). The first frame seeds the tile grid; with
        /// `--live-refresh` each current frame is re-sampled at the tile origins.
        source_dir: PathBuf,
        output_dir: PathBuf,
        /// Number of output frames to render.
        #[arg(long, default_value_t = 120)]
        frames: usize,
        /// Edge length (pixels) of each stamped tile / source patch.
        #[arg(long, default_value_t = 28)]
        tile_size: u32,
        /// Spacing (pixels) between tile homes. `> tile-size` = sparse ribbons; `=` = dense smear.
        #[arg(long, default_value_t = 60)]
        grid_spacing: u32,
        /// Field strength per frame (pixels). 0 holds the static grid (no trails); higher = longer ribbons.
        #[arg(long, default_value_t = 1.6)]
        advect: f32,
        /// Vortex scale (lattice cells per pixel). Smaller = larger coherent vortices.
        #[arg(long, default_value_t = 0.008)]
        turbulence_scale: f32,
        /// Fine-detail octave weight relative to the steady big vortices (0 = pure vortices).
        #[arg(long, default_value_t = 0.1)]
        detail: f32,
        /// Re-sample each tile's patch from its origin cell in the current source frame every
        /// frame, so a video plays through the trails (off = frozen seed patches).
        #[arg(long, default_value_t = true)]
        live_refresh: bool,
        #[arg(long, default_value_t = 0)]
        seed: u64,
    },
    /// Render a fluid colour-sort mosaic (experimental, deterministic; Slice 1 —
    /// CPU-only). Tiles of both sources are relocated by colour: local same-colour
    /// cohesion plus colour-blind repulsion phase-separate them into colour domains
    /// that fill the frame (settled before frame zero), then a divergence-free fluid
    /// field advects them so the colour groups flow and intermix. `--cohesion 0
    /// --repulsion 0 --fluid-strength 0 --jitter 0 --settle-iterations 0` leaves the
    /// source grids overlaid in place.
    RenderFluidMosaicSequence {
        /// Source A video frames (PNG sequence; only the first frame seeds the sim).
        source_a_dir: PathBuf,
        /// Source B video frames (PNG sequence; only the first frame seeds the sim).
        source_b_dir: PathBuf,
        output_dir: PathBuf,
        /// Number of output frames to simulate.
        #[arg(long, default_value_t = 120)]
        frames: usize,
        /// Uniform tile edge length in pixels.
        #[arg(long, default_value_t = 8)]
        tile_size: u32,
        /// Quantization levels per RGB channel for colour binning (>= 2). Colour
        /// groups = bins^3.
        #[arg(long, default_value_t = 5)]
        color_bins: u32,
        /// Per-step pull of each tile toward the local mean of nearby same-colour
        /// tiles, in [0, 1] (local cohesion → emergent colour domains). [default: 0.035;
        /// 0.015 when --vortex-flow > 0, so domains spread enough to flow without voids]
        #[arg(long)]
        cohesion: Option<f32>,
        /// Neighbourhood radius (pixels) over which same-colour cohesion is gathered.
        #[arg(long, default_value_t = 24.0)]
        cohesion_radius: f32,
        /// Colour-blind short-range repulsion (pixels/step) — the stiff incompressible
        /// pressure that keeps tiles spread so colour domains fill the frame instead
        /// of contracting into voids. [default: 1.4; 3.0 when --vortex-flow > 0]
        #[arg(long)]
        repulsion: Option<f32>,
        /// Radius (pixels) within which tiles repel one another.
        /// [default: 10.0; 16.0 when --vortex-flow > 0]
        #[arg(long)]
        repulsion_radius: Option<f32>,
        /// Amplitude of the analytic fluid velocity field (pixels/step).
        /// [default: 0.5; 0.0 when --vortex-flow > 0, so the vortex flow is the sole current]
        #[arg(long)]
        fluid_strength: Option<f32>,
        /// Spatial frequency of the curl field (radians/pixel); smaller = broader currents.
        #[arg(long, default_value_t = 0.01)]
        fluid_scale: f32,
        /// Temporal phase advance of the fluid per frame (churn speed).
        #[arg(long, default_value_t = 0.15)]
        fluid_drift: f32,
        /// Per-step velocity damping in [0, 1) (keeps motion bounded).
        #[arg(long, default_value_t = 0.88)]
        damping: f32,
        /// Warmup cohesion+repulsion iterations before frame zero (grouped initial state).
        #[arg(long, default_value_t = 60)]
        settle_iterations: u32,
        /// Per-step animated random nudge (pixels) keeping groups alive.
        /// [default: 0.03; 0.0 when --vortex-flow > 0, so the vortex flow isn't masked by wobble]
        #[arg(long)]
        jitter: Option<f32>,
        /// Render flat mean-colour tiles instead of carrying each tile's original
        /// source pixel patch (the v1 look; this is the off case for the texture
        /// readout). Sorting/motion are identical either way.
        #[arg(long)]
        flat_tiles: bool,
        /// Variable-size tiles: quadtree-subdivide each `tile_size` cell down toward
        /// `min_tile_size` where local colour variance is high (flat regions stay
        /// coarse, detail gets fine). Off by default; omitting it is the off case for
        /// the adaptive readout.
        #[arg(long)]
        adaptive_tiles: bool,
        /// Smallest tile edge the quadtree may reach (only with --adaptive-tiles).
        #[arg(long, default_value_t = 4)]
        min_tile_size: u32,
        /// Sum-of-per-channel variance above which a cell subdivides (only with
        /// --adaptive-tiles). Lower ⇒ finer tiles.
        #[arg(long, default_value_t = 0.004)]
        subdivide_threshold: f32,
        /// Live colour refresh: re-sample each tile's painted colour/patch from the
        /// current source frame every frame so the two videos play through the flowing
        /// mosaic (render-only; the simulation is unaffected). Sources cycle if the
        /// render outlasts the clip. Off by default; omitting it is the off case.
        #[arg(long)]
        live_refresh: bool,
        /// Sim-driving live re-sort: like --live-refresh, but also re-bins each tile from
        /// the current source frame so the cohesion force follows the live colour and
        /// domains migrate to track the video (not just the painted pixels). Implies the
        /// live colour refresh. Off by default; render-only --live-refresh is the off case.
        #[arg(long)]
        live_resort: bool,
        /// Cluster-blob layout: cohesion pulls each tile toward its colour bin's global
        /// centroid so each colour gathers into one compact blob, instead of the default
        /// local phase-separation into screen-filling domains. Off by default.
        #[arg(long)]
        cluster_blob: bool,
        /// Dispersion-band intensity: when > 0, a soft-edged vertical band that sweeps
        /// across the canvas amplifies each in-band tile's jitter + fluid so colour
        /// domains shatter into confetti where the wipe sits, then re-gather behind it
        /// (advance-time only). 0 (default) = no band.
        #[arg(long, default_value_t = 0.0)]
        dispersion_band: f32,
        /// Dispersion-band width as a fraction of the canvas width (0..1).
        #[arg(long, default_value_t = 0.25)]
        band_width: f32,
        /// Dispersion-band sweep speed in canvas-widths per frame (0 = static band).
        #[arg(long, default_value_t = 0.02)]
        band_speed: f32,
        /// Dispersion-band centre at frame zero, as a fraction of the canvas width (0..1).
        #[arg(long, default_value_t = 0.0)]
        band_start: f32,
        /// Faux-fluid turbulence amplitude (pixels/step) — a curl-of-value-noise field
        /// added to the analytic fluid for organic, evolving currents. 0 (default) = off.
        #[arg(long, default_value_t = 0.0)]
        turbulence: f32,
        /// Turbulence spatial frequency (lattice cells per pixel). Smaller = broader currents.
        #[arg(long, default_value_t = 0.02)]
        turbulence_scale: f32,
        /// Turbulence temporal evolution rate per frame (how fast the currents churn).
        #[arg(long, default_value_t = 0.3)]
        turbulence_speed: f32,
        /// Steady-vortex flow amplitude (pixels/step) — the shared faux-fluid vortex field
        /// added to each tile so colour domains swirl along persistent vortices. 0 = off.
        #[arg(long, default_value_t = 0.0)]
        vortex_flow: f32,
        /// Vortex scale (lattice cells per pixel) for the vortex flow. Smaller = larger vortices.
        #[arg(long, default_value_t = 0.008)]
        vortex_scale: f32,
        /// Fine-detail octave weight for the vortex flow (big vortices stay steady).
        #[arg(long, default_value_t = 0.1)]
        vortex_detail: f32,
        /// Drift rate per frame of the vortex flow's fine detail.
        #[arg(long, default_value_t = 0.06)]
        vortex_speed: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
    },
    /// Render a descriptor-coagulated flow blend (experimental, deterministic;
    /// Slice 1 — CPU-only, single-frame, no advection/feedback yet). Both sources
    /// are mangled together: cells group into irregular coagulated patches by
    /// per-cell descriptor (mean colour + texture), then a hard/dirty composite
    /// interleaves A and B. `--coagulation-strength 0` (with `--randomness 0` and
    /// `--bias 0`) is Source B verbatim.
    RenderCoagulatedBlendSequence {
        /// Source A video frames (PNG sequence) — the intruding material.
        source_a_dir: PathBuf,
        /// Source B video frames (PNG sequence) — the carrier baseline.
        source_b_dir: PathBuf,
        output_dir: PathBuf,
        /// Ownership-field cell edge length in pixels (>= 1).
        #[arg(long, default_value_t = 16)]
        patch_size: u32,
        /// Weight on per-cell mean-colour luminance in the A-vs-B preference.
        #[arg(long, default_value_t = 1.0)]
        color_weight: f32,
        /// Weight on per-cell texture energy (luma variance + gradient magnitude).
        #[arg(long, default_value_t = 0.0)]
        texture_weight: f32,
        /// Spatial-coherence relaxation passes that clump patches (anti-checkerboard).
        #[arg(long, default_value_t = 2)]
        coherence_passes: u32,
        /// Per-pass neighbour pull in [0, 1].
        #[arg(long, default_value_t = 0.5)]
        coherence_strength: f32,
        /// Seeded per-cell scatter that breaks uniform crossfades.
        #[arg(long, default_value_t = 0.0)]
        randomness: f32,
        /// Master coagulation amount; 0 (with randomness/bias 0) = B passthrough.
        #[arg(long, default_value_t = 0.0)]
        coagulation_strength: f32,
        /// 0 = soft lerp; 1 = dithered hard threshold (dirty edges).
        #[arg(long, default_value_t = 0.0)]
        edge_hardness: f32,
        /// Seeded per-pixel jitter on the hard-threshold boundary.
        #[arg(long, default_value_t = 0.0)]
        edge_dither: f32,
        /// Per-cell coherent sub-block offset of the field lookup, in fractions of a
        /// cell (ragged, datamosh-y edges). 0 = clean grid.
        #[arg(long, default_value_t = 0.0)]
        block_jitter: f32,
        /// Baseline A ownership added to every cell (0 keeps B dominant).
        #[arg(long, default_value_t = 0.0)]
        bias: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Vector field that advects the ownership field across frames (Slice 2):
        /// `a-flow`/`b-flow` (optical flow between consecutive source frames),
        /// `mixed` (per-cell mean), or `turbulence` (synthetic, needs no motion).
        #[arg(long, value_enum, default_value_t = CliCoagulationFlowSource::AFlow)]
        advect_source: CliCoagulationFlowSource,
        /// Flow scale for field advection. `0` (with `--refresh 1`) keeps the blend
        /// stateless (Slice 1, byte-identical).
        #[arg(long, default_value_t = 0.0)]
        advect_amount: f32,
        /// How much each frame re-seeds the field from fresh descriptors: `1` =
        /// re-seed every frame (no memory ≡ Slice 1), `0` = the field only advects.
        #[arg(long, default_value_t = 1.0)]
        refresh: f32,
        /// Strength of synthetic turbulence (only `--advect-source turbulence`).
        #[arg(long, default_value_t = 1.0)]
        turbulence: f32,
        /// Output feedback smear: fraction of the previous output frame held into
        /// this one, leaving trails as patches move (0 = no smear). >0 forces the
        /// stateful path.
        #[arg(long, default_value_t = 0.0)]
        smear: f32,
        /// Per-frame decay of the held smear trail (1 = no fade, 0 = none kept).
        #[arg(long, default_value_t = 0.9)]
        smear_decay: f32,
        /// Composite backend. `metal` is gated against the CPU reference per frame.
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        max_frames: Option<usize>,
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
        /// Scales both texture dimensions (luma variance + gradient magnitude) in
        /// the selection distance, so Source A's per-tile spatial busyness matches
        /// carrier grains of similar structure (0 = off, the default).
        #[arg(long, default_value_t = 0.0)]
        texture_weight: f32,
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
        /// Anti-repeat penalty added to the squared feature distance of grains
        /// used in recent output frames (0 = off, the default). Pushes temporal
        /// diversity so the mosaic keeps finding fresh material.
        #[arg(long, default_value_t = 0.0)]
        anti_repeat_weight: f32,
        /// Number of frames a selected grain stays penalized (penalty decays
        /// linearly to zero). Only matters when --anti-repeat-weight > 0.
        #[arg(long, default_value_t = 8)]
        anti_repeat_cooldown: u32,
        /// Temporal-coherence reward (the smooth-motion complement to anti-repeat):
        /// penalty added to the squared feature distance of grains whose source
        /// frame is far from each tile's previous pick (0 = off, the default).
        /// Keeps each tile's source frame drifting smoothly instead of jumping.
        #[arg(long, default_value_t = 0.0)]
        coherence_weight: f32,
        /// Frame distance over which the coherence penalty saturates (penalty grows
        /// linearly to the weight). Only matters when --coherence-weight > 0.
        #[arg(long, default_value_t = 8)]
        coherence_reach: u32,
        /// Spatial-origin coherence reward: penalty added to the squared feature
        /// distance of grains whose origin is far (in grain-tile units) from each
        /// tile's previous pick (0 = off, the default). Shares --coherence-reach as
        /// its saturation distance; keeps a tile's pick from teleporting across the
        /// frame even when it stays on a nearby source frame.
        #[arg(long, default_value_t = 0.0)]
        spatial_coherence_weight: f32,
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
    /// Render a short curated A-modulates-B preview bundle from extracted PNG
    /// source directories. This is the user-facing "show me the character of the
    /// patch" path: flow displacement, flow feedback, granular mosaic, and
    /// controlled datamosh are rendered into named stills, a contact sheet, a
    /// continuous PNG sequence, and optionally an H.264 MP4 via external ffmpeg.
    RenderShowcase {
        /// Source A video frames (PNG sequence); acts as the modulator.
        modulator_dir: PathBuf,
        /// Source B video frames (PNG sequence); acts as the carrier.
        carrier_dir: PathBuf,
        /// Output bundle directory.
        output_dir: PathBuf,
        /// How aggressively the curated settings should degrade the carrier.
        #[arg(long, value_enum, default_value_t = CliShowcaseIntensity::Destructive)]
        intensity: CliShowcaseIntensity,
        /// Frames rendered for each effect segment.
        #[arg(long, default_value_t = 15)]
        frames_per_effect: usize,
        /// Render-frame rate for the preview sequence and MP4.
        #[arg(long, default_value_t = 12.0)]
        frame_rate: f64,
        /// Granular tile size for the mosaic segment. Larger is faster and blockier.
        #[arg(long, default_value_t = 48)]
        granular_grain_size: u32,
        /// Seed shared by seeded showcase effects.
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Render backend for parity-gated effects.
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        /// Skip the optional H.264 MP4 encode and write only PNG outputs.
        #[arg(long)]
        no_mp4: bool,
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
        #[arg(long, default_value_t = 1, value_parser = parse_feedback_iterations)]
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
        #[arg(long, default_value_t = 1, value_parser = parse_feedback_iterations)]
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
    QueueAddFluidAdvectSequence {
        queue_path: PathBuf,
        source_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 120)]
        frames: u32,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long, default_value_t = 12.0)]
        advect: f32,
        #[arg(long, default_value_t = 0.008)]
        turbulence_scale: f32,
        #[arg(long, default_value_t = 0.06)]
        turbulence_speed: f32,
        #[arg(long, default_value_t = 0.1)]
        detail: f32,
        #[arg(long, default_value_t = 0.05)]
        reinject: f32,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddFluidAdvectTwoSourceSequence {
        queue_path: PathBuf,
        source_a_dir: PathBuf,
        source_b_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 120)]
        frames: u32,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long, default_value_t = 1.0)]
        advect: f32,
        #[arg(long, default_value_t = 0.08)]
        reinject: f32,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddOpticalFlowAdvectSequence {
        queue_path: PathBuf,
        source_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 120)]
        frames: u32,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long, default_value_t = 1.0)]
        advect: f32,
        #[arg(long, default_value_t = 0.08)]
        reinject: f32,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddFieldParticlesSequence {
        queue_path: PathBuf,
        source_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 120)]
        frames: u32,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long, default_value_t = 8)]
        spacing: u32,
        #[arg(long, default_value_t = 8)]
        particle_size: u32,
        #[arg(long, default_value_t = 6.0)]
        advect: f32,
        #[arg(long, default_value_t = 0.008)]
        turbulence_scale: f32,
        #[arg(long, default_value_t = 0.06)]
        turbulence_speed: f32,
        #[arg(long, default_value_t = 0.1)]
        detail: f32,
        #[arg(long, default_value_t = false)]
        live_colour: bool,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddCascadeTrailsSequence {
        queue_path: PathBuf,
        source_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 120)]
        frames: u32,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long, default_value_t = 28)]
        tile_size: u32,
        #[arg(long, default_value_t = 60)]
        grid_spacing: u32,
        #[arg(long, default_value_t = 1.6)]
        advect: f32,
        #[arg(long, default_value_t = 0.008)]
        turbulence_scale: f32,
        #[arg(long, default_value_t = 0.1)]
        detail: f32,
        #[arg(long, default_value_t = true)]
        live_refresh: bool,
        #[arg(long, default_value_t = 0)]
        seed: u64,
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
        /// Scales both texture dims (luma variance + gradient magnitude); 0 = off.
        #[arg(long, default_value_t = 0.0)]
        texture_weight: f32,
        #[arg(long)]
        modulator_rms_cache: Option<PathBuf>,
        #[arg(long)]
        carrier_rms_cache: Option<PathBuf>,
        /// STFT cache for Source A; appends a spectral-centroid query dimension.
        #[arg(long)]
        modulator_centroid_cache: Option<PathBuf>,
        /// STFT cache for Source B; appends a spectral-centroid dimension to each pool grain.
        #[arg(long)]
        carrier_centroid_cache: Option<PathBuf>,
        /// Trailing pool window in frames (0 = whole-clip, the default).
        #[arg(long, default_value_t = 0)]
        pool_window: u32,
        /// Anti-repeat penalty for grains used in recent output frames (0 = off).
        #[arg(long, default_value_t = 0.0)]
        anti_repeat_weight: f32,
        /// Frames a selected grain stays penalized. Only matters when weight > 0.
        #[arg(long, default_value_t = 8)]
        anti_repeat_cooldown: u32,
        /// Temporal-coherence reward for source-frame continuity (0 = off).
        #[arg(long, default_value_t = 0.0)]
        coherence_weight: f32,
        /// Frame distance over which the coherence penalty saturates. Only matters when weight > 0.
        #[arg(long, default_value_t = 8)]
        coherence_reach: u32,
        /// Spatial-origin coherence reward for grain-origin continuity within a
        /// frame (0 = off). Shares --coherence-reach as its saturation distance.
        #[arg(long, default_value_t = 0.0)]
        spatial_coherence_weight: f32,
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
    /// Enqueue a video-vocoder PNG-frame sequence job (luma-band tonal routing).
    QueueAddVideoVocoderSequence {
        queue_path: PathBuf,
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 8)]
        bands: u32,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long, value_enum, default_value_t = CliVocoderMode::Match)]
        mode: CliVocoderMode,
        #[arg(long)]
        max_frames: Option<u32>,
        #[arg(long, default_value_t = 24.0)]
        frame_rate: f64,
        #[arg(long)]
        project_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    QueueAddSpectralCrossSynth {
        queue_path: PathBuf,
        modulator_wav: PathBuf,
        carrier_wav: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, value_enum, default_value_t = CliCrossSynthMode::Gain)]
        mode: CliCrossSynthMode,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long, value_enum, default_value_t = CliFilterType::Lowpass)]
        filter_type: CliFilterType,
        #[arg(long, default_value_t = 2048)]
        rms_window: usize,
        #[arg(long, default_value_t = 512)]
        rms_hop: usize,
        #[arg(long, default_value_t = 1024)]
        fft_size: usize,
        #[arg(long, default_value_t = 256)]
        stft_hop: usize,
        #[arg(long, value_enum, default_value_t = CliWindowFunction::Hann)]
        window: CliWindowFunction,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddAudioImpulseConvolution {
        queue_path: PathBuf,
        modulator_wav: PathBuf,
        carrier_wav: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long)]
        max_impulse_samples: Option<u32>,
        #[arg(long, value_enum, default_value_t = CliConvolutionMethod::Direct)]
        method: CliConvolutionMethod,
        #[arg(long)]
        resample_impulse: bool,
        /// IR channel mapping: `mono` or `per-channel` (true-stereo).
        #[arg(long, value_enum, default_value_t = CliIrMode::Mono)]
        ir_mode: CliIrMode,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddVideoAudioRoute {
        queue_path: PathBuf,
        modulator_dir: PathBuf,
        carrier_wav: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, value_enum, default_value_t = CliVideoAudioRouteDescriptor::Luma)]
        descriptor: CliVideoAudioRouteDescriptor,
        #[arg(long, value_enum, default_value_t = CliVideoAudioRouteMode::Gain)]
        mode: CliVideoAudioRouteMode,
        /// Filter response for `--mode filter` (ignored otherwise).
        #[arg(long, value_enum, default_value_t = CliFilterType::Lowpass)]
        filter_type: CliFilterType,
        /// How the descriptor envelope is resampled onto B's audio grid.
        #[arg(long, value_enum, default_value_t = CliVideoAudioRouteSampling::Hold)]
        sampling: CliVideoAudioRouteSampling,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long, default_value_t = 30.0)]
        fps: f64,
        #[arg(long)]
        project_path: Option<PathBuf>,
    },
    QueueAddAudioVideoRouteSequence {
        queue_path: PathBuf,
        modulator_wav: PathBuf,
        carrier_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        #[arg(long, default_value_t = 8.0)]
        shift_x: f32,
        #[arg(long, default_value_t = 0.0)]
        shift_y: f32,
        #[arg(long, default_value_t = 2048)]
        rms_window: u32,
        #[arg(long, default_value_t = 512)]
        rms_hop: u32,
        #[arg(long, default_value_t = 30.0)]
        frame_rate: f64,
        #[arg(long)]
        max_frames: Option<u32>,
        #[arg(long)]
        project_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    QueueAddDatamoshSequence {
        queue_path: PathBuf,
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 0)]
        keyframe_interval: u32,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        /// Macroblock size for codec-simulated mosh: `1` = smooth bloom, `N >= 2`
        /// quantizes A's flow to NxN blocks so whole macroblocks slide.
        #[arg(long, default_value_t = 1)]
        block_size: u32,
        /// Block-residual gain: re-inject the intra-block motion discarded by
        /// quantization. `0` = block path; needs block-size >= 2.
        #[arg(long, default_value_t = 0.0)]
        residual_gain: f32,
        /// Decay on the residual accumulator: `0` = one-frame kick, `->1` = drift.
        #[arg(long, default_value_t = 0.9)]
        residual_decay: f32,
        /// Per-block keep/drop threshold: macroblocks whose mean motion magnitude is
        /// below this snap back to the carrier (intra-block refresh) while busier
        /// blocks rot. `0` = no per-block refresh; needs block-size >= 2.
        #[arg(long, default_value_t = 0.0)]
        block_refresh_threshold: f32,
        /// FFglitch-style motion-vector remix on the block-MV grid (block-size 2 or
        /// more): `sort` pools motion by descending magnitude, `shuffle` permutes by
        /// `--remix-seed`. `none` = off.
        #[arg(long, value_enum, default_value_t = CliVectorRemixMode::None)]
        vector_remix: CliVectorRemixMode,
        /// Seed for `--vector-remix shuffle` (deterministic permutation).
        #[arg(long, default_value_t = 0)]
        remix_seed: u64,
        /// Named deterministic destructive preset. `custom` keeps the explicit knobs.
        #[arg(long, value_enum, default_value_t = CliDatamoshPreset::Custom)]
        preset: CliDatamoshPreset,
        /// Reuse/write per-frame temporal optical-flow sidecars.
        #[arg(long)]
        flow_cache_dir: Option<PathBuf>,
        #[arg(long)]
        max_frames: Option<u32>,
        #[arg(long)]
        project_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    QueueRunDatamoshSequence {
        queue_path: PathBuf,
    },
    QueueAddConvolutionalBlendSequence {
        queue_path: PathBuf,
        modulator_dir: PathBuf,
        carrier_dir: PathBuf,
        output_root_dir: PathBuf,
        #[arg(long, default_value_t = 3)]
        kernel_size: u32,
        #[arg(long, default_value_t = 1.0)]
        amount: f32,
        /// Kernel extraction: `luma` (one luminance kernel) or `color` (per R/G/B).
        #[arg(long, value_enum, default_value_t = CliKernelMode::Luma)]
        kernel_mode: CliKernelMode,
        #[arg(long)]
        max_frames: Option<u32>,
        #[arg(long)]
        project_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = CliRenderBackend::Cpu)]
        backend: CliRenderBackend,
    },
    QueueRunConvolutionalBlendSequence {
        queue_path: PathBuf,
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
    QueueRunFluidAdvectSequence {
        queue_path: PathBuf,
    },
    QueueRunFluidAdvectTwoSourceSequence {
        queue_path: PathBuf,
    },
    QueueRunOpticalFlowAdvectSequence {
        queue_path: PathBuf,
    },
    QueueRunFieldParticlesSequence {
        queue_path: PathBuf,
    },
    QueueRunCascadeTrailsSequence {
        queue_path: PathBuf,
    },
    QueueRunGranularMosaicSequence {
        queue_path: PathBuf,
    },
    QueueRunGranularMosaicPoolSequence {
        queue_path: PathBuf,
    },
    QueueRunVideoVocoderSequence {
        queue_path: PathBuf,
    },
    QueueRunSpectralCrossSynth {
        queue_path: PathBuf,
    },
    QueueRunAudioImpulseConvolution {
        queue_path: PathBuf,
    },
    QueueRunVideoAudioRoute {
        queue_path: PathBuf,
    },
    QueueRunAudioVideoRouteSequence {
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
pub(crate) enum CliWindowFunction {
    Hann,
    Hamming,
    Rectangular,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliDatamoshBitstreamOperation {
    #[default]
    PframeDuplicate,
    RemoveKeyframe,
    MotionTransfer,
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
pub(crate) enum CliCrossSynthMode {
    #[default]
    Gain,
    Filter,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliVideoAudioRouteMode {
    /// A's per-frame descriptor envelope scales B's amplitude.
    #[default]
    Gain,
    /// A's per-frame descriptor drives an equal-power stereo pan of B.
    Pan,
    /// A's per-frame descriptor sweeps a one-pole LP/HP filter cutoff on B.
    Filter,
}

impl From<CliVideoAudioRouteMode> for VideoAudioRouteMode {
    fn from(value: CliVideoAudioRouteMode) -> Self {
        match value {
            CliVideoAudioRouteMode::Gain => Self::Gain,
            CliVideoAudioRouteMode::Pan => Self::Pan,
            CliVideoAudioRouteMode::Filter => Self::Filter,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliVideoAudioRouteDescriptor {
    /// Per-frame mean Rec.709 luma (brightness).
    #[default]
    Luma,
    /// Per-frame mean optical-flow magnitude (motion).
    Flow,
}

impl From<CliVideoAudioRouteDescriptor> for VideoAudioRouteDescriptor {
    fn from(value: CliVideoAudioRouteDescriptor) -> Self {
        match value {
            CliVideoAudioRouteDescriptor::Luma => Self::Luma,
            CliVideoAudioRouteDescriptor::Flow => Self::Flow,
        }
    }
}

impl From<CliFilterType> for VideoAudioRouteFilterType {
    fn from(value: CliFilterType) -> Self {
        match value {
            CliFilterType::Lowpass => Self::Lowpass,
            CliFilterType::Highpass => Self::Highpass,
        }
    }
}

pub(crate) fn video_audio_route_filter_type(value: VideoAudioRouteFilterType) -> FilterType {
    match value {
        VideoAudioRouteFilterType::Lowpass => FilterType::Lowpass,
        VideoAudioRouteFilterType::Highpass => FilterType::Highpass,
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliVideoAudioRouteSampling {
    /// Step: hold each frame's value until the next frame.
    #[default]
    Hold,
    /// Linearly interpolate between frames (a smooth curve).
    Smooth,
}

impl From<CliVideoAudioRouteSampling> for VideoAudioRouteSampling {
    fn from(value: CliVideoAudioRouteSampling) -> Self {
        match value {
            CliVideoAudioRouteSampling::Hold => Self::Hold,
            CliVideoAudioRouteSampling::Smooth => Self::Smooth,
        }
    }
}

pub(crate) fn video_audio_route_sampling(value: VideoAudioRouteSampling) -> EnvelopeSampling {
    match value {
        VideoAudioRouteSampling::Hold => EnvelopeSampling::Hold,
        VideoAudioRouteSampling::Smooth => EnvelopeSampling::Smooth,
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliFilterType {
    #[default]
    Lowpass,
    Highpass,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliConvolutionMethod {
    #[default]
    Direct,
    Fft,
}

impl From<CliConvolutionMethod> for AudioConvolutionMethod {
    fn from(value: CliConvolutionMethod) -> Self {
        match value {
            CliConvolutionMethod::Direct => Self::Direct,
            CliConvolutionMethod::Fft => Self::Fft,
        }
    }
}

impl From<CliConvolutionMethod> for ConvolutionMethod {
    fn from(value: CliConvolutionMethod) -> Self {
        match value {
            CliConvolutionMethod::Direct => Self::Direct,
            CliConvolutionMethod::Fft => Self::Fft,
        }
    }
}

/// Map a persisted core convolution method onto the audio-crate enum.
pub(crate) fn audio_convolution_method(method: ConvolutionMethod) -> AudioConvolutionMethod {
    match method {
        ConvolutionMethod::Direct => AudioConvolutionMethod::Direct,
        ConvolutionMethod::Fft => AudioConvolutionMethod::Fft,
    }
}

/// Manifest string for a persisted convolution method.
pub(crate) fn convolution_method_label(method: ConvolutionMethod) -> &'static str {
    match method {
        ConvolutionMethod::Direct => "direct",
        ConvolutionMethod::Fft => "fft",
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliCoagulationFlowSource {
    #[default]
    AFlow,
    BFlow,
    Mixed,
    Turbulence,
}

impl From<CliCoagulationFlowSource> for CoagulationFlowSource {
    fn from(value: CliCoagulationFlowSource) -> Self {
        match value {
            CliCoagulationFlowSource::AFlow => Self::AFlow,
            CliCoagulationFlowSource::BFlow => Self::BFlow,
            CliCoagulationFlowSource::Mixed => Self::Mixed,
            CliCoagulationFlowSource::Turbulence => Self::Turbulence,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliKernelMode {
    #[default]
    Luma,
    Color,
}

impl From<CliKernelMode> for KernelMode {
    fn from(value: CliKernelMode) -> Self {
        match value {
            CliKernelMode::Luma => Self::Luma,
            CliKernelMode::Color => Self::Color,
        }
    }
}

/// Manifest string + algorithm id for a persisted convolution-blend kernel mode.
pub(crate) fn kernel_mode_label(mode: KernelMode) -> &'static str {
    match mode {
        KernelMode::Luma => "luma",
        KernelMode::Color => "color",
    }
}

pub(crate) fn convolution_blend_algorithm(mode: KernelMode) -> &'static str {
    match mode {
        KernelMode::Luma => CONVOLUTION_BLEND_ALGORITHM,
        KernelMode::Color => CONVOLUTION_BLEND_COLOR_ALGORITHM,
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliIrMode {
    #[default]
    Mono,
    PerChannel,
}

impl From<CliIrMode> for IrMode {
    fn from(value: CliIrMode) -> Self {
        match value {
            CliIrMode::Mono => Self::Mono,
            CliIrMode::PerChannel => Self::PerChannel,
        }
    }
}

/// Map a persisted core IR mode onto the audio-crate enum (orphan rule: both
/// foreign to this crate, so a free helper rather than `From`).
pub(crate) fn audio_ir_mode(mode: IrMode) -> AudioIrMode {
    match mode {
        IrMode::Mono => AudioIrMode::Mono,
        IrMode::PerChannel => AudioIrMode::PerChannel,
    }
}

/// Manifest string for a persisted IR mode.
pub(crate) fn ir_mode_label(mode: IrMode) -> &'static str {
    match mode {
        IrMode::Mono => "mono",
        IrMode::PerChannel => "per_channel",
    }
}

/// Algorithm id for a persisted IR mode.
pub(crate) fn impulse_convolution_algorithm(mode: IrMode) -> &'static str {
    match mode {
        IrMode::Mono => IMPULSE_CONVOLUTION_BLEND_ALGORITHM,
        IrMode::PerChannel => PER_CHANNEL_IMPULSE_CONVOLUTION_BLEND_ALGORITHM,
    }
}

impl From<CliFilterType> for FilterType {
    fn from(value: CliFilterType) -> Self {
        match value {
            CliFilterType::Lowpass => Self::Lowpass,
            CliFilterType::Highpass => Self::Highpass,
        }
    }
}

impl From<CliCrossSynthMode> for CrossSynthMode {
    fn from(value: CliCrossSynthMode) -> Self {
        match value {
            CliCrossSynthMode::Gain => Self::Gain,
            CliCrossSynthMode::Filter => Self::Filter,
        }
    }
}

impl From<CliFilterType> for CrossSynthFilterType {
    fn from(value: CliFilterType) -> Self {
        match value {
            CliFilterType::Lowpass => Self::Lowpass,
            CliFilterType::Highpass => Self::Highpass,
        }
    }
}

impl From<CliWindowFunction> for CrossSynthWindow {
    fn from(value: CliWindowFunction) -> Self {
        match value {
            CliWindowFunction::Hann => Self::Hann,
            CliWindowFunction::Hamming => Self::Hamming,
            CliWindowFunction::Rectangular => Self::Rectangular,
        }
    }
}

// Core ↔ audio enums are both foreign to this crate, so the orphan rule forbids
// `From` impls; free helpers convert a persisted job's analysis knobs at run time.
pub(crate) fn cross_synth_filter_type(value: CrossSynthFilterType) -> FilterType {
    match value {
        CrossSynthFilterType::Lowpass => FilterType::Lowpass,
        CrossSynthFilterType::Highpass => FilterType::Highpass,
    }
}

pub(crate) fn cross_synth_window(value: CrossSynthWindow) -> WindowFunction {
    match value {
        CrossSynthWindow::Hann => WindowFunction::Hann,
        CrossSynthWindow::Hamming => WindowFunction::Hamming,
        CrossSynthWindow::Rectangular => WindowFunction::Rectangular,
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliRenderBackend {
    #[default]
    Cpu,
    Metal,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CliSourceRole {
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
pub(crate) enum CliVectorRemixMode {
    /// No remix — the block-quantized flow is used unchanged (off path).
    #[default]
    None,
    /// Reassign block MVs in descending-magnitude order (motion pools coherently).
    Sort,
    /// Deterministic seeded permutation of block MVs (motion scrambles).
    Shuffle,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliDatamoshPreset {
    #[default]
    Custom,
    CodecBloom,
    StructuredMelt,
    MacroblockRot,
    VectorShuffle,
    ScanlineSmear,
    CodecEngrave,
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliShowcaseIntensity {
    /// Clearer source relationship, moderate degradation.
    Balanced,
    /// Stronger beyond-recognition preview settings.
    #[default]
    Destructive,
}

fn parse_feedback_iterations(value: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| "iterations must be an integer".to_string())?;
    if parsed == 1 {
        Ok(parsed)
    } else {
        Err("the current flow-feedback renderer supports exactly one iteration; use feedback amount, mix, decay, and structure instead".to_string())
    }
}

impl From<CliVectorRemixMode> for VectorRemixMode {
    fn from(value: CliVectorRemixMode) -> Self {
        match value {
            CliVectorRemixMode::None => Self::None,
            CliVectorRemixMode::Sort => Self::Sort,
            CliVectorRemixMode::Shuffle => Self::Shuffle,
        }
    }
}

impl From<CliDatamoshPreset> for DatamoshPreset {
    fn from(value: CliDatamoshPreset) -> Self {
        match value {
            CliDatamoshPreset::Custom => Self::Custom,
            CliDatamoshPreset::CodecBloom => Self::CodecBloom,
            CliDatamoshPreset::StructuredMelt => Self::StructuredMelt,
            CliDatamoshPreset::MacroblockRot => Self::MacroblockRot,
            CliDatamoshPreset::VectorShuffle => Self::VectorShuffle,
            CliDatamoshPreset::ScanlineSmear => Self::ScanlineSmear,
            CliDatamoshPreset::CodecEngrave => Self::CodecEngrave,
        }
    }
}

// The schema mirror in core (used by the persisted datamosh job). Allowed by the
// orphan rule because the trait's type parameter (`CliVectorRemixMode`) is local.
impl From<CliVectorRemixMode> for morphogen_core::VectorRemixMode {
    fn from(value: CliVectorRemixMode) -> Self {
        match value {
            CliVectorRemixMode::None => Self::None,
            CliVectorRemixMode::Sort => Self::Sort,
            CliVectorRemixMode::Shuffle => Self::Shuffle,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliGrainSelection {
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

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliVocoderMode {
    /// Tonal envelope transfer: remap B's luma distribution to match A's
    /// (histogram specification). The headline look; ignores `--bands`.
    #[default]
    Match,
    /// Per-band gain routing: A's luma histogram scales B's tonal bands.
    Gain,
}

impl From<CliVocoderMode> for VideoVocoderMode {
    fn from(value: CliVocoderMode) -> Self {
        match value {
            CliVocoderMode::Match => Self::Match,
            CliVocoderMode::Gain => Self::Gain,
        }
    }
}

impl From<VideoVocoderMode> for CliVocoderMode {
    fn from(value: VideoVocoderMode) -> Self {
        match value {
            VideoVocoderMode::Match => Self::Match,
            VideoVocoderMode::Gain => Self::Gain,
        }
    }
}

/// Algorithm identifier stamped on sidecars and provenance for a selection mode.
pub(crate) fn grain_selection_algorithm(mode: GrainSelectionMode) -> &'static str {
    match mode {
        GrainSelectionMode::Luma => GRANULAR_MOSAIC_ALGORITHM,
        GrainSelectionMode::MultimodalRgb => MULTIMODAL_GRAIN_ALGORITHM,
    }
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum CliFlowSource {
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
pub(crate) enum CliStructureMode {
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
