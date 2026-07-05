//! CLI-side modulation-plan builder.
//!
//! Parses `--modulate` route specs, extracts each referenced descriptor
//! envelope exactly once from the modulator media (the CLI owns image/audio
//! decoding), and evaluates the routed knob values at each output frame's
//! time. The engine (route grammar, sampling, per-effect apply/clamp) lives in
//! `morphogen_render::modulation`; contract in
//! `docs/MODULATION_MATRIX_MILESTONE.md`.

use std::fs;
use std::path::{Path, PathBuf};

use morphogen_audio::{
    load_wav_f32, onset_strength_from_stft, rms_envelope, spectral_centroid_from_magnitudes,
    stft_magnitude_cache, StftConfig, WindowFunction,
};
use morphogen_render::{
    modulated_value, parse_modulation_route, peak_normalize, validate_route_targets,
    ModulationRoute, ModulationSampling, ModulationSource,
};
use serde::{Deserialize, Serialize};

use crate::audio::{build_edge_density_samples, build_flow_magnitude_samples, build_luma_samples};
use crate::error::CliError;
use crate::render::{feedback_modulation_frames_fingerprint, DatamoshSequenceSettings};

/// Analysis defaults fixed by the milestone contract (recorded there, not knobs).
const MODULATION_WINDOW: usize = 2048;
const MODULATION_HOP: usize = 512;

/// Everything needed to evaluate routed knob values per output frame.
pub(crate) struct ModulationPlan {
    routes: Vec<(ModulationRoute, Vec<(f64, f32)>)>,
    sampling: ModulationSampling,
    fps: f64,
}

