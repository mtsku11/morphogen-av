//! `render-composition` — arrange finished render jobs into a piece on a
//! global timeline. A **Composition** is an ordered list of **Scenes**, each
//! scene a full effect chain over its own source, joined by deterministic
//! **Transitions**. See `docs/COMPOSITION_MILESTONE.md` for the contract.
//!
//! Slice 1 (this module): the spec types, whole-document validation, and the
//! single-scene passthrough — a one-scene composition renders that scene via
//! the *existing* `render-chain` execution path (`run_chain_spec`, not a
//! mirror) into `<out>/scene_01_<name>/` and assembles its final frames into
//! `<out>/frames/`. Anchor A1: this is byte-identical, frame for frame, to
//! `render-chain` run directly on the scene's chain spec + input, keeping a
//! composition a *view over* the engine rather than a second engine.
//!
//! Deferred to later slices (refused cleanly here, never silently ignored):
//! multi-scene cut assembly + manifest (S2), crossfade transitions (S3), the
//! scene fingerprint cache + resume/refusal (S4), the master clock (S5).

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::chain::{run_chain_spec, validate_chain_spec, ChainSpec};
use crate::error::CliError;
use crate::imaging::collect_image_frames;

/// Only composition spec version this build understands. The field exists so a
/// future format change is a clear error, not a silent best-effort parse (the
/// chain-spec precedent).
const COMPOSITION_SPEC_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Spec types
// ---------------------------------------------------------------------------

/// One composition: a global frame rate and an ordered list of scenes. The
/// `master` clock (composition-level modulator media) is a later slice and is
/// intentionally absent from this type — adding it as an optional field then
/// stays backward compatible with specs written now.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompositionSpec {
    pub version: u32,
    /// Global frame rate; every scene (and, later, master media) must agree.
    pub fps: f64,
    pub scenes: Vec<Scene>,
}

/// One scene: a name (becomes `scene_<NN>_<name>/`), a pre-overlap length, the
/// input frames its chain reads, the chain itself (a verbatim `render-chain`
/// spec — no parallel effect vocabulary), and an optional transition to the
/// next scene. The last/only scene must omit `transition_out`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scene {
    pub name: String,
    /// Pre-overlap length in frames (the timeline-assembly unit; S2+). Must be
    /// at least 1. Slice 1 renders every input frame the chain produces and
    /// does not yet truncate to this length (single scene, no timeline).
    pub duration_frames: u32,
    /// Stage-1 input frames (PNG sequence) for this scene's chain.
    pub input_dir: PathBuf,
    /// A `render-chain` spec document, embedded verbatim.
    pub chain: ChainSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition_out: Option<Transition>,
}

