#![forbid(unsafe_code)]

pub mod apple_backend_plan;
pub mod error;
pub mod extract;
pub mod ffmpeg;
pub mod probe;

pub use error::MediaError;
pub use extract::{extract_audio_wav, extract_video_frames};
pub use ffmpeg::{
    extract_audio_wav_command, extract_video_frames_command, ffprobe_command, CommandSpec,
};
pub use probe::{probe_media, MediaProbe, MediaStreamProbe};
