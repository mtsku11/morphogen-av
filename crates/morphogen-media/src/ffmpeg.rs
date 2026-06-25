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

/// Encode `input` to an AVI/MPEG-4 stream shaped for bitstream datamosh: a single
/// leading I-frame followed by P-frames only (no B-frames, no audio), so a chosen
/// P-frame chunk can be duplicated to bloom its motion. Uses ffmpeg's built-in
/// `mpeg4` encoder (LGPL — not libxvid), keeping us free of GPL-only dependencies.
pub fn encode_datamosh_avi_command(
    input: impl AsRef<Path>,
    output_avi: impl AsRef<Path>,
    fps: f64,
) -> CommandSpec {
    encode_datamosh_avi_command_sized(input, output_avi, fps, None)
}

/// Like [`encode_datamosh_avi_command`] but forces the output to `width`x`height`.
/// Motion-transfer splices one clip's P-frames onto another's I-frame, so both must
/// share a macroblock grid — the modulator is scaled to the carrier's dimensions.
pub fn encode_datamosh_avi_command_scaled(
    input: impl AsRef<Path>,
    output_avi: impl AsRef<Path>,
    fps: f64,
    width: u32,
    height: u32,
) -> CommandSpec {
    encode_datamosh_avi_command_sized(input, output_avi, fps, Some((width, height)))
}

fn encode_datamosh_avi_command_sized(
    input: impl AsRef<Path>,
    output_avi: impl AsRef<Path>,
    fps: f64,
    size: Option<(u32, u32)>,
) -> CommandSpec {
    let vf = match size {
        Some((w, h)) => format!("scale={w}:{h},fps={fps}"),
        None => format!("fps={fps}"),
    };
    CommandSpec::new(
        "ffmpeg",
        vec![
            "-y".to_string(),
            "-i".to_string(),
            input.as_ref().to_string_lossy().to_string(),
            "-an".to_string(),
            "-vf".to_string(),
            vf,
            "-c:v".to_string(),
            "mpeg4".to_string(),
            "-q:v".to_string(),
            "4".to_string(),
            "-bf".to_string(),
            "0".to_string(),
            "-g".to_string(),
            "999999".to_string(),
            "-sc_threshold".to_string(),
            "0".to_string(),
            output_avi.as_ref().to_string_lossy().to_string(),
        ],
    )
}

/// Decode a (possibly mangled) AVI back into a `frame_%06d.png` sequence.
pub fn decode_avi_frames_command(
    input_avi: impl AsRef<Path>,
    output_dir: impl AsRef<Path>,
) -> CommandSpec {
    let output_pattern = frame_pattern(output_dir.as_ref());
    CommandSpec::new(
        "ffmpeg",
        vec![
            "-y".to_string(),
            "-i".to_string(),
            input_avi.as_ref().to_string_lossy().to_string(),
            output_pattern.to_string_lossy().to_string(),
        ],
    )
}

/// The first line of `ffmpeg -version` (e.g. "ffmpeg version 7.1 ..."), recorded in
/// the bitstream-datamosh sidecar so a non-reproducible run is at least traceable.
pub fn ffmpeg_version() -> Result<String, MediaError> {
    let spec = CommandSpec::new("ffmpeg", vec!["-version".to_string()]);
    let stdout = run_command_stdout(&spec)?;
    let text = String::from_utf8_lossy(&stdout);
    Ok(text.lines().next().unwrap_or("").trim().to_string())
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
    fn datamosh_encode_command_forces_pframe_only_mpeg4_avi() {
        let spec = encode_datamosh_avi_command("in.mov", "out.avi", 24.0);

        assert_eq!(spec.program, "ffmpeg");
        assert!(spec.args.contains(&"mpeg4".to_string()));
        assert!(spec.args.contains(&"-an".to_string()));
        assert!(spec.args.contains(&"-bf".to_string()));
        assert!(spec.args.contains(&"0".to_string()));
        assert!(spec.args.contains(&"-sc_threshold".to_string()));
        assert!(spec.args.contains(&"fps=24".to_string()));
        assert!(spec.args.iter().any(|arg| arg.ends_with("out.avi")));
    }

    #[test]
    fn datamosh_encode_command_scaled_forces_common_dimensions() {
        let spec = encode_datamosh_avi_command_scaled("in.mov", "out.avi", 24.0, 128, 96);

        assert!(spec.args.contains(&"mpeg4".to_string()));
        // The scale filter precedes the fps filter so both clips share a grid.
        assert!(spec.args.contains(&"scale=128:96,fps=24".to_string()));
    }

    #[test]
    fn datamosh_decode_command_targets_png_sequence() {
        let spec = decode_avi_frames_command("mosh.avi", "frames");

        assert_eq!(spec.program, "ffmpeg");
        assert!(spec.args.contains(&"mosh.avi".to_string()));
        assert!(spec.args.iter().any(|arg| arg.ends_with("frame_%06d.png")));
    }

    #[test]
    fn missing_binary_reports_clear_error() {
        let spec = CommandSpec::new("morphogen-av-definitely-missing-binary", Vec::new());
        let error = run_command_stdout(&spec).expect_err("missing binary should fail");

        assert!(matches!(error, MediaError::MissingBinary { .. }));
    }
}
