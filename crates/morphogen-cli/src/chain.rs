//! `render-chain` — run an ordered list of stateless single-source effect
//! stages, each stage's output feeding the next, with a chain-level provenance
//! manifest. See `docs/EFFECT_CHAIN_MILESTONE.md` for the acceptance contract.
//!
//! Slice 1 scope: CPU-only, stateless, single-source stages
//! (`retro_static`, `channel_shift` [constant shifts only], `palette_quantize`,
//! `rutt_etra`). The whole spec is parsed and validated (every stage's
//! settings through the effect's own `validate()`) **before** any stage
//! renders, so an invalid stage-N leaves nothing on disk.

use std::{fs, path::Path};

use morphogen_core::RenderBackend;
use morphogen_render::{
    ChannelShiftSettings, ModulationSampling, PaletteQuantizeSettings, QuantizeMode,
    RetroStaticSettings, RuttEtraSettings, ScanlineFilter, CHANNEL_SHIFT_ALGORITHM,
    PALETTE_QUANTIZE_ALGORITHM, RETRO_STATIC_ALGORITHM, RUTT_ETRA_ALGORITHM,
};
use serde::{Deserialize, Serialize};

use crate::error::CliError;
use crate::render::{
    render_channel_shift_sequence, render_palette_quantize_sequence, render_retro_static_sequence,
    render_rutt_etra_sequence, ChannelShiftSequenceRequest, ModulationCliArgs,
    PaletteQuantizeSequenceRequest, RetroStaticSequenceRequest, RuttEtraSequenceRequest,
};

/// Only spec version this slice understands. The field exists for forward
/// compatibility (list → graph, see the milestone doc); a mismatched version
/// is a clear error rather than a silent best-effort parse.
const CHAIN_SPEC_VERSION: u32 = 1;

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
}

impl ChainStage {
    fn effect_tag(&self) -> &'static str {
        match self {
            ChainStage::RetroStatic(_) => "retro_static",
            ChainStage::ChannelShift(_) => "channel_shift",
            ChainStage::PaletteQuantize(_) => "palette_quantize",
            ChainStage::RuttEtra(_) => "rutt_etra",
        }
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
        }
        Ok(())
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

// ---------------------------------------------------------------------------
// Mechanic
// ---------------------------------------------------------------------------

/// A no-op modulation configuration — Slice 1 has no per-stage modulation.
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

/// `render-chain <spec.json> <input-frames-dir> <output-dir>`.
///
/// Parses and validates the whole spec before rendering anything, then runs
/// each stage in order: stage 1 reads `input_dir`, stage *i* (i > 1) reads
/// stage *i-1*'s output directory. Writes `<output-dir>/chain-manifest.json`
/// after the final stage and prints the final stage's directory.
pub(crate) fn render_chain(
    spec_path: &Path,
    input_dir: &Path,
    output_dir: &Path,
) -> Result<(), CliError> {
    let spec_text = fs::read_to_string(spec_path)?;
    let spec = parse_chain_spec(&spec_text)?;
    validate_chain_spec(&spec)?;

    // Nothing is written to output_dir until every stage has validated.
    fs::create_dir_all(output_dir)?;

    let mut previous_dir = input_dir.to_path_buf();
    let mut manifest_stages = Vec::with_capacity(spec.stages.len());
    let mut final_frame_count = 0usize;
    let mut final_stage_dir = output_dir.to_path_buf();

    for (position, stage) in spec.stages.iter().enumerate() {
        let stage_index = position + 1;
        let effect_tag = stage.effect_tag();
        let stage_dir = output_dir.join(format!("stage_{stage_index:02}_{effect_tag}"));

        let (algorithm, settings_json, frame_count) = match stage {
            ChainStage::RetroStatic(spec) => {
                let settings = RetroStaticSettings::from(spec.clone());
                let result = render_retro_static_sequence(RetroStaticSequenceRequest {
                    source_dir: &previous_dir,
                    output_dir: &stage_dir,
                    settings,
                    frames: u32::MAX,
                    backend: RenderBackend::Cpu,
                    modulation: no_modulation(),
                })?;
                (
                    RETRO_STATIC_ALGORITHM,
                    serde_json::to_value(settings)?,
                    result.frame_count,
                )
            }
            ChainStage::ChannelShift(spec) => {
                let settings = ChannelShiftSettings::from(spec.clone());
                let result = render_channel_shift_sequence(ChannelShiftSequenceRequest {
                    source_b_dir: &previous_dir,
                    output_dir: &stage_dir,
                    settings,
                    frames: u32::MAX,
                    backend: RenderBackend::Cpu,
                    source_a_dir: None,
                    flow_gain: 0.0,
                    radius: 0,
                    modulation: no_modulation(),
                })?;
                (
                    CHANNEL_SHIFT_ALGORITHM,
                    serde_json::to_value(settings)?,
                    result.frame_count,
                )
            }
            ChainStage::PaletteQuantize(spec) => {
                let settings = PaletteQuantizeSettings::from(spec.clone());
                let result = render_palette_quantize_sequence(PaletteQuantizeSequenceRequest {
                    source_b_dir: &previous_dir,
                    output_dir: &stage_dir,
                    settings,
                    frames: u32::MAX,
                    backend: RenderBackend::Cpu,
                    modulation: no_modulation(),
                })?;
                (
                    PALETTE_QUANTIZE_ALGORITHM,
                    serde_json::to_value(settings)?,
                    result.frame_count,
                )
            }
            ChainStage::RuttEtra(spec) => {
                let settings = RuttEtraSettings::from(spec.clone());
                let result = render_rutt_etra_sequence(RuttEtraSequenceRequest {
                    source_b_dir: &previous_dir,
                    output_dir: &stage_dir,
                    settings,
                    frames: u32::MAX,
                    modulation: no_modulation(),
                })?;
                (
                    RUTT_ETRA_ALGORITHM,
                    serde_json::to_value(settings)?,
                    result.frame_count,
                )
            }
        };

        manifest_stages.push(serde_json::json!({
            "effect": effect_tag,
            "algorithm": algorithm,
            "settings": settings_json,
        }));
        final_frame_count = frame_count;
        final_stage_dir = stage_dir.clone();
        previous_dir = stage_dir;
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
        final_stage_dir.display(),
    );

    Ok(())
}
