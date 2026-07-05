use std::fs;
use std::path::{Path, PathBuf};

use image::{imageops, ImageBuffer, ImageReader, Rgba};
use morphogen_core::{DatamoshPreset, FlowSource, RenderBackend};
use morphogen_media::{CommandSpec, MediaError};
use morphogen_render::{
    FlowFeedbackSettings, GranularMosaicSettings, StructureMode, VectorRemixMode,
};
use serde::Serialize;
use serde_json::json;

use crate::args::CliShowcaseIntensity;
use crate::error::CliError;
use crate::imaging::collect_image_frames;
use crate::render::{
    render_datamosh_sequence, render_feedback_sequence, render_frame_sequence,
    render_granular_mosaic_pool_sequence, DatamoshSequenceRequest, FeedbackSequenceRenderRequest,
    FrameSequenceRenderRequest, GranularMosaicPoolSequenceRequest, ModulationCliArgs,
    RmsAmountConfig,
};

pub(crate) struct ShowcaseRenderRequest<'a> {
    pub(crate) modulator_dir: &'a Path,
    pub(crate) carrier_dir: &'a Path,
    pub(crate) output_dir: &'a Path,
    pub(crate) intensity: CliShowcaseIntensity,
    pub(crate) frames_per_effect: usize,
    pub(crate) frame_rate: f64,
    pub(crate) granular_grain_size: u32,
    pub(crate) seed: u64,
    pub(crate) backend: RenderBackend,
    pub(crate) encode_mp4: bool,
}

#[derive(Debug, Clone, Copy)]
struct ShowcaseTuning {
    flow_amount: f32,
    feedback_carrier_amount: f32,
    feedback_amount: f32,
    feedback_mix: f32,
    feedback_decay: f32,
    feedback_structure_mix: f32,
    granular_variation: f32,
    granular_anti_repeat: f32,
    datamosh_amount: f32,
    datamosh_block_size: u32,
    datamosh_residual_gain: f32,
    datamosh_residual_decay: f32,
}

#[derive(Debug, Serialize)]
struct ShowcaseSegmentManifest {
    name: &'static str,
    slug: &'static str,
    source: &'static str,
    first_frame: usize,
    frame_count: usize,
    representative_still: String,
    settings: serde_json::Value,
}

