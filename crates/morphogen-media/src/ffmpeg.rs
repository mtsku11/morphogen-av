use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use crate::MediaError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }

    pub fn to_command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        command
    }
}

pub fn ffprobe_command(path: impl AsRef<Path>) -> CommandSpec {
    CommandSpec::new(
        "ffprobe",
        vec![
            "-v".to_string(),
            "error".to_string(),
            "-print_format".to_string(),
            "json".to_string(),
            "-show_format".to_string(),
            "-show_streams".to_string(),
            path.as_ref().to_string_lossy().to_string(),
        ],
    )
}

pub fn extract_video_frames_command(
    input: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
    fps: f64,
    max_frames: Option<u32>,
) -> CommandSpec {
    let output_pattern = frame_pattern(output_dir.as_ref());
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.as_ref().to_string_lossy().to_string(),
        "-vf".to_string(),
        format!("fps={fps}"),
    ];

    if let Some(max_frames) = max_frames {
        args.push("-frames:v".to_string());
        args.push(max_frames.to_string());
    }

    args.push(output_pattern.to_string_lossy().to_string());
    CommandSpec::new("ffmpeg", args)
}

pub fn extract_audio_wav_command(
    input: impl AsRef<Path>,
    output_wav: impl AsRef<Path>,
    sample_rate: u32,
) -> CommandSpec {
    extract_audio_wav_command_with_max_duration(input, output_wav, sample_rate, None)
}

pub fn extract_audio_wav_command_with_max_duration(
    input: impl AsRef<Path>,
    output_wav: impl AsRef<Path>,
    sample_rate: u32,
    max_duration: Option<Duration>,
) -> CommandSpec {
    let mut args = vec![
        "-y".to_string(),
        "-i".to_string(),
        input.as_ref().to_string_lossy().to_string(),
    ];

    if let Some(max_duration) = max_duration {
        args.push("-t".to_string());
        args.push(format!("{:.6}", max_duration.as_secs_f64()));
    }

    args.extend([
        "-vn".to_string(),
        "-acodec".to_string(),
        "pcm_f32le".to_string(),
        "-ar".to_string(),
        sample_rate.to_string(),
        output_wav.as_ref().to_string_lossy().to_string(),
    ]);
    CommandSpec::new("ffmpeg", args)
}

pub(crate) fn run_command_stdout(spec: &CommandSpec) -> Result<Vec<u8>, MediaError> {
    let output = spec.to_command().output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            MediaError::MissingBinary {
                binary: spec.program.clone(),
            }
        } else {
            MediaError::Io(error)
        }
    })?;

    if !output.status.success() {
        return Err(MediaError::CommandFailed {
            binary: spec.program.clone(),
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(output.stdout)
}

pub(crate) fn run_command_status(spec: &CommandSpec) -> Result<(), MediaError> {
    run_command_stdout(spec)?;
    Ok(())
}

fn frame_pattern(output_dir: &Path) -> PathBuf {
    output_dir.join("frame_%06d.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffprobe_command_is_constructed_without_running_ffprobe() {
        let spec = ffprobe_command("/media/source.mov");

        assert_eq!(spec.program, "ffprobe");
        assert!(spec.args.contains(&"-show_streams".to_string()));
        assert!(spec.args.contains(&"/media/source.mov".to_string()));
    }

    #[test]
    fn frame_extraction_command_includes_fps_and_limit() {
        let spec = extract_video_frames_command("in.mov", "frames", 12.5, Some(8));

        assert_eq!(spec.program, "ffmpeg");
        assert!(spec.args.contains(&"fps=12.5".to_string()));
        assert!(spec.args.contains(&"-frames:v".to_string()));
        assert!(spec.args.contains(&"8".to_string()));
        assert!(spec.args.iter().any(|arg| arg.ends_with("frame_%06d.png")));
    }

    #[test]
    fn audio_extraction_command_targets_float_wav() {
        let spec = extract_audio_wav_command("in.mov", "out.wav", 48_000);

        assert!(spec.args.contains(&"pcm_f32le".to_string()));
        assert!(spec.args.contains(&"48000".to_string()));
        assert!(spec.args.contains(&"out.wav".to_string()));
    }

    #[test]
    fn audio_extraction_command_can_limit_proxy_duration() {
        let spec = extract_audio_wav_command_with_max_duration(
            "in.mov",
            "out.wav",
            48_000,
            Some(Duration::from_secs(10)),
        );

        assert!(spec.args.contains(&"-t".to_string()));
        assert!(spec.args.contains(&"10.000000".to_string()));
    }

    #[test]
    fn missing_binary_reports_clear_error() {
        let spec = CommandSpec::new("morphogen-av-definitely-missing-binary", Vec::new());
        let error = run_command_stdout(&spec).expect_err("missing binary should fail");

        assert!(matches!(error, MediaError::MissingBinary { .. }));
    }
}
