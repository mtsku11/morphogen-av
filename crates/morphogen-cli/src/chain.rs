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
    apply_channel_shift_modulation, apply_flow_feedback_modulation,
    apply_palette_quantize_modulation, apply_retro_static_modulation, apply_rutt_etra_modulation,
    parse_modulation_route, validate_route_targets, ChannelShiftSettings, FlowFeedbackSettings,
    ModulationSampling, PaletteQuantizeSettings, QuantizeMode, RetroStaticSettings,
    RuttEtraSettings, ScanlineFilter, StructureMode, CHANNEL_SHIFT_ALGORITHM,
    PALETTE_QUANTIZE_ALGORITHM, RETRO_STATIC_ALGORITHM, RUTT_ETRA_ALGORITHM,
};
use serde::{Deserialize, Serialize};

use crate::error::CliError;
use crate::imaging::{collect_image_frames, update_fnv1a};
use crate::modulate::{parse_named_modulator_specs, resolve_modulator_media};
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

    /// Validate this stage's knobs through the effect's own `validate()`,
    /// plus its modulation block (route grammar, duplicate targets, unknown
    /// targets via the effect's apply fn, media requirements). Called for
    /// every stage before any stage renders.
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
        self.validate_modulation()
    }

    /// This stage's modulation block, if any (Slice 3).
    pub(crate) fn modulation_spec(&self) -> Option<&StageModulationSpec> {
        match self {
            ChainStage::RetroStatic(spec) => spec.modulation.as_ref(),
            ChainStage::ChannelShift(spec) => spec.modulation.as_ref(),
            ChainStage::PaletteQuantize(spec) => spec.modulation.as_ref(),
            ChainStage::RuttEtra(spec) => spec.modulation.as_ref(),
            ChainStage::FlowFeedback(spec) => spec.modulation.as_ref(),
        }
    }

    /// This stage's modulation block, mutably (composition master-clock support).
    fn modulation_spec_mut(&mut self) -> Option<&mut StageModulationSpec> {
        match self {
            ChainStage::RetroStatic(spec) => spec.modulation.as_mut(),
            ChainStage::ChannelShift(spec) => spec.modulation.as_mut(),
            ChainStage::PaletteQuantize(spec) => spec.modulation.as_mut(),
            ChainStage::RuttEtra(spec) => spec.modulation.as_mut(),
            ChainStage::FlowFeedback(spec) => spec.modulation.as_mut(),
        }
    }

    /// Make `<name>` resolvable as a named modulator on this stage by pointing
    /// it at the given media — the seam the composition uses to bind its master
    /// clock into a scene's chain. A no-op on a stage with no modulation block
    /// (it has no routes to resolve). Not exposed in the chain's own CLI/queue
    /// surface: only the composition layer calls it.
    pub(crate) fn inject_named_modulator_media(
        &mut self,
        name: &str,
        audio: Option<&Path>,
        frames: Option<&Path>,
    ) {
        let Some(modulation) = self.modulation_spec_mut() else {
            return;
        };
        if let Some(audio) = audio {
            modulation
                .named_modulator_audio
                .push(format!("{name}={}", audio.display()));
        }
        if let Some(frames) = frames {
            modulation
                .named_modulator_frames
                .push(format!("{name}={}", frames.display()));
        }
    }

    /// Probe one route target against this effect's apply function on a
    /// scratch settings copy (the queue-add precedent) — an unknown target
    /// fails at spec-validation time, before anything renders.
    fn probe_modulation_target(&self, target: &str) -> Result<(), CliError> {
        match self {
            ChainStage::RetroStatic(spec) => {
                let mut settings = RetroStaticSettings::from(spec.clone());
                apply_retro_static_modulation(&mut settings, target, 0.0)?;
            }
            ChainStage::ChannelShift(spec) => {
                let mut settings = ChannelShiftSettings::from(spec.clone());
                apply_channel_shift_modulation(&mut settings, target, 0.0)?;
            }
            ChainStage::PaletteQuantize(spec) => {
                let mut settings = PaletteQuantizeSettings::from(spec.clone());
                apply_palette_quantize_modulation(&mut settings, target, 0.0)?;
            }
            ChainStage::RuttEtra(spec) => {
                let mut settings = RuttEtraSettings::from(spec.clone());
                apply_rutt_etra_modulation(&mut settings, target, 0.0)?;
            }
            ChainStage::FlowFeedback(spec) => {
                let mut settings = FlowFeedbackSettings::from(spec.clone());
                apply_flow_feedback_modulation(&mut settings, target, 0.0)?;
            }
        }
        Ok(())
    }

    /// Validate the stage's modulation block: route grammar + duplicate
    /// targets, unknown targets (apply-fn probe), envelope fps rules, and
    /// media requirements (a route's source must have its default or named
    /// media declared in the block — the missing-media wording comes from
    /// the shared `resolve_modulator_media`).
    fn validate_modulation(&self) -> Result<(), CliError> {
        let Some(modulation) = self.modulation_spec() else {
            return Ok(());
        };
        if let Some(fps) = modulation.fps {
            if matches!(self, ChainStage::FlowFeedback(_)) {
                return Err(CliError::Message(
                    "a flow_feedback stage's modulation envelopes sample against its \
                     pinned frame rate (one timeline per stateful render) — remove the \
                     modulation \"fps\" field"
                        .to_string(),
                ));
            }
            if !fps.is_finite() || fps <= 0.0 {
                return Err(CliError::Message(
                    "stage modulation fps must be positive and finite".to_string(),
                ));
            }
        }
        let routes = modulation
            .routes
            .iter()
            .map(|spec| parse_modulation_route(spec))
            .collect::<Result<Vec<_>, _>>()?;
        validate_route_targets(&routes)?;
        let named_audio = parse_named_modulator_specs(
            &modulation.named_modulator_audio,
            "modulation.named_modulator_audio",
        )?;
        let named_frames = parse_named_modulator_specs(
            &modulation.named_modulator_frames,
            "modulation.named_modulator_frames",
        )?;
        for route in &routes {
            self.probe_modulation_target(&route.target)?;
            if route.source.needs_audio() {
                resolve_modulator_media(
                    route,
                    modulation.modulator_audio.as_deref(),
                    &named_audio,
                    "a modulation.modulator_audio path on this stage",
                    "a modulation.named_modulator_audio entry",
                )?;
            }
            if route.source.needs_frames() {
                resolve_modulator_media(
                    route,
                    modulation.modulator_frames.as_deref(),
                    &named_frames,
                    "a modulation.modulator_frames path on this stage",
                    "a modulation.named_modulator_frames entry",
                )?;
            }
        }
        Ok(())
    }

    /// The `ModulationCliArgs` this stage renders with. A feedback stage's
    /// envelope base is its pinned frame rate (the one-timeline-per-stateful-
    /// render rule); stateless stages default to the direct CLI's
    /// `--modulation-fps` default of 12.
    fn modulation_args(&self) -> ModulationCliArgs<'_> {
        match self.modulation_spec() {
            None => no_modulation(),
            Some(modulation) => ModulationCliArgs {
                modulate: &modulation.routes,
                modulator_audio: modulation.modulator_audio.as_deref(),
                modulator_frames: modulation.modulator_frames.as_deref(),
                sampling: modulation.sampling,
                fps: if matches!(self, ChainStage::FlowFeedback(_)) {
                    CHAIN_FEEDBACK_FRAME_RATE
                } else {
                    modulation.fps.unwrap_or(12.0)
                },
                cache_dir: None,
                named_modulator_audio: &modulation.named_modulator_audio,
                named_modulator_frames: &modulation.named_modulator_frames,
                modulator_midi: None,
                named_modulator_midi: &[],
            },
        }
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