pub(crate) fn render_showcase(request: ShowcaseRenderRequest<'_>) -> Result<(), CliError> {
    validate_showcase_request(&request)?;

    let available_frames = collect_image_frames(request.modulator_dir)?
        .len()
        .min(collect_image_frames(request.carrier_dir)?.len());
    if available_frames == 0 {
        return Err(CliError::Message(
            "showcase render requires at least one PNG frame in both source directories"
                .to_string(),
        ));
    }
    let frames_per_effect = request.frames_per_effect.min(available_frames);
    let tuning = showcase_tuning(request.intensity);

    fs::create_dir_all(request.output_dir)?;
    let segments_dir = request.output_dir.join("segments");
    let frames_dir = request.output_dir.join("frames");
    let stills_dir = request.output_dir.join("stills");
    reset_owned_dir(&segments_dir)?;
    reset_owned_dir(&frames_dir)?;
    reset_owned_dir(&stills_dir)?;

    let flow_dir = segments_dir.join("01_flow_displace");
    let feedback_dir = segments_dir.join("02_flow_feedback");
    let granular_dir = segments_dir.join("03_granular_mosaic");
    let datamosh_dir = segments_dir.join("04_vector_datamosh");

    render_frame_sequence(FrameSequenceRenderRequest {
        modulator_dir: request.modulator_dir,
        carrier_dir: request.carrier_dir,
        output_dir: &flow_dir,
        amount: tuning.flow_amount,
        flow_cache_dir: None,
        max_frames: Some(frames_per_effect),
        backend: request.backend,
        rms: RmsAmountConfig {
            wav_path: None,
            frame_rate: request.frame_rate,
            window_size: 2048,
            hop_size: 512,
            amount_scale: tuning.flow_amount,
        },
    })?;

    render_feedback_sequence(FeedbackSequenceRenderRequest {
        modulator_dir: request.modulator_dir,
        carrier_dir: request.carrier_dir,
        output_dir: &feedback_dir,
        flow_cache_dir: None,
        max_frames: Some(frames_per_effect),
        reset_at_frame: None,
        frame_rate: request.frame_rate,
        settings: FlowFeedbackSettings {
            carrier_amount: tuning.feedback_carrier_amount,
            feedback_amount: tuning.feedback_amount,
            feedback_mix: tuning.feedback_mix,
            decay: tuning.feedback_decay,
            iterations: 1,
            structure_mix: tuning.feedback_structure_mix,
            structure_mode: StructureMode::SingleScale,
        },
        output_bit_depth: 8,
        temporal_supersampling: 1,
        backend: request.backend,
        flow_source: FlowSource::OpticalFlow,
        job_id: "showcase-feedback",
        provenance: None,
        stop_after_frame: false,
        modulation: ModulationCliArgs::default(),
    })?;

    render_granular_mosaic_pool_sequence(GranularMosaicPoolSequenceRequest {
        modulator_dir: request.modulator_dir,
        carrier_dir: request.carrier_dir,
        output_dir: &granular_dir,
        settings: GranularMosaicSettings {
            grain_size: request.granular_grain_size,
            rearrangement: 1.0,
            variation: tuning.granular_variation,
            seed: request.seed,
        },
        audio_weight: 1.0,
        texture_weight: 1.0,
        modulator_rms_cache: None,
        carrier_rms_cache: None,
        modulator_centroid_cache: None,
        carrier_centroid_cache: None,
        pool_window: 0,
        anti_repeat_weight: tuning.granular_anti_repeat,
        anti_repeat_cooldown: 8,
        coherence_weight: 0.1,
        coherence_reach: 8,
        spatial_coherence_weight: 0.1,
        frame_rate: request.frame_rate,
        max_frames: Some(frames_per_effect),
        grain_cache_dir: None,
        backend: request.backend,
        carrier_wav_path: None,
    })?;

    render_datamosh_sequence(DatamoshSequenceRequest {
        modulator_dir: request.modulator_dir,
        carrier_dir: request.carrier_dir,
        output_dir: &datamosh_dir,
        flow_cache_dir: None,
        keyframe_interval: 0,
        amount: tuning.datamosh_amount,
        block_size: tuning.datamosh_block_size,
        residual_gain: tuning.datamosh_residual_gain,
        residual_decay: tuning.datamosh_residual_decay,
        refresh_threshold: 0.0,
        vector_remix: VectorRemixMode::Shuffle,
        remix_seed: request.seed,
        preset: DatamoshPreset::Custom,
        backend: request.backend,
        max_frames: Some(frames_per_effect),
        job_id: "showcase-datamosh",
        provenance: None,
        stop_after_frame: false,
        modulation: ModulationCliArgs::default(),
    })?;

    let feedback_frames_dir = feedback_dir.join("frames");
    let segment_sources = [
        (
            "Flow Displace",
            "flow_displace",
            &flow_dir,
            json!({ "amount": tuning.flow_amount }),
        ),
        (
            "Flow Feedback",
            "flow_feedback",
            &feedback_frames_dir,
            json!({
                "carrier_amount": tuning.feedback_carrier_amount,
                "feedback_amount": tuning.feedback_amount,
                "feedback_mix": tuning.feedback_mix,
                "decay": tuning.feedback_decay,
                "structure_mix": tuning.feedback_structure_mix
            }),
        ),
        (
            "Granular Mosaic",
            "granular_mosaic",
            &granular_dir,
            json!({
                "grain_size": request.granular_grain_size,
                "variation": tuning.granular_variation,
                "texture_weight": 1.0,
                "anti_repeat_weight": tuning.granular_anti_repeat
            }),
        ),
        (
            "Vector Datamosh",
            "vector_datamosh",
            &datamosh_dir,
            json!({
                "amount": tuning.datamosh_amount,
                "block_size": tuning.datamosh_block_size,
                "residual_gain": tuning.datamosh_residual_gain,
                "residual_decay": tuning.datamosh_residual_decay,
                "vector_remix": "shuffle"
            }),
        ),
    ];

    let mut next_output_frame = 0usize;
    let mut manifests = Vec::new();
    let mut representative_stills = Vec::new();
    for (segment_index, (name, slug, source_dir, settings)) in segment_sources.iter().enumerate() {
        let source_frames = collect_image_frames(source_dir)?;
        let segment_frames = source_frames.len().min(frames_per_effect);
        if segment_frames == 0 {
            return Err(CliError::Message(format!(
                "showcase segment '{}' produced no frames",
                name
            )));
        }
        let first_frame = next_output_frame;
        for source in source_frames.iter().take(segment_frames) {
            fs::copy(
                source,
                frames_dir.join(format!("frame_{next_output_frame:06}.png")),
            )?;
            next_output_frame += 1;
        }

        let representative = source_frames
            .get(segment_frames - 1)
            .ok_or_else(|| CliError::Message("missing representative frame".to_string()))?;
        let still_relative = format!("stills/{:02}_{slug}.png", segment_index + 1);
        let still_path = request.output_dir.join(&still_relative);
        fs::copy(representative, &still_path)?;
        representative_stills.push(still_path);
        manifests.push(ShowcaseSegmentManifest {
            name,
            slug,
            source: match *slug {
                "flow_displace" => "Source A luminance flow displaces Source B",
                "flow_feedback" => "Source A optical flow pushes Source B through feedback",
                "granular_mosaic" => "Source A descriptors select temporal grains from Source B",
                _ => "Source A optical flow reuses motion over Source B",
            },
            first_frame,
            frame_count: segment_frames,
            representative_still: still_relative,
            settings: settings.clone(),
        });
    }

    let contact_sheet = request.output_dir.join("contact_sheet.png");
    write_contact_sheet(&representative_stills, &contact_sheet)?;

    let mp4_path = request.output_dir.join("showcase.mp4");
    let encoded_mp4 = if request.encode_mp4 {
        encode_showcase_mp4(&frames_dir, &mp4_path, request.frame_rate)?;
        Some(path_relative_to_output(request.output_dir, &mp4_path))
    } else {
        None
    };

    let manifest = json!({
        "task": "render_showcase",
        "intensity": showcase_intensity_label(request.intensity),
        "modulator_dir": request.modulator_dir.to_string_lossy(),
        "carrier_dir": request.carrier_dir.to_string_lossy(),
        "frame_rate": request.frame_rate,
        "frames_per_effect": frames_per_effect,
        "total_frames": next_output_frame,
        "backend": format!("{:?}", request.backend),
        "segments": manifests,
        "outputs": {
            "frames": "frames",
            "contact_sheet": "contact_sheet.png",
            "mp4": encoded_mp4
        }
    });
    fs::write(
        request.output_dir.join("showcase.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    println!(
        "rendered {} showcase with {} frame(s) at {}",
        showcase_intensity_label(request.intensity),
        next_output_frame,
        request.output_dir.display()
    );
    if request.encode_mp4 {
        println!("wrote iPhone-compatible MP4 to {}", mp4_path.display());
    }
    println!("wrote contact sheet to {}", contact_sheet.display());
    Ok(())
}

pub(crate) fn showcase_mp4_command(
    frames_dir: &Path,
    output_path: &Path,
    frame_rate: f64,
) -> CommandSpec {
    CommandSpec::new(
        "ffmpeg",
        vec![
            "-y".to_string(),
            "-framerate".to_string(),
            format_frame_rate(frame_rate),
            "-i".to_string(),
            frames_dir
                .join("frame_%06d.png")
                .to_string_lossy()
                .to_string(),
            "-r".to_string(),
            "24".to_string(),
            "-c:v".to_string(),
            "libx264".to_string(),
            "-crf".to_string(),
            "18".to_string(),
            "-preset".to_string(),
            "medium".to_string(),
            "-pix_fmt".to_string(),
            "yuv420p".to_string(),
            "-movflags".to_string(),
            "+faststart".to_string(),
            output_path.to_string_lossy().to_string(),
        ],
    )
}

fn validate_showcase_request(request: &ShowcaseRenderRequest<'_>) -> Result<(), CliError> {
    if request.frames_per_effect == 0 {
        return Err(CliError::Message(
            "frames-per-effect must be greater than zero".to_string(),
        ));
    }
    if !request.frame_rate.is_finite() || request.frame_rate <= 0.0 {
        return Err(CliError::Message(
            "frame-rate must be a positive finite number".to_string(),
        ));
    }
    if request.granular_grain_size == 0 {
        return Err(CliError::Message(
            "granular-grain-size must be greater than zero".to_string(),
        ));
    }
    Ok(())
}

fn showcase_tuning(intensity: CliShowcaseIntensity) -> ShowcaseTuning {
    match intensity {
        CliShowcaseIntensity::Balanced => ShowcaseTuning {
            flow_amount: 42.0,
            feedback_carrier_amount: 16.0,
            feedback_amount: 48.0,
            feedback_mix: 0.84,
            feedback_decay: 0.985,
            feedback_structure_mix: 0.4,
            granular_variation: 0.75,
            granular_anti_repeat: 0.2,
            datamosh_amount: 2.5,
            datamosh_block_size: 16,
            datamosh_residual_gain: 0.25,
            datamosh_residual_decay: 0.92,
        },
        CliShowcaseIntensity::Destructive => ShowcaseTuning {
            flow_amount: 120.0,
            feedback_carrier_amount: 30.0,
            feedback_amount: 90.0,
            feedback_mix: 0.94,
            feedback_decay: 0.99,
            feedback_structure_mix: 0.6,
            granular_variation: 1.0,
            granular_anti_repeat: 0.45,
            datamosh_amount: 10.0,
            datamosh_block_size: 32,
            datamosh_residual_gain: 0.8,
            datamosh_residual_decay: 0.98,
        },
    }
}

fn showcase_intensity_label(intensity: CliShowcaseIntensity) -> &'static str {
    match intensity {
        CliShowcaseIntensity::Balanced => "balanced",
        CliShowcaseIntensity::Destructive => "destructive",
    }
}

