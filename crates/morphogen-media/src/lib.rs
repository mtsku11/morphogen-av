#![forbid(unsafe_code)]

pub mod apple_backend_plan;
pub mod avi;
pub mod error;
pub mod extract;
pub mod ffmpeg;
pub mod probe;

pub use avi::{count_p_frames, duplicate_p_frame, remove_leading_keyframe};
pub use error::MediaError;
pub use extract::{
    decode_avi_frames, encode_datamosh_avi, extract_audio_wav, extract_audio_wav_with_max_duration,
    extract_video_frames,
};
pub use ffmpeg::{
    decode_avi_frames_command, encode_datamosh_avi_command, extract_audio_wav_command,
    extract_audio_wav_command_with_max_duration, extract_video_frames_command, ffmpeg_version,
    ffprobe_command, CommandSpec,
};
pub use probe::{probe_media, MediaProbe, MediaStreamProbe};
