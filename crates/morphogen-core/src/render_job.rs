use serde::{Deserialize, Serialize};

use crate::{AnalysisKind, SourceRole};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderSettings {
    pub width: u32,
    pub height: u32,
    pub quality: RenderQuality,
    pub export_format: ExportFormat,
    pub temporal_supersampling: u32,
    pub deterministic: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderQuality {
    DraftPreview,
    HighQualityOffline,
    FloatMaster,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportFormat {
    Png { bit_depth: u8 },
    ImageSequence { extension: String, bit_depth: u8 },
    ExrSequence { compression: String },
    ProRes { profile: String },
    Wav { bit_depth: u8 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderJob {
    pub id: String,
    pub project_path: Option<String>,
    pub settings: RenderSettings,
    #[serde(default)]
    pub task: RenderJobTask,
    #[serde(default)]
    pub provenance: Option<RenderJobProvenance>,
    pub status: RenderJobStatus,
    #[serde(default)]
    pub output: Option<RenderJobOutputMetadata>,
    #[serde(default)]
    pub failure: Option<RenderJobFailure>,
}

/// Durable record of why a job failed, persisted in the queue so a failure
/// survives the process that produced it rather than only surfacing on stderr.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderJobFailure {
    pub message: String,
}

fn default_carrier_keyframes() -> u32 {
    1
}

fn default_block_collage_tile_size() -> u32 {
    96
}

fn default_pixel_sort_threshold_low() -> f32 {
    0.25
}

fn default_pixel_sort_threshold_high() -> f32 {
    0.80
}

fn default_block_collage_threshold() -> f32 {
    0.5
}

fn default_block_collage_cluster_scale() -> f32 {
    0.25
}

fn default_river_speed() -> f32 {
    3.0
}

fn default_river_turbulence() -> f32 {
    0.8
}

fn default_cascade_collage_scrib_amp_scale() -> f32 {
    1.0
}

fn default_cascade_collage_morph_rate() -> f32 {
    0.12
}

fn default_cascade_collage_bright_osc() -> f32 {
    0.12
}

fn default_cascade_collage_edge_width() -> f32 {
    2.5
}

fn default_cascade_collage_edge_strength() -> f32 {
    0.85
}

fn default_cascade_collage_face_strength() -> f32 {
    0.55
}

fn default_cascade_collage_face_sat() -> f32 {
    0.85
}

fn default_cascade_collage_hue_steps() -> u32 {
    5
}

fn default_cascade_collage_tile_scale() -> f32 {
    1.0
}

fn default_cascade_collage_detail_tiles() -> u32 {
    4
}

fn default_cascade_collage_block_opacity() -> f32 {
    1.0
}

fn default_retro_static_real_bpp() -> u32 {
    4
}

fn default_retro_static_assumed_bpp() -> u32 {
    3
}

fn default_retro_static_filter() -> String {
    "paeth".to_string()
}

fn default_retro_static_strength() -> f32 {
    1.0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RenderJobTask {
    #[default]
    TestRender,
    FrameSequenceFlowDisplace {
        modulator_frame_directory: String,
        carrier_frame_directory: String,
        output_directory: String,
        flow_cache_directory: Option<String>,
        amount: f32,
        max_frames: Option<u32>,
        frame_rate: f64,
        #[serde(default)]
        backend: RenderBackend,
    },
    FrameSequenceFlowFeedback {
        modulator_frame_directory: String,
        carrier_frame_directory: String,
        output_directory: String,
        flow_cache_directory: Option<String>,
        carrier_amount: f32,
        feedback_amount: f32,
        feedback_mix: f32,
        decay: f32,
        iterations: u32,
        max_frames: Option<u32>,
        #[serde(default)]
        reset_at_frame: Option<u32>,
        frame_rate: f64,
        #[serde(default)]
        backend: RenderBackend,
        #[serde(default)]
        flow_source: FlowSource,
        /// Structure-preserving morph strength: re-injects the displaced
        /// carrier's high-frequency band each frame so detail keeps
        /// regenerating instead of washing to fog at high `feedback_mix`.
        /// Defaults to 0.0 (disabled) so legacy jobs keep their meaning.
        #[serde(default)]
        structure_mix: f32,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty and keep their meaning). Envelope times are
        /// sampled against this job's `frame_rate`. Stateful: the routes join
        /// the render's checkpoint contract.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        /// Modulator frames for the luma/flow envelopes. Distinct from
        /// `modulator_frame_directory`, which is the effect's Source A.
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    FrameSequenceFluidAdvect {
        source_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        advect: f32,
        turbulence_scale: f32,
        turbulence_speed: f32,
        detail: f32,
        reinject: f32,
        seed: u64,
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty and keep their meaning). Envelope times are
        /// sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    FrameSequenceFluidAdvectTwoSource {
        modulator_frame_directory: String,
        carrier_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        advect: f32,
        reinject: f32,
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty and keep their meaning). Envelope times are
        /// sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        /// Modulator frames for the luma/flow envelopes. Distinct from
        /// `modulator_frame_directory`, which is the effect's Source A.
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    FrameSequenceOpticalFlowAdvect {
        source_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        advect: f32,
        reinject: f32,
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty and keep their meaning). Envelope times are
        /// sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    /// Descriptor-coagulated flow blend (two-source) over a paired PNG sequence.
    /// Modulation targets coagulation_strength/edge_hardness/bias are
    /// provenance-only (coagulated has no checkpoint path). `advect_source` is
    /// stored as a string — its render-only enum can't cross the core boundary
    /// (the cascade-trails `field` precedent).
    FrameSequenceCoagulatedBlend {
        source_a_directory: String,
        source_b_directory: String,
        output_directory: String,
        frame_rate: f64,
        patch_size: u32,
        color_weight: f32,
        texture_weight: f32,
        coherence_passes: u32,
        coherence_strength: f32,
        randomness: f32,
        coagulation_strength: f32,
        edge_hardness: f32,
        edge_dither: f32,
        block_jitter: f32,
        bias: f32,
        seed: u64,
        #[serde(default)]
        advect_source: String,
        advect_amount: f32,
        refresh: f32,
        turbulence: f32,
        smear: f32,
        smear_decay: f32,
        #[serde(default)]
        max_frames: Option<u32>,
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated). Envelope times are
        /// sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    FrameSequenceDispersionBlend {
        source_a_directory: String,
        source_b_directory: String,
        output_directory: String,
        frame_rate: f64,
        block_size: u32,
        color_weight: f32,
        texture_weight: f32,
        coagulation_strength: f32,
        randomness: f32,
        coherence_passes: u32,
        coherence_strength: f32,
        bias: f32,
        ownership_refresh: f32,
        coherent_amount: f32,
        scatter_amount: f32,
        damping: f32,
        dispersion_ramp: u32,
        smear: f32,
        smear_decay: f32,
        seed: u64,
        #[serde(default)]
        max_frames: Option<u32>,
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    FrameSequenceFluidMosaic {
        source_a_directory: String,
        source_b_directory: String,
        output_directory: String,
        frame_rate: f64,
        frames: u32,
        tile_size: u32,
        color_bins: u32,
        cohesion: f32,
        cohesion_radius: f32,
        repulsion: f32,
        repulsion_radius: f32,
        fluid_strength: f32,
        fluid_scale: f32,
        fluid_drift: f32,
        damping: f32,
        settle_iterations: u32,
        jitter: f32,
        turbulence: f32,
        turbulence_scale: f32,
        turbulence_speed: f32,
        vortex_flow: f32,
        vortex_scale: f32,
        seed: u64,
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    FrameSequenceFieldParticles {
        source_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        spacing: u32,
        particle_size: u32,
        advect: f32,
        turbulence_scale: f32,
        turbulence_speed: f32,
        detail: f32,
        #[serde(default)]
        live_color: bool,
        seed: u64,
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty). Envelope times sample against `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    FrameSequenceCascadeTrails {
        source_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        tile_size: u32,
        grid_spacing: u32,
        advect: f32,
        turbulence_scale: f32,
        detail: f32,
        #[serde(default)]
        live_refresh: bool,
        seed: u64,
        #[serde(default)]
        field: String,
        #[serde(default)]
        river_direction: f32,
        #[serde(default = "default_river_speed")]
        river_speed: f32,
        #[serde(default = "default_river_turbulence")]
        river_turbulence: f32,
        #[serde(default)]
        temporal_tiles: bool,
        #[serde(default)]
        decay: f32,
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    /// Scribbled-edge tile cascade — a procedural/textured collage of rect/L tiles,
    /// each re-stamped in an in-frame cascade with one scribbled morphing edge. Tiles
    /// carry a crop of the source video (texture + colour). Stateless single-frame
    /// composite (no cross-frame state), unlike `FrameSequenceCascadeTrails`.
    FrameSequenceCascadeCollage {
        source_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        #[serde(default = "default_cascade_collage_scrib_amp_scale")]
        scrib_amp_scale: f32,
        #[serde(default = "default_cascade_collage_morph_rate")]
        morph_rate: f32,
        #[serde(default)]
        frame_hue_rate: f32,
        #[serde(default = "default_cascade_collage_bright_osc")]
        bright_osc: f32,
        #[serde(default = "default_cascade_collage_edge_width")]
        edge_width: f32,
        #[serde(default = "default_cascade_collage_edge_strength")]
        edge_strength: f32,
        #[serde(default = "default_cascade_collage_face_strength")]
        face_strength: f32,
        #[serde(default = "default_cascade_collage_face_sat")]
        face_sat: f32,
        #[serde(default = "default_cascade_collage_hue_steps")]
        hue_steps: u32,
        #[serde(default)]
        edge_detect: f32,
        #[serde(default = "default_cascade_collage_tile_scale")]
        tile_scale: f32,
        #[serde(default = "default_cascade_collage_detail_tiles")]
        detail_tiles: u32,
        #[serde(default)]
        hue_rotate: f32,
        #[serde(default)]
        block_blend: String,
        #[serde(default = "default_cascade_collage_block_opacity")]
        block_opacity: f32,
        #[serde(default)]
        seed: u64,
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    /// Retro static — deliberate scanline-filter misread glitch: simulate a
    /// PNG-style adaptive filter, then deliberately decode it at the wrong
    /// bytes-per-pixel stride. Stateless single-source, integer-domain (CPU/Metal
    /// bit-identical).
    FrameSequenceRetroStatic {
        source_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        #[serde(default = "default_retro_static_real_bpp")]
        real_bpp: u32,
        #[serde(default = "default_retro_static_assumed_bpp")]
        assumed_bpp: u32,
        #[serde(default = "default_retro_static_filter")]
        filter: String,
        #[serde(default = "default_retro_static_strength")]
        strength: f32,
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty and keep their meaning). Envelope times are
        /// sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    /// Channel shift (RGB split / chromatic aberration): each colour channel is
    /// sampled from the carrier at an independently offset position. Optional
    /// A-flow mode adds per-row X shifts from Source A's optical flow (CPU-only).
    FrameSequenceChannelShift {
        carrier_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        #[serde(default)]
        shift_r_x: f32,
        #[serde(default)]
        shift_r_y: f32,
        #[serde(default)]
        shift_g_x: f32,
        #[serde(default)]
        shift_g_y: f32,
        #[serde(default)]
        shift_b_x: f32,
        #[serde(default)]
        shift_b_y: f32,
        /// Source A frames for the flow-driven per-row shift mode; `None` =
        /// constant offsets only. Distinct from `modulator_frames_directory`,
        /// which feeds the modulation-matrix luma/flow envelopes.
        #[serde(default)]
        flow_source_frame_directory: Option<String>,
        /// Per-row shift gain over A's mean row X-flow. `0` = flow mode off.
        #[serde(default)]
        flow_gain: f32,
        /// Lucas-Kanade window radius for the flow-driven mode.
        #[serde(default = "default_channel_shift_flow_radius")]
        flow_radius: i32,
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated). Envelope times
        /// are sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
        /// Spatial matte source label (Tier 5.4 S2): `a-luma` / `a-flow` /
        /// `a-edge`. `None` = no matte (pre-slice queue JSON deserializes to
        /// `None`, matching pre-slice behaviour exactly).
        #[serde(default)]
        matte_source: Option<String>,
        /// Matte-media frame directory. Defaults to the command's Source A
        /// directory at run time when unset (mirrors the direct CLI default).
        #[serde(default)]
        matte_frames: Option<String>,
        /// Matte gain (defaults to `1.0` at run time when `matte_source` is set).
        #[serde(default)]
        matte_gain: Option<f32>,
    },
    /// Palette quantize / posterize: collapse the carrier's colours to discrete
    /// per-channel levels (posterize) or the built-in neon palette. Stateless
    /// single-source, integer-domain (CPU/Metal bit-identical).
    FrameSequencePaletteQuantize {
        carrier_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        /// Quantize mode label: `posterize` or `palette`.
        #[serde(default = "default_palette_quantize_mode")]
        mode: String,
        /// Discrete steps per channel for posterize mode (2–256; 256 =
        /// byte-identical passthrough).
        #[serde(default = "default_palette_quantize_levels")]
        levels: u32,
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty and keep their meaning). Envelope times are
        /// sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
        /// Spatial matte source label (Tier 5.4 S2): `a-luma` / `a-flow` /
        /// `a-edge`. `None` = no matte (pre-slice queue JSON deserializes to
        /// `None`, matching pre-slice behaviour exactly).
        #[serde(default)]
        matte_source: Option<String>,
        /// Matte-media frame directory. Defaults to the command's Source A
        /// directory at run time when unset (mirrors the direct CLI default).
        #[serde(default)]
        matte_frames: Option<String>,
        /// Matte gain (defaults to `1.0` at run time when `matte_source` is set).
        #[serde(default)]
        matte_gain: Option<f32>,
    },
    /// Rutt-Etra scanline: re-render the carrier as sparse horizontal
    /// scanlines on black, each displaced vertically by its own luminance.
    /// Stateless single-source; the Metal gather kernel is parity-gated
    /// per-frame against the CPU reference.
    FrameSequenceRuttEtra {
        carrier_frame_directory: String,
        output_directory: String,
        /// Optional Source A (modulator) frame directory. When present, A's luma
        /// drives the displacement (two-source cross-synthesis) and B supplies
        /// the colour; when absent, B displaces its own scanlines. Skip-when-none
        /// so pre-two-source queue JSON stays byte-identical.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source_a_directory: Option<String>,
        frames: u32,
        frame_rate: f64,
        /// Rows between scanlines (top row always included).
        #[serde(default = "default_rutt_etra_line_pitch")]
        line_pitch: u32,
        /// Vertical displacement in px at luma 1.0; sign sets direction.
        #[serde(default = "default_rutt_etra_displacement_depth")]
        displacement_depth: f32,
        /// Each filled cell extends downward by this many px.
        #[serde(default = "default_rutt_etra_line_thickness")]
        line_thickness: u32,
        /// White lines instead of source colour.
        #[serde(default)]
        mono: bool,
        /// Render backend; `#[serde(default)]` so pre-Metal queue JSON loads
        /// as CPU without breaking existing jobs.
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated). Envelope times
        /// are sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
        /// Spatial matte source label (Tier 5.4 S2): `a-luma` / `a-flow` /
        /// `a-edge`. `None` = no matte (pre-slice queue JSON deserializes to
        /// `None`, matching pre-slice behaviour exactly).
        #[serde(default)]
        matte_source: Option<String>,
        /// Matte-media frame directory. Defaults to the command's Source A
        /// directory at run time when unset (mirrors the direct CLI default).
        #[serde(default)]
        matte_frames: Option<String>,
        /// Matte gain (defaults to `1.0` at run time when `matte_source` is set).
        #[serde(default)]
        matte_gain: Option<f32>,
    },
    /// An effect chain run from a resolved chain-spec document
    /// (`docs/EFFECT_CHAIN_MILESTONE.md`). The spec is persisted verbatim as
    /// JSON rather than mirrored into typed core fields: the spec is already
    /// the canonical, versioned, add-time-validated serialized form (owned by
    /// the CLI), and a typed mirror here would be a third copy of every
    /// effect's knob vocabulary to keep in sync.
    RenderChain {
        input_frame_directory: String,
        output_directory: String,
        spec: serde_json::Value,
    },
    /// A composition — an ordered list of scenes (each a chain over its own
    /// source) on a global timeline, from a resolved composition-spec document
    /// (`docs/COMPOSITION_MILESTONE.md`). Like `RenderChain`, the spec is
    /// persisted verbatim as JSON rather than mirrored into typed core fields
    /// (it is the canonical, versioned, add-time-validated form owned by the
    /// CLI). Sources are per-scene inside the spec, so there is no top-level
    /// input directory.
    RenderComposition {
        output_directory: String,
        spec: serde_json::Value,
    },
    /// Hard binary tile collage: each NxN block independently shows Source A or
    /// Source B based on a spatially-coherent value-noise ownership field.
    /// No blending — hard cuts at every tile boundary.
    FrameSequenceBlockCollage {
        modulator_frame_directory: String,
        carrier_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        #[serde(default = "default_block_collage_tile_size")]
        tile_size: u32,
        #[serde(default = "default_block_collage_threshold")]
        threshold: f32,
        #[serde(default = "default_block_collage_cluster_scale")]
        cluster_scale: f32,
        #[serde(default)]
        evolution_speed: f32,
        #[serde(default)]
        seed: u64,
    },
    /// Threshold-bounded pixel sort. Source B's pixels are sorted within contiguous
    /// runs where the sortability mask (B's own key or a cross-synth A-derived mask)
    /// falls in [`threshold_low`, `threshold_high`].
    FrameSequencePixelSort {
        modulator_frame_directory: String,
        carrier_frame_directory: String,
        output_directory: String,
        frames: u32,
        frame_rate: f64,
        #[serde(default)]
        axis: PixelSortAxis,
        #[serde(default)]
        key: PixelSortKey,
        #[serde(default)]
        direction: PixelSortDirection,
        #[serde(default = "default_pixel_sort_threshold_low")]
        threshold_low: f32,
        #[serde(default = "default_pixel_sort_threshold_high")]
        threshold_high: f32,
        /// Maximum streak length in pixels; 0 = unbounded.
        #[serde(default)]
        max_span: u32,
        #[serde(default)]
        mask_source: PixelSortMaskSource,
        /// Lucas-Kanade window radius for the `a-flow` mask mode.
        #[serde(default)]
        flow_radius: i32,
        /// Render backend. Metal is self-mask only; cross-synth modes are CPU-only.
        #[serde(default)]
        backend: RenderBackend,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty and keep their meaning). Envelope times are
        /// sampled against this job's `frame_rate`.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    FrameSequenceGranularMosaic {
        modulator_frame_directory: String,
        carrier_frame_directory: String,
        output_directory: String,
        grain_cache_directory: Option<String>,
        grain_size: u32,
        rearrangement: f32,
        variation: f32,
        seed: u64,
        max_frames: Option<u32>,
        frame_rate: f64,
        #[serde(default)]
        backend: RenderBackend,
        #[serde(default)]
        audio_modulation: Option<GranularAudioModulation>,
        /// Grain-matching feature space. Defaults to [`GrainSelectionMode::Luma`]
        /// so legacy jobs serialized before multimodal selection keep their
        /// original 1-D luminance matching.
        #[serde(default)]
        selection_mode: GrainSelectionMode,
    },
    /// Step 6b joint-AV path: grains are drawn from a whole-clip temporal pool and
    /// matched on a combined `[mean_color | audio]` vector
    /// (`pooled_av_nearest_grain_cpu_v1`). The cross-frame render has a
    /// parity-gated Metal port selected via `backend`.
    FrameSequenceGranularMosaicPool {
        modulator_frame_directory: String,
        carrier_frame_directory: String,
        output_directory: String,
        grain_cache_directory: Option<String>,
        grain_size: u32,
        rearrangement: f32,
        variation: f32,
        seed: u64,
        /// Scales every audio dimension in the selection distance.
        audio_weight: f32,
        /// Scales both texture dims (luma variance + gradient magnitude); `0` = off.
        #[serde(default)]
        texture_weight: f32,
        /// RMS cache for Source A; supplies the per-output-frame query audio.
        #[serde(default)]
        modulator_rms_cache: Option<String>,
        /// RMS cache for Source B; supplies each pool grain's carrier audio.
        #[serde(default)]
        carrier_rms_cache: Option<String>,
        /// STFT cache for Source A; adds a spectral-centroid (k=2) query dimension.
        #[serde(default)]
        modulator_centroid_cache: Option<String>,
        /// STFT cache for Source B; adds each pool grain's spectral-centroid dim.
        #[serde(default)]
        carrier_centroid_cache: Option<String>,
        /// Trailing pool window (last N carrier frames); `0` = whole-clip pool.
        #[serde(default)]
        pool_window: u32,
        /// Anti-repeat weight (penalizes recently-used grains); `0` = off.
        #[serde(default)]
        anti_repeat_weight: f32,
        /// Anti-repeat cooldown frames over which the penalty decays to zero.
        #[serde(default)]
        anti_repeat_cooldown: u32,
        /// Temporal-coherence weight (rewards source-frame continuity); `0` = off.
        #[serde(default)]
        coherence_weight: f32,
        /// Frame distance over which the coherence penalty saturates.
        #[serde(default)]
        coherence_reach: u32,
        /// Spatial-origin coherence weight (rewards grain-origin continuity within
        /// a frame, sharing `coherence_reach`); `0` = off.
        #[serde(default)]
        spatial_coherence_weight: f32,
        max_frames: Option<u32>,
        frame_rate: f64,
        /// Render backend; the Metal path is gated per-frame against the CPU
        /// reference. Defaults to CPU so legacy jobs keep their meaning.
        #[serde(default)]
        backend: RenderBackend,
    },
    /// Video vocoder: Source A's per-frame luma distribution reweights Source B's
    /// tonal bands. `match` mode (default) remaps B's luma onto A's via histogram
    /// specification; `gain` mode applies a per-band gain envelope. The `match`
    /// render has a parity-gated Metal port selected via `backend`.
    FrameSequenceVideoVocoder {
        modulator_frame_directory: String,
        carrier_frame_directory: String,
        output_directory: String,
        /// Luma band count (`gain` mode only).
        bands: u32,
        /// Blend from Source B passthrough (`0`) to full routing (`1`).
        amount: f32,
        /// Tonal-routing mode. Defaults to [`VideoVocoderMode::Match`].
        #[serde(default)]
        mode: VideoVocoderMode,
        max_frames: Option<u32>,
        frame_rate: f64,
        /// Render backend; the Metal path (match mode) is gated per-frame against
        /// the CPU reference. Defaults to CPU so legacy jobs keep their meaning.
        #[serde(default)]
        backend: RenderBackend,
    },
    /// Audio-to-video descriptor routing: Source A's peak-normalized RMS envelope
    /// drives the per-frame displacement amount applied to Source B's frames via
    /// the parity-gated flow displace. See `docs/AUDIO_VIDEO_ROUTE_MILESTONE.md`.
    FrameSequenceAudioVideoRoute {
        /// Source A audio (WAV); its RMS envelope is the modulator.
        modulator_wav: String,
        /// Source B video frames (PNG sequence) to displace.
        carrier_frame_directory: String,
        output_directory: String,
        /// Global displacement scale; multiplies the normalized RMS gain
        /// (`0` = Source B passthrough).
        amount: f32,
        /// Uniform displacement field x/y components in pixels at full amount.
        shift_x: f32,
        shift_y: f32,
        /// RMS analysis window / hop (samples) for Source A.
        rms_window: u32,
        rms_hop: u32,
        /// Output frame rate; maps frame index → time for the envelope lookup.
        frame_rate: f64,
        max_frames: Option<u32>,
        /// Render backend; the displace Metal path is gated per-frame against the
        /// CPU reference. Defaults to CPU so legacy jobs keep their meaning.
        #[serde(default)]
        backend: RenderBackend,
    },
    /// Controlled datamosh (flow-reuse "bloom/melt"): Source A's per-frame optical
    /// flow repeatedly advects Source B's previous output; keyframes snap back to
    /// the carrier. See `docs/DATAMOSH_MILESTONE.md`.
    FrameSequenceDatamosh {
        /// Source A video frames (PNG sequence); supplies the per-frame motion.
        modulator_frame_directory: String,
        /// Source B video frames (PNG sequence) to mosh.
        carrier_frame_directory: String,
        output_directory: String,
        /// Keyframe ("keep") interval: `1` = passthrough (snap to B every frame),
        /// `N` = snap every N frames, `0` = only frame 0 (full melt from B[0]).
        keyframe_interval: u32,
        /// Per-step scale on A's flow; `0` freezes the held frame.
        amount: f32,
        max_frames: Option<u32>,
        /// Render backend; the displace Metal path is gated per-frame against the
        /// CPU reference. Defaults to CPU so legacy jobs keep their meaning.
        #[serde(default)]
        backend: RenderBackend,
        /// Macroblock size (codec-simulated mosh): `0`/`1` = smooth bloom, `N >= 2`
        /// quantizes A's flow to NxN blocks before advection. Defaults to `0` so
        /// legacy jobs (no field) keep the smooth bloom meaning.
        #[serde(default)]
        block_size: u32,
        /// Block-residual gain: re-inject the intra-block motion discarded by
        /// quantization. `0` = block path (no residual); needs `block_size >= 2`.
        /// Defaults to `0` so legacy jobs keep their meaning.
        #[serde(default)]
        residual_gain: f32,
        /// Decay on the per-pixel residual accumulator: `0` = one-frame kick,
        /// `->1` = long-lived drift. Irrelevant when `residual_gain == 0`.
        #[serde(default)]
        residual_decay: f32,
        /// Per-block keep/drop threshold: macroblocks whose mean motion magnitude is
        /// below this snap back to the carrier (intra-block refresh) while busier
        /// blocks rot. `0` = no per-block refresh; needs `block_size >= 2`. Defaults
        /// to `0` so legacy jobs keep their meaning.
        #[serde(default)]
        block_refresh_threshold: f32,
        /// FFglitch-style motion-vector remix on the block-MV grid (needs
        /// `block_size >= 2`). Defaults to [`VectorRemixMode::None`] so legacy jobs
        /// keep the block/residual/refresh meaning.
        #[serde(default)]
        vector_remix: VectorRemixMode,
        /// Seed for `vector_remix == Shuffle` (deterministic permutation). Defaults
        /// to `0`.
        #[serde(default)]
        remix_seed: u64,
        /// Named destructive preset. `Custom` keeps the explicit knobs above.
        /// Presets resolve to deterministic knob sets at render time.
        #[serde(default)]
        preset: DatamoshPreset,
        /// Optional reusable temporal optical-flow cache root. Each P-frame stores
        /// one `frame_XXXXXX/manifest.json` + `frame_000000.flowf32` sidecar.
        #[serde(default)]
        flow_cache_directory: Option<String>,
        /// Persisted modulation routes (empty = unmodulated; pre-slice jobs
        /// deserialize to empty and keep their meaning). Envelope times are
        /// sampled against this job's `frame_rate`. Stateful: the routes join
        /// the render's checkpoint contract.
        #[serde(default)]
        modulation_routes: Vec<RenderJobModulationRoute>,
        #[serde(default)]
        modulator_audio_path: Option<String>,
        /// Modulator frames for the luma/flow envelopes. Distinct from
        /// `modulator_frame_directory`, which is the effect's Source A.
        #[serde(default)]
        modulator_frames_directory: Option<String>,
        #[serde(default)]
        modulation_sampling: ModulationSampling,
        /// Named-modulator media referenced by routes' `<name>.` prefix
        /// (empty = no named routes; pre-slice jobs deserialize to empty).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_audio: Vec<NamedModulatorMedia>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_frames: Vec<NamedModulatorMedia>,
        #[serde(default)]
        modulator_midi_path: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        named_modulator_midi: Vec<NamedModulatorMedia>,
    },
    /// Real bitstream datamosh via AVI chunk surgery: ffmpeg encodes to MPEG-4,
    /// pure-Rust RIFF surgery duplicates/removes/splices chunks, ffmpeg decodes to
    /// PNG. Non-deterministic by design (depends on ffmpeg codec version).
    DatamoshBitstream {
        /// Input video file (any ffmpeg-decodable container). For pframe-duplicate /
        /// remove-keyframe this is the clip to mosh; for motion-transfer it is the
        /// MODULATOR (Source A, the motion donor).
        input_video: String,
        output_directory: String,
        /// Frame rate to encode/decode at.
        fps: f64,
        /// Which bitstream operation to perform.
        #[serde(default)]
        operation: DatamoshBitstreamOperation,
        /// Which P-frame to bloom (0-based among P-frames). Relevant for pframe-duplicate.
        #[serde(default)]
        p_frame_index: u32,
        /// Extra copies of that P-frame to insert. Relevant for pframe-duplicate.
        #[serde(default)]
        duplicate_count: u32,
        /// motion-transfer only: the CARRIER (Source B) video whose appearance is kept.
        #[serde(default)]
        carrier_video: Option<String>,
        /// motion-transfer only: leading carrier frames kept before modulator motion.
        #[serde(default = "default_carrier_keyframes")]
        carrier_keyframes: u32,
        /// Named bitstream preset. Custom keeps the explicit knobs above.
        #[serde(default)]
        preset: DatamoshBitstreamPreset,
    },
    /// Convolutional AV blending (image kernel): each Source A frame supplies a
    /// normalized KxK luma kernel that Source B's matching frame is convolved with
    /// (parity-gated), blended by `amount`. See `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.
    FrameSequenceConvolutionBlend {
        /// Source A video frames (PNG sequence); each supplies the kernel.
        modulator_frame_directory: String,
        /// Source B video frames (PNG sequence) to convolve.
        carrier_frame_directory: String,
        output_directory: String,
        /// Kernel edge length (odd, >= 1).
        kernel_size: u32,
        /// Wet/dry blend from Source B passthrough (`0`) to fully convolved (`1`).
        amount: f32,
        max_frames: Option<u32>,
        /// Render backend; the convolution Metal path is gated per-frame against
        /// the CPU reference. Defaults to CPU so legacy jobs keep their meaning.
        #[serde(default)]
        backend: RenderBackend,
        /// Kernel extraction: one luma kernel (default) or a per-channel colour
        /// kernel from each of A's R/G/B channels. Defaults to
        /// [`KernelMode::Luma`] so jobs serialized before colour mode keep meaning.
        #[serde(default)]
        kernel_mode: KernelMode,
    },
    /// Spectral audio cross-synthesis: Source A's analysis envelope shapes Source
    /// B's audio. `gain` scales B's amplitude by A's peak-normalized RMS envelope;
    /// `filter` sweeps a one-pole filter on B from A's spectral-centroid envelope.
    /// Time-domain MVP (CPU-only; the STFT is magnitude-only so there is no Metal
    /// path and nothing to parity-gate).
    AudioSpectralCrossSynth {
        modulator_wav: String,
        carrier_wav: String,
        output_directory: String,
        /// `gain` or `filter`. Defaults to [`CrossSynthMode::Gain`].
        #[serde(default)]
        mode: CrossSynthMode,
        /// Blend from Source B passthrough (`0`) to full shaping (`1`).
        amount: f32,
        /// One-pole filter response (`filter` mode). Defaults to
        /// [`CrossSynthFilterType::Lowpass`].
        #[serde(default)]
        filter_type: CrossSynthFilterType,
        /// RMS analysis window/hop for A's envelope (`gain` mode).
        rms_window: u32,
        rms_hop: u32,
        /// STFT analysis parameters for A's centroid envelope (`filter` mode).
        fft_size: u32,
        stft_hop: u32,
        /// STFT window function (`filter` mode). Defaults to
        /// [`CrossSynthWindow::Hann`].
        #[serde(default)]
        window: CrossSynthWindow,
        /// Log-band count for A's spectral envelope (`vocode` mode). Defaults
        /// to 32 so jobs serialized before the vocode tier deserialize cleanly.
        #[serde(default = "default_cross_synth_vocode_bands")]
        vocode_bands: u32,
    },
    /// Audio impulse convolution: Source B (carrier) convolved with Source A's
    /// L1-normalized mono impulse response, blended wet/dry by `amount`
    /// (convolution-reverb-style). CPU-only — no Metal path to parity-gate.
    AudioImpulseConvolution {
        modulator_wav: String,
        carrier_wav: String,
        output_directory: String,
        /// Blend from Source B passthrough (`0`) to full wet (`1`).
        amount: f32,
        /// Optional head-truncation of the impulse response (samples).
        #[serde(default)]
        max_impulse_samples: Option<u32>,
        /// Convolution implementation. Defaults to [`ConvolutionMethod::Direct`].
        #[serde(default)]
        method: ConvolutionMethod,
        /// Resample A's IR to B's sample rate (Lanczos) instead of erroring on a
        /// rate mismatch. Defaults to `false`.
        #[serde(default)]
        resample_impulse: bool,
        /// IR channel mapping: one mono downmix IR (default) or a per-channel
        /// true-stereo IR from each Source A channel. Defaults to
        /// [`IrMode::Mono`] so jobs serialized before this keep their meaning.
        #[serde(default)]
        ir_mode: IrMode,
    },
    /// Video-to-Audio Descriptor Routing: a per-frame Source A visual descriptor
    /// envelope drives Source B's audio amplitude (`gain`) or stereo position
    /// (`pan`). CPU-only — no Metal path to parity-gate.
    VideoAudioRoute {
        /// Source A video frames (PNG sequence); each frame's descriptor
        /// (mean luma or optical-flow magnitude) is the modulator signal.
        modulator_directory: String,
        /// Source B audio (WAV) to shape.
        carrier_wav: String,
        output_directory: String,
        /// Which Source A visual descriptor drives the envelope. Defaults to
        /// [`VideoAudioRouteDescriptor::Luma`] so jobs serialized before
        /// optical-flow descriptors keep their mean-luma meaning.
        #[serde(default)]
        descriptor: VideoAudioRouteDescriptor,
        /// `gain`, `pan`, or `filter`. Defaults to [`VideoAudioRouteMode::Gain`].
        #[serde(default)]
        mode: VideoAudioRouteMode,
        /// Filter response for `filter` mode (ignored otherwise). Defaults to
        /// [`VideoAudioRouteFilterType::Lowpass`].
        #[serde(default)]
        filter_type: VideoAudioRouteFilterType,
        /// How the descriptor envelope is resampled onto B's audio grid.
        /// Defaults to [`VideoAudioRouteSampling::Hold`].
        #[serde(default)]
        sampling: VideoAudioRouteSampling,
        /// Blend from Source B passthrough (`0`) to full routing (`1`).
        amount: f32,
        /// Frame rate mapping A's frame index to time for the descriptor lookup.
        fps: f64,
    },
}

/// Selects how Source A's impulse response is mapped onto the carrier channels.
/// The serde default is [`IrMode::Mono`] (one downmixed IR for all channels).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IrMode {
    /// One downmixed mono IR applied to every carrier channel
    /// (`impulse_response_convolution_blend_cpu_v1`).
    #[default]
    Mono,
    /// One IR per Source A channel, applied channel-wise (true-stereo)
    /// (`per_channel_impulse_response_convolution_blend_cpu_v1`).
    PerChannel,
}

