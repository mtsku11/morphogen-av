//! `render-chain` — run an ordered list of single-source effect stages, each
//! stage's output feeding the next, with a chain-level provenance manifest.
//! See `docs/EFFECT_CHAIN_MILESTONE.md` for the acceptance contract.
//!
//! Slice 1: CPU-only, stateless, single-source stages (`retro_static`,
//! `channel_shift` [constant shifts only], `palette_quantize`, `rutt_etra`).
//! The whole spec is parsed and validated (every stage's settings through the
//! effect's own `validate()`) **before** any stage renders, so an invalid
//! stage-N leaves nothing on disk.
//!
//! Slice 2: the stateful `flow_feedback` stage (the chain's single input
//! feeds both the modulator and carrier roles — self-feedback; a second
//! input binding is the explicitly deferred two-source/graph problem) plus
//! chain re-run semantics: a `chain-record.json` (resolved spec + input
//! fingerprint) is written before stage 1 renders, completed stages carry a
//! `stage-complete.json` marker and are skipped on re-run, an interrupted
//! stateful stage resumes from its own checkpoint scoped inside its stage
//! directory, and a changed spec (or changed input frames) against an
//! existing output directory refuses cleanly without touching anything.

use std::{
    fs,
    path::{Path, PathBuf},
};

use morphogen_core::{FlowSource, RenderBackend};
use morphogen_render::{
    ChannelShiftSettings, FlowFeedbackSettings, ModulationSampling, PaletteQuantizeSettings,
    QuantizeMode, RetroStaticSettings, RuttEtraSettings, ScanlineFilter, StructureMode,
    CHANNEL_SHIFT_ALGORITHM, PALETTE_QUANTIZE_ALGORITHM, RETRO_STATIC_ALGORITHM,
    RUTT_ETRA_ALGORITHM,
};
use serde::{Deserialize, Serialize};

use crate::error::CliError;
use crate::imaging::{collect_image_frames, update_fnv1a};
use crate::render::{
    render_channel_shift_sequence, render_feedback_sequence, render_palette_quantize_sequence,
    render_retro_static_sequence, render_rutt_etra_sequence, ChannelShiftSequenceRequest,
    FeedbackSequenceRenderRequest, ModulationCliArgs, PaletteQuantizeSequenceRequest,
    RetroStaticSequenceRequest, RuttEtraSequenceRequest, FLOW_FEEDBACK_RENDER_CONTRACT_VERSION,
};

/// Only spec version this slice understands. The field exists for forward
/// compatibility (list → graph, see the milestone doc); a mismatched version
/// is a clear error rather than a silent best-effort parse.
const CHAIN_SPEC_VERSION: u32 = 1;

/// Recorded at the output root after validation, before stage 1 renders;
/// gates every re-run (a changed spec or changed input frames refuses).
const CHAIN_RECORD_FILE: &str = "chain-record.json";

/// Per-stage completion marker, written into the stage directory after its
/// handler returns. Its presence (with matching effect/algorithm/settings)
/// is the skip signal on re-run; a stage without one re-renders (stateless)
/// or resumes from its own checkpoint (stateful).
const STAGE_COMPLETE_FILE: &str = "stage-complete.json";

/// The flow-feedback stage shares the direct CLI's job id so the stage
/// directory is fully interoperable with `render-feedback-sequence`: an
/// interrupted stage resumes through the identical checkpoint contract, and
/// a standalone render into a stage directory is indistinguishable from the
/// chain's own (both are gated by the same contract equality).
const CHAIN_FEEDBACK_JOB_ID: &str = "direct-feedback-sequence";

// Pinned flow-feedback export knobs (not exposed in the stage spec —
// demand-driven vocabulary growth; these are the direct CLI defaults).
const CHAIN_FEEDBACK_FRAME_RATE: f64 = 12.0;
const CHAIN_FEEDBACK_OUTPUT_BIT_DEPTH: u8 = 8;
const CHAIN_FEEDBACK_TEMPORAL_SUPERSAMPLING: u32 = 1;

// ---------------------------------------------------------------------------
// Spec types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChainSpec {
    pub version: u32,
    pub stages: Vec<ChainStage>,
}

