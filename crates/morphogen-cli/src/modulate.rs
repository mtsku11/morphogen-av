//! CLI-side modulation-plan builder.
//!
//! Parses `--modulate` route specs, extracts each referenced descriptor
//! envelope exactly once from the modulator media (the CLI owns image/audio
//! decoding), and evaluates the routed knob values at each output frame's
//! time. The engine (route grammar, sampling, per-effect apply/clamp) lives in
//! `morphogen_render::modulation`; contract in
//! `docs/MODULATION_MATRIX_MILESTONE.md`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use morphogen_audio::{
    load_wav_f32, midi_seconds_for_tick, onset_strength_from_stft, rms_envelope,
    spectral_centroid_from_magnitudes, stft_magnitude_cache, MidiEventKind, MidiFile, StftConfig,
    WindowFunction,
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

/// Pre-extracted envelopes for one route: leaf-source `spec_text()` → samples.
type RouteEnvelopes = HashMap<String, Vec<(f64, f32)>>;

/// Everything needed to evaluate routed knob values per output frame.
pub(crate) struct ModulationPlan {
    /// Each route paired with its leaf envelopes. Combinator routes carry one
    /// entry per distinct media-backed leaf; pure-function routes carry an
    /// empty map.
    routes: Vec<(ModulationRoute, RouteEnvelopes)>,
    sampling: ModulationSampling,
    fps: f64,
}