/// Selects the audio-impulse convolution implementation. The serde default is
/// [`ConvolutionMethod::Direct`] (the reference path) so jobs serialized before
/// the FFT HQ tier keep their direct-convolution meaning.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConvolutionMethod {
    /// Direct time-domain convolution (`O(B·L)`).
    #[default]
    Direct,
    /// Frequency-domain convolution via FFT (`O(N log N)`), gated against direct.
    Fft,
}

/// Selects the convolution-blend kernel extraction. The serde default is
/// [`KernelMode::Luma`] (one luminance kernel applied to all channels) so jobs
/// serialized before colour mode keep their meaning.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KernelMode {
    /// One luma-derived K×K kernel applied to every carrier channel
    /// (`image_kernel_convolution_blend_cpu_v1`).
    #[default]
    Luma,
    /// A separate K×K kernel from each of A's R/G/B channels, applied channel-wise
    /// (`image_color_kernel_convolution_blend_cpu_v1`).
    Color,
}

/// Selects the spectral cross-synth descriptor→target mapping.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossSynthMode {
    /// A's peak-normalized RMS envelope scales B's amplitude
    /// (`rms_gain_cross_synth_cpu_v1`).
    #[default]
    Gain,
    /// A's spectral-centroid envelope sweeps a one-pole filter on B
    /// (`centroid_filter_cross_synth_cpu_v1`).
    Filter,
    /// A's log-band spectral envelope reweights B's complex spectrum through
    /// a real inverse STFT (`phase_vocoder_cross_synth_cpu_v1`).
    Vocode,
}

