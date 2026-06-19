use std::path::Path;

use crate::{AudioBufferF32, AudioError};

pub fn load_wav_f32(path: impl AsRef<Path>) -> Result<AudioBufferF32, AudioError> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate;
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => {
            let mut samples = Vec::new();
            for sample in reader.samples::<f32>() {
                samples.push(sample?);
            }
            samples
        }
        hound::SampleFormat::Int => read_integer_samples(&mut reader, spec.bits_per_sample)?,
    };

    AudioBufferF32::new(channels, sample_rate, samples)
}

pub fn save_wav_f32(path: impl AsRef<Path>, buffer: &AudioBufferF32) -> Result<(), AudioError> {
    let channels = u16::try_from(buffer.channels).map_err(|_| {
        AudioError::InvalidBuffer(format!(
            "channel count {} is too large for WAV",
            buffer.channels
        ))
    })?;
    let spec = hound::WavSpec {
        channels,
        sample_rate: buffer.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in &buffer.samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(())
}

fn read_integer_samples<R: std::io::Read>(
    reader: &mut hound::WavReader<R>,
    bits_per_sample: u16,
) -> Result<Vec<f32>, AudioError> {
    let scale = integer_scale(bits_per_sample)?;

    if bits_per_sample <= 16 {
        let mut samples = Vec::new();
        for sample in reader.samples::<i16>() {
            samples.push(sample? as f32 / scale);
        }
        return Ok(samples);
    }

    let mut samples = Vec::new();
    for sample in reader.samples::<i32>() {
        samples.push(sample? as f32 / scale);
    }
    Ok(samples)
}

fn integer_scale(bits_per_sample: u16) -> Result<f32, AudioError> {
    if bits_per_sample == 0 || bits_per_sample > 32 {
        return Err(AudioError::InvalidSettings(format!(
            "unsupported integer WAV bit depth {bits_per_sample}"
        )));
    }

    Ok((1_i64 << u32::from(bits_per_sample - 1)) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_wav_f32_round_trips_float_stem() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("stem.wav");
        let buffer =
            AudioBufferF32::new(2, 48_000, vec![0.25, -0.25, 0.5, -0.5]).expect("valid buffer");

        save_wav_f32(&path, &buffer).expect("save wav");
        let decoded = load_wav_f32(&path).expect("load wav");

        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.sample_rate, 48_000);
        assert_eq!(decoded.samples, buffer.samples);
    }
}
