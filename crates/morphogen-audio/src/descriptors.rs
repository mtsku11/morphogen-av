use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioDescriptorFrame {
    pub time_seconds: f64,
    pub rms: f32,
    pub spectral_centroid_hz: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioAnalysisCache {
    pub cache_format: String,
    pub sample_rate: u32,
    pub frame_size: usize,
    pub hop_size: usize,
    pub frames: Vec<AudioDescriptorFrame>,
}

impl AudioAnalysisCache {
    /// Wrap an RMS-envelope descriptor sequence as a serializable cache sidecar.
    pub fn rms_envelope_cache(
        sample_rate: u32,
        frame_size: usize,
        hop_size: usize,
        frames: Vec<AudioDescriptorFrame>,
    ) -> Self {
        Self {
            cache_format: "rms_envelope_v1".to_string(),
            sample_rate,
            frame_size,
            hop_size,
            frames,
        }
    }
}
