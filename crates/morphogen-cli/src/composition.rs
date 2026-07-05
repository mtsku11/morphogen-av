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

use morphogen_render::ImageBufferF32;
use serde::{Deserialize, Serialize};

use crate::chain::{run_chain_spec, validate_chain_spec, ChainSpec};
use crate::error::CliError;
use crate::imaging::{collect_image_frames, load_image_f32, save_png, update_fnv1a};

/// Only composition spec version this build understands. The field exists so a
/// future format change is a clear error, not a silent best-effort parse (the
/// chain-spec precedent).
const COMPOSITION_SPEC_VERSION: u32 = 1;

/// Persisted at the output root; records the scene skeleton (names in order)
/// and each scene's render-cache fingerprint so a re-run can skip unchanged
/// scenes and refuse a structurally changed spec. Lives at the root — never
/// inside a scene directory — so scene directories stay pristine for the
/// chain's own reconciliation.
const COMPOSITION_RECORD_FILE: &str = "composition-record.json";

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
        // The last (or only) scene transitions to nothing; a non-last scene
        // may declare a transition (an absent one is an implicit cut).
        match (&scene.transition_out, index == last_index) {
            (Some(_), true) => {
                return Err(CliError::Message(format!(
                    "scene {:?} is the last scene and must omit transition_out \
                     (transitions join a scene to the next one)",
                    scene.name
                )));
            }
            (Some(transition), false) => validate_transition(&scene.name, transition)?,
            (None, _) => {}
        }
        validate_chain_spec(&scene.chain)?;
    }

    // A scene's incoming (previous scene's crossfade) and outgoing crossfade
    // windows must not collide: their combined length can't exceed the scene's
    // frames, or a single frame would have to be both a head-blend and a
    // tail-blend at once. This also subsumes "a transition longer than either
    // adjacent scene" (the milestone rule).
    for (index, scene) in spec.scenes.iter().enumerate() {
        let incoming = if index == 0 {
            0
        } else {
            crossfade_frames(&spec.scenes[index - 1].transition_out)
        };
        let outgoing = crossfade_frames(&scene.transition_out);
        if incoming + outgoing > scene.duration_frames {
            return Err(CliError::Message(format!(
                "scene {:?} has overlapping transitions: incoming crossfade {incoming} + \
                 outgoing crossfade {outgoing} exceed its duration_frames {}",
                scene.name, scene.duration_frames
            )));
        }
    }

    Ok(())
}

/// A `cut` has no overlap, so a non-zero `frames` on it is a spec error. A
/// `crossfade` of any length is well-formed here; whether it fits the adjacent
/// scenes is the overlap-bounds check in [`validate_composition_spec`].
fn validate_transition(scene_name: &str, transition: &Transition) -> Result<(), CliError> {
    match transition.kind {
        TransitionKind::Cut if transition.frames != 0 => Err(CliError::Message(format!(
            "scene {scene_name:?} has a cut transition with frames {} — a cut has no overlap; \
             remove the frames field",
            transition.frames
        ))),
        _ => Ok(()),
    }
}