/// One stage's knobs, tagged by `"effect"`. Field names/defaults mirror the
/// effect's queue-task/CLI spellings exactly (one vocabulary per knob
/// everywhere). Each inner spec struct is a local mirror of the effect's own
/// settings struct with `deny_unknown_fields` — kept separate from the
/// render-crate settings types so this JSON-facing contract doesn't change
/// their behavior for other callers (queue persistence, direct CLI).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub(crate) enum ChainStage {
    RetroStatic(RetroStaticStageSpec),
    ChannelShift(ChannelShiftStageSpec),
    PaletteQuantize(PaletteQuantizeStageSpec),
    RuttEtra(RuttEtraStageSpec),
    FlowFeedback(FlowFeedbackStageSpec),
}

impl ChainStage {
    fn effect_tag(&self) -> &'static str {
        match self {
            ChainStage::RetroStatic(_) => "retro_static",
            ChainStage::ChannelShift(_) => "channel_shift",
            ChainStage::PaletteQuantize(_) => "palette_quantize",
            ChainStage::RuttEtra(_) => "rutt_etra",
            ChainStage::FlowFeedback(_) => "flow_feedback",
        }
    }

    /// The algorithm identifier recorded in the chain manifest and the
    /// stage-completion marker. Flow feedback has no effect-level algorithm
    /// constant — its stateful identity is the checkpoint task name plus the
    /// render-contract version — so the chain derives its id from that
    /// version (a contract bump changes the recorded id automatically).
    fn algorithm_id(&self) -> String {
        match self {
            ChainStage::RetroStatic(_) => RETRO_STATIC_ALGORITHM.to_string(),
            ChainStage::ChannelShift(_) => CHANNEL_SHIFT_ALGORITHM.to_string(),
            ChainStage::PaletteQuantize(_) => PALETTE_QUANTIZE_ALGORITHM.to_string(),
            ChainStage::RuttEtra(_) => RUTT_ETRA_ALGORITHM.to_string(),
            ChainStage::FlowFeedback(_) => {
                format!("flow_feedback_cpu_v{FLOW_FEEDBACK_RENDER_CONTRACT_VERSION}")
            }
        }
    }

    /// The stage's resolved settings as a JSON value (defaults filled). This
    /// is what the chain manifest and the stage-completion marker record.
    /// Serialized through the string form so f32 knobs keep their shortest
    /// (spec-vocabulary) representation instead of the widened f64 (`0.72`,
    /// not `0.7200000286102295` — `serde_json::to_value` widens f32).
    fn settings_value(&self) -> Result<serde_json::Value, CliError> {
        let text = match self {
            ChainStage::RetroStatic(spec) => serde_json::to_string(spec)?,
            ChainStage::ChannelShift(spec) => serde_json::to_string(spec)?,
            ChainStage::PaletteQuantize(spec) => serde_json::to_string(spec)?,
            ChainStage::RuttEtra(spec) => serde_json::to_string(spec)?,
            ChainStage::FlowFeedback(spec) => serde_json::to_string(spec)?,
        };
        Ok(serde_json::from_str(&text)?)
    }

    /// Validate this stage's knobs through the effect's own `validate()`.
    /// Called for every stage before any stage renders.
    fn validate(&self) -> Result<(), CliError> {
        match self {
            ChainStage::RetroStatic(spec) => {
                RetroStaticSettings::from(spec.clone()).validate()?;
            }
            // Constant-offset channel shift has no invalid knob values in
            // Slice 1 (any finite pixel offset is valid).
            ChainStage::ChannelShift(_) => {}
            ChainStage::PaletteQuantize(spec) => {
                PaletteQuantizeSettings::from(spec.clone()).validate()?;
            }
            ChainStage::RuttEtra(spec) => {
                RuttEtraSettings::from(spec.clone()).validate()?;
            }
            ChainStage::FlowFeedback(spec) => {
                FlowFeedbackSettings::from(spec.clone()).validate()?;
            }
        }
        Ok(())
    }

    /// The directory the *next* stage reads: stateless handlers write frames
    /// directly into the stage directory; the flow-feedback handler writes
    /// them into its own `frames/` subdirectory (its checkpoint contract owns
    /// the stage-directory root for `checkpoint.json`, `state/`, and
    /// `manifest.json`).
    fn frames_dir(&self, stage_dir: &Path) -> PathBuf {
        match self {
            ChainStage::FlowFeedback(_) => stage_dir.join("frames"),
            _ => stage_dir.to_path_buf(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct RetroStaticStageSpec {
    pub real_bpp: u32,
    pub assumed_bpp: u32,
    pub filter: ScanlineFilter,
    pub strength: f32,
}

impl Default for RetroStaticStageSpec {
    fn default() -> Self {
        let d = RetroStaticSettings::default();
        Self {
            real_bpp: d.real_bpp,
            assumed_bpp: d.assumed_bpp,
            filter: d.filter,
            strength: d.strength,
        }
    }
}

impl From<RetroStaticStageSpec> for RetroStaticSettings {
    fn from(spec: RetroStaticStageSpec) -> Self {
        Self {
            real_bpp: spec.real_bpp,
            assumed_bpp: spec.assumed_bpp,
            filter: spec.filter,
            strength: spec.strength,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct ChannelShiftStageSpec {
    pub shift_r_x: f32,
    pub shift_r_y: f32,
    pub shift_g_x: f32,
    pub shift_g_y: f32,
    pub shift_b_x: f32,
    pub shift_b_y: f32,
}

impl Default for ChannelShiftStageSpec {
    fn default() -> Self {
        let d = ChannelShiftSettings::default();
        Self {
            shift_r_x: d.shift_r_x,
            shift_r_y: d.shift_r_y,
            shift_g_x: d.shift_g_x,
            shift_g_y: d.shift_g_y,
            shift_b_x: d.shift_b_x,
            shift_b_y: d.shift_b_y,
        }
    }
}

impl From<ChannelShiftStageSpec> for ChannelShiftSettings {
    fn from(spec: ChannelShiftStageSpec) -> Self {
        Self {
            shift_r_x: spec.shift_r_x,
            shift_r_y: spec.shift_r_y,
            shift_g_x: spec.shift_g_x,
            shift_g_y: spec.shift_g_y,
            shift_b_x: spec.shift_b_x,
            shift_b_y: spec.shift_b_y,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct PaletteQuantizeStageSpec {
    pub mode: QuantizeMode,
    pub levels: u32,
}

impl Default for PaletteQuantizeStageSpec {
    fn default() -> Self {
        let d = PaletteQuantizeSettings::default();
        Self {
            mode: d.mode,
            levels: d.levels,
        }
    }
}

impl From<PaletteQuantizeStageSpec> for PaletteQuantizeSettings {
    fn from(spec: PaletteQuantizeStageSpec) -> Self {
        Self {
            mode: spec.mode,
            levels: spec.levels,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct RuttEtraStageSpec {
    pub line_pitch: u32,
    pub displacement_depth: f32,
    pub line_thickness: u32,
    pub mono: bool,
}

impl Default for RuttEtraStageSpec {
    fn default() -> Self {
        let d = RuttEtraSettings::default();
        Self {
            line_pitch: d.line_pitch,
            displacement_depth: d.displacement_depth,
            line_thickness: d.line_thickness,
            mono: d.mono,
        }
    }
}

impl From<RuttEtraStageSpec> for RuttEtraSettings {
    fn from(spec: RuttEtraStageSpec) -> Self {
        Self {
            line_pitch: spec.line_pitch,
            displacement_depth: spec.displacement_depth,
            line_thickness: spec.line_thickness,
            mono: spec.mono,
        }
    }
}

/// Stateful flow-feedback stage (Slice 2). The chain's single input feeds
/// both the modulator and carrier roles (self-feedback: the input's own
/// motion drives the flow that smears its history). Knob names and defaults
/// match the direct `render-feedback-sequence` CLI; export knobs
/// (bit depth 8, supersampling 1, frame rate 12) are pinned.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct FlowFeedbackStageSpec {
    pub carrier_amount: f32,
    pub feedback_amount: f32,
    pub feedback_mix: f32,
    pub decay: f32,
    pub iterations: u32,
    pub structure_mix: f32,
    pub structure_mode: StructureMode,
    pub flow_source: FlowSource,
}

impl Default for FlowFeedbackStageSpec {
    fn default() -> Self {
        // The direct CLI defaults (`render-feedback-sequence`), not
        // `FlowSource::default()` (that serde default exists for legacy
        // queue files; new renders default to optical flow at the CLI).
        Self {
            carrier_amount: 12.0,
            feedback_amount: 24.0,
            feedback_mix: 0.72,
            decay: 0.995,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: StructureMode::SingleScale,
            flow_source: FlowSource::OpticalFlow,
        }
    }
}

impl From<FlowFeedbackStageSpec> for FlowFeedbackSettings {
    fn from(spec: FlowFeedbackStageSpec) -> Self {
        Self {
            carrier_amount: spec.carrier_amount,
            feedback_amount: spec.feedback_amount,
            feedback_mix: spec.feedback_mix,
            decay: spec.decay,
            iterations: spec.iterations,
            structure_mix: spec.structure_mix,
            structure_mode: spec.structure_mode,
        }
    }
}

// ---------------------------------------------------------------------------
// Chain record + stage-completion markers (re-run semantics, Slice 2)
// ---------------------------------------------------------------------------

/// Content fingerprint of the chain's input frame directory (same recipe as
/// the feedback contract's source fingerprint: fnv1a64 over each frame's
/// file name and bytes, in sorted order).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainInputFingerprint {
    directory: String,
    frame_count: u32,
    checksum: String,
}

/// Persisted to `<output>/chain-record.json` after validation and before
/// stage 1 renders, so an interrupted run (no `chain-manifest.json` yet) can
/// still gate its re-run. Skipping completed stages assumes both an
/// unchanged spec and unchanged input frames — both are compared on re-run.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChainRecord {
    spec: ChainSpec,
    input: ChainInputFingerprint,
}

/// Written into a stage directory after its handler returns successfully.
/// Chosen over reusing per-effect artifacts as the done signal because it is
/// uniform across stage types and unambiguous: an interrupted stateless
/// stage still leaves frames (there is no other way to tell "all frames" —
/// the expected count depends on the input directory), and only rutt-etra
/// and flow-feedback write their own manifests.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StageCompletionMarker {
    effect: String,
    algorithm: String,
    frame_count: usize,
    settings: serde_json::Value,
}

fn chain_input_fingerprint(input_dir: &Path) -> Result<ChainInputFingerprint, CliError> {
    let frames = collect_image_frames(input_dir)?;
    if frames.is_empty() {
        return Err(CliError::Message(format!(
            "chain input directory {} contains no supported image frames",
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
    let frame_count = u32::try_from(frames.len()).map_err(|_| {
        CliError::Message("chain input contains more than u32::MAX frames".to_string())
    })?;
    Ok(ChainInputFingerprint {
        directory: input_dir.to_string_lossy().to_string(),
        frame_count,
        checksum: format!("fnv1a64:{checksum:016x}"),
    })
}

/// Reconcile the output directory with this run's spec + input. Returns
/// `true` when re-running against an existing chain record (resume/skip
/// mode). A fresh directory gets the record written **before** any stage
/// renders; a dirty directory without a record, a changed spec, or changed
/// input frames refuses without touching anything.
fn reconcile_chain_record(
    output_dir: &Path,
    spec: &ChainSpec,
    input: &ChainInputFingerprint,
) -> Result<bool, CliError> {
    let record_path = output_dir.join(CHAIN_RECORD_FILE);
    if record_path.is_file() {
        let recorded: ChainRecord = serde_json::from_str(&fs::read_to_string(&record_path)?)
            .map_err(|error| {
                CliError::Message(format!(
                    "unreadable chain record at {}: {error}; use a new output directory",
                    record_path.display()
                ))
            })?;
        if serde_json::to_value(&recorded.spec)? != serde_json::to_value(spec)? {
            return Err(CliError::Message(format!(
                "chain spec does not match the spec recorded in {}; a changed spec \
                 invalidates existing stage outputs — use a new output directory",
                output_dir.display()
            )));
        }
        if recorded.input.checksum != input.checksum {
            return Err(CliError::Message(format!(
                "input frames changed since the chain output in {} was first rendered; \
                 completed stages cannot be reused — use a new output directory",
                output_dir.display()
            )));
        }
        return Ok(true);
    }
    if output_dir.is_dir() && fs::read_dir(output_dir)?.next().is_some() {
        return Err(CliError::Message(format!(
            "output directory {} already contains files but no {CHAIN_RECORD_FILE}; \
             refusing to mix chain output with existing content — use a fresh directory",
            output_dir.display()
        )));
    }
    fs::create_dir_all(output_dir)?;
    let record = ChainRecord {
        spec: spec.clone(),
        input: input.clone(),
    };
    fs::write(&record_path, serde_json::to_string_pretty(&record)?)?;
    Ok(false)
}

fn read_stage_marker(marker_path: &Path) -> Option<StageCompletionMarker> {
    if !marker_path.is_file() {
        return None;
    }
    // A truncated/unparseable marker (interrupted mid-write) means the stage
    // is not complete; re-rendering is safe because every stage is
    // deterministic in (input frames, settings).
    fs::read_to_string(marker_path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

// ---------------------------------------------------------------------------
// Mechanic
// ---------------------------------------------------------------------------

/// A no-op modulation configuration — the chain has no per-stage modulation
/// yet (Slice 3).
fn no_modulation() -> ModulationCliArgs<'static> {
    ModulationCliArgs {
        modulate: &[],
        modulator_audio: None,
        modulator_frames: None,
        sampling: ModulationSampling::default(),
        fps: 12.0,
        cache_dir: None,
        named_modulator_audio: &[],
        named_modulator_frames: &[],
    }
}

pub(crate) fn parse_chain_spec(spec_text: &str) -> Result<ChainSpec, CliError> {
    serde_json::from_str(spec_text)
        .map_err(|error| CliError::Message(format!("invalid chain spec: {error}")))
}

fn validate_chain_spec(spec: &ChainSpec) -> Result<(), CliError> {
    if spec.version != CHAIN_SPEC_VERSION {
        return Err(CliError::Message(format!(
            "unsupported chain spec version {} (this build understands version {})",
            spec.version, CHAIN_SPEC_VERSION
        )));
    }
    if spec.stages.is_empty() {
        return Err(CliError::Message(
            "chain spec must contain at least one stage".to_string(),
        ));
    }
    for stage in &spec.stages {
        stage.validate()?;
    }
    Ok(())
}

/// Run one stage's handler (stage `i` reads `previous_dir`, writes into
/// `stage_dir`) and return its frame count.
fn render_stage(
    stage: &ChainStage,
    previous_dir: &Path,
    stage_dir: &Path,
) -> Result<usize, CliError> {
    let frame_count = match stage {
        ChainStage::RetroStatic(spec) => {
            render_retro_static_sequence(RetroStaticSequenceRequest {
                source_dir: previous_dir,
                output_dir: stage_dir,
                settings: RetroStaticSettings::from(spec.clone()),
                frames: u32::MAX,
                backend: RenderBackend::Cpu,
                modulation: no_modulation(),
            })?
            .frame_count
        }
        ChainStage::ChannelShift(spec) => {
            render_channel_shift_sequence(ChannelShiftSequenceRequest {
                source_b_dir: previous_dir,
                output_dir: stage_dir,
                settings: ChannelShiftSettings::from(spec.clone()),
                frames: u32::MAX,
                backend: RenderBackend::Cpu,
                source_a_dir: None,
                flow_gain: 0.0,
                radius: 0,
                modulation: no_modulation(),
            })?
            .frame_count
        }
        ChainStage::PaletteQuantize(spec) => {
            render_palette_quantize_sequence(PaletteQuantizeSequenceRequest {
                source_b_dir: previous_dir,
                output_dir: stage_dir,
                settings: PaletteQuantizeSettings::from(spec.clone()),
                frames: u32::MAX,
                backend: RenderBackend::Cpu,
                modulation: no_modulation(),
            })?
            .frame_count
        }
        ChainStage::RuttEtra(spec) => {
            render_rutt_etra_sequence(RuttEtraSequenceRequest {
                source_b_dir: previous_dir,
                output_dir: stage_dir,
                settings: RuttEtraSettings::from(spec.clone()),
                frames: u32::MAX,
                modulation: no_modulation(),
            })?
            .frame_count
        }
        ChainStage::FlowFeedback(spec) => {
            render_feedback_sequence(FeedbackSequenceRenderRequest {
                modulator_dir: previous_dir,
                carrier_dir: previous_dir,
                output_dir: stage_dir,
                flow_cache_dir: None,
                max_frames: None,
                reset_at_frame: None,
                frame_rate: CHAIN_FEEDBACK_FRAME_RATE,
                settings: FlowFeedbackSettings::from(spec.clone()),
                output_bit_depth: CHAIN_FEEDBACK_OUTPUT_BIT_DEPTH,
                temporal_supersampling: CHAIN_FEEDBACK_TEMPORAL_SUPERSAMPLING,
                backend: RenderBackend::Cpu,
                flow_source: spec.flow_source,
                job_id: CHAIN_FEEDBACK_JOB_ID,
                provenance: None,
                stop_after_frame: false,
                modulation: no_modulation(),
            })?
            .frame_count
        }
    };
    Ok(frame_count)
}

/// `render-chain <spec.json> <input-frames-dir> <output-dir>`.
///
/// Parses and validates the whole spec before rendering anything, then runs
/// each stage in order: stage 1 reads `input_dir`, stage *i* (i > 1) reads
/// stage *i-1*'s output frames. Re-running the same spec into the same
/// output directory skips completed stages (their `stage-complete.json`
/// markers) and resumes an interrupted stateful stage from its checkpoint;
/// a changed spec or changed input refuses. Writes
/// `<output-dir>/chain-manifest.json` after the final stage and prints the
/// final stage's frames directory.
pub(crate) fn render_chain(
    spec_path: &Path,
    input_dir: &Path,
    output_dir: &Path,
) -> Result<(), CliError> {
    let spec_text = fs::read_to_string(spec_path)?;
    let spec = parse_chain_spec(&spec_text)?;
    validate_chain_spec(&spec)?;

    // Nothing is written to output_dir until the whole spec has validated
    // and the input has been fingerprinted; the first write is the chain
    // record itself (so even an interrupted stage 1 leaves a re-run gate).
    let input_fingerprint = chain_input_fingerprint(input_dir)?;
    let resuming = reconcile_chain_record(output_dir, &spec, &input_fingerprint)?;

    let mut previous_dir = input_dir.to_path_buf();
    let mut manifest_stages = Vec::with_capacity(spec.stages.len());
    let mut final_frame_count = 0usize;
    let mut final_frames_dir = output_dir.to_path_buf();

    for (position, stage) in spec.stages.iter().enumerate() {
        let stage_index = position + 1;
        let effect_tag = stage.effect_tag();
        let stage_dir = output_dir.join(format!("stage_{stage_index:02}_{effect_tag}"));
        let algorithm = stage.algorithm_id();
        let settings_value = stage.settings_value()?;
        let marker_path = stage_dir.join(STAGE_COMPLETE_FILE);

        let frame_count = match read_stage_marker(&marker_path) {
            Some(marker) if resuming => {
                if marker.effect != effect_tag
                    || marker.algorithm != algorithm
                    || marker.settings != settings_value
                {
                    return Err(CliError::Message(format!(
                        "stage marker at {} does not match the chain spec; the output \
                         directory is inconsistent — use a new output directory",
                        marker_path.display()
                    )));
                }
                println!("stage {stage_index:02} ({effect_tag}) already complete — skipped");
                marker.frame_count
            }
            _ => {
                let frame_count = render_stage(stage, &previous_dir, &stage_dir)?;
                let marker = StageCompletionMarker {
                    effect: effect_tag.to_string(),
                    algorithm: algorithm.clone(),
                    frame_count,
                    settings: settings_value.clone(),
                };
                fs::write(&marker_path, serde_json::to_string_pretty(&marker)?)?;
                frame_count
            }
        };

        manifest_stages.push(serde_json::json!({
            "effect": effect_tag,
            "algorithm": algorithm,
            "settings": settings_value,
        }));
        final_frame_count = frame_count;
        final_frames_dir = stage.frames_dir(&stage_dir);
        previous_dir = final_frames_dir.clone();
    }

    let chain_manifest = serde_json::json!({
        "version": spec.version,
        "frame_count": final_frame_count,
        "stages": manifest_stages,
    });
    fs::write(
        output_dir.join("chain-manifest.json"),
        serde_json::to_string_pretty(&chain_manifest)?,
    )?;

    println!(
        "rendered chain with {} stage(s) ({} frame(s)) from {} to {}; final stage output: {}",
        spec.stages.len(),
        final_frame_count,
        input_dir.display(),
        output_dir.display(),
        final_frames_dir.display(),
    );

    Ok(())
}