fn default_cross_synth_vocode_bands() -> u32 {
    32
}

/// Mode for Video-to-Audio Descriptor Routing. The serde default is
/// [`VideoAudioRouteMode::Gain`].
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoAudioRouteMode {
    /// A's peak-normalized per-frame descriptor envelope scales B's amplitude.
    #[default]
    Gain,
    /// A's per-frame descriptor drives an equal-power stereo pan of B.
    Pan,
    /// A's per-frame descriptor sweeps a one-pole LP/HP filter cutoff on B.
    Filter,
}

/// Which Source A visual descriptor drives Video-to-Audio routing. The serde
/// default is [`VideoAudioRouteDescriptor::Luma`].
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoAudioRouteDescriptor {
    /// Per-frame mean Rec.709 luma (brightness).
    #[default]
    Luma,
    /// Per-frame mean optical-flow magnitude (motion), from the temporal
    /// Lucas-Kanade estimator (frame zero has no motion ⇒ `0`).
    Flow,
}

/// One-pole filter response for `filter`-mode Video-to-Audio routing. The serde
/// default is [`VideoAudioRouteFilterType::Lowpass`].
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoAudioRouteFilterType {
    #[default]
    Lowpass,
    Highpass,
}

/// How the per-frame descriptor envelope is resampled onto B's audio grid. The
/// serde default is [`VideoAudioRouteSampling::Hold`] (step) so jobs serialized
/// before time-resampled curves keep their hold-last meaning.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoAudioRouteSampling {
    /// Step: hold each frame's value until the next frame.
    #[default]
    Hold,
    /// Linearly interpolate between frames (a smooth curve).
    Smooth,
}

