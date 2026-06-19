use serde::{Deserialize, Serialize};

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
    pub status: RenderJobStatus,
    #[serde(default)]
    pub output: Option<RenderJobOutputMetadata>,
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
}
