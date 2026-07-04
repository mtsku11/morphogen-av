use std::fs;
use std::path::Path;
use std::time::Duration;

use morphogen_audio::{
    load_wav_f32, onset_strength_from_stft, rms_envelope, save_wav_f32, stft_magnitude_cache,
    AudioAnalysisCache, AudioBufferF32, StftConfig, WindowFunction,
};
use morphogen_core::{AnalysisCacheEntry, AnalysisKind, MediaProxy, Project, SourceRole};
use morphogen_media::{
    extract_audio_wav_with_max_duration, extract_video_frames, probe_media, MediaError,
};
use morphogen_render::{luminance_gradient_flow_cpu, write_flow_cache};

use crate::error::CliError;
use crate::imaging::{
    box_downscale, collect_image_frames, load_image_f32, save_png, synthetic_flow,
    write_parent_dirs, BOX_DOWNSCALE_ALGORITHM,
};
pub(crate) fn init_example(output_path: &Path) -> Result<(), CliError> {
    let project = Project::example_two_source_flow_displace();
    project.validate()?;
    write_parent_dirs(output_path)?;
    let json = serde_json::to_string_pretty(&project)?;
    fs::write(output_path, json)?;
    println!("wrote example project to {}", output_path.display());
    Ok(())
}

pub(crate) fn probe(media_path: &Path) -> Result<(), CliError> {
    match probe_media(media_path) {
        Ok(probe) => {
            println!("media: {}", probe.path);
            if let Some(format_name) = probe.format_name {
                println!("format: {format_name}");
            }
            if let Some(duration) = probe.duration_seconds {
                println!("duration_seconds: {duration:.3}");
            }
            for stream in probe.streams {
                println!(
                    "stream {}: type={:?} codec={:?} size={:?}x{:?} sample_rate={:?} channels={:?}",
                    stream.index,
                    stream.codec_type,
                    stream.codec_name,
                    stream.width,
                    stream.height,
                    stream.sample_rate,
                    stream.channels
                );
            }
            Ok(())
        }
        Err(MediaError::MissingBinary { binary }) => {
            Err(missing_media_binary(binary, "media probing"))
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn extract_frames(
    input: &Path,
    output_dir: &Path,
    fps: f64,
    max_frames: Option<u32>,
) -> Result<(), CliError> {
    if !fps.is_finite() || fps <= 0.0 {
        return Err(CliError::Message(
            "fps must be a positive finite number".to_string(),
        ));
    }

    match extract_video_frames(input, output_dir, fps, max_frames) {
        Ok(()) => {
            println!(
                "extracted video frames from {} to {}",
                input.display(),
                output_dir.display()
            );
            Ok(())
        }
        Err(MediaError::MissingBinary { binary }) => {
            Err(missing_media_binary(binary, "video frame extraction"))
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn extract_audio(
    input: &Path,
    output_wav: &Path,
    sample_rate: u32,
    max_duration_seconds: Option<f64>,
) -> Result<(), CliError> {
    if sample_rate == 0 {
        return Err(CliError::Message(
            "sample-rate must be greater than zero".to_string(),
        ));
    }
    let max_duration = max_duration_seconds
        .map(|duration| {
            if !duration.is_finite() || duration <= 0.0 {
                return Err(CliError::Message(
                    "max-duration-seconds must be a positive finite number".to_string(),
                ));
            }
            Ok(Duration::from_secs_f64(duration))
        })
        .transpose()?;

    write_parent_dirs(output_wav)?;

    match extract_audio_wav_with_max_duration(input, output_wav, sample_rate, max_duration) {
        Ok(()) => {
            println!(
                "extracted audio from {} to {}",
                input.display(),
                output_wav.display()
            );
            Ok(())
        }
        Err(MediaError::MissingBinary { binary }) => {
            Err(missing_media_binary(binary, "audio WAV extraction"))
        }
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn downscale_frames(
    input_dir: &Path,
    output_dir: &Path,
    scale: u32,
    max_frames: Option<u32>,
) -> Result<(), CliError> {
    if scale == 0 {
        return Err(CliError::Message(
            "scale must be an integer >= 1".to_string(),
        ));
    }

    let frames = collect_image_frames(input_dir)?;
    let frame_count = match max_frames {
        Some(cap) => (cap as usize).min(frames.len()),
        None => frames.len(),
    };

    fs::create_dir_all(output_dir)?;

    for frame_path in frames.iter().take(frame_count) {
        let image = load_image_f32(frame_path)?;
        let downscaled = box_downscale(&image, scale)?;
        let file_name = frame_path.file_name().ok_or_else(|| {
            CliError::Message(format!(
                "frame path {} has no file name",
                frame_path.display()
            ))
        })?;
        save_png(&downscaled, &output_dir.join(file_name))?;
    }

    println!(
        "downscaled {frame_count} frame(s) from {} to {} (algorithm {BOX_DOWNSCALE_ALGORITHM}, scale {scale})",
        input_dir.display(),
        output_dir.display()
    );

    Ok(())
}

pub(crate) fn export_audio_stem(
    input_wav: &Path,
    output_wav: &Path,
    gain: f32,
) -> Result<(), CliError> {
    if !gain.is_finite() {
        return Err(CliError::Message("gain must be finite".to_string()));
    }

    let input = load_wav_f32(input_wav)?;
    let output = apply_gain(&input, gain)?;
    write_parent_dirs(output_wav)?;
    save_wav_f32(output_wav, &output)?;
    println!(
        "exported WAV stem from {} to {}",
        input_wav.display(),
        output_wav.display()
    );
    Ok(())
}
pub(crate) fn cache_stft(
    input_wav: &Path,
    output_json: &Path,
    fft_size: usize,
    hop_size: usize,
    window: WindowFunction,
) -> Result<(), CliError> {
    let buffer = load_wav_f32(input_wav)?;
    let cache = stft_magnitude_cache(
        &buffer,
        StftConfig {
            fft_size,
            hop_size,
            window,
        },
    )?;

    write_parent_dirs(output_json)?;
    fs::write(output_json, serde_json::to_string_pretty(&cache)?)?;
    println!(
        "wrote STFT cache with {} frame(s) and {} bin(s) to {}",
        cache.frames.len(),
        cache.bin_count,
        output_json.display()
    );
    Ok(())
}

pub(crate) fn cache_onsets(
    input_wav: &Path,
    output_json: &Path,
    fft_size: usize,
    hop_size: usize,
    window: WindowFunction,
) -> Result<(), CliError> {
    let buffer = load_wav_f32(input_wav)?;
    let stft = stft_magnitude_cache(
        &buffer,
        StftConfig {
            fft_size,
            hop_size,
            window,
        },
    )?;
    let onsets = onset_strength_from_stft(&stft)?;

    write_parent_dirs(output_json)?;
    fs::write(output_json, serde_json::to_string_pretty(&onsets)?)?;
    println!(
        "wrote onset-strength cache with {} frame(s) to {}",
        onsets.frames.len(),
        output_json.display()
    );
    Ok(())
}

pub(crate) fn cache_rms(
    input_wav: &Path,
    output_json: &Path,
    window_size: usize,
    hop_size: usize,
) -> Result<(), CliError> {
    let buffer = load_wav_f32(input_wav)?;
    let frames = rms_envelope(&buffer, window_size, hop_size)?;
    let cache =
        AudioAnalysisCache::rms_envelope_cache(buffer.sample_rate, window_size, hop_size, frames);

    write_parent_dirs(output_json)?;
    fs::write(output_json, serde_json::to_string_pretty(&cache)?)?;
    println!(
        "wrote RMS envelope cache with {} frame(s) to {}",
        cache.frames.len(),
        output_json.display()
    );
    Ok(())
}

pub(crate) fn apply_gain(buffer: &AudioBufferF32, gain: f32) -> Result<AudioBufferF32, CliError> {
    let mut samples = Vec::with_capacity(buffer.samples.len());
    for sample in &buffer.samples {
        let scaled = *sample * gain;
        if !scaled.is_finite() {
            return Err(CliError::Message(
                "gain produced a non-finite sample".to_string(),
            ));
        }
        samples.push(scaled);
    }

    AudioBufferF32::new(buffer.channels, buffer.sample_rate, samples).map_err(CliError::from)
}

pub(crate) fn missing_media_binary(binary: String, operation: &str) -> CliError {
    CliError::Message(format!(
        "{binary} is not installed or not on PATH. Install FFmpeg tools to use {operation}, or use render-test without external media."
    ))
}

pub(crate) fn inspect_project(project_path: &Path) -> Result<(), CliError> {
    let json = fs::read_to_string(project_path)?;
    let project: Project = serde_json::from_str(&json)?;
    project.validate()?;

    println!("{}", project.summary());
    println!(
        "timeline: {} fps, {} Hz",
        project.timeline.frame_rate, project.timeline.sample_rate
    );
    println!("sources:");
    for source in &project.sources {
        println!("  - {} ({:?}) {}", source.label, source.role, source.uri);
    }
    println!("routes:");
    for route in &project.graph.routes {
        println!(
            "  - {}.{} -> {}.{} amount={}",
            route.from_node, route.from_output, route.to_node, route.to_parameter, route.amount
        );
    }

    Ok(())
}

pub(crate) struct ProjectRegisterProxyRequest<'a> {
    pub(crate) project_path: &'a Path,
    pub(crate) source_id: Option<&'a str>,
    pub(crate) source_role: Option<SourceRole>,
    pub(crate) frame_dir: &'a Path,
    pub(crate) audio: Option<&'a Path>,
    pub(crate) analysis_cache: &'a [String],
}

pub(crate) fn project_register_proxy(
    request: ProjectRegisterProxyRequest<'_>,
) -> Result<(), CliError> {
    let json = fs::read_to_string(request.project_path)?;
    let mut project: Project = serde_json::from_str(&json)?;
    let source_id = resolve_project_source_id(&project, request.source_id, request.source_role)?;

    let proxy = MediaProxy {
        frame_directory: request.frame_dir.to_string_lossy().to_string(),
        audio_path: request.audio.map(|path| path.to_string_lossy().to_string()),
    };

    let caches = request
        .analysis_cache
        .iter()
        .map(|spec| parse_analysis_cache_spec(spec, &source_id))
        .collect::<Result<Vec<_>, _>>()?;
    let cache_count = caches.len();

    project.register_source_proxy(&source_id, proxy, caches)?;
    project.validate()?;

    fs::write(
        request.project_path,
        serde_json::to_string_pretty(&project)?,
    )?;
    println!(
        "registered proxy for source '{}' with {} analysis-cache reference(s) in {}",
        source_id,
        cache_count,
        request.project_path.display()
    );
    Ok(())
}

pub(crate) fn resolve_project_source_id(
    project: &Project,
    source_id: Option<&str>,
    source_role: Option<SourceRole>,
) -> Result<String, CliError> {
    match (source_id, source_role) {
        (Some(source_id), None) => Ok(source_id.to_string()),
        (None, Some(source_role)) => {
            let mut matching_sources = project
                .sources
                .iter()
                .filter(|source| source.role == source_role);
            let source = matching_sources.next().ok_or_else(|| {
                CliError::Message(format!(
                    "project has no {:?} source to register a proxy for",
                    source_role
                ))
            })?;
            if matching_sources.next().is_some() {
                return Err(CliError::Message(format!(
                    "project has multiple {:?} sources; use --source-id",
                    source_role
                )));
            }
            Ok(source.id.clone())
        }
        _ => Err(CliError::Message(
            "provide exactly one of --source-id or --source-role".to_string(),
        )),
    }
}

pub(crate) fn parse_analysis_cache_spec(
    spec: &str,
    source_id: &str,
) -> Result<AnalysisCacheEntry, CliError> {
    let (kind_name, path) = spec.split_once('=').ok_or_else(|| {
        CliError::Message(format!(
            "analysis-cache '{spec}' must be in the form kind=path"
        ))
    })?;
    if path.trim().is_empty() {
        return Err(CliError::Message(format!(
            "analysis-cache '{spec}' has an empty path"
        )));
    }
    let kind = parse_analysis_kind(kind_name)?;

    Ok(AnalysisCacheEntry {
        id: format!("cache-{}-{}", kind_name.trim(), source_id),
        source_id: source_id.to_string(),
        kind,
        path: path.to_string(),
        frame_count: None,
        sample_count: None,
    })
}

pub(crate) fn parse_analysis_kind(name: &str) -> Result<AnalysisKind, CliError> {
    match name.trim() {
        "luminance" => Ok(AnalysisKind::Luminance),
        "edge_map" => Ok(AnalysisKind::EdgeMap),
        "optical_flow" => Ok(AnalysisKind::OpticalFlow),
        "depth_map" => Ok(AnalysisKind::DepthMap),
        "audio_rms" => Ok(AnalysisKind::AudioRms),
        "spectral_centroid" => Ok(AnalysisKind::SpectralCentroid),
        "onset_strength" => Ok(AnalysisKind::OnsetStrength),
        "stft" => Ok(AnalysisKind::Stft),
        "grain_descriptors" => Ok(AnalysisKind::GrainDescriptors),
        other => Err(CliError::Message(format!(
            "unknown analysis kind '{other}'"
        ))),
    }
}
pub(crate) fn cache_synthetic_flow(
    output_dir: &Path,
    width: u32,
    height: u32,
) -> Result<(), CliError> {
    let flow = synthetic_flow(width, height)?;
    let manifest = write_flow_cache(output_dir, &flow, "synthetic_swirl_v1")?;
    println!(
        "wrote synthetic flow cache {}x{} with {} frame(s) to {}",
        manifest.width,
        manifest.height,
        manifest.frames.len(),
        output_dir.display()
    );
    Ok(())
}

pub(crate) fn cache_luminance_flow(
    modulator_image: &Path,
    output_dir: &Path,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(), CliError> {
    let modulator = load_image_f32(modulator_image)?;
    let width = width.unwrap_or(modulator.width);
    let height = height.unwrap_or(modulator.height);
    let flow = luminance_gradient_flow_cpu(&modulator, width, height)?;
    let manifest = write_flow_cache(output_dir, &flow, "luminance_gradient_cpu_v1")?;
    println!(
        "wrote luminance flow cache {}x{} with {} frame(s) to {}",
        manifest.width,
        manifest.height,
        manifest.frames.len(),
        output_dir.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use morphogen_render::ImageBufferF32;

    fn write_solid_frame(path: &Path, width: u32, height: u32, value: f32) {
        let image = ImageBufferF32::from_fn(width, height, |_, _| [value, value, value, 1.0])
            .expect("build frame");
        save_png(&image, path).expect("write frame");
    }

    #[test]
    fn downscale_frames_rejects_scale_zero() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let input_dir = temp_dir.path().join("in");
        let output_dir = temp_dir.path().join("out");
        fs::create_dir_all(&input_dir).expect("create input dir");
        write_solid_frame(&input_dir.join("frame_000000.png"), 4, 4, 0.5);

        let error = downscale_frames(&input_dir, &output_dir, 0, None)
            .expect_err("scale 0 must be rejected");
        assert_eq!(error.to_string(), "scale must be an integer >= 1");
    }

    #[test]
    fn downscale_frames_respects_max_frames_cap_in_sorted_order() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let input_dir = temp_dir.path().join("in");
        let output_dir = temp_dir.path().join("out");
        fs::create_dir_all(&input_dir).expect("create input dir");
        for index in 0..3u32 {
            write_solid_frame(
                &input_dir.join(format!("frame_{index:06}.png")),
                4,
                4,
                index as f32 * 0.25,
            );
        }

        downscale_frames(&input_dir, &output_dir, 2, Some(2)).expect("downscale frames");

        assert!(output_dir.join("frame_000000.png").exists());
        assert!(output_dir.join("frame_000001.png").exists());
        assert!(
            !output_dir.join("frame_000002.png").exists(),
            "max-frames must cap processing in sorted order"
        );
    }

    #[test]
    fn downscale_frames_output_names_mirror_input_basenames() {
        let temp_dir = tempfile::tempdir().expect("create temp dir");
        let input_dir = temp_dir.path().join("in");
        let output_dir = temp_dir.path().join("out");
        fs::create_dir_all(&input_dir).expect("create input dir");
        write_solid_frame(&input_dir.join("my_odd_name.png"), 6, 6, 0.75);

        downscale_frames(&input_dir, &output_dir, 3, None).expect("downscale frames");

        let output_path = output_dir.join("my_odd_name.png");
        assert!(
            output_path.exists(),
            "output basename must mirror the input basename"
        );
        let decoded = image::ImageReader::open(&output_path)
            .expect("open output frame")
            .decode()
            .expect("decode output frame");
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
    }
}