/// Composes the deterministic algorithm id for Video-to-Audio routing from the
/// visual descriptor and the audio mapping, following the project's
/// `{descriptor}_{mapping}_route_cpu_v1` convention. The audio routing math is
/// descriptor-neutral; the id records which visual signal drove it.
pub fn video_audio_route_algorithm_id(
    descriptor: VideoAudioRouteDescriptor,
    mode: VideoAudioRouteMode,
) -> &'static str {
    match (descriptor, mode) {
        (VideoAudioRouteDescriptor::Luma, VideoAudioRouteMode::Gain) => "luma_gain_route_cpu_v1",
        (VideoAudioRouteDescriptor::Luma, VideoAudioRouteMode::Pan) => "luma_pan_route_cpu_v1",
        (VideoAudioRouteDescriptor::Luma, VideoAudioRouteMode::Filter) => {
            "luma_filter_route_cpu_v1"
        }
        (VideoAudioRouteDescriptor::Flow, VideoAudioRouteMode::Gain) => "flow_gain_route_cpu_v1",
        (VideoAudioRouteDescriptor::Flow, VideoAudioRouteMode::Pan) => "flow_pan_route_cpu_v1",
        (VideoAudioRouteDescriptor::Flow, VideoAudioRouteMode::Filter) => {
            "flow_filter_route_cpu_v1"
        }
    }
}

