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
