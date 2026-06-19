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
}