fn reset_owned_dir(path: &Path) -> Result<(), CliError> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}

fn write_contact_sheet(stills: &[PathBuf], output_path: &Path) -> Result<(), CliError> {
    if stills.is_empty() {
        return Err(CliError::Message(
            "cannot write a showcase contact sheet without stills".to_string(),
        ));
    }

    let cell_width = 400u32;
    let decoded = stills
        .iter()
        .map(|path| {
            let image = ImageReader::open(path)?.decode()?.to_rgba8();
            let height = ((cell_width as f32 / image.width() as f32) * image.height() as f32)
                .round()
                .max(1.0) as u32;
            Ok(imageops::resize(
                &image,
                cell_width,
                height,
                imageops::FilterType::Triangle,
            ))
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let cell_height = decoded
        .iter()
        .map(ImageBuffer::height)
        .max()
        .ok_or_else(|| CliError::Message("missing contact-sheet height".to_string()))?;
    let mut sheet = ImageBuffer::from_pixel(
        cell_width * decoded.len() as u32,
        cell_height,
        Rgba([0, 0, 0, u8::MAX]),
    );
    for (index, image) in decoded.iter().enumerate() {
        imageops::replace(&mut sheet, image, i64::from(index as u32 * cell_width), 0);
    }
    sheet.save(output_path)?;
    Ok(())
}

fn encode_showcase_mp4(
    frames_dir: &Path,
    output_path: &Path,
    frame_rate: f64,
) -> Result<(), CliError> {
    let spec = showcase_mp4_command(frames_dir, output_path, frame_rate);
    let output = spec.to_command().output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::Media(MediaError::MissingBinary {
                binary: spec.program.clone(),
            })
        } else {
            CliError::Io(error)
        }
    })?;
    if !output.status.success() {
        return Err(CliError::Media(MediaError::CommandFailed {
            binary: spec.program,
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        }));
    }
    Ok(())
}

fn format_frame_rate(frame_rate: f64) -> String {
    if frame_rate.fract() == 0.0 {
        format!("{frame_rate:.0}")
    } else {
        format!("{frame_rate:.6}")
    }
}

fn path_relative_to_output(output_dir: &Path, path: &Path) -> String {
    path.strip_prefix(output_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string()
}