/// One-pole filter response for `filter`-mode cross-synth.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossSynthFilterType {
    #[default]
    Lowpass,
    Highpass,
}

/// STFT window function for `filter`-mode cross-synth analysis. Mirrors the audio
/// crate's window set; lives in core so a persisted job is self-contained.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CrossSynthWindow {
    #[default]
    Hann,
    Hamming,
    Rectangular,
}

/// Selects the video-vocoder tonal-routing mode.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoVocoderMode {
    /// Histogram specification: remap B's luma distribution onto A's
    /// (`luma_histogram_spec_vocoder_cpu_v1`). The default headline look.
    #[default]
    Match,
    /// Per-band gain routing: A's luma histogram scales B's tonal bands
    /// (`luma_band_gain_vocoder_cpu_v1`).
    Gain,
}

/// Selects the feature space used to match Source A regions to Source B grains.
///
/// The serde default is [`GrainSelectionMode::Luma`] so granular jobs serialized
/// before step 6 keep their original 1-D luminance matching.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GrainSelectionMode {
    /// 1-D nearest neighbor on mean luminance (`luma_nearest_grain_cpu_v1`).
    #[default]
    Luma,
    /// Multimodal nearest neighbor on mean RGB (`multimodal_nearest_grain_cpu_v1`).
    MultimodalRgb,
}

/// Cache-backed Source A audio controls for a granular-mosaic sequence. Each
/// cache is sampled at the output frame time, preserving deterministic offline
/// routing independently of realtime audio playback.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GranularAudioModulation {
    pub rms_cache_path: Option<String>,
    pub onset_cache_path: Option<String>,
    pub stft_cache_path: Option<String>,
    pub rms_variation_scale: f32,
    pub onset_rearrangement_scale: f32,
    pub centroid_grain_size_scale: f32,
}

/// Selects the vector field that drives flow displacement and feedback.
///
/// The serde default is [`FlowSource::Luminance`] so legacy feedback jobs that
/// were serialized before optical flow existed keep their original meaning.
/// New jobs default to [`FlowSource::OpticalFlow`] at the CLI layer.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlowSource {
    /// Single-frame luminance-gradient field.
    #[default]
    Luminance,
    /// Temporal Lucas-Kanade optical flow between consecutive modulator frames.
    OpticalFlow,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderBackend {
    #[default]
    Cpu,
    /// Render on the Metal compute backend, gated by a per-frame CPU parity check.
    Metal,
}

/// FFglitch-style motion-vector remix on the per-block MV grid (datamosh). The
/// schema mirror of the render crate's remix enum; the CLI maps between them.
/// Defaults to [`VectorRemixMode::None`] so jobs serialized before this field keep
/// the block/residual/refresh meaning.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VectorRemixMode {
    /// No remix — the block-quantized flow is used unchanged (off path).
    #[default]
    None,
    /// Reassign block MVs by descending magnitude (motion pools coherently).
    Sort,
    /// Deterministic seeded permutation of block MVs (motion scrambles).
    Shuffle,
}

/// Sort direction along the sort axis.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PixelSortAxis {
    #[default]
    Row,
    Col,
}

/// Component used to rank pixels within a sortable span.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PixelSortKey {
    #[default]
    Luma,
    Hue,
    Sat,
    Red,
    Green,
    Blue,
}

/// Whether pixels are sorted low→high or high→low within each span.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PixelSortDirection {
    #[default]
    Asc,
    Desc,
}

/// What drives the per-pixel sortability mask.
/// Mirrors [`morphogen_render::MaskSource`] for queue-job serialisation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PixelSortMaskSource {
    #[default]
    #[serde(rename = "self")]
    SelfMask,
    ALuma,
    AEdge,
    AFlow,
}

/// An LFO waveform shape on a persisted modulation route.
/// Mirrors `morphogen_render::LfoShape` for queue-job serialisation.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LfoShape {
    Sine,
    Triangle,
    Square,
    Saw,
}

impl LfoShape {
    /// The CLI route-grammar spelling (`sine`, `triangle`, `square`, `saw`).
    pub fn name(self) -> &'static str {
        match self {
            LfoShape::Sine => "sine",
            LfoShape::Triangle => "triangle",
            LfoShape::Square => "square",
            LfoShape::Saw => "saw",
        }
    }
}

/// Which analysis descriptor drives a persisted modulation route.
/// Mirrors `morphogen_render::ModulationSource` for queue-job serialisation.
///
/// `Lfo`'s `f32` fields force dropping `Eq`; `Breakpoints`' `Vec` forces
/// dropping `Copy`. `PartialEq` is kept; nothing in this crate requires `Eq`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum ModulationSource {
    AudioRms,
    AudioOnset,
    AudioCentroid,
    Luma,
    Flow,
    /// Peak-normalized mean Sobel gradient magnitude per frame (edge density).
    EdgeDensity,
    /// Controller `n`'s value from the modulator MIDI file (absolute /127).
    MidiCc(u8),
    /// Note-on velocity from the modulator MIDI file (absolute /127).
    MidiVelocity,
    /// Note-on count per sliding 1.0s window (peak-normalized).
    MidiNoteDensity,
    /// The most recent note-on's key (absolute /127).
    MidiPitch,
    /// Internal deterministic modulator — a pure function of
    /// `(frame_time, params)`; no media, no sidecar, no fingerprint.
    Lfo {
        shape: LfoShape,
        rate_hz: f32,
        phase: f32,
    },
    /// User-defined piecewise-linear envelope — inline knots, no media.
    Breakpoints {
        points: Vec<[f32; 2]>,
    },
    // ── Signal combinators ────────────────────────────────────────────────────
    Sum(Box<ModulationSource>, Box<ModulationSource>),
    Mul(Box<ModulationSource>, Box<ModulationSource>),
    Invert(Box<ModulationSource>),
    Min(Box<ModulationSource>, Box<ModulationSource>),
    Max(Box<ModulationSource>, Box<ModulationSource>),
    Gate {
        signal: Box<ModulationSource>,
        threshold: f32,
    },
}

impl ModulationSource {
    pub fn name(&self) -> &'static str {
        match self {
            ModulationSource::AudioRms => "audio-rms",
            ModulationSource::AudioOnset => "audio-onset",
            ModulationSource::AudioCentroid => "audio-centroid",
            ModulationSource::Luma => "luma",
            ModulationSource::Flow => "flow",
            ModulationSource::EdgeDensity => "edge-density",
            ModulationSource::MidiCc(_) => "midi-cc",
            ModulationSource::MidiVelocity => "midi-velocity",
            ModulationSource::MidiNoteDensity => "midi-note-density",
            ModulationSource::MidiPitch => "midi-pitch",
            ModulationSource::Lfo { .. } => "lfo",
            ModulationSource::Breakpoints { .. } => "breakpoints",
            ModulationSource::Sum(..) => "sum",
            ModulationSource::Mul(..) => "mul",
            ModulationSource::Invert(..) => "invert",
            ModulationSource::Min(..) => "min",
            ModulationSource::Max(..) => "max",
            ModulationSource::Gate { .. } => "gate",
        }
    }

    pub fn spec_text(&self) -> String {
        match self {
            ModulationSource::MidiCc(controller) => format!("midi-cc({controller})"),
            ModulationSource::Lfo { shape, rate_hz, phase } =>
                format!("lfo({},{},{})", shape.name(), rate_hz, phase),
            ModulationSource::Breakpoints { points } => {
                let pairs: Vec<String> = points
                    .iter()
                    .map(|[t, v]| format!("{}:{}", t, v))
                    .collect();
                format!("breakpoints({})", pairs.join(";"))
            }
            ModulationSource::Sum(a, b) =>
                format!("sum({},{})", a.spec_text(), b.spec_text()),
            ModulationSource::Mul(a, b) =>
                format!("mul({},{})", a.spec_text(), b.spec_text()),
            ModulationSource::Invert(x) =>
                format!("invert({})", x.spec_text()),
            ModulationSource::Min(a, b) =>
                format!("min({},{})", a.spec_text(), b.spec_text()),
            ModulationSource::Max(a, b) =>
                format!("max({},{})", a.spec_text(), b.spec_text()),
            ModulationSource::Gate { signal, threshold } =>
                format!("gate({},{})", signal.spec_text(), threshold),
            other => other.name().to_string(),
        }
    }
}

/// How a persisted modulation envelope is evaluated at each output frame.
/// Mirrors `morphogen_render::ModulationSampling`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModulationSampling {
    #[default]
    Hold,
    Smooth,
}

/// One flat modulation route persisted on a render job: the two-node degenerate
/// case of the node graph's [`ModulationRoute`](crate::graph::ModulationRoute)
/// (the modulator media is the implicit from-node, `source` its output, the
/// job's effect the implicit to-node, `target` its parameter, and `amount` is
/// generalized to `scale`/`offset`). See `docs/MODULATION_MATRIX_MILESTONE.md`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderJobModulationRoute {
    pub target: String,
    pub source: ModulationSource,
    #[serde(default = "default_modulation_scale")]
    pub scale: f32,
    #[serde(default)]
    pub offset: f32,
    /// Per-route sampling override; `None` inherits the job-level sampling.
    /// Skipped when unset so pre-slice jobs/manifests stay byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<ModulationSampling>,
    /// Named-modulator prefix; `None` reads the default modulator media.
    /// Skipped when unset so pre-slice jobs/manifests stay byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modulator: Option<String>,
}

