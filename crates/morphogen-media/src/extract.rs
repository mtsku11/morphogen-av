use std::{fs, path::Path};

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
    let spec = ffmpeg::extract_audio_wav_command(input, output_wav, sample_rate);
    ffmpeg::run_command_status(&spec)
}
