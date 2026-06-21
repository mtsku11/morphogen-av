use crate::AudioError;

pub fn spectral_centroid(frame: &[f32], sample_rate: u32) -> Result<f32, AudioError> {
    if frame.is_empty() {
        return Err(AudioError::InvalidSettings(
            "spectral centroid requires at least one sample".to_string(),
        ));
    }
    if sample_rate == 0 {
        return Err(AudioError::InvalidSettings(
            "sample rate must be greater than zero".to_string(),
        ));
    }

    let n = frame.len();
    let max_bin = n / 2;
    let mut weighted_sum = 0.0_f64;
    let mut magnitude_sum = 0.0_f64;

    for bin in 0..=max_bin {
        let mut real = 0.0_f64;
        let mut imaginary = 0.0_f64;

        for (index, sample) in frame.iter().enumerate() {
            let phase = -2.0 * std::f64::consts::PI * bin as f64 * index as f64 / n as f64;
            real += *sample as f64 * phase.cos();
            imaginary += *sample as f64 * phase.sin();
        }

        let magnitude = (real * real + imaginary * imaginary).sqrt();
        let frequency = bin as f64 * sample_rate as f64 / n as f64;
        weighted_sum += frequency * magnitude;
        magnitude_sum += magnitude;
    }

    if magnitude_sum <= f64::EPSILON {
        return Ok(0.0);
    }

    Ok((weighted_sum / magnitude_sum) as f32)
}

/// Computes a spectral centroid from one-sided DFT magnitudes. This is the
/// cache-friendly counterpart to [`spectral_centroid`]: STFT analysis has
/// already paid for the transform, so routing should not repeat it per frame.
pub fn spectral_centroid_from_magnitudes(
    magnitudes: &[f32],
    fft_size: usize,
    sample_rate: u32,
) -> Result<f32, AudioError> {
    if fft_size == 0 {
        return Err(AudioError::InvalidSettings(
            "spectral centroid FFT size must be greater than zero".to_string(),
        ));
    }
    if sample_rate == 0 {
        return Err(AudioError::InvalidSettings(
            "sample rate must be greater than zero".to_string(),
        ));
    }

    let expected_bin_count = fft_size / 2 + 1;
    if magnitudes.len() != expected_bin_count {
        return Err(AudioError::InvalidSettings(format!(
            "spectral centroid expected {expected_bin_count} magnitude bins, got {}",
            magnitudes.len()
        )));
    }

    let mut weighted_sum = 0.0_f64;
    let mut magnitude_sum = 0.0_f64;
    for (bin, magnitude) in magnitudes.iter().copied().enumerate() {
        if !magnitude.is_finite() || magnitude < 0.0 {
            return Err(AudioError::InvalidSettings(
                "spectral centroid magnitudes must be finite and non-negative".to_string(),
            ));
        }
        let frequency = bin as f64 * sample_rate as f64 / fft_size as f64;
        weighted_sum += frequency * magnitude as f64;
        magnitude_sum += magnitude as f64;
    }

    if magnitude_sum <= f64::EPSILON {
        return Ok(0.0);
    }

    Ok((weighted_sum / magnitude_sum) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magnitude_centroid_uses_the_stft_bin_frequencies() {
        let centroid =
            spectral_centroid_from_magnitudes(&[0.0, 0.0, 1.0, 0.0, 0.0], 8, 8).expect("centroid");
        assert_eq!(centroid, 2.0);
    }

    #[test]
    fn magnitude_centroid_rejects_invalid_stft_data() {
        assert!(spectral_centroid_from_magnitudes(&[1.0], 8, 48_000).is_err());
        assert!(spectral_centroid_from_magnitudes(&[0.0, -1.0, 0.0, 0.0, 0.0], 8, 48_000).is_err());
    }
}