/// Envelope identity: which modulator's media, analyzed by which descriptor.
type EnvelopeKey = (Option<String>, ModulationSource);

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
                // A route's own sampling (`@hold`/`@smooth`) overrides the
                // render-level default.
                modulated_value(route, samples, t, route.sampling.unwrap_or(self.sampling)),
            )
        })
    }

    /// One-line human summary of the resolved routes (printed by the CLI —
    /// the direct-render analogue of manifest provenance).
    pub(crate) fn describe(&self) -> String {
        self.routes
            .iter()
            .map(|(route, _)| {
                let suffix = match route.sampling {
                    Some(ModulationSampling::Hold) => "@hold",
                    Some(ModulationSampling::Smooth) => "@smooth",
                    None => "",
                };
                let modulator = route
                    .modulator
                    .as_deref()
                    .map(|name| format!("{name}."))
                    .unwrap_or_default();
                format!(
                    "{}={modulator}{}:{},{}{suffix}",
                    route.target,
                    route.source.spec_text(),
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
    /// Optional sidecar directory for extracted luma/flow envelopes (the
    /// per-frame extraction paths; audio envelopes are cheap and not cached).
    /// Reuse only on a full algorithm/fps/content-fingerprint match — like
    /// every analysis sidecar, it never joins a render's contract.
    pub(crate) cache_dir: Option<&'a Path>,
    /// Raw `<name>=<path>` specs for named modulators (routes reference them
    /// as `<name>.<source>`); the unnamed flags above stay the default
    /// modulator.
    pub(crate) named_modulator_audio: &'a [String],
    pub(crate) named_modulator_frames: &'a [String],
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
    let named_audio =
        parse_named_modulator_specs(request.named_modulator_audio, "--named-modulator-audio")?;
    let named_frames =
        parse_named_modulator_specs(request.named_modulator_frames, "--named-modulator-frames")?;

    // Each distinct (modulator, source) pair is extracted exactly once.
    let mut envelopes: Vec<(EnvelopeKey, Vec<(f64, f32)>)> = Vec::new();
    for route in &routes {
        let key: EnvelopeKey = (route.modulator.clone(), route.source);
        if envelopes.iter().any(|(existing, _)| *existing == key) {
            continue;
        }
        let samples = extract_envelope(route, &request, &named_audio, &named_frames)?;
        envelopes.push((key, samples));
    }

    let routes = routes
        .into_iter()
        .map(|route| {
            let key: EnvelopeKey = (route.modulator.clone(), route.source);
            let samples = envelopes
                .iter()
                .find(|(existing, _)| *existing == key)
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

/// Parse repeatable `<name>=<path>` named-modulator specs; duplicate names
/// are ambiguous and rejected.
pub(crate) fn parse_named_modulator_specs(
    specs: &[String],
    flag: &str,
) -> Result<Vec<(String, PathBuf)>, CliError> {
    let mut named = Vec::new();
    for spec in specs {
        let (name, path) = spec.split_once('=').ok_or_else(|| {
            CliError::Message(format!("invalid {flag} '{spec}' (expected <name>=<path>)"))
        })?;
        let name = name.trim();
        if name.is_empty() {
            return Err(CliError::Message(format!(
                "invalid {flag} '{spec}': empty modulator name"
            )));
        }
        if named.iter().any(|(existing, _)| existing == name) {
            return Err(CliError::Message(format!("duplicate {flag} name '{name}'")));
        }
        named.push((name.to_string(), PathBuf::from(path.trim())));
    }
    Ok(named)
}

/// Resolve the media a route reads: the default `--modulator-*` flags for an
/// unnamed route, or the same-named `--named-modulator-*` entry.
/// `default_flag_hint` keeps the pre-slice error text for unnamed routes
/// (e.g. `--modulator-audio <wav>`).
pub(crate) fn resolve_modulator_media<'a>(
    route: &ModulationRoute,
    default_media: Option<&'a Path>,
    named: &'a [(String, PathBuf)],
    default_flag_hint: &str,
    named_flag: &str,
) -> Result<&'a Path, CliError> {
    match route.modulator.as_deref() {
        None => default_media.ok_or_else(|| {
            CliError::Message(format!(
                "modulation source '{}' requires {default_flag_hint}",
                route.source.name()
            ))
        }),
        Some(name) => named
            .iter()
            .find(|(existing, _)| existing == name)
            .map(|(_, path)| path.as_path())
            .ok_or_else(|| {
                CliError::Message(format!(
                    "modulation source '{name}.{}' requires {named_flag} {name}=<path>",
                    route.source.name()
                ))
            }),
    }
}

fn extract_envelope(
    route: &ModulationRoute,
    request: &ModulationRequest<'_>,
    named_audio: &[(String, PathBuf)],
    named_frames: &[(String, PathBuf)],
) -> Result<Vec<(f64, f32)>, CliError> {
    let source = route.source;
    let resolve_audio = || {
        resolve_modulator_media(
            route,
            request.modulator_audio,
            named_audio,
            "--modulator-audio <wav>",
            "--named-modulator-audio",
        )
    };
    let resolve_frames = || {
        resolve_modulator_media(
            route,
            request.modulator_frames,
            named_frames,
            "--modulator-frames <dir>",
            "--named-modulator-frames",
        )
    };
    match source {
        // A pure function of (frame_time, params) — no media, no sidecar, no
        // fingerprint. `modulated_value` computes the value directly from
        // the route; this envelope is never consulted.
        ModulationSource::Lfo { .. } => Ok(Vec::new()),
        ModulationSource::AudioRms => {
            let buffer = load_wav_f32(resolve_audio()?)?;
            let frames = rms_envelope(&buffer, MODULATION_WINDOW, MODULATION_HOP)?;
            let mut samples: Vec<(f64, f32)> = frames
                .iter()
                .map(|frame| (frame.time_seconds, frame.rms))
                .collect();
            peak_normalize(&mut samples);
            Ok(samples)
        }
        ModulationSource::AudioOnset => {
            let buffer = load_wav_f32(resolve_audio()?)?;
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
            let buffer = load_wav_f32(resolve_audio()?)?;
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
            let frames_dir = resolve_frames()?;
            cached_frames_envelope(route, frames_dir, request, |dir| {
                // Mean Rec.709 luma is already absolute [0, 1].
                build_luma_samples(dir, request.fps, None)
            })
        }
        ModulationSource::Flow => {
            let frames_dir = resolve_frames()?;
            cached_frames_envelope(route, frames_dir, request, |dir| {
                let mut samples = build_flow_magnitude_samples(dir, request.fps, None)?;
                peak_normalize(&mut samples);
                Ok(samples)
            })
        }
        ModulationSource::EdgeDensity => {
            let frames_dir = resolve_frames()?;
            cached_frames_envelope(route, frames_dir, request, |dir| {
                let mut samples = build_edge_density_samples(dir, request.fps, None)?;
                peak_normalize(&mut samples);
                Ok(samples)
            })
        }
    }
}

/// Envelope-sidecar algorithm identifiers. Bump when the corresponding
/// extraction changes so stale sidecars invalidate.
fn envelope_cache_algorithm(source: ModulationSource) -> &'static str {
    match source {
        ModulationSource::Luma => "modulation_envelope_luma_v1",
        ModulationSource::Flow => "modulation_envelope_flow_v1",
        ModulationSource::EdgeDensity => "modulation_envelope_edge_density_v1",
        // Audio envelopes are not cached.
        _ => unreachable!("only frames-based envelopes are cached"),
    }
}

/// One extracted envelope persisted as a reusable analysis sidecar: algorithm
/// id, the sampling convention (fps + sample count), and the modulator-frames
/// content fingerprint. `samples` are the final normalized values —
/// `serde_json` round-trips finite floats exactly, so a cache hit is
/// byte-identical to a fresh extraction.
#[derive(Serialize, Deserialize)]
struct EnvelopeCacheSidecar {
    algorithm: String,
    fps: f64,
    modulator_frames: String,
    checksum: String,
    frame_count: usize,
    samples: Vec<(f64, f32)>,
}

/// Extract a frames-based envelope through the optional sidecar cache: a
/// matching sidecar (algorithm + fps + content fingerprint) is reused; any
/// mismatch or unreadable sidecar regenerates and overwrites it. Named
/// modulators get their own sidecar file (`envelope_<name>.<source>.json`)
/// so two modulators never collide; the default modulator keeps the
/// unprefixed filename.
fn cached_frames_envelope(
    route: &ModulationRoute,
    frames_dir: &Path,
    request: &ModulationRequest<'_>,
    extract: impl FnOnce(&Path) -> Result<Vec<(f64, f32)>, CliError>,
) -> Result<Vec<(f64, f32)>, CliError> {
    let source = route.source;
    let Some(cache_dir) = request.cache_dir else {
        return extract(frames_dir);
    };
    let envelope_label = match route.modulator.as_deref() {
        Some(name) => format!("{name}.{}", source.name()),
        None => source.name().to_string(),
    };
    let algorithm = envelope_cache_algorithm(source);
    let sidecar_path = cache_dir.join(format!("envelope_{envelope_label}.json"));
    let fingerprint = feedback_modulation_frames_fingerprint(frames_dir)?;

    if let Ok(text) = fs::read_to_string(&sidecar_path) {
        if let Ok(sidecar) = serde_json::from_str::<EnvelopeCacheSidecar>(&text) {
            if sidecar.algorithm == algorithm
                && sidecar.fps == request.fps
                && sidecar.checksum == fingerprint.checksum
            {
                println!(
                    "reused modulation envelope sidecar for '{envelope_label}' from {}",
                    sidecar_path.display()
                );
                return Ok(sidecar.samples);
            }
        }
    }

    let samples = extract(frames_dir)?;
    fs::create_dir_all(cache_dir)?;
    let sidecar = EnvelopeCacheSidecar {
        algorithm: algorithm.to_string(),
        fps: request.fps,
        modulator_frames: fingerprint.path,
        checksum: fingerprint.checksum,
        frame_count: samples.len(),
        samples: samples.clone(),
    };
    fs::write(&sidecar_path, serde_json::to_string_pretty(&sidecar)?)?;
    println!(
        "generated modulation envelope sidecar for '{envelope_label}' at {}",
        sidecar_path.display()
    );
    Ok(samples)
}

fn modulation_stft_config() -> StftConfig {
    StftConfig {
        fft_size: MODULATION_WINDOW,
        hop_size: MODULATION_HOP,
        window: WindowFunction::Hann,
    }
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

    #[test]
    fn pure_lfo_route_builds_a_plan_without_modulator_media() {
        // Acceptance criterion 3: a pure-LFO route with no --modulator-*
        // flags renders — the no-media-needed point of the milestone.
        let request = ModulationRequest {
            specs: &["displacement_depth=lfo(sine,0.5):100".to_string()],
            modulator_audio: None,
            modulator_frames: None,
            sampling: ModulationSampling::Hold,
            fps: 30.0,
            cache_dir: None,
            named_modulator_audio: &[],
            named_modulator_frames: &[],
        };
        let plan = build_modulation_plan(request)
            .unwrap()
            .expect("a non-empty spec list must build a plan");
        let values: Vec<(&str, f32)> = plan.frame_values(0).collect();
        assert_eq!(values, vec![("displacement_depth", 0.0)]);
        assert_eq!(plan.describe(), "displacement_depth=lfo(sine,0.5,0):100,0");
    }

    #[test]
    fn zero_routes_stays_the_exact_unmodulated_path() {
        let request = ModulationRequest {
            specs: &[],
            modulator_audio: None,
            modulator_frames: None,
            sampling: ModulationSampling::Hold,
            fps: 30.0,
            cache_dir: None,
            named_modulator_audio: &[],
            named_modulator_frames: &[],
        };
        assert!(build_modulation_plan(request).unwrap().is_none());
    }
}
