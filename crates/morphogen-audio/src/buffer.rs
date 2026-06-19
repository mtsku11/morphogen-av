use serde::{Deserialize, Serialize};

use crate::AudioError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioBufferF32 {
    pub channels: usize,
    pub sample_rate: u32,
    pub frames: usize,
    pub samples: Vec<f32>,
}

impl AudioBufferF32 {
    pub fn new(channels: usize, sample_rate: u32, samples: Vec<f32>) -> Result<Self, AudioError> {
        if channels == 0 {
            return Err(AudioError::InvalidBuffer(
                "channel count must be greater than zero".to_string(),
            ));
        }
        if sample_rate == 0 {
            return Err(AudioError::InvalidBuffer(
                "sample rate must be greater than zero".to_string(),
            ));
        }
        if samples.len() % channels != 0 {
            return Err(AudioError::InvalidBuffer(
                "interleaved sample length must be divisible by channel count".to_string(),
            ));
        }

        Ok(Self {
            channels,
            sample_rate,
            frames: samples.len() / channels,
            samples,
        })
    }

    pub fn silence(channels: usize, sample_rate: u32, frames: usize) -> Result<Self, AudioError> {
        let sample_count = channels.checked_mul(frames).ok_or_else(|| {
            AudioError::InvalidBuffer("requested silence buffer is too large".to_string())
        })?;

        Self::new(channels, sample_rate, vec![0.0; sample_count])
    }

    pub fn sample(&self, frame: usize, channel: usize) -> Option<f32> {
        if frame >= self.frames || channel >= self.channels {
            return None;
        }

        self.samples.get(frame * self.channels + channel).copied()
    }
}