/// A stage's optional modulation block (Slice 3): the standard route
/// grammar plus the media/sampling companions, mirroring `ModulationCliArgs`
/// semantics. A nested object rather than flattened fields because serde's
/// `deny_unknown_fields` does not compose with `flatten`. Skipped when
/// absent so pre-slice markers, records, and manifests stay byte-identical.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct StageModulationSpec {
    /// `--modulate` route specs verbatim
    /// (`<target>=[<name>.]<source>[:<scale>[,<offset>]][@hold|@smooth]`).
    /// LFO sources need no media — the natural chain modulators.
    pub routes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modulator_audio: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modulator_frames: Option<PathBuf>,
    pub sampling: ModulationSampling,
    /// Envelope fps for stateless stages (`None` = the direct CLI's
    /// `--modulation-fps` default of 12). Invalid on a `flow_feedback`
    /// stage, whose envelope base is its pinned frame rate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    /// Repeatable `<name>=<path>` named-modulator media specs.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub named_modulator_audio: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub named_modulator_frames: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub(crate) struct RetroStaticStageSpec {
    pub real_bpp: u32,
    pub assumed_bpp: u32,
    pub filter: ScanlineFilter,
    pub strength: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modulation: Option<StageModulationSpec>,
}

impl Default for RetroStaticStageSpec {
    fn default() -> Self {
        let d = RetroStaticSettings::default();
        Self {
            real_bpp: d.real_bpp,
            assumed_bpp: d.assumed_bpp,
            filter: d.filter,
            strength: d.strength,
            modulation: None,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modulation: Option<StageModulationSpec>,
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
            modulation: None,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modulation: Option<StageModulationSpec>,
}

impl Default for PaletteQuantizeStageSpec {
    fn default() -> Self {
        let d = PaletteQuantizeSettings::default();
        Self {
            mode: d.mode,
            levels: d.levels,
            modulation: None,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modulation: Option<StageModulationSpec>,
}

impl Default for RuttEtraStageSpec {
    fn default() -> Self {
        let d = RuttEtraSettings::default();
        Self {
            line_pitch: d.line_pitch,
            displacement_depth: d.displacement_depth,
            line_thickness: d.line_thickness,
            mono: d.mono,
            modulation: None,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modulation: Option<StageModulationSpec>,
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
            modulation: None,
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
        modulator_midi: None,
        named_modulator_midi: &[],
    }
}

pub(crate) fn parse_chain_spec(spec_text: &str) -> Result<ChainSpec, CliError> {
    serde_json::from_str(spec_text)
        .map_err(|error| CliError::Message(format!("invalid chain spec: {error}")))
}

pub(crate) fn validate_chain_spec(spec: &ChainSpec) -> Result<(), CliError> {
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
                modulation: stage.modulation_args(),
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
                modulation: stage.modulation_args(),
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
                modulation: stage.modulation_args(),
            })?
            .frame_count
        }
        ChainStage::RuttEtra(spec) => {
            render_rutt_etra_sequence(RuttEtraSequenceRequest {
                source_b_dir: previous_dir,
                output_dir: stage_dir,
                source_a_dir: None,
                settings: RuttEtraSettings::from(spec.clone()),
                frames: u32::MAX,
                backend: RenderBackend::Cpu,
                modulation: stage.modulation_args(),
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
                modulation: stage.modulation_args(),
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
    let spec = parse_and_validate_chain_spec(&spec_text)?;
    let summary = run_chain_spec(&spec, input_dir, output_dir)?;
    println!(
        "rendered chain with {} stage(s) ({} frame(s)) from {} to {}; final stage output: {}",
        spec.stages.len(),
        summary.frame_count,
        input_dir.display(),
        output_dir.display(),
        summary.final_frames_dir.display(),
    );
    Ok(())
}

/// Parse + whole-spec validation in one step (the add-time gate shared by
/// the direct command and `queue-add-chain`).
pub(crate) fn parse_and_validate_chain_spec(spec_text: &str) -> Result<ChainSpec, CliError> {
    let spec = parse_chain_spec(spec_text)?;
    validate_chain_spec(&spec)?;
    Ok(spec)
}

/// The resolved spec (defaults filled) as a JSON document — what a queue job
/// persists. Serialized through the string form so f32 knobs keep their
/// shortest representation (the `settings_value` precedent).
pub(crate) fn resolved_chain_spec_value(spec: &ChainSpec) -> Result<serde_json::Value, CliError> {
    Ok(serde_json::from_str(&serde_json::to_string(spec)?)?)
}

/// Rebuild + re-validate a spec from a persisted queue-job document.
pub(crate) fn chain_spec_from_value(value: &serde_json::Value) -> Result<ChainSpec, CliError> {
    parse_and_validate_chain_spec(&serde_json::to_string(value)?)
}

pub(crate) struct ChainRunSummary {
    pub(crate) frame_count: usize,
    pub(crate) final_frames_dir: PathBuf,
}

/// The chain mechanic proper, on an already-validated spec: stage loop,
/// re-run reconciliation, markers, and the chain manifest.
pub(crate) fn run_chain_spec(
    spec: &ChainSpec,
    input_dir: &Path,
    output_dir: &Path,
) -> Result<ChainRunSummary, CliError> {
    // Nothing is written to output_dir until the whole spec has validated
    // and the input has been fingerprinted; the first write is the chain
    // record itself (so even an interrupted stage 1 leaves a re-run gate).
    let input_fingerprint = chain_input_fingerprint(input_dir)?;
    let resuming = reconcile_chain_record(output_dir, spec, &input_fingerprint)?;

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

    Ok(ChainRunSummary {
        frame_count: final_frame_count,
        final_frames_dir,
    })
}