/// The overlap length a transition contributes: a crossfade's frame count, or 0
/// for a cut (and for an absent transition, i.e. an implicit cut).
fn crossfade_frames(transition: &Option<Transition>) -> u32 {
    match transition {
        Some(Transition {
            kind: TransitionKind::Crossfade,
            frames,
        }) => *frames,
        _ => 0,
    }
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

/// The chain manifest a scene render leaves in its directory, embedded whole in
/// the composition manifest so the piece is reproducible from that one file.
fn read_scene_chain_manifest(scene_dir: &Path) -> Result<serde_json::Value, CliError> {
    let path = scene_dir.join("chain-manifest.json");
    Ok(serde_json::from_str(&fs::read_to_string(&path)?)?)
}

/// One scene's entry in the composition record: its name (the structural
/// skeleton) and render-cache fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SceneRecord {
    name: String,
    fingerprint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CompositionRecord {
    scenes: Vec<SceneRecord>,
}

fn read_composition_record(path: &Path) -> Option<CompositionRecord> {
    if !path.is_file() {
        return None;
    }
    // A truncated/unparseable record (interrupted mid-write) is treated as
    // absent: the structural guard simply doesn't fire and every scene
    // re-renders, which is safe because each scene is deterministic.
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

/// Content hash of a scene's input frames (fnv1a64 over each frame's file name
/// and bytes, in sorted order) — path-independent, so identical footage in a
/// moved directory still hits the cache. Mirrors the chain's input-fingerprint
/// recipe (content, not the directory path).
fn source_content_hash(input_dir: &Path) -> Result<String, CliError> {
    let frames = collect_image_frames(input_dir)?;
    if frames.is_empty() {
        return Err(CliError::Message(format!(
            "scene input directory {} contains no supported image frames",
            input_dir.display()
        )));
    }
    let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
    for frame in &frames {
        update_fnv1a(
            &mut checksum,
            frame.file_name().unwrap_or_default().as_encoded_bytes(),
        );
        update_fnv1a(&mut checksum, &[0]);
        update_fnv1a(&mut checksum, &fs::read(frame)?);
    }
    Ok(format!("fnv1a64:{checksum:016x}"))
}

/// A scene's render-cache key: everything that determines its rendered frames —
/// the resolved chain spec (stages, settings, routes), the input footage
/// content, and the composition fps (which governs modulation sampling).
/// Deliberately excludes `duration_frames` / `transition_out` / `name`: those
/// drive the timeline *assembly* (always redone) or are structural (a
/// name/order change refuses), not the pixels the scene renders.
fn scene_fingerprint(scene: &Scene, fps: f64) -> Result<String, CliError> {
    let chain_json = serde_json::to_string(&scene.chain)?;
    let content = source_content_hash(&scene.input_dir)?;
    let mut checksum = 0xcbf2_9ce4_8422_2325_u64;
    update_fnv1a(&mut checksum, chain_json.as_bytes());
    update_fnv1a(&mut checksum, &[0]);
    update_fnv1a(&mut checksum, content.as_bytes());
    update_fnv1a(&mut checksum, &[0]);
    update_fnv1a(&mut checksum, &fps.to_bits().to_le_bytes());
    Ok(format!("fnv1a64:{checksum:016x}"))
}

/// A rendered scene, carried from the render pass into the assembly pass.
struct RenderedScene {
    name: String,
    directory: String,
    /// Final-stage frames, sorted; length == `duration` == the scene's
    /// declared `duration_frames`.
    frames: Vec<PathBuf>,
    duration: usize,
    /// Outgoing crossfade overlap in frames (0 for a cut or the last scene).
    out_frames: usize,
    transition_out: Option<Transition>,
    chain_manifest: serde_json::Value,
}

/// Blend `tail` toward `head` per pixel in f32 on the decoded RGBA, weight
/// `weight` on `head` (the incoming scene): `out = (1 − w)·tail + w·head`.
/// Written as an 8-bit PNG (round half away from zero, the `save_png`
/// convention). The two frames must share dimensions.
fn crossfade_frame(
    tail: &Path,
    head: &Path,
    weight: f32,
    destination: &Path,
) -> Result<(), CliError> {
    let tail_image = load_image_f32(tail)?;
    let head_image = load_image_f32(head)?;
    if tail_image.width != head_image.width || tail_image.height != head_image.height {
        return Err(CliError::Message(format!(
            "crossfade frames differ in size ({}x{} vs {}x{}); scenes in a composition must \
             share dimensions",
            tail_image.width, tail_image.height, head_image.width, head_image.height
        )));
    }
    let blended = ImageBufferF32::from_fn(tail_image.width, tail_image.height, |x, y| {
        let a = tail_image.pixel(x, y).unwrap_or([0.0; 4]);
        let b = head_image.pixel(x, y).unwrap_or([0.0; 4]);
        [
            (1.0 - weight) * a[0] + weight * b[0],
            (1.0 - weight) * a[1] + weight * b[1],
            (1.0 - weight) * a[2] + weight * b[2],
            (1.0 - weight) * a[3] + weight * b[3],
        ]
    })
    .map_err(CliError::from)?;
    save_png(&blended, destination)
}

/// `render-composition <spec.json> <output-dir>`.
///
/// Validates the whole spec, renders each scene via the existing chain path
/// into `<output-dir>/scene_<NN>_<name>/`, assembles the scenes onto the global
/// timeline in `<output-dir>/frames/` — cut transitions concatenate, a
/// crossfade of N frames blends the last N frames of a scene with the first N
/// of the next (weight `(i+1)/(N+1)` on the incoming frame) — and writes
/// `<output-dir>/composition-manifest.json`.
pub(crate) fn render_composition(spec_path: &Path, output_dir: &Path) -> Result<(), CliError> {
    let spec_text = fs::read_to_string(spec_path)?;
    let spec = parse_and_validate_composition_spec(&spec_text)?;

    fs::create_dir_all(output_dir)?;
    let record_path = output_dir.join(COMPOSITION_RECORD_FILE);
    let prior = read_composition_record(&record_path);

    // Structural guard: the set and order of scene names is the composition's
    // skeleton. A changed skeleton (rename/reorder/add/remove) refuses rather
    // than risk reusing another scene's cached frames — the chain-refusal
    // precedent. A changed *setting* keeps the skeleton and re-renders only its
    // scene (below).
    if let Some(prior) = &prior {
        let prior_names: Vec<&str> = prior.scenes.iter().map(|s| s.name.as_str()).collect();
        let new_names: Vec<&str> = spec.scenes.iter().map(|s| s.name.as_str()).collect();
        if prior_names != new_names {
            return Err(CliError::Message(format!(
                "the composition's scenes changed (names or order) since {} was first rendered; \
                 reusing cached scene frames would be unsound — use a fresh output directory",
                output_dir.display()
            )));
        }
    }

    // The working record carries prior fingerprints forward (skeleton matches,
    // so by index) so an unchanged scene hits the cache; each scene overwrites
    // its own entry as it completes and the record is persisted after every
    // scene, so an interrupted run resumes.
    let mut record = CompositionRecord {
        scenes: spec
            .scenes
            .iter()
            .map(|scene| SceneRecord {
                name: scene.name.clone(),
                fingerprint: String::new(),
            })
            .collect(),
    };
    if let Some(prior) = &prior {
        for (slot, prior_scene) in record.scenes.iter_mut().zip(&prior.scenes) {
            slot.fingerprint = prior_scene.fingerprint.clone();
        }
    }
    fs::write(&record_path, serde_json::to_string_pretty(&record)?)?;

    // Pass 1: render (or reuse) every scene and collect its final frames. A
    // crossfade needs the *next* scene's head frames, so assembly is a second
    // pass over all scenes rather than interleaved with rendering.
    let mut scenes: Vec<RenderedScene> = Vec::with_capacity(spec.scenes.len());
    for (index, scene) in spec.scenes.iter().enumerate() {
        let scene_number = index + 1;
        let directory = format!("scene_{scene_number:02}_{}", scene.name);
        let scene_dir = output_dir.join(&directory);
        let fingerprint = scene_fingerprint(scene, spec.fps)?;

        // Reuse only when the recorded fingerprint matches AND the render is
        // complete on disk (the chain writes chain-manifest.json last). Disk is
        // truth: a manually deleted scene dir re-renders even if the record
        // still names it.
        let same_fingerprint = record.scenes[index].fingerprint == fingerprint;
        let complete_on_disk = scene_dir.join("chain-manifest.json").is_file();
        let reuse = same_fingerprint && complete_on_disk;

        // A stale scene dir (changed spec/input) would make the chain refuse a
        // changed spec into it, so clear it for a fresh render. A
        // same-fingerprint-but-incomplete dir is left intact so the chain
        // resumes from its own stage markers/checkpoint.
        if scene_dir.exists() && !same_fingerprint {
            fs::remove_dir_all(&scene_dir)?;
        }

        // Cheap when reusing: the chain fast-skips completed stages and returns
        // the summary without re-rendering.
        let summary = run_chain_spec(&scene.chain, &scene.input_dir, &scene_dir)?;

        // The declared timeline length must match what the scene actually
        // rendered so global numbering and overlap math are exact rather than
        // silently drifting from the spec.
        if summary.frame_count != scene.duration_frames as usize {
            return Err(CliError::Message(format!(
                "scene {:?} declares duration_frames {} but its chain rendered {} frame(s); \
                 the declared length must match the render",
                scene.name, scene.duration_frames, summary.frame_count
            )));
        }

        record.scenes[index].fingerprint = fingerprint;
        fs::write(&record_path, serde_json::to_string_pretty(&record)?)?;
        println!(
            "scene {scene_number:02} ({}) {}",
            scene.name,
            if reuse { "reused from cache" } else { "rendered" }
        );

        scenes.push(RenderedScene {
            name: scene.name.clone(),
            directory,
            frames: collect_image_frames(&summary.final_frames_dir)?,
            duration: summary.frame_count,
            out_frames: crossfade_frames(&scene.transition_out) as usize,
            transition_out: scene.transition_out.clone(),
            chain_manifest: read_scene_chain_manifest(&scene_dir)?,
        });
    }

    // Pass 2: walk the timeline. Each scene writes its frames not consumed by
    // the *previous* transition; the last `out_frames` of those are blended
    // with the next scene's head (which are therefore skipped there). The
    // timeline is rebuilt from scratch so a changed transition can't leave
    // stale high-index frames behind.
    let frames_dir = output_dir.join("frames");
    if frames_dir.exists() {
        fs::remove_dir_all(&frames_dir)?;
    }
    fs::create_dir_all(&frames_dir)?;
    let mut global = 0usize;
    let mut start = 0usize;
    let mut manifest_scenes = Vec::with_capacity(scenes.len());
    for k in 0..scenes.len() {
        let scene = &scenes[k];
        let in_frames = if k == 0 { 0 } else { scenes[k - 1].out_frames };
        let out_frames = scene.out_frames;

        // Solo zone: frames owned outright by this scene.
        for j in in_frames..(scene.duration - out_frames) {
            let destination = frames_dir.join(format!("frame_{global:06}.png"));
            fs::copy(&scene.frames[j], &destination)?;
            global += 1;
        }
        // Tail-overlap zone: blend into the next scene's head (validation
        // guarantees a next scene exists whenever out_frames > 0).
        if out_frames > 0 {
            let next = &scenes[k + 1];
            for i in 0..out_frames {
                let tail = &scene.frames[scene.duration - out_frames + i];
                let head = &next.frames[i];
                let weight = (i as f32 + 1.0) / (out_frames as f32 + 1.0);
                let destination = frames_dir.join(format!("frame_{global:06}.png"));
                crossfade_frame(tail, head, weight, &destination)?;
                global += 1;
            }
        }

        manifest_scenes.push(serde_json::json!({
            "name": scene.name,
            "directory": scene.directory,
            "start_frame": start,
            "length": scene.duration,
            "transition_out": scene.transition_out,
            "chain_manifest": scene.chain_manifest,
        }));
        start += scene.duration - out_frames;
    }

    let composition_manifest = serde_json::json!({
        "version": spec.version,
        "fps": spec.fps,
        "frame_count": global,
        "scenes": manifest_scenes,
    });
    fs::write(
        output_dir.join("composition-manifest.json"),
        serde_json::to_string_pretty(&composition_manifest)?,
    )?;

    println!(
        "rendered composition with {} scene(s) ({} frame(s)) from {} to {}; timeline frames: {}",
        spec.scenes.len(),
        global,
        spec_path.display(),
        output_dir.display(),
        frames_dir.display(),
    );
    Ok(())
}
