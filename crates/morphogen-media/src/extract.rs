use std::{fs, path::Path, time::Duration};

use crate::{ffmpeg, MediaError};

pub fn extract_video_frames(
    input: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    fps: f64,
    max_frames: Option<u32>,
) -> Result<(), MediaError> {
    fs::create_dir_all(output_dir.as_ref())?;
    let spec = ffmpeg::extract_video_frames_command(input, output_dir, fps, max_frames);
    ffmpeg::run_command_status(&spec)
}

pub fn extract_audio_wav(
    input: impl AsRef<Path>,
    output_wav: impl AsRef<Path>,
    sample_rate: u32,
) -> Result<(), MediaError> {
    extract_audio_wav_with_max_duration(input, output_wav, sample_rate, None)
}

pub fn extract_audio_wav_with_max_duration(
    input: impl AsRef<Path>,
    output_wav: impl AsRef<Path>,
    sample_rate: u32,
    max_duration: Option<Duration>,
) -> Result<(), MediaError> {
    let spec = ffmpeg::extract_audio_wav_command_with_max_duration(
        input,
        output_wav,
        sample_rate,
        max_duration,
    );
    ffmpeg::run_command_status(&spec)
}