/// One named modulator's media, persisted on a task alongside the default
/// `modulator_audio_path`/`modulator_frames_directory`. A route's `<name>.`
/// prefix (see [`RenderJobModulationRoute::modulator`]) resolves against the
/// matching-kind vector on the same task. See
/// `docs/MODULATION_MATRIX_MILESTONE.md` "Named modulators".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NamedModulatorMedia {
    pub name: String,
    pub path: String,
}

fn default_modulation_scale() -> f32 {
    1.0
}

fn default_channel_shift_flow_radius() -> i32 {
    4
}

fn default_palette_quantize_mode() -> String {
    "posterize".to_string()
}

fn default_palette_quantize_levels() -> u32 {
    256
}

fn default_rutt_etra_line_pitch() -> u32 {
    8
}

fn default_rutt_etra_displacement_depth() -> f32 {
    48.0
}

fn default_rutt_etra_line_thickness() -> u32 {
    1
}

/// Named deterministic datamosh presets for the flow-reuse path. Bitstream
/// operations (P-frame bloom, void mosh, motion transfer) have their own queue
/// job type [`DatamoshBitstream`](RenderJobTask::DatamoshBitstream) and preset
/// enum [`DatamoshBitstreamPreset`].
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatamoshPreset {
    /// Use the explicit render knobs without modification.
    #[default]
    Custom,
    /// Smooth recursive flow reuse: the foundational bloom/melt path.
    CodecBloom,
    /// Strong structured melt using block motion plus residual haze.
    StructuredMelt,
    /// Coarse macroblocks with residual haze and per-block refresh.
    MacroblockRot,
    /// Deterministic block-vector shuffle.
    VectorShuffle,
    /// Horizontal scanline tearing plus sparse chroma/black codec debris.
    ScanlineSmear,
    /// Edge-aware internal hatching, chroma offsets, and block stepping.
    CodecEngrave,
}

/// Bitstream datamosh operation type — controls the AVI chunk surgery performed.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatamoshBitstreamOperation {
    /// Duplicate a P-frame N times so the decoder re-applies its motion (bloom).
    #[default]
    PframeDuplicate,
    /// Remove the leading I-frame so the decoder starts from prediction data.
    RemoveKeyframe,
    /// Splice modulator (Source A) P-frame motion onto carrier (Source B) I-frame.
    MotionTransfer,
}

