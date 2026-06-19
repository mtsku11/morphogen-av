use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisKind {
    Luminance,
    EdgeMap,
    OpticalFlow,
    DepthMap,
    AudioRms,
    SpectralCentroid,
    OnsetStrength,
    Stft,
    GrainDescriptors,
}
