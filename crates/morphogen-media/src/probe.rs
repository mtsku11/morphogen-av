use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{ffmpeg, MediaError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaProbe {
    pub path: String,
    pub format_name: Option<String>,
    pub duration_seconds: Option<f64>,
    pub streams: Vec<MediaStreamProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MediaStreamProbe {
    pub index: u32,
    pub codec_type: Option<String>,
    pub codec_name: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
}

pub fn probe_media(path: impl AsRef<Path>) -> Result<MediaProbe, MediaError> {
    let path_ref = path.as_ref();
    let spec = ffmpeg::ffprobe_command(path_ref);
    let stdout = ffmpeg::run_command_stdout(&spec)?;
    let parsed: FfprobeOutput = serde_json::from_slice(&stdout)?;

    Ok(MediaProbe {
        path: path_ref.to_string_lossy().to_string(),
        format_name: parsed
            .format
            .as_ref()
            .and_then(|format| format.format_name.clone()),
        duration_seconds: parsed
            .format
            .as_ref()
            .and_then(|format| parse_optional_f64(format.duration.as_deref())),
        streams: parsed
            .streams
            .into_iter()
            .map(MediaStreamProbe::from)
            .collect(),
    })
}

fn parse_optional_f64(value: Option<&str>) -> Option<f64> {
    value.and_then(|value| value.parse::<f64>().ok())
}

fn parse_optional_u32(value: Option<String>) -> Option<u32> {
    value.and_then(|value| value.parse::<u32>().ok())
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    #[serde(default)]
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: u32,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    sample_rate: Option<String>,
    channels: Option<u32>,
}

impl From<FfprobeStream> for MediaStreamProbe {
    fn from(stream: FfprobeStream) -> Self {
        Self {
            index: stream.index,
            codec_type: stream.codec_type,
            codec_name: stream.codec_name,
            width: stream.width,
            height: stream.height,
            sample_rate: parse_optional_u32(stream.sample_rate),
            channels: stream.channels,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_ffprobe_json() {
        let json = br#"{
          "format": { "format_name": "mov,mp4", "duration": "1.500000" },
          "streams": [
            { "index": 0, "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080 },
            { "index": 1, "codec_type": "audio", "codec_name": "aac", "sample_rate": "48000", "channels": 2 }
          ]
        }"#;

        let parsed: FfprobeOutput = serde_json::from_slice(json).expect("parse ffprobe JSON");
        let probe = MediaProbe {
            path: "example.mov".to_string(),
            format_name: parsed
                .format
                .as_ref()
                .and_then(|format| format.format_name.clone()),
            duration_seconds: parsed
                .format
                .as_ref()
                .and_then(|format| parse_optional_f64(format.duration.as_deref())),
            streams: parsed
                .streams
                .into_iter()
                .map(MediaStreamProbe::from)
                .collect(),
        };

        assert_eq!(probe.duration_seconds, Some(1.5));
        assert_eq!(probe.streams[0].width, Some(1920));
        assert_eq!(probe.streams[1].sample_rate, Some(48_000));
    }
}