/// Dedup key: (named_modulator, leaf_spec_text).
type EnvelopeKey = (Option<String>, String);

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
        self.routes.iter().map(move |(route, envelopes)| {
            (
                route.target.as_str(),
                modulated_value(route, envelopes, t, self.sampling),
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
    /// Modulator Standard MIDI File for `midi-*` sources.
    pub(crate) modulator_midi: Option<&'a Path>,
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
    pub(crate) named_modulator_midi: &'a [String],
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
    let named_midi =
        parse_named_modulator_specs(request.named_modulator_midi, "--named-modulator-midi")?;

    // Each distinct (modulator, leaf_spec_text) pair is extracted exactly once.
    // Combinator routes may have multiple media-backed leaves; each is extracted
    // and stored in a per-route HashMap keyed by leaf spec_text().
    let mut leaf_cache: Vec<(EnvelopeKey, Vec<(f64, f32)>)> = Vec::new();
    for route in &routes {
        for leaf in route.source.leaf_media_sources() {
            let key: EnvelopeKey = (route.modulator.clone(), leaf.spec_text());
            if leaf_cache.iter().any(|(existing, _)| *existing == key) {
                continue;
            }
            // Create a thin route context carrying the leaf source + inherited modulator.
            let leaf_route = ModulationRoute {
                target: String::new(),
                source: leaf,
                scale: 1.0,
                offset: 0.0,
                sampling: None,
                modulator: route.modulator.clone(),
            };
            let samples = extract_envelope(
                &leaf_route,
                &request,
                &named_audio,
                &named_frames,
                &named_midi,
            )?;
            leaf_cache.push((key, samples));
        }
    }

    let routes = routes
        .into_iter()
        .map(|route| {
            let mut envelopes: RouteEnvelopes = HashMap::new();
            for leaf in route.source.leaf_media_sources() {
                let key: EnvelopeKey = (route.modulator.clone(), leaf.spec_text());
                if let Some((_, samples)) = leaf_cache.iter().find(|(k, _)| *k == key) {
                    envelopes.insert(leaf.spec_text(), samples.clone());
                }
            }
            (route, envelopes)
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
    named_midi: &[(String, PathBuf)],
) -> Result<Vec<(f64, f32)>, CliError> {
    let source = route.source.clone();
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
    let resolve_midi = || {
        resolve_modulator_media(
            route,
            request.modulator_midi,
            named_midi,
            "--modulator-midi <file.mid>",
            "--named-modulator-midi",
        )
    };
    match source {
        // Pure functions of (frame_time, params) — no media, no sidecar, no
        // fingerprint. `modulated_value` evaluates them directly; this
        // returned envelope is never consulted.
        ModulationSource::Lfo { .. } => Ok(Vec::new()),
        ModulationSource::Breakpoints { .. } => Ok(Vec::new()),
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
        ModulationSource::MidiCc(controller) => {
            let midi = MidiFile::load(resolve_midi()?)?;
            Ok(build_midi_cc_samples(&midi, controller))
        }
        ModulationSource::MidiVelocity => {
            let midi = MidiFile::load(resolve_midi()?)?;
            Ok(build_midi_velocity_samples(&midi))
        }
        ModulationSource::MidiNoteDensity => {
            let midi = MidiFile::load(resolve_midi()?)?;
            Ok(build_midi_note_density_samples(&midi))
        }
        ModulationSource::MidiPitch => {
            let midi = MidiFile::load(resolve_midi()?)?;
            Ok(build_midi_pitch_samples(&midi))
        }
        // Combinators are never passed to extract_envelope directly — only their
        // atomic media leaves are extracted via leaf_media_sources() in build_plan.
        ModulationSource::Sum(..)
        | ModulationSource::Mul(..)
        | ModulationSource::Invert(..)
        | ModulationSource::Min(..)
        | ModulationSource::Max(..)
        | ModulationSource::Gate { .. } => {
            unreachable!("combinator sources are never extracted directly")
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
    let source = route.source.clone();
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

/// Every MIDI envelope's silence value (contract: CC/velocity/pitch all use
/// 0 for "nothing has happened yet" / "no notes sounding").
const MIDI_SILENCE_VALUE: f32 = 0.0;

/// Note-density sliding-window parameters (contract: the RMS-hop convention).
const MIDI_DENSITY_WINDOW_SECONDS: f64 = 1.0;
const MIDI_DENSITY_HOP_SECONDS: f64 = 0.0625;

/// The file's total duration in tempo-mapped seconds (the latest event's
/// tick; 0 for an empty file).
fn midi_duration_seconds(midi: &MidiFile, segments: &[(u64, u32)]) -> f64 {
    let max_tick = midi
        .events
        .iter()
        .map(|event| event.tick)
        .max()
        .unwrap_or(0);
    midi_seconds_for_tick(segments, midi.division, max_tick)
}

/// Every MIDI envelope gets a sample at `t = 0` (silence value, unless an
/// event already sits at tick 0) and a final sample holding the last value at
/// end-of-track time, so pre-first-event and post-last-event frames are
/// defined (contract: "Sources & envelopes").
fn finalize_midi_envelope(samples: &mut Vec<(f64, f32)>, silence: f32, end_seconds: f64) {
    if samples.first().map_or(true, |&(t, _)| t > 0.0) {
        samples.insert(0, (0.0, silence));
    }
    if let Some(&(last_t, last_v)) = samples.last() {
        if last_t < end_seconds {
            samples.push((end_seconds, last_v));
        }
    }
}

/// `midi-cc(<n>)`: `value / 127.0` at each matching Control Change event
/// (any channel — channels merge, last-writer-wins at equal ticks per the
/// merge order), **absolute** normalization.
fn build_midi_cc_samples(midi: &MidiFile, controller: u8) -> Vec<(f64, f32)> {
    let segments = midi.tempo_segments();
    let mut samples: Vec<(f64, f32)> = midi
        .events
        .iter()
        .filter_map(|event| match event.kind {
            MidiEventKind::ControlChange {
                controller: c,
                value,
                ..
            } if c == controller => Some((
                midi_seconds_for_tick(&segments, midi.division, event.tick),
                value as f32 / 127.0,
            )),
            _ => None,
        })
        .collect();
    finalize_midi_envelope(
        &mut samples,
        MIDI_SILENCE_VALUE,
        midi_duration_seconds(midi, &segments),
    );
    samples
}

/// `midi-velocity`: `velocity / 127.0` at each note-on; an additional 0
/// sample when the sounding-note count (summed across all channels/keys)
/// drops to zero. **Absolute** normalization.
fn build_midi_velocity_samples(midi: &MidiFile) -> Vec<(f64, f32)> {
    let segments = midi.tempo_segments();
    let mut samples = Vec::new();
    let mut sounding: i64 = 0;
    for event in &midi.events {
        let t = midi_seconds_for_tick(&segments, midi.division, event.tick);
        match event.kind {
            MidiEventKind::NoteOn { velocity, .. } => {
                sounding += 1;
                samples.push((t, velocity as f32 / 127.0));
            }
            MidiEventKind::NoteOff { .. } => {
                sounding = (sounding - 1).max(0);
                if sounding == 0 {
                    samples.push((t, 0.0));
                }
            }
            _ => {}
        }
    }
    finalize_midi_envelope(
        &mut samples,
        MIDI_SILENCE_VALUE,
        midi_duration_seconds(midi, &segments),
    );
    samples
}

/// `midi-pitch`: `key / 127.0` at each note-on, holding through note-off (no
/// sample is emitted on note-off — the hold sampling convention does the
/// rest). **Absolute** normalization.
fn build_midi_pitch_samples(midi: &MidiFile) -> Vec<(f64, f32)> {
    let segments = midi.tempo_segments();
    let mut samples: Vec<(f64, f32)> = midi
        .events
        .iter()
        .filter_map(|event| match event.kind {
            MidiEventKind::NoteOn { key, .. } => Some((
                midi_seconds_for_tick(&segments, midi.division, event.tick),
                key as f32 / 127.0,
            )),
            _ => None,
        })
        .collect();
    finalize_midi_envelope(
        &mut samples,
        MIDI_SILENCE_VALUE,
        midi_duration_seconds(midi, &segments),
    );
    samples
}

/// `midi-note-density`: note-on count per sliding 1.0s window `[start,
/// start+1.0)`, sampled every 62.5ms across the file's duration (the RMS-hop
/// convention), then peak-normalized (**relative** — fixtures must span
/// sparse→busy).
fn build_midi_note_density_samples(midi: &MidiFile) -> Vec<(f64, f32)> {
    let segments = midi.tempo_segments();
    let note_on_times: Vec<f64> = midi
        .events
        .iter()
        .filter(|event| matches!(event.kind, MidiEventKind::NoteOn { .. }))
        .map(|event| midi_seconds_for_tick(&segments, midi.division, event.tick))
        .collect();
    let duration = midi_duration_seconds(midi, &segments);

    let mut samples = Vec::new();
    let mut index: u64 = 0;
    loop {
        let start = index as f64 * MIDI_DENSITY_HOP_SECONDS;
        let end = (start + MIDI_DENSITY_WINDOW_SECONDS).min(duration.max(start));
        let count = note_on_times
            .iter()
            .filter(|&&t| t >= start && t < end)
            .count();
        samples.push((start, count as f32));
        if start >= duration {
            break;
        }
        index += 1;
    }
    peak_normalize(&mut samples);
    samples
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
            modulator_midi: None,
            sampling: ModulationSampling::Hold,
            fps: 30.0,
            cache_dir: None,
            named_modulator_audio: &[],
            named_modulator_frames: &[],
            named_modulator_midi: &[],
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
            modulator_midi: None,
            sampling: ModulationSampling::Hold,
            fps: 30.0,
            cache_dir: None,
            named_modulator_audio: &[],
            named_modulator_frames: &[],
            named_modulator_midi: &[],
        };
        assert!(build_modulation_plan(request).unwrap().is_none());
    }

    // ── MIDI envelope-shape tests ───────────────────────────────────────────
    // SMF fixtures are built as byte arrays in test code (no binary fixtures
    // in the repo), mirroring the parser-level fixtures in
    // `morphogen_audio::midi`'s own test module.

    fn vlq(mut value: u32) -> Vec<u8> {
        let mut bytes = vec![(value & 0x7F) as u8];
        value >>= 7;
        while value > 0 {
            bytes.push(((value & 0x7F) as u8) | 0x80);
            value >>= 7;
        }
        bytes.reverse();
        bytes
    }

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(id);
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    fn note_on(channel: u8, key: u8, velocity: u8) -> Vec<u8> {
        vec![0x90 | channel, key, velocity]
    }

    fn note_off(channel: u8, key: u8, velocity: u8) -> Vec<u8> {
        vec![0x80 | channel, key, velocity]
    }

    fn control_change(channel: u8, controller: u8, value: u8) -> Vec<u8> {
        vec![0xB0 | channel, controller, value]
    }

    fn end_of_track() -> Vec<u8> {
        vec![0xFF, 0x2F, 0x00]
    }

    /// A format-0, single-track, PPQ-480 SMF (division stays the default
    /// 120 BPM tempo — no Set Tempo event) built from `(delta_ticks,
    /// event_bytes)` pairs, with an End-of-Track appended.
    fn build_smf(events: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut header = Vec::new();
        header.extend_from_slice(&0u16.to_be_bytes()); // format 0
        header.extend_from_slice(&1u16.to_be_bytes()); // ntrks
        header.extend_from_slice(&480u16.to_be_bytes()); // PPQ division
        let mut bytes = chunk(b"MThd", &header);

        let mut track_body = Vec::new();
        for (delta, data) in events {
            track_body.extend_from_slice(&vlq(*delta));
            track_body.extend_from_slice(data);
        }
        track_body.extend_from_slice(&vlq(0));
        track_body.extend_from_slice(&end_of_track());
        bytes.extend_from_slice(&chunk(b"MTrk", &track_body));
        bytes
    }

    #[test]
    fn midi_cc_envelope_is_a_staircase_with_leading_silence_sample() {
        // CC 74 ramps 0 -> 64 -> 127 at ticks 240 and 480 (120 BPM default:
        // quarter note = 0.5s, so ticks 240/480 land at 0.25s/0.5s).
        let bytes = build_smf(&[
            (240, control_change(0, 74, 64)),
            (240, control_change(0, 74, 127)),
        ]);
        let midi = MidiFile::parse(&bytes).expect("valid fixture parses");
        let samples = build_midi_cc_samples(&midi, 74);

        // No event at tick 0 → a synthetic silence sample is prepended.
        assert_eq!(samples[0], (0.0, 0.0));
        assert!((samples[1].0 - 0.25).abs() < 1e-9);
        assert!((samples[1].1 - 64.0 / 127.0).abs() < 1e-6);
        assert!((samples[2].0 - 0.5).abs() < 1e-9);
        assert!((samples[2].1 - 1.0).abs() < 1e-6);
        assert_eq!(
            samples.len(),
            3,
            "last event sits at end-of-track: no extra padding sample"
        );

        // A different controller number sees no events at all — just the
        // t=0 silence sample and the end-of-track hold.
        let empty = build_midi_cc_samples(&midi, 1);
        assert_eq!(empty.len(), 2);
        assert_eq!(empty[0], (0.0, 0.0));
        assert!((empty[1].0 - 0.5).abs() < 1e-9);
        assert_eq!(empty[1].1, 0.0);
    }

    #[test]
    fn midi_velocity_envelope_samples_zero_when_sounding_count_hits_zero() {
        let bytes = build_smf(&[(0, note_on(0, 60, 100)), (240, note_off(0, 60, 64))]);
        let midi = MidiFile::parse(&bytes).expect("valid fixture parses");
        let samples = build_midi_velocity_samples(&midi);

        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0], (0.0, 100.0 / 127.0));
        assert!((samples[1].0 - 0.25).abs() < 1e-9);
        assert_eq!(samples[1].1, 0.0);
    }

    #[test]
    fn midi_pitch_holds_through_note_off_no_new_sample() {
        let bytes = build_smf(&[
            (0, note_on(0, 60, 100)),
            (240, note_off(0, 60, 64)),
            (240, note_on(0, 72, 100)),
        ]);
        let midi = MidiFile::parse(&bytes).expect("valid fixture parses");
        let samples = build_midi_pitch_samples(&midi);

        // Only the two note-ons produce samples — note-off is silent, so the
        // pitch holds across it via the ordinary hold-sampling machinery.
        assert_eq!(samples.len(), 2);
        let held = morphogen_render::sample_envelope(&samples, 0.3, ModulationSampling::Hold);
        assert_eq!(
            held,
            60.0 / 127.0,
            "pitch must hold through the note-off at t=0.25"
        );
    }

    #[test]
    fn midi_note_density_peak_normalizes_sparse_vs_busy() {
        // One note near the start, then a cluster of five notes a second
        // later — density must be higher (and peak-normalized to 1.0) in the
        // busy region than in the sparse region (contract: fixtures must
        // span sparse→busy).
        let events: Vec<(u32, Vec<u8>)> = vec![
            (0, note_on(0, 60, 100)),
            (1_920, note_on(0, 61, 100)), // +4 quarter notes = +2.0s at 120bpm
            (10, note_on(0, 62, 100)),
            (10, note_on(0, 63, 100)),
            (10, note_on(0, 64, 100)),
            (10, note_on(0, 65, 100)),
        ];
        let bytes = build_smf(&events);
        let midi = MidiFile::parse(&bytes).expect("valid fixture parses");
        let samples = build_midi_note_density_samples(&midi);

        let peak = samples.iter().map(|&(_, v)| v).fold(0.0_f32, f32::max);
        assert_eq!(peak, 1.0, "peak-normalized: the busiest window reads 1.0");
        let near_start =
            morphogen_render::sample_envelope(&samples, 0.05, ModulationSampling::Hold);
        assert!(
            near_start < peak,
            "the sparse opening window must read below the busy peak"
        );
    }
}
