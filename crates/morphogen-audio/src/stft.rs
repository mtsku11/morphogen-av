use serde::{Deserialize, Serialize};

use crate::{AudioBufferF32, AudioError};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WindowFunction {
    Hann,
    Hamming,
    Rectangular,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct StftConfig {
    pub fft_size: usize,
    pub hop_size: usize,
    pub window: WindowFunction,
}

impl StftConfig {
    pub fn validate(&self) -> Result<(), AudioError> {
        if self.fft_size == 0 || !self.fft_size.is_power_of_two() {
            return Err(AudioError::InvalidSettings(
                "fft_size must be a non-zero power of two".to_string(),
            ));
        }
        if self.hop_size == 0 {
            return Err(AudioError::InvalidSettings(
                "hop_size must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }

    pub fn planned_frame_count(&self, sample_count: usize) -> Result<usize, AudioError> {
        self.validate()?;
        if sample_count < self.fft_size {
            return Ok(0);
        }
        Ok(1 + (sample_count - self.fft_size) / self.hop_size)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StftAnalysisCache {
    pub cache_format: String,
    pub sample_rate: u32,
    pub channels: usize,
    pub channel_mix: String,
    pub fft_size: usize,
    pub hop_size: usize,
    pub window: WindowFunction,
    pub bin_count: usize,
    pub frames: Vec<StftFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StftFrame {
    pub index: usize,
    pub time_seconds: f64,
    pub magnitudes: Vec<f32>,
}

pub fn stft_magnitude_cache(
    buffer: &AudioBufferF32,
    config: StftConfig,
) -> Result<StftAnalysisCache, AudioError> {
    config.validate()?;
    let frame_count = config.planned_frame_count(buffer.frames)?;
    let bin_count = config.fft_size / 2 + 1;
    let mut frames = Vec::with_capacity(frame_count);

    for frame_index in 0..frame_count {
        let start_frame = frame_index * config.hop_size;
        let mut windowed_samples = Vec::with_capacity(config.fft_size);

        for offset in 0..config.fft_size {
            let mono = mono_sample(buffer, start_frame + offset)?;
            let window = window_value(offset, config.fft_size, config.window);
            windowed_samples.push((mono as f64 * window) as f32);
        }

        frames.push(StftFrame {
            index: frame_index,
            time_seconds: start_frame as f64 / buffer.sample_rate as f64,
            magnitudes: dft_magnitudes(&windowed_samples),
        });
    }

    Ok(StftAnalysisCache {
        cache_format: "stft_magnitude_v1".to_string(),
        sample_rate: buffer.sample_rate,
        channels: buffer.channels,
        channel_mix: "mean_channels".to_string(),
        fft_size: config.fft_size,
        hop_size: config.hop_size,
        window: config.window,
        bin_count,
        frames,
    })
}

fn mono_sample(buffer: &AudioBufferF32, frame: usize) -> Result<f32, AudioError> {
    let start = frame.checked_mul(buffer.channels).ok_or_else(|| {
        AudioError::InvalidBuffer("frame index overflow while mixing STFT sample".to_string())
    })?;
    let end = start.checked_add(buffer.channels).ok_or_else(|| {
        AudioError::InvalidBuffer("channel range overflow while mixing STFT sample".to_string())
    })?;
    let samples = buffer.samples.get(start..end).ok_or_else(|| {
        AudioError::InvalidBuffer("STFT frame exceeds audio buffer length".to_string())
    })?;
    let sum: f32 = samples.iter().copied().sum();
    Ok(sum / buffer.channels as f32)
}

fn dft_magnitudes(frame: &[f32]) -> Vec<f32> {
    let n = frame.len();
    let normalization = n as f64;
    let mut magnitudes = Vec::with_capacity(n / 2 + 1);

    for bin in 0..=n / 2 {
        let mut real = 0.0_f64;
        let mut imaginary = 0.0_f64;

        for (index, sample) in frame.iter().enumerate() {
            let phase = -2.0 * std::f64::consts::PI * bin as f64 * index as f64 / n as f64;
            real += *sample as f64 * phase.cos();
            imaginary += *sample as f64 * phase.sin();
        }

        magnitudes.push(((real * real + imaginary * imaginary).sqrt() / normalization) as f32);
    }

    magnitudes
}

pub(crate) fn window_value(index: usize, size: usize, window: WindowFunction) -> f64 {
    if size <= 1 {
        return 1.0;
    }

    let phase = 2.0 * std::f64::consts::PI * index as f64 / (size - 1) as f64;
    match window {
        WindowFunction::Hann => 0.5 - 0.5 * phase.cos(),
        WindowFunction::Hamming => 0.54 - 0.46 * phase.cos(),
        WindowFunction::Rectangular => 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stft_cache_detects_dominant_frequency_bin() {
        let buffer =
            AudioBufferF32::new(1, 4, vec![1.0, 0.0, -1.0, 0.0]).expect("valid test buffer");
        let cache = stft_magnitude_cache(
            &buffer,
            StftConfig {
                fft_size: 4,
                hop_size: 2,
                window: WindowFunction::Rectangular,
            },
        )
        .expect("calculate stft");

        assert_eq!(cache.cache_format, "stft_magnitude_v1");
        assert_eq!(cache.bin_count, 3);
        assert_eq!(cache.frames.len(), 1);
        assert!(cache.frames[0].magnitudes[1] > cache.frames[0].magnitudes[0]);
        assert!(cache.frames[0].magnitudes[1] > cache.frames[0].magnitudes[2]);
    }

    #[test]
    fn stft_cache_serializes_window_and_frames() {
        let buffer =
            AudioBufferF32::new(1, 8, vec![0.0, 1.0, 0.0, -1.0]).expect("valid test buffer");
        let cache = stft_magnitude_cache(
            &buffer,
            StftConfig {
                fft_size: 4,
                hop_size: 4,
                window: WindowFunction::Hann,
            },
        )
        .expect("calculate stft");

        let json = serde_json::to_string(&cache).expect("serialize stft cache");
        assert!(json.contains("\"cache_format\":\"stft_magnitude_v1\""));
        assert!(json.contains("\"window\":\"hann\""));
        assert!(json.contains("\"magnitudes\""));
    }

    #[test]
    fn stft_config_rejects_invalid_fft_size() {
        let config = StftConfig {
            fft_size: 3,
            hop_size: 1,
            window: WindowFunction::Hann,
        };

        assert!(config.validate().is_err());
    }
}
