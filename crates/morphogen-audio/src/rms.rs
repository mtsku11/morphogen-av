use crate::{AudioBufferF32, AudioDescriptorFrame, AudioError};

pub fn rms_envelope(
    buffer: &AudioBufferF32,
    window_size: usize,
    hop_size: usize,
) -> Result<Vec<AudioDescriptorFrame>, AudioError> {
    if window_size == 0 {
        return Err(AudioError::InvalidSettings(
            "window_size must be greater than zero".to_string(),
        ));
    }
    if hop_size == 0 {
        return Err(AudioError::InvalidSettings(
            "hop_size must be greater than zero".to_string(),
        ));
    }

    let mut descriptors = Vec::new();
    let mut start = 0;

    while start < buffer.frames {
        let end = (start + window_size).min(buffer.frames);
        let rms = root_mean_square(buffer, start, end);
        descriptors.push(AudioDescriptorFrame {
            time_seconds: start as f64 / buffer.sample_rate as f64,
            rms,
            spectral_centroid_hz: None,
        });

        if end == buffer.frames {
            break;
        }
        start += hop_size;
    }

    Ok(descriptors)
}

fn root_mean_square(buffer: &AudioBufferF32, start: usize, end: usize) -> f32 {
    let mut sum = 0.0_f64;
    let mut count = 0_usize;

    for frame in start..end {
        for channel in 0..buffer.channels {
            let index = frame * buffer.channels + channel;
            if let Some(sample) = buffer.samples.get(index) {
                let sample = *sample as f64;
                sum += sample * sample;
                count += 1;
            }
        }
    }

    if count == 0 {
        return 0.0;
    }

    (sum / count as f64).sqrt() as f32
}
