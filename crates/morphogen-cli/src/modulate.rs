//! CLI-side modulation-plan builder.
//!
//! Parses `--modulate` route specs, extracts each referenced descriptor
//! envelope exactly once from the modulator media (the CLI owns image/audio
//! decoding), and evaluates the routed knob values at each output frame's
//! time. The engine (route grammar, sampling, per-effect apply/clamp) lives in
//! `morphogen_render::modulation`; contract in
//! `docs/MODULATION_MATRIX_MILESTONE.md`.

use std::path::Path;

use morphogen_audio::{
    load_wav_f32, onset_strength_from_stft, rms_envelope, spectral_centroid_from_magnitudes,
    stft_magnitude_cache, StftConfig, WindowFunction,
};
use morphogen_render::{
    modulated_value, parse_modulation_route, peak_normalize, validate_route_targets,
    ModulationRoute, ModulationSampling, ModulationSource,
};

use crate::audio::{build_flow_magnitude_samples, build_luma_samples};
use crate::error::CliError;
use crate::render::DatamoshSequenceSettings;

/// Analysis defaults fixed by the milestone contract (recorded there, not knobs).
const MODULATION_WINDOW: usize = 2048;
const MODULATION_HOP: usize = 512;

/// Everything needed to evaluate routed knob values per output frame.
pub(crate) struct ModulationPlan {
    routes: Vec<(ModulationRoute, Vec<(f64, f32)>)>,
    sampling: ModulationSampling,
    fps: f64,
}

impl ModulationPlan {
    /// Routed `(target, mapped value)` pairs for output frame `index` (the
    /// caller applies them through the effect's clamping apply function).
    /// The resolved routes in CLI order — the canonical form persisted into a
    /// stateful effect's sequence contract.
    pub(crate) fn route_list(&self) -> Vec<ModulationRoute> {
        self.routes.iter().map(|(route, _)| route.clone()).collect()
    }

