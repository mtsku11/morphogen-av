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
        max_frames: Option<u32>,
        frame_rate: f64,
        /// Render backend; the Metal path is gated per-frame against the CPU
        /// reference. Defaults to CPU so legacy jobs keep their meaning.
        #[serde(default)]
        backend: RenderBackend,
    },
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
            modulator_rms_cache: Some("/tmp/a-rms.json".to_string()),
            carrier_rms_cache: Some("/tmp/b-rms.json".to_string()),
            modulator_centroid_cache: Some("/tmp/a-stft.json".to_string()),
            carrier_centroid_cache: Some("/tmp/b-stft.json".to_string()),
            pool_window: 12,
            anti_repeat_weight: 0.5,
            anti_repeat_cooldown: 6,
            coherence_weight: 0.75,
            coherence_reach: 4,
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
            modulator_rms_cache,
            carrier_rms_cache,
            modulator_centroid_cache,
            carrier_centroid_cache,
            pool_window,
            anti_repeat_weight,
            anti_repeat_cooldown,
            coherence_weight,
            coherence_reach,
            backend,
            ..
        } = task
        else {
            panic!("expected pool task");
        };
        assert_eq!(modulator_rms_cache, None);
        assert_eq!(carrier_rms_cache, None);
        // Pool-selection knobs added after the original schema default to off, so
        // jobs serialized before this sweep keep their whole-clip / no-scheduler meaning.
        assert_eq!(modulator_centroid_cache, None);
        assert_eq!(carrier_centroid_cache, None);
        assert_eq!(pool_window, 0);
        assert_eq!(anti_repeat_weight, 0.0);
        assert_eq!(anti_repeat_cooldown, 0);
        assert_eq!(coherence_weight, 0.0);
        assert_eq!(coherence_reach, 0);
        assert_eq!(backend, RenderBackend::Cpu);
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
}
