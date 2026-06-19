use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioDescriptorFrame {
    pub time_seconds: f64,
    pub rms: f32,
    pub spectral_centroid_hz: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioAnalysisCache {
    pub sample_rate: u32,
    pub frame_size: usize,
    pub hop_size: usize,
    pub frames: Vec<AudioDescriptorFrame>,
}
