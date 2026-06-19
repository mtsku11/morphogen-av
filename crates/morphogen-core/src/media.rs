use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaSource {
    pub id: String,
    pub label: String,
    pub role: SourceRole,
    pub uri: String,
    /// Ingested proxy media (extracted frames and audio) for this source, when present.
    #[serde(default)]
    pub proxy: Option<MediaProxy>,
}

/// References to the deterministic proxy media produced when a source movie is
/// ingested: a directory of extracted PNG frames and an optional extracted WAV stem.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaProxy {
    pub frame_directory: String,
    #[serde(default)]
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceRole {
    Modulator,
    Carrier,
}