    pub(crate) fn frame_values(&self, index: usize) -> impl Iterator<Item = (&str, f32)> + '_ {
        let t = index as f64 / self.fps;
        self.routes.iter().map(move |(route, samples)| {
            (
                route.target.as_str(),
                modulated_value(route, samples, t, self.sampling),
            )
        })
    }

    /// One-line human summary of the resolved routes (printed by the CLI —
    /// the direct-render analogue of manifest provenance).
    pub(crate) fn describe(&self) -> String {
        self.routes
            .iter()
            .map(|(route, _)| {
                format!(
                    "{}={}:{},{}",
                    route.target,
                    route.source.name(),
                    route.scale,
                    route.offset
                )
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Datamosh modulation targets. The apply function lives CLI-side (unlike the
/// other effects') because `DatamoshSequenceSettings` is itself a CLI-side
/// struct. Excluded by the milestone's stateful-target rule: `keyframe_interval`
/// restructures the sequence; `block_size` selects the bloom-vs-macroblock code
/// path and the tier state topology; `vector_remix`/`remix_seed`/`preset`/
/// `scanline_smear`/`codec_engrave` are structural mode/seed selections.
pub(crate) const DATAMOSH_MODULATION_TARGETS: &[&str] = &[
    "amount",
    "residual_gain",
    "residual_decay",
    "refresh_threshold",
];

/// Overwrite one routed datamosh knob with a mapped value. Clamps mirror
/// `render_datamosh_sequence`'s own validation (finite, non-negative); the
/// upper bound is the shared modulation pixel range, so a runaway
/// `scale·envelope` can never abort a frame mid-sequence (clamp-never-error).
pub(crate) fn apply_datamosh_modulation(
    settings: &mut DatamoshSequenceSettings,
    target: &str,
    value: f32,
) -> Result<(), CliError> {
    let clamped = value.clamp(0.0, 4096.0);
    match target {
        "amount" => settings.amount = clamped,
        "residual_gain" => settings.residual_gain = clamped,
        "residual_decay" => settings.residual_decay = clamped,
        "refresh_threshold" => settings.refresh_threshold = clamped,
        _ => {
            return Err(CliError::Message(format!(
                "unknown datamosh modulation target '{target}' (available: {})",
                DATAMOSH_MODULATION_TARGETS.join(", ")
            )))
        }
    }
    Ok(())
}

pub(crate) struct ModulationRequest<'a> {
    pub(crate) specs: &'a [String],
    pub(crate) modulator_audio: Option<&'a Path>,
    pub(crate) modulator_frames: Option<&'a Path>,
    pub(crate) sampling: ModulationSampling,
    /// Maps output frame index → seconds; also the modulator frame timeline.
    pub(crate) fps: f64,
}

/// Build the plan, or `None` when no routes are given (the exact off path).
pub(crate) fn build_modulation_plan(
    request: ModulationRequest<'_>,
) -> Result<Option<ModulationPlan>, CliError> {
    if request.specs.is_empty() {
        return Ok(None);
    }
    if !request.fps.is_finite() || request.fps <= 0.0 {
        return Err(CliError::Message(
            "modulation fps must be positive and finite".to_string(),
        ));
    }

    let routes = request
        .specs
        .iter()
        .map(|spec| parse_modulation_route(spec))
        .collect::<Result<Vec<_>, _>>()?;
    validate_route_targets(&routes)?;

    let mut envelopes: Vec<(ModulationSource, Vec<(f64, f32)>)> = Vec::new();
    for route in &routes {
        if envelopes.iter().any(|(source, _)| *source == route.source) {
            continue;
        }
        let samples = extract_envelope(route.source, &request)?;
        envelopes.push((route.source, samples));
    }

    let routes = routes
        .into_iter()
        .map(|route| {
            let samples = envelopes
                .iter()
                .find(|(source, _)| *source == route.source)
                .map(|(_, samples)| samples.clone())
                .unwrap_or_default();
            (route, samples)
        })
        .collect();

    Ok(Some(ModulationPlan {
        routes,
        sampling: request.sampling,
        fps: request.fps,
    }))
}

fn extract_envelope(
    source: ModulationSource,
    request: &ModulationRequest<'_>,
) -> Result<Vec<(f64, f32)>, CliError> {
    match source {
        ModulationSource::AudioRms => {
            let buffer = load_modulator_wav(source, request)?;
            let frames = rms_envelope(&buffer, MODULATION_WINDOW, MODULATION_HOP)?;
            let mut samples: Vec<(f64, f32)> = frames
                .iter()
                .map(|frame| (frame.time_seconds, frame.rms))
                .collect();
            peak_normalize(&mut samples);
            Ok(samples)
        }
        ModulationSource::AudioOnset => {
            let buffer = load_modulator_wav(source, request)?;
            let stft = stft_magnitude_cache(&buffer, modulation_stft_config())?;
            let onsets = onset_strength_from_stft(&stft)?;
            let mut samples: Vec<(f64, f32)> = onsets
                .frames
                .iter()
                .map(|frame| (frame.time_seconds, frame.strength))
                .collect();
            peak_normalize(&mut samples);
            Ok(samples)
        }
        ModulationSource::AudioCentroid => {
            let buffer = load_modulator_wav(source, request)?;
            let stft = stft_magnitude_cache(&buffer, modulation_stft_config())?;
            let nyquist = stft.sample_rate as f32 / 2.0;
            let mut samples = Vec::with_capacity(stft.frames.len());
            for frame in &stft.frames {
                let centroid = spectral_centroid_from_magnitudes(
                    &frame.magnitudes,
                    stft.fft_size,
                    stft.sample_rate,
                )?;
                // Absolute normalization: centroid / Nyquist ∈ [0, 1].
                samples.push((frame.time_seconds, centroid / nyquist));
            }
            Ok(samples)
        }
        ModulationSource::Luma => {
            let frames_dir = require_modulator_frames(source, request)?;
            // Mean Rec.709 luma is already absolute [0, 1].
            build_luma_samples(frames_dir, request.fps, None)
        }
        ModulationSource::Flow => {
            let frames_dir = require_modulator_frames(source, request)?;
            let mut samples = build_flow_magnitude_samples(frames_dir, request.fps, None)?;
            peak_normalize(&mut samples);
            Ok(samples)
        }
    }
}

fn modulation_stft_config() -> StftConfig {
    StftConfig {
        fft_size: MODULATION_WINDOW,
        hop_size: MODULATION_HOP,
        window: WindowFunction::Hann,
    }
}

fn load_modulator_wav(
    source: ModulationSource,
    request: &ModulationRequest<'_>,
) -> Result<morphogen_audio::AudioBufferF32, CliError> {
    let path = request.modulator_audio.ok_or_else(|| {
        CliError::Message(format!(
            "modulation source '{}' requires --modulator-audio <wav>",
            source.name()
        ))
    })?;
    Ok(load_wav_f32(path)?)
}

fn require_modulator_frames<'a>(
    source: ModulationSource,
    request: &ModulationRequest<'a>,
) -> Result<&'a Path, CliError> {
    request.modulator_frames.ok_or_else(|| {
        CliError::Message(format!(
            "modulation source '{}' requires --modulator-frames <dir>",
            source.name()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use morphogen_core::DatamoshPreset;

    #[test]
    fn datamosh_targets_clamp_to_render_validation_ranges() {
        let mut settings = DatamoshSequenceSettings {
            keyframe_interval: 0,
            amount: 1.0,
            block_size: 16,
            residual_gain: 0.5,
            residual_decay: 0.8,
            refresh_threshold: 0.0,
            vector_remix: "none".to_string(),
            remix_seed: 0,
            preset: DatamoshPreset::Custom,
            scanline_smear: false,
            codec_engrave: false,
        };
        apply_datamosh_modulation(&mut settings, "amount", -3.0).unwrap();
        assert_eq!(settings.amount, 0.0);
        apply_datamosh_modulation(&mut settings, "amount", 99999.0).unwrap();
        assert_eq!(settings.amount, 4096.0);
        apply_datamosh_modulation(&mut settings, "residual_gain", 0.25).unwrap();
        assert_eq!(settings.residual_gain, 0.25);
        apply_datamosh_modulation(&mut settings, "residual_decay", -1.0).unwrap();
        assert_eq!(settings.residual_decay, 0.0);
        apply_datamosh_modulation(&mut settings, "refresh_threshold", 1.5).unwrap();
        assert_eq!(settings.refresh_threshold, 1.5);
        // Structural knobs are deliberately not targets: keyframe_interval
        // restructures the sequence, block_size selects the code path and the
        // tier state topology, the rest are mode/seed selections.
        for excluded in [
            "keyframe_interval",
            "block_size",
            "vector_remix",
            "remix_seed",
            "preset",
            "scanline_smear",
            "codec_engrave",
        ] {
            assert!(
                apply_datamosh_modulation(&mut settings, excluded, 1.0).is_err(),
                "'{excluded}' must not be a modulation target"
            );
        }
    }
}
