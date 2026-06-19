use serde::{Deserialize, Serialize};

use crate::{AudioError, StftAnalysisCache};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OnsetStrengthCache {
    pub cache_format: String,
    pub source_cache_format: String,
    pub sample_rate: u32,
    pub hop_size: usize,
    pub frames: Vec<OnsetStrengthFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OnsetStrengthFrame {
    pub index: usize,
    pub time_seconds: f64,
    pub strength: f32,
}

pub fn onset_strength_from_stft(
    stft: &StftAnalysisCache,
) -> Result<OnsetStrengthCache, AudioError> {
    if stft.bin_count == 0 {
        return Err(AudioError::InvalidSettings(
            "onset strength requires at least one STFT bin".to_string(),
        ));
    }

    let mut frames = Vec::with_capacity(stft.frames.len());
    let mut previous: Option<&[f32]> = None;

    for frame in &stft.frames {
        if frame.magnitudes.len() != stft.bin_count {
            return Err(AudioError::InvalidSettings(format!(
                "STFT frame {} has {} magnitude bin(s), expected {}",
                frame.index,
                frame.magnitudes.len(),
                stft.bin_count
            )));
        }

        let strength = previous
            .map(|previous_magnitudes| spectral_flux(previous_magnitudes, &frame.magnitudes))
            .unwrap_or(0.0);

        frames.push(OnsetStrengthFrame {
            index: frame.index,
            time_seconds: frame.time_seconds,
            strength,
        });
        previous = Some(&frame.magnitudes);
    }

    Ok(OnsetStrengthCache {
        cache_format: "onset_strength_v1".to_string(),
        source_cache_format: stft.cache_format.clone(),
        sample_rate: stft.sample_rate,
        hop_size: stft.hop_size,
        frames,
    })
}

fn spectral_flux(previous: &[f32], current: &[f32]) -> f32 {
    previous
        .iter()
        .zip(current)
        .map(|(prev, curr)| (curr - prev).max(0.0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{stft_magnitude_cache, AudioBufferF32, StftConfig, WindowFunction};

    #[test]
    fn onset_strength_detects_positive_spectral_change() {
        let buffer = AudioBufferF32::new(1, 8, vec![0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0])
            .expect("valid buffer");
        let stft = stft_magnitude_cache(
            &buffer,
            StftConfig {
                fft_size: 4,
                hop_size: 4,
                window: WindowFunction::Rectangular,
            },
        )
        .expect("calculate stft");
        let onsets = onset_strength_from_stft(&stft).expect("calculate onsets");

        assert_eq!(onsets.cache_format, "onset_strength_v1");
        assert_eq!(onsets.frames.len(), 2);
        assert_eq!(onsets.frames[0].strength, 0.0);
        assert!(onsets.frames[1].strength > 0.0);
    }

    #[test]
    fn onset_strength_rejects_malformed_stft_frame() {
        let stft = StftAnalysisCache {
            cache_format: "stft_magnitude_v1".to_string(),
            sample_rate: 48_000,
            channels: 1,
            channel_mix: "mean_channels".to_string(),
            fft_size: 4,
            hop_size: 2,
            window: WindowFunction::Hann,
            bin_count: 3,
            frames: vec![crate::StftFrame {
                index: 0,
                time_seconds: 0.0,
                magnitudes: vec![0.0, 1.0],
            }],
        };

        assert!(onset_strength_from_stft(&stft).is_err());
    }
}