/// A deterministic join between two scenes. `cut` (or `crossfade` with
/// `frames: 0`) is the hard boundary; a non-zero `crossfade` blends the
/// overlap. Only the grammar is validated in Slice 1 — the overlap math and
/// the "transition no longer than either adjacent scene" rule land with the
/// assembly slices.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Transition {
    #[serde(rename = "type")]
    pub kind: TransitionKind,
    /// Overlap length in frames. Ignored for `cut`; `crossfade` with `0` ≡ cut.
    #[serde(default)]
    pub frames: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TransitionKind {
    Cut,
    Crossfade,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

/// A scene name becomes a directory component (`scene_<NN>_<name>/`); restrict
/// it to path-safe characters so a spec can't traverse or escape the output
/// directory.
fn valid_scene_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub(crate) fn parse_composition_spec(spec_text: &str) -> Result<CompositionSpec, CliError> {
    serde_json::from_str(spec_text)
        .map_err(|error| CliError::Message(format!("invalid composition spec: {error}")))
}

/// Whole-document validation, run before anything renders (the chain
/// precedent): version, fps, scene names/uniqueness, per-scene durations and
/// chains, and the transition rules that hold in this slice.
pub(crate) fn validate_composition_spec(spec: &CompositionSpec) -> Result<(), CliError> {
    if spec.version != COMPOSITION_SPEC_VERSION {
        return Err(CliError::Message(format!(
            "unsupported composition spec version {} (this build understands version {})",
            spec.version, COMPOSITION_SPEC_VERSION
        )));
    }
    if !spec.fps.is_finite() || spec.fps <= 0.0 {
        return Err(CliError::Message(
            "composition fps must be positive and finite".to_string(),
        ));
    }
    if spec.scenes.is_empty() {
        return Err(CliError::Message(
            "composition spec must contain at least one scene".to_string(),
        ));
    }

    let mut seen_names = std::collections::HashSet::new();
    let last_index = spec.scenes.len() - 1;
    for (index, scene) in spec.scenes.iter().enumerate() {
        if !valid_scene_name(&scene.name) {
            return Err(CliError::Message(format!(
                "scene name {:?} is invalid — use only ASCII letters, digits, '_' or '-'",
                scene.name
            )));
        }
        if !seen_names.insert(scene.name.as_str()) {
            return Err(CliError::Message(format!(
                "duplicate scene name {:?}; scene names must be unique",
                scene.name
            )));
        }
        if scene.duration_frames == 0 {
            return Err(CliError::Message(format!(
                "scene {:?} has duration_frames 0; a scene must be at least 1 frame",
                scene.name
            )));
        }
        // The last (or only) scene transitions to nothing.
        if index == last_index && scene.transition_out.is_some() {
            return Err(CliError::Message(format!(
                "scene {:?} is the last scene and must omit transition_out \
                 (transitions join a scene to the next one)",
                scene.name
            )));
        }
        validate_chain_spec(&scene.chain)?;
    }

    // Slice 1 renders and assembles a single scene. Multi-scene cut assembly is
    // the next slice; refuse rather than silently rendering only scene 1.
    if spec.scenes.len() > 1 {
        return Err(CliError::Message(
            "multi-scene compositions are not implemented yet (Slice 2: cut assembly + \
             manifest); this build renders a single-scene composition"
                .to_string(),
        ));
    }

    Ok(())
}

/// Parse + whole-spec validation in one step.
pub(crate) fn parse_and_validate_composition_spec(
    spec_text: &str,
) -> Result<CompositionSpec, CliError> {
    let spec = parse_composition_spec(spec_text)?;
    validate_composition_spec(&spec)?;
    Ok(spec)
}

// ---------------------------------------------------------------------------
// Mechanic
// ---------------------------------------------------------------------------

/// Copy a scene's final frames into `<out>/frames/` with global timeline
/// numbering. For the single scene of Slice 1 the global index equals the
/// scene's own frame index, so each frame is a verbatim copy (this keeps the
/// assembled `frames/` byte-identical to the scene render — anchor A1).
fn assemble_frames(final_frames_dir: &Path, frames_dir: &Path) -> Result<usize, CliError> {
    let scene_frames = collect_image_frames(final_frames_dir)?;
    fs::create_dir_all(frames_dir)?;
    for (global_index, source) in scene_frames.iter().enumerate() {
        let destination = frames_dir.join(format!("frame_{global_index:06}.png"));
        fs::copy(source, &destination)?;
    }
    Ok(scene_frames.len())
}

/// `render-composition <spec.json> <output-dir>`.
///
/// Validates the whole spec, renders the (single, in this slice) scene via the
/// existing chain path into `<output-dir>/scene_01_<name>/`, and assembles its
/// final frames into `<output-dir>/frames/`.
pub(crate) fn render_composition(spec_path: &Path, output_dir: &Path) -> Result<(), CliError> {
    let spec_text = fs::read_to_string(spec_path)?;
    let spec = parse_and_validate_composition_spec(&spec_text)?;

    // Validation guarantees exactly one scene in this slice.
    let scene = &spec.scenes[0];
    let scene_dir = output_dir.join(format!("scene_01_{}", scene.name));
    let summary = run_chain_spec(&scene.chain, &scene.input_dir, &scene_dir)?;

    let frames_dir = output_dir.join("frames");
    let assembled = assemble_frames(&summary.final_frames_dir, &frames_dir)?;

    println!(
        "rendered composition with 1 scene ({assembled} frame(s)) from {} to {}; \
         timeline frames: {}",
        spec_path.display(),
        output_dir.display(),
        frames_dir.display(),
    );
    Ok(())
}