/// Named bitstream datamosh presets — resolve to an operation + knob set at queue time.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatamoshBitstreamPreset {
    /// Use the explicit knobs without modification.
    #[default]
    Custom,
    /// Gentle P-frame bloom: p_frame_index=0, duplicate_count=8.
    Bloom,
    /// Heavy P-frame melt: p_frame_index=0, duplicate_count=60.
    HeavyMelt,
    /// Remove the leading keyframe so the decoder hallucinates.
    VoidMosh,
    /// Motion transfer with 1 carrier keyframe (pure transfer).
    MotionGraft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderJobProvenance {
    pub sources: Vec<RenderJobSourceProvenance>,
    #[serde(default)]
    pub analysis_caches: Vec<RenderJobAnalysisCacheProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderJobSourceProvenance {
    pub source_id: String,
    pub role: SourceRole,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderJobAnalysisCacheProvenance {
    pub kind: AnalysisKind,
    pub path: String,
    pub producer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderJobOutputMetadata {
    pub output_directory: String,
    #[serde(default)]
    pub frame_paths: Vec<String>,
    #[serde(default)]
    pub audio_stem_paths: Vec<String>,
    pub timing: RenderTimingMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenderTimingMetadata {
    pub frame_rate: f64,
    pub frame_count: u32,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub sample_rate: u32,
    pub audio_sample_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderJobStatus {
    Queued,
    Running,
    Complete,
    Failed,
    Cancelled,
}

impl RenderJobStatus {
    /// A job in a terminal state will not be run and cannot be cancelled.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Failed | Self::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_chain_task_round_trips_with_embedded_spec_document() {
        let task = RenderJobTask::RenderChain {
            input_frame_directory: "/frames".to_string(),
            output_directory: "/out/job-0001".to_string(),
            spec: serde_json::json!({
                "version": 1,
                "stages": [{"effect": "rutt_etra", "line_pitch": 4}],
            }),
        };
        let json = serde_json::to_string(&task).expect("serialize chain task");
        assert!(json.starts_with(r#"{"type":"render_chain""#));
        let decoded: RenderJobTask = serde_json::from_str(&json).expect("decode chain task");
        assert_eq!(decoded, task);
    }

    #[test]
    fn modulation_source_unit_variants_serialize_as_bare_strings() {
        // Pre-slice queue JSON must stay byte-identical after the Lfo
        // variant lands: unit variants are still plain kebab-case strings.
        for (source, expected) in [
            (ModulationSource::AudioRms, "\"audio-rms\""),
            (ModulationSource::AudioOnset, "\"audio-onset\""),
            (ModulationSource::AudioCentroid, "\"audio-centroid\""),
            (ModulationSource::Luma, "\"luma\""),
            (ModulationSource::Flow, "\"flow\""),
        ] {
            assert_eq!(
                serde_json::to_string(&source).expect("serialize source"),
                expected
            );
        }
    }

    #[test]
    fn lfo_modulation_source_serializes_as_an_object_with_exact_literals() {
        // 0.5/0.25-style literals: exactly representable in f32, so the JSON
        // round-trip compares clean (the established f32 JSON trap rule).
        let source = ModulationSource::Lfo {
            shape: LfoShape::Saw,
            rate_hz: 0.5,
            phase: 0.25,
        };
        let json = serde_json::to_string(&source).expect("serialize lfo source");
        assert_eq!(
            json,
            r#"{"lfo":{"shape":"saw","rate_hz":0.5,"phase":0.25}}"#
        );
        let decoded: ModulationSource = serde_json::from_str(&json).expect("deserialize lfo");
        assert_eq!(decoded, source);
        assert_eq!(source.spec_text(), "lfo(saw,0.5,0.25)");
        assert_eq!(source.name(), "lfo");
    }

    #[test]
    fn frame_sequence_task_without_backend_field_defaults_to_cpu() {
        let json = r#"{
            "type": "frame_sequence_flow_displace",
            "modulator_frame_directory": "/tmp/mod",
            "carrier_frame_directory": "/tmp/car",
            "output_directory": "/tmp/out",
            "flow_cache_directory": null,
            "amount": 16.0,
            "max_frames": null,
            "frame_rate": 24.0
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize legacy task");
        let RenderJobTask::FrameSequenceFlowDisplace { backend, .. } = task else {
            panic!("expected frame-sequence task");
        };
        assert_eq!(backend, RenderBackend::Cpu);
    }

    #[test]
    fn rutt_etra_task_without_matte_fields_defaults_to_no_matte() {
        // Pre-Tier-5.4-S2 queue JSON (no matte_source/matte_frames/matte_gain
        // keys at all) must deserialize with matte off, byte-identical in
        // meaning to before the slice landed.
        let json = r#"{
            "type": "frame_sequence_rutt_etra",
            "carrier_frame_directory": "/tmp/car",
            "output_directory": "/tmp/out",
            "frames": 4,
            "frame_rate": 24.0
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize legacy task");
        let RenderJobTask::FrameSequenceRuttEtra {
            matte_source,
            matte_frames,
            matte_gain,
            ..
        } = task
        else {
            panic!("expected rutt-etra frame-sequence task");
        };
        assert_eq!(matte_source, None);
        assert_eq!(matte_frames, None);
        assert_eq!(matte_gain, None);
    }

    #[test]
    fn rutt_etra_task_with_matte_round_trips() {
        let task = RenderJobTask::FrameSequenceRuttEtra {
            carrier_frame_directory: "/tmp/car".to_string(),
            output_directory: "/tmp/out".to_string(),
            source_a_directory: Some("/tmp/a".to_string()),
            frames: 4,
            frame_rate: 24.0,
            line_pitch: 8,
            displacement_depth: 48.0,
            line_thickness: 1,
            mono: false,
            backend: RenderBackend::Cpu,
            modulation_routes: Vec::new(),
            modulator_audio_path: None,
            modulator_frames_directory: None,
            modulation_sampling: ModulationSampling::Hold,
            named_modulator_audio: Vec::new(),
            named_modulator_frames: Vec::new(),
            modulator_midi_path: None,
            named_modulator_midi: Vec::new(),
            matte_source: Some("a-luma".to_string()),
            matte_frames: Some("/tmp/a".to_string()),
            matte_gain: Some(0.5),
        };

        let json = serde_json::to_string(&task).expect("serialize matte task");
        let decoded: RenderJobTask = serde_json::from_str(&json).expect("deserialize matte task");
        assert_eq!(decoded, task);
    }

    #[test]
    fn feedback_task_serializes_temporal_parameters() {
        let task = RenderJobTask::FrameSequenceFlowFeedback {
            modulator_frame_directory: "/tmp/mod".to_string(),
            carrier_frame_directory: "/tmp/car".to_string(),
            output_directory: "/tmp/out".to_string(),
            flow_cache_directory: Some("/tmp/out/cache/flow".to_string()),
            carrier_amount: 12.0,
            feedback_amount: 24.0,
            feedback_mix: 0.72,
            decay: 0.995,
            iterations: 1,
            max_frames: Some(48),
            reset_at_frame: Some(12),
            frame_rate: 24.0,
            backend: RenderBackend::Cpu,
            flow_source: FlowSource::OpticalFlow,
            structure_mix: 0.6,
            modulation_routes: Vec::new(),
            modulator_audio_path: None,
            modulator_frames_directory: None,
            modulation_sampling: ModulationSampling::Hold,
            named_modulator_audio: Vec::new(),
            named_modulator_frames: Vec::new(),
            modulator_midi_path: None,
            named_modulator_midi: Vec::new(),
        };

        let json = serde_json::to_string(&task).expect("serialize feedback task");
        let decoded: RenderJobTask =
            serde_json::from_str(&json).expect("deserialize feedback task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn feedback_task_without_flow_source_defaults_to_luminance() {
        let json = r#"{
            "type": "frame_sequence_flow_feedback",
            "modulator_frame_directory": "/tmp/mod",
            "carrier_frame_directory": "/tmp/car",
            "output_directory": "/tmp/out",
            "flow_cache_directory": null,
            "carrier_amount": 12.0,
            "feedback_amount": 24.0,
            "feedback_mix": 0.72,
            "decay": 0.995,
            "iterations": 1,
            "max_frames": null,
            "frame_rate": 24.0
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize legacy task");
        let RenderJobTask::FrameSequenceFlowFeedback {
            flow_source,
            structure_mix,
            ..
        } = task
        else {
            panic!("expected feedback task");
        };
        assert_eq!(flow_source, FlowSource::Luminance);
        assert_eq!(structure_mix, 0.0);
    }

    #[test]
    fn fluid_advect_task_serializes_render_settings() {
        let task = RenderJobTask::FrameSequenceFluidAdvect {
            source_frame_directory: "/tmp/source".to_string(),
            output_directory: "/tmp/out".to_string(),
            frames: 48,
            frame_rate: 24.0,
            advect: 12.0,
            turbulence_scale: 0.008,
            turbulence_speed: 0.06,
            detail: 0.1,
            reinject: 0.05,
            seed: 42,
            backend: RenderBackend::Metal,
            modulation_routes: Vec::new(),
            modulator_audio_path: None,
            modulator_frames_directory: None,
            modulation_sampling: ModulationSampling::Hold,
            named_modulator_audio: Vec::new(),
            named_modulator_frames: Vec::new(),
            modulator_midi_path: None,
            named_modulator_midi: Vec::new(),
        };

        let json = serde_json::to_string(&task).expect("serialize fluid task");
        let decoded: RenderJobTask = serde_json::from_str(&json).expect("deserialize fluid task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn field_particles_task_without_backend_defaults_to_cpu() {
        let json = r#"{
            "type": "frame_sequence_field_particles",
            "source_frame_directory": "/tmp/source",
            "output_directory": "/tmp/out",
            "frames": 48,
            "frame_rate": 24.0,
            "spacing": 8,
            "particle_size": 8,
            "advect": 6.0,
            "turbulence_scale": 0.008,
            "turbulence_speed": 0.06,
            "detail": 0.1,
            "seed": 42
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize field task");
        let RenderJobTask::FrameSequenceFieldParticles {
            backend,
            live_color,
            ..
        } = task
        else {
            panic!("expected field-particles task");
        };
        assert_eq!(backend, RenderBackend::Cpu);
        assert!(!live_color);
    }

    #[test]
    fn granular_mosaic_task_serializes_render_settings() {
        let task = RenderJobTask::FrameSequenceGranularMosaic {
            modulator_frame_directory: "/tmp/mod".to_string(),
            carrier_frame_directory: "/tmp/car".to_string(),
            output_directory: "/tmp/out".to_string(),
            grain_cache_directory: Some("/tmp/out/cache/grains".to_string()),
            grain_size: 24,
            rearrangement: 1.0,
            variation: 0.35,
            seed: 42,
            max_frames: Some(48),
            frame_rate: 24.0,
            backend: RenderBackend::Metal,
            audio_modulation: Some(GranularAudioModulation {
                rms_cache_path: Some("/tmp/a-rms.json".to_string()),
                onset_cache_path: Some("/tmp/a-onsets.json".to_string()),
                stft_cache_path: Some("/tmp/a-stft.json".to_string()),
                rms_variation_scale: 0.6,
                onset_rearrangement_scale: 0.4,
                centroid_grain_size_scale: 12.0,
            }),
            selection_mode: GrainSelectionMode::MultimodalRgb,
        };

        let json = serde_json::to_string(&task).expect("serialize granular task");
        let decoded: RenderJobTask =
            serde_json::from_str(&json).expect("deserialize granular task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn granular_mosaic_pool_task_serializes_render_settings() {
        let task = RenderJobTask::FrameSequenceGranularMosaicPool {
            modulator_frame_directory: "/tmp/mod".to_string(),
            carrier_frame_directory: "/tmp/car".to_string(),
            output_directory: "/tmp/out".to_string(),
            grain_cache_directory: Some("/tmp/out/cache/pool".to_string()),
            grain_size: 16,
            rearrangement: 1.0,
            variation: 0.0,
            seed: 7,
            audio_weight: 1.0,
            texture_weight: 0.5,
            modulator_rms_cache: Some("/tmp/a-rms.json".to_string()),
            carrier_rms_cache: Some("/tmp/b-rms.json".to_string()),
            modulator_centroid_cache: Some("/tmp/a-stft.json".to_string()),
            carrier_centroid_cache: Some("/tmp/b-stft.json".to_string()),
            pool_window: 12,
            anti_repeat_weight: 0.5,
            anti_repeat_cooldown: 6,
            coherence_weight: 0.75,
            coherence_reach: 4,
            spatial_coherence_weight: 0.25,
            max_frames: Some(48),
            frame_rate: 24.0,
            backend: RenderBackend::Metal,
        };

        let json = serde_json::to_string(&task).expect("serialize pool task");
        let decoded: RenderJobTask = serde_json::from_str(&json).expect("deserialize pool task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn granular_mosaic_pool_task_without_audio_caches_defaults_to_none() {
        let json = r#"{
            "type": "frame_sequence_granular_mosaic_pool",
            "modulator_frame_directory": "/tmp/mod",
            "carrier_frame_directory": "/tmp/car",
            "output_directory": "/tmp/out",
            "grain_cache_directory": null,
            "grain_size": 16,
            "rearrangement": 1.0,
            "variation": 0.0,
            "seed": 7,
            "audio_weight": 1.0,
            "max_frames": null,
            "frame_rate": 24.0
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize pool task");
        let RenderJobTask::FrameSequenceGranularMosaicPool {
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
            ..
        } = task
        else {
            panic!("expected pool task");
        };
        assert_eq!(modulator_rms_cache, None);
        assert_eq!(carrier_rms_cache, None);
        assert_eq!(texture_weight, 0.0);
        // Pool-selection knobs added after the original schema default to off, so
        // jobs serialized before this sweep keep their whole-clip / no-scheduler meaning.
        assert_eq!(modulator_centroid_cache, None);
        assert_eq!(carrier_centroid_cache, None);
        assert_eq!(pool_window, 0);
        assert_eq!(anti_repeat_weight, 0.0);
        assert_eq!(anti_repeat_cooldown, 0);
        assert_eq!(coherence_weight, 0.0);
        assert_eq!(coherence_reach, 0);
        assert_eq!(spatial_coherence_weight, 0.0);
        assert_eq!(backend, RenderBackend::Cpu);
    }

    #[test]
    fn video_vocoder_task_round_trips() {
        let task = RenderJobTask::FrameSequenceVideoVocoder {
            modulator_frame_directory: "/tmp/mod".to_string(),
            carrier_frame_directory: "/tmp/car".to_string(),
            output_directory: "/tmp/out".to_string(),
            bands: 8,
            amount: 0.5,
            mode: VideoVocoderMode::Gain,
            max_frames: Some(24),
            frame_rate: 24.0,
            backend: RenderBackend::Metal,
        };

        let json = serde_json::to_string(&task).expect("serialize vocoder task");
        let decoded: RenderJobTask = serde_json::from_str(&json).expect("deserialize vocoder task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn spectral_cross_synth_task_round_trips() {
        let task = RenderJobTask::AudioSpectralCrossSynth {
            modulator_wav: "/tmp/a.wav".to_string(),
            carrier_wav: "/tmp/b.wav".to_string(),
            output_directory: "/tmp/out".to_string(),
            mode: CrossSynthMode::Filter,
            amount: 0.5,
            filter_type: CrossSynthFilterType::Highpass,
            rms_window: 2048,
            rms_hop: 512,
            fft_size: 1024,
            stft_hop: 256,
            window: CrossSynthWindow::Hamming,
            vocode_bands: 16,
        };

        let json = serde_json::to_string(&task).expect("serialize cross-synth task");
        let decoded: RenderJobTask =
            serde_json::from_str(&json).expect("deserialize cross-synth task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn spectral_cross_synth_task_defaults_mode_filter_type_and_window() {
        let json = r#"{
            "type": "audio_spectral_cross_synth",
            "modulator_wav": "/tmp/a.wav",
            "carrier_wav": "/tmp/b.wav",
            "output_directory": "/tmp/out",
            "amount": 1.0,
            "rms_window": 2048,
            "rms_hop": 512,
            "fft_size": 1024,
            "stft_hop": 256
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize cross-synth task");
        let RenderJobTask::AudioSpectralCrossSynth {
            mode,
            filter_type,
            window,
            vocode_bands,
            ..
        } = task
        else {
            panic!("expected cross-synth task");
        };
        assert_eq!(mode, CrossSynthMode::Gain);
        assert_eq!(filter_type, CrossSynthFilterType::Lowpass);
        assert_eq!(window, CrossSynthWindow::Hann);
        // Pre-vocode queue JSON (no vocode_bands key) deserializes to the
        // contract default.
        assert_eq!(vocode_bands, 32);
    }

    #[test]
    fn video_audio_route_task_round_trips() {
        let task = RenderJobTask::VideoAudioRoute {
            modulator_directory: "/tmp/a".to_string(),
            carrier_wav: "/tmp/b.wav".to_string(),
            output_directory: "/tmp/out".to_string(),
            descriptor: VideoAudioRouteDescriptor::Flow,
            mode: VideoAudioRouteMode::Filter,
            filter_type: VideoAudioRouteFilterType::Highpass,
            sampling: VideoAudioRouteSampling::Smooth,
            amount: 0.5,
            fps: 30.0,
        };

        let json = serde_json::to_string(&task).expect("serialize video-audio route task");
        let decoded: RenderJobTask =
            serde_json::from_str(&json).expect("deserialize video-audio route task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn video_audio_route_task_defaults_descriptor_luma_mode_gain() {
        let json = r#"{
            "type": "video_audio_route",
            "modulator_directory": "/tmp/a",
            "carrier_wav": "/tmp/b.wav",
            "output_directory": "/tmp/out",
            "amount": 1.0,
            "fps": 24.0
        }"#;

        let task: RenderJobTask =
            serde_json::from_str(json).expect("deserialize video-audio route");
        let RenderJobTask::VideoAudioRoute {
            descriptor,
            mode,
            filter_type,
            sampling,
            ..
        } = task
        else {
            panic!("expected video-audio route task");
        };
        assert_eq!(descriptor, VideoAudioRouteDescriptor::Luma);
        assert_eq!(mode, VideoAudioRouteMode::Gain);
        assert_eq!(filter_type, VideoAudioRouteFilterType::Lowpass);
        assert_eq!(sampling, VideoAudioRouteSampling::Hold);
    }

    #[test]
    fn video_audio_route_algorithm_id_composes_descriptor_and_mode() {
        use VideoAudioRouteDescriptor::*;
        use VideoAudioRouteMode::*;
        // Luma ids are unchanged from the original slice (back-compatible).
        assert_eq!(
            video_audio_route_algorithm_id(Luma, Gain),
            "luma_gain_route_cpu_v1"
        );
        assert_eq!(
            video_audio_route_algorithm_id(Luma, Pan),
            "luma_pan_route_cpu_v1"
        );
        assert_eq!(
            video_audio_route_algorithm_id(Luma, Filter),
            "luma_filter_route_cpu_v1"
        );
        assert_eq!(
            video_audio_route_algorithm_id(Flow, Gain),
            "flow_gain_route_cpu_v1"
        );
        assert_eq!(
            video_audio_route_algorithm_id(Flow, Pan),
            "flow_pan_route_cpu_v1"
        );
        assert_eq!(
            video_audio_route_algorithm_id(Flow, Filter),
            "flow_filter_route_cpu_v1"
        );
    }

    #[test]
    fn audio_impulse_convolution_task_round_trips() {
        let task = RenderJobTask::AudioImpulseConvolution {
            modulator_wav: "/tmp/ir.wav".to_string(),
            carrier_wav: "/tmp/b.wav".to_string(),
            output_directory: "/tmp/out".to_string(),
            amount: 0.5,
            max_impulse_samples: Some(4096),
            method: ConvolutionMethod::Fft,
            resample_impulse: true,
            ir_mode: IrMode::PerChannel,
        };

        let json = serde_json::to_string(&task).expect("serialize impulse-convolution task");
        let decoded: RenderJobTask =
            serde_json::from_str(&json).expect("deserialize impulse-convolution task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn audio_impulse_convolution_task_defaults_max_impulse_samples_to_none() {
        let json = r#"{
            "type": "audio_impulse_convolution",
            "modulator_wav": "/tmp/ir.wav",
            "carrier_wav": "/tmp/b.wav",
            "output_directory": "/tmp/out",
            "amount": 1.0
        }"#;

        let task: RenderJobTask =
            serde_json::from_str(json).expect("deserialize impulse-convolution task");
        let RenderJobTask::AudioImpulseConvolution {
            max_impulse_samples,
            method,
            resample_impulse,
            ir_mode,
            ..
        } = task
        else {
            panic!("expected impulse-convolution task");
        };
        assert_eq!(max_impulse_samples, None);
        assert_eq!(method, ConvolutionMethod::Direct);
        assert!(!resample_impulse);
        assert_eq!(ir_mode, IrMode::Mono);
    }

    #[test]
    fn video_vocoder_task_defaults_mode_to_match_and_backend_to_cpu() {
        let json = r#"{
            "type": "frame_sequence_video_vocoder",
            "modulator_frame_directory": "/tmp/mod",
            "carrier_frame_directory": "/tmp/car",
            "output_directory": "/tmp/out",
            "bands": 8,
            "amount": 1.0,
            "max_frames": null,
            "frame_rate": 24.0
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize vocoder task");
        let RenderJobTask::FrameSequenceVideoVocoder { mode, backend, .. } = task else {
            panic!("expected vocoder task");
        };
        assert_eq!(mode, VideoVocoderMode::Match);
        assert_eq!(backend, RenderBackend::Cpu);
    }

    #[test]
    fn audio_video_route_task_round_trips() {
        let task = RenderJobTask::FrameSequenceAudioVideoRoute {
            modulator_wav: "/tmp/a.wav".to_string(),
            carrier_frame_directory: "/tmp/car".to_string(),
            output_directory: "/tmp/out".to_string(),
            amount: 0.5,
            shift_x: 8.0,
            shift_y: -2.0,
            rms_window: 2048,
            rms_hop: 512,
            frame_rate: 30.0,
            max_frames: Some(48),
            backend: RenderBackend::Metal,
        };

        let json = serde_json::to_string(&task).expect("serialize audio-route task");
        let decoded: RenderJobTask =
            serde_json::from_str(&json).expect("deserialize audio-route task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn audio_video_route_task_defaults_backend_to_cpu() {
        let json = r#"{
            "type": "frame_sequence_audio_video_route",
            "modulator_wav": "/tmp/a.wav",
            "carrier_frame_directory": "/tmp/car",
            "output_directory": "/tmp/out",
            "amount": 1.0,
            "shift_x": 8.0,
            "shift_y": 0.0,
            "rms_window": 2048,
            "rms_hop": 512,
            "frame_rate": 30.0,
            "max_frames": null
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize audio-route task");
        let RenderJobTask::FrameSequenceAudioVideoRoute { backend, .. } = task else {
            panic!("expected audio-route task");
        };
        assert_eq!(backend, RenderBackend::Cpu);
    }

    #[test]
    fn convolution_blend_task_round_trips() {
        let task = RenderJobTask::FrameSequenceConvolutionBlend {
            modulator_frame_directory: "/tmp/mod".to_string(),
            carrier_frame_directory: "/tmp/car".to_string(),
            output_directory: "/tmp/out".to_string(),
            kernel_size: 5,
            amount: 0.5,
            max_frames: Some(24),
            backend: RenderBackend::Metal,
            kernel_mode: KernelMode::Color,
        };

        let json = serde_json::to_string(&task).expect("serialize convolution-blend task");
        let decoded: RenderJobTask =
            serde_json::from_str(&json).expect("deserialize convolution-blend task");

        assert_eq!(decoded, task);
    }

    #[test]
    fn convolution_blend_task_defaults_backend_to_cpu() {
        let json = r#"{
            "type": "frame_sequence_convolution_blend",
            "modulator_frame_directory": "/tmp/mod",
            "carrier_frame_directory": "/tmp/car",
            "output_directory": "/tmp/out",
            "kernel_size": 3,
            "amount": 1.0,
            "max_frames": null
        }"#;

        let task: RenderJobTask =
            serde_json::from_str(json).expect("deserialize convolution-blend task");
        let RenderJobTask::FrameSequenceConvolutionBlend {
            backend,
            kernel_mode,
            ..
        } = task
        else {
            panic!("expected convolution-blend task");
        };
        assert_eq!(backend, RenderBackend::Cpu);
        assert_eq!(kernel_mode, KernelMode::Luma);
    }

    #[test]
    fn granular_mosaic_task_without_audio_modulation_defaults_to_none() {
        let json = r#"{
            "type": "frame_sequence_granular_mosaic",
            "modulator_frame_directory": "/tmp/mod",
            "carrier_frame_directory": "/tmp/car",
            "output_directory": "/tmp/out",
            "grain_cache_directory": null,
            "grain_size": 24,
            "rearrangement": 1.0,
            "variation": 0.35,
            "seed": 42,
            "max_frames": null,
            "frame_rate": 24.0
        }"#;

        let task: RenderJobTask = serde_json::from_str(json).expect("deserialize legacy task");
        let RenderJobTask::FrameSequenceGranularMosaic {
            audio_modulation, ..
        } = task
        else {
            panic!("expected granular task");
        };
        assert_eq!(audio_modulation, None);
    }

    #[test]
    fn datamosh_bitstream_task_roundtrips_and_defaults() {
        let task = RenderJobTask::DatamoshBitstream {
            input_video: "/tmp/input.mov".to_string(),
            output_directory: "/tmp/out".to_string(),
            fps: 24.0,
            operation: DatamoshBitstreamOperation::PframeDuplicate,
            p_frame_index: 3,
            duplicate_count: 12,
            carrier_video: None,
            carrier_keyframes: 1,
            preset: DatamoshBitstreamPreset::Bloom,
        };
        let json = serde_json::to_string(&task).expect("serialize");
        let roundtripped: RenderJobTask = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(task, roundtripped);

        // Minimal JSON with only required fields — defaults fill the rest.
        let minimal = r#"{
            "type": "datamosh_bitstream",
            "input_video": "/tmp/input.mov",
            "output_directory": "/tmp/out",
            "fps": 24.0
        }"#;
        let from_minimal: RenderJobTask =
            serde_json::from_str(minimal).expect("deserialize minimal");
        let RenderJobTask::DatamoshBitstream {
            operation,
            p_frame_index,
            duplicate_count,
            carrier_video,
            carrier_keyframes,
            preset,
            ..
        } = from_minimal
        else {
            panic!("expected DatamoshBitstream");
        };
        assert_eq!(operation, DatamoshBitstreamOperation::PframeDuplicate);
        assert_eq!(p_frame_index, 0);
        assert_eq!(duplicate_count, 0);
        assert_eq!(carrier_video, None);
        assert_eq!(carrier_keyframes, 1);
        assert_eq!(preset, DatamoshBitstreamPreset::Custom);
    }
}
