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

/// Encode a video to the P-frame-only AVI/MPEG-4 substrate used for bitstream
/// datamosh (see [`ffmpeg::encode_datamosh_avi_command`]).
pub fn encode_datamosh_avi(
    input: impl AsRef<Path>,
    output_avi: impl AsRef<Path>,
    fps: f64,
) -> Result<(), MediaError> {
    let spec = ffmpeg::encode_datamosh_avi_command(input, output_avi, fps);
    ffmpeg::run_command_status(&spec)
}

/// Decode a (possibly mangled) AVI to a `frame_%06d.png` sequence.
pub fn decode_avi_frames(
    input_avi: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
) -> Result<(), MediaError> {
    fs::create_dir_all(output_dir.as_ref())?;
    let spec = ffmpeg::decode_avi_frames_command(input_avi, output_dir);
    ffmpeg::run_command_status(&spec)
}
