//! Modulation matrix — typed analysis envelopes routed onto effect knobs.
//!
//! A [`ModulationRoute`] is the modular-synth patch cable: it binds one
//! normalized analysis envelope (audio RMS/onset/centroid, video luma/flow —
//! extracted by the caller, who owns media decoding) to one numeric knob on an
//! effect's settings struct, through an affine `value·scale + offset` mapping
//! clamped to the knob's declared range. The effect code itself is untouched
//! and unaware of modulation: the caller overwrites the routed knobs on a copy
//! of the settings each frame, then runs the ordinary render function.
//!
//! Deterministic throughout: envelopes are pure functions of the modulator
//! media, sampling is `hold` (step) or `smooth` (linear) at the output frame's
//! time, and clamping (never erroring) guarantees an envelope cannot abort a
//! render mid-sequence. Zero routes is the exact unmodulated code path.
//!
//! Contract: `docs/MODULATION_MATRIX_MILESTONE.md`.

use serde::{Deserialize, Serialize};

use crate::{
    ChannelShiftSettings, FlowFeedbackSettings, FluidAdvectSettings, FluidAdvectTwoSourceSettings,
    PaletteQuantizeSettings, PixelSortSettings, QuantizeMode, RenderError, RetroStaticSettings,
    RuttEtraSettings, ScanlineFilter, SortAxis, SortDirection,
};

/// An LFO waveform shape (milestone doc, "Semantics"). Every shape emits
/// `[0,1]` and is `0.0` at `p = 0` (square is low-first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LfoShape {
    Sine,
    Triangle,
    Square,
    Saw,
}

impl LfoShape {
    /// The CLI spelling (`sine`, `triangle`, `square`, `saw`).
    pub fn name(self) -> &'static str {
        match self {
            LfoShape::Sine => "sine",
            LfoShape::Triangle => "triangle",
            LfoShape::Square => "square",
            LfoShape::Saw => "saw",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "sine" => Some(LfoShape::Sine),
            "triangle" => Some(LfoShape::Triangle),
            "square" => Some(LfoShape::Square),
            "saw" => Some(LfoShape::Saw),
            _ => None,
        }
    }

    /// Evaluate the shape at phase-fraction `p ∈ [0,1)`. Pinned formulas —
    /// changing any changes rendered frames (milestone doc, "Semantics").
    pub fn evaluate(self, p: f64) -> f64 {
        match self {
            LfoShape::Sine => 0.5 - 0.5 * (2.0 * std::f64::consts::PI * p).cos(),
            LfoShape::Triangle => {
                if p < 0.5 {
                    2.0 * p
                } else {
                    2.0 - 2.0 * p
                }
            }
            LfoShape::Saw => p,
            LfoShape::Square => {
                if p < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
        }
    }
}

/// Which analysis descriptor drives a route.
///
/// The `f32` fields on `Lfo` force dropping the `Eq` derive (keep `Copy`,
/// `PartialEq`); nothing requires `Eq` — `EnvelopeKey` comparisons are `==`
/// in a `Vec`, there are no map keys.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModulationSource {
    /// Peak-normalized RMS envelope of the modulator WAV (**relative**).
    AudioRms,
    /// Peak-normalized spectral-flux onset strength (**relative**).
    AudioOnset,
    /// Spectral centroid / Nyquist (**absolute**).
    AudioCentroid,
    /// Per-frame mean Rec.709 luma of the modulator frames (**absolute**).
    Luma,
    /// Peak-normalized mean temporal optical-flow magnitude (**relative**).
    Flow,
    /// A pure function of `(frame_time, params)` — no media, no sidecar, no
    /// fingerprint. `rate_hz` is cycles/second on the envelope timeline;
    /// `phase` is a phase offset in cycles (`fract` applied at eval time).
    Lfo {
        shape: LfoShape,
        rate_hz: f32,
        phase: f32,
    },
}

impl ModulationSource {
    /// The CLI spelling (`audio-rms`, `audio-onset`, `audio-centroid`,
    /// `luma`, `flow`). The LFO source's spelling is dynamic (shape/rate/
    /// phase), so this returns a generic `"lfo"` tag for it — use
    /// [`ModulationSource::spec_text`] for the round-trippable spelling.
    pub fn name(self) -> &'static str {
        match self {
            ModulationSource::AudioRms => "audio-rms",
            ModulationSource::AudioOnset => "audio-onset",
            ModulationSource::AudioCentroid => "audio-centroid",
            ModulationSource::Luma => "luma",
            ModulationSource::Flow => "flow",
            ModulationSource::Lfo { .. } => "lfo",
        }
    }

    /// The exact round-trippable spelling for the CLI grammar's source
    /// clause: media variants keep their `name()` spelling; the LFO variant
    /// spells `lfo(<shape>,<rate_hz>,<phase>)` with `f32`'s `Display` (exact
    /// round-trip, the established queue-identity mechanism).
    pub fn spec_text(self) -> String {
        match self {
            ModulationSource::Lfo {
                shape,
                rate_hz,
                phase,
            } => format!("lfo({},{},{})", shape.name(), rate_hz, phase),
            other => other.name().to_string(),
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "audio-rms" => Some(ModulationSource::AudioRms),
            "audio-onset" => Some(ModulationSource::AudioOnset),
            "audio-centroid" => Some(ModulationSource::AudioCentroid),
            "luma" => Some(ModulationSource::Luma),
            "flow" => Some(ModulationSource::Flow),
            _ => None,
        }
    }

    /// True when the route needs `--modulator-audio`.
    pub fn needs_audio(self) -> bool {
        matches!(
            self,
            ModulationSource::AudioRms
                | ModulationSource::AudioOnset
                | ModulationSource::AudioCentroid
        )
    }

    /// True when the route needs `--modulator-frames`.
    pub fn needs_frames(self) -> bool {
        matches!(self, ModulationSource::Luma | ModulationSource::Flow)
    }
}

/// How the sparse envelope is evaluated at an output frame's time. Matches the
/// video-audio route's resample semantics, applied per frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModulationSampling {
    /// Step: the latest sample at or before the frame time.
    #[default]
    Hold,
    /// Linear interpolation between the bracketing samples.
    Smooth,
}

/// One patch cable: `settings.<target> = clamp(envelope(t)·scale + offset)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModulationRoute {
    /// Knob name on the effect's settings struct (effect-specific registry).
    pub target: String,
    pub source: ModulationSource,
    #[serde(default = "default_scale")]
    pub scale: f32,
    #[serde(default)]
    pub offset: f32,
    /// Per-route sampling override (`@hold` / `@smooth` in the CLI grammar);
    /// `None` inherits the render-level sampling. Skipped when unset so
    /// pre-slice checkpoints and manifests stay byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<ModulationSampling>,
    /// Named modulator this route reads (`<name>.<source>` in the CLI
    /// grammar, media supplied by `--named-modulator-audio/frames name=path`);
    /// `None` reads the default `--modulator-audio`/`--modulator-frames`
    /// media. Skipped when unset so pre-slice checkpoints and manifests stay
    /// byte-identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modulator: Option<String>,
}

fn default_scale() -> f32 {
    1.0
}

/// Parse the CLI route grammar
/// `<target>=[<name>.]<source>[:<scale>[,<offset>]][@hold|@smooth]`.
pub fn parse_modulation_route(spec: &str) -> Result<ModulationRoute, RenderError> {
    let bad = |detail: &str| {
        RenderError::InvalidModulationRoute(format!(
            "'{spec}': {detail} (expected <target>=[<name>.]<source>[:<scale>[,<offset>]][@hold|@smooth])"
        ))
    };
    // The optional per-route sampling suffix comes off first so the rest of
    // the grammar is unchanged.
    let (body, sampling) = match spec.rsplit_once('@') {
        Some((body, suffix)) => {
            let sampling = match suffix.trim() {
                "hold" => ModulationSampling::Hold,
                "smooth" => ModulationSampling::Smooth,
                other => {
                    return Err(bad(&format!(
                        "unknown sampling '{other}' (available: hold, smooth)"
                    )))
                }
            };
            (body, Some(sampling))
        }
        None => (spec, None),
    };
    let (target, rest) = body.split_once('=').ok_or_else(|| bad("missing '='"))?;
    let target = target.trim();
    if target.is_empty() {
        return Err(bad("empty target"));
    }
    let (source_name, params) = match rest.split_once(':') {
        Some((source, params)) => (source, Some(params)),
        None => (rest, None),
    };
    // An optional `<name>.` prefix selects a named modulator; the source
    // names themselves contain no '.', so the first dot is unambiguous —
    // except `lfo(...)`, whose parens may themselves contain a '.'
    // (`lfo(sine,0.5)`). An `lfo(...)` body never takes a named-modulator
    // prefix (no media to name), so the dot-split must not run on it; a real
    // prefix ahead of one (`wob.lfo(sine)`) is a distinct, explicit error.
    let (modulator, source_name) = if source_name.trim().starts_with("lfo(") {
        (None, source_name)
    } else {
        match source_name.split_once('.') {
            Some((name, inner)) => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(bad("empty modulator name"));
                }
                if inner.trim().starts_with("lfo(") {
                    return Err(bad(
                        "a named modulator prefix is not allowed on an lfo source (lfo needs no media)",
                    ));
                }
                (Some(name.to_string()), inner)
            }
            None => (None, source_name),
        }
    };
    let source_name = source_name.trim();
    let source = if source_name.starts_with("lfo(") {
        parse_lfo_source(source_name, &bad)?
    } else {
        ModulationSource::parse(source_name).ok_or_else(|| {
            bad(&format!(
                "unknown source '{source_name}' (available: audio-rms, audio-onset, audio-centroid, luma, flow, lfo)"
            ))
        })?
    };
    let (scale, offset) = match params {
        None => (1.0, 0.0),
        Some(params) => {
            let (scale_text, offset_text) = match params.split_once(',') {
                Some((scale, offset)) => (scale, Some(offset)),
                None => (params, None),
            };
            let scale: f32 = scale_text
                .trim()
                .parse()
                .map_err(|_| bad(&format!("invalid scale '{}'", scale_text.trim())))?;
            let offset: f32 = match offset_text {
                None => 0.0,
                Some(text) => text
                    .trim()
                    .parse()
                    .map_err(|_| bad(&format!("invalid offset '{}'", text.trim())))?,
            };
            (scale, offset)
        }
    };
    if !scale.is_finite() || !offset.is_finite() {
        return Err(bad("scale and offset must be finite"));
    }
    Ok(ModulationRoute {
        target: target.to_string(),
        source,
        scale,
        offset,
        sampling,
        modulator,
    })
}

/// Parse the `lfo(<shape>[,<rate_hz>[,<phase>]])` source body (parens
/// included, already trimmed). `bad` is the caller's spec-scoped error
/// constructor (milestone doc, "Grammar").
fn parse_lfo_source(
    text: &str,
    bad: &dyn Fn(&str) -> RenderError,
) -> Result<ModulationSource, RenderError> {
    let inner = text
        .strip_prefix("lfo(")
        .and_then(|rest| rest.strip_suffix(')'))
        .ok_or_else(|| bad(&format!("malformed lfo(...) source '{text}'")))?;
    let mut parts = inner.split(',');
    let shape_text = parts.next().unwrap_or("").trim();
    let shape = LfoShape::parse(shape_text).ok_or_else(|| {
        bad(&format!(
            "unknown lfo shape '{shape_text}' (available: sine, triangle, square, saw)"
        ))
    })?;
    let rate_hz: f32 = match parts.next() {
        None => 1.0,
        Some(text) => text
            .trim()
            .parse()
            .map_err(|_| bad(&format!("invalid lfo rate_hz '{}'", text.trim())))?,
    };
    if !rate_hz.is_finite() || rate_hz <= 0.0 {
        return Err(bad("lfo rate_hz must be finite and greater than 0"));
    }
    let phase: f32 = match parts.next() {
        None => 0.0,
        Some(text) => text
            .trim()
            .parse()
            .map_err(|_| bad(&format!("invalid lfo phase '{}'", text.trim())))?,
    };
    if !phase.is_finite() {
        return Err(bad("lfo phase must be finite"));
    }
    if parts.next().is_some() {
        return Err(bad("lfo(...) accepts at most shape, rate_hz, phase"));
    }
    Ok(ModulationSource::Lfo {
        shape,
        rate_hz,
        phase,
    })
}

/// Reject two routes driving the same target (ambiguous intent).
pub fn validate_route_targets(routes: &[ModulationRoute]) -> Result<(), RenderError> {
    for (index, route) in routes.iter().enumerate() {
        if routes[..index]
            .iter()
            .any(|prior| prior.target == route.target)
        {
            return Err(RenderError::InvalidModulationRoute(format!(
                "target '{}' is driven by more than one route",
                route.target
            )));
        }
    }
    Ok(())
}

/// Peak-normalize non-negative envelope values in place. A peak of zero (a
/// silent/static modulator) yields an all-zero envelope, not an error — the
/// documented **relative**-normalization trap.
pub fn peak_normalize(samples: &mut [(f64, f32)]) {
    let peak = samples
        .iter()
        .map(|&(_, value)| value)
        .fold(0.0_f32, f32::max);
    if peak > 0.0 {
        for (_, value) in samples.iter_mut() {
            *value /= peak;
        }
    } else {
        for (_, value) in samples.iter_mut() {
            *value = 0.0;
        }
    }
}

/// Evaluate a sparse `(time_seconds, value)` envelope at time `t`. An empty
/// envelope yields `0.0`; before the first sample the first value holds, after
/// the last sample the last value holds.
pub fn sample_envelope(samples: &[(f64, f32)], t: f64, sampling: ModulationSampling) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    // Index of the latest sample at or before t (or 0 when t precedes all).
    let mut cursor = 0;
    while cursor + 1 < samples.len() && samples[cursor + 1].0 <= t {
        cursor += 1;
    }
    match sampling {
        ModulationSampling::Hold => samples[cursor].1,
        ModulationSampling::Smooth => {
            if cursor + 1 >= samples.len() || t <= samples[cursor].0 {
                return samples[cursor].1;
            }
            let (t0, v0) = samples[cursor];
            let (t1, v1) = samples[cursor + 1];
            let span = t1 - t0;
            if span <= 0.0 {
                return v0;
            }
            let alpha = ((t - t0) / span).clamp(0.0, 1.0) as f32;
            v0 + (v1 - v0) * alpha
        }
    }
}

/// The mapped (pre-clamp) knob value for a route at time `t`.
pub fn modulated_value(
    route: &ModulationRoute,
    samples: &[(f64, f32)],
    t: f64,
    sampling: ModulationSampling,
) -> f32 {
    if let ModulationSource::Lfo {
        shape,
        rate_hz,
        phase,
    } = route.source
    {
        // LFO routes bypass envelope extraction and sampling entirely — a
        // pure function of `(t, params)`. `sampling` (`@hold`/`@smooth`) is a
        // documented no-op here (nothing sparse to sample). All math in f64,
        // cast to f32 at the end (milestone doc, "Semantics").
        let x = rate_hz as f64 * t + phase as f64;
        let p = x - x.floor(); // fract(x) = x - x.floor(), so p ∈ [0,1)
        let raw = shape.evaluate(p);
        return (raw * route.scale as f64 + route.offset as f64) as f32;
    }
    sample_envelope(samples, t, sampling) * route.scale + route.offset
}

// ─── Per-effect target registries ─────────────────────────────────────────────

/// Pixel shifts are clamped to a generous-but-sane pixel range.
const SHIFT_RANGE: (f32, f32) = (-4096.0, 4096.0);

pub const RETRO_STATIC_MODULATION_TARGETS: &[&str] = &["strength", "filter"];
pub const PIXEL_SORT_MODULATION_TARGETS: &[&str] =
    &["threshold_low", "threshold_high", "direction", "axis"];
pub const CHANNEL_SHIFT_MODULATION_TARGETS: &[&str] = &[
    "shift_r_x",
    "shift_r_y",
    "shift_g_x",
    "shift_g_y",
    "shift_b_x",
    "shift_b_y",
];
pub const PALETTE_QUANTIZE_MODULATION_TARGETS: &[&str] = &["levels", "mode"];
pub const RUTT_ETRA_MODULATION_TARGETS: &[&str] =
    &["displacement_depth", "line_pitch", "line_thickness"];
/// Flow feedback is the first **stateful** effect with modulation targets:
/// each frame's state update consumes that frame's knobs, so frame N depends
/// on the whole knob history (milestone doc, "Stateful targets").
/// `structure_mode` is deliberately excluded — multiscale is CPU-only, and an
/// envelope must not drive a render into a backend-invalid configuration.
pub const FLOW_FEEDBACK_MODULATION_TARGETS: &[&str] = &[
    "carrier_amount",
    "feedback_amount",
    "feedback_mix",
    "decay",
    "structure_mix",
];
/// Fluid advect (single-source procedural dye) is stateful — each frame's dye
/// update consumes that frame's knobs — but has no checkpoint/resume path, so
/// only the per-frame application rule of the milestone's "Stateful targets"
/// section applies. `seed` is deliberately excluded (a structural field, like
/// datamosh `remix_seed`).
pub const FLUID_ADVECT_MODULATION_TARGETS: &[&str] = &[
    "advect",
    "turbulence_scale",
    "turbulence_speed",
    "detail",
    "reinject",
];
/// Two-source / optical-flow advect share one settings struct (and this
/// registry) across `render-fluid-advect-two-source-sequence` and
/// `render-optical-flow-advect-sequence`.
pub const FLUID_ADVECT_TWO_SOURCE_MODULATION_TARGETS: &[&str] = &["advect", "reinject"];

/// Posterize levels range: 2 = harshest, 256 = the documented byte-identical
/// passthrough (deliberately reachable by modulation).
const LEVELS_RANGE: (f32, f32) = (2.0, 256.0);

/// Rutt-Etra ranges (milestone doc, "Modulation targets"): depth is a signed
/// pixel push (0 = the flat off case, deliberately reachable); pitch and
/// thickness share the contracted integer rule.
const DISPLACEMENT_DEPTH_RANGE: (f32, f32) = (-512.0, 512.0);
const LINE_PITCH_RANGE: (f32, f32) = (1.0, 256.0);
const LINE_THICKNESS_RANGE: (f32, f32) = (1.0, 64.0);

// Enum-target variant orders (contract: milestone doc table). Unimplemented
// variants are excluded — an envelope must not select an erroring variant
// (palette-quantize `kmeans`).
const SORT_DIRECTION_VARIANTS: [SortDirection; 2] = [SortDirection::Asc, SortDirection::Desc];
const SORT_AXIS_VARIANTS: [SortAxis; 2] = [SortAxis::Row, SortAxis::Col];
const SCANLINE_FILTER_VARIANTS: [ScanlineFilter; 5] = [
    ScanlineFilter::None,
    ScanlineFilter::Sub,
    ScanlineFilter::Up,
    ScanlineFilter::Average,
    ScanlineFilter::Paeth,
];
const QUANTIZE_MODE_VARIANTS: [QuantizeMode; 2] = [QuantizeMode::Posterize, QuantizeMode::Palette];

/// The contracted integer conversion: clamp to the declared range, then round
/// to nearest with ties away from zero (`f32::round`). Clamp-then-round is
/// safe because integer bounds round to themselves.
fn integer_knob(value: f32, range: (f32, f32)) -> u32 {
    value.clamp(range.0, range.1).round() as u32
}

/// The contracted enum conversion: the integer rule over variant indices in
/// declared order. Note a `[0, 1]` envelope at the default `scale 1` only
/// spans indices 0 and 1; sweeping N variants needs `scale ≈ N−1`.
fn enum_knob<T: Copy>(value: f32, variants: &[T]) -> T {
    variants[integer_knob(value, (0.0, (variants.len() - 1) as f32)) as usize]
}

fn unknown_target(effect: &str, target: &str, available: &[&str]) -> RenderError {
    RenderError::InvalidModulationRoute(format!(
        "unknown {effect} modulation target '{target}' (available: {})",
        available.join(", ")
    ))
}

/// Overwrite one routed retro-static knob with a mapped value (clamped).
pub fn apply_retro_static_modulation(
    settings: &mut RetroStaticSettings,
    target: &str,
    value: f32,
) -> Result<(), RenderError> {
    match target {
        "strength" => settings.strength = value.clamp(0.0, 1.0),
        "filter" => settings.filter = enum_knob(value, &SCANLINE_FILTER_VARIANTS),
        _ => {
            return Err(unknown_target(
                "retro-static",
                target,
                RETRO_STATIC_MODULATION_TARGETS,
            ))
        }
    }
    Ok(())
}

/// Overwrite one routed pixel-sort knob with a mapped value (clamped).
///
/// Modulation may drive `threshold_low` above `threshold_high`; that frame is
/// the effect's own documented empty-mask passthrough, not an error.
pub fn apply_pixel_sort_modulation(
    settings: &mut PixelSortSettings,
    target: &str,
    value: f32,
) -> Result<(), RenderError> {
    match target {
        "threshold_low" => settings.threshold_low = value.clamp(0.0, 1.0),
        "threshold_high" => settings.threshold_high = value.clamp(0.0, 1.0),
        "direction" => settings.direction = enum_knob(value, &SORT_DIRECTION_VARIANTS),
        "axis" => settings.axis = enum_knob(value, &SORT_AXIS_VARIANTS),
        _ => {
            return Err(unknown_target(
                "pixel-sort",
                target,
                PIXEL_SORT_MODULATION_TARGETS,
            ))
        }
    }
    Ok(())
}

/// Overwrite one routed channel-shift knob with a mapped value (clamped).
pub fn apply_channel_shift_modulation(
    settings: &mut ChannelShiftSettings,
    target: &str,
    value: f32,
) -> Result<(), RenderError> {
    let clamped = value.clamp(SHIFT_RANGE.0, SHIFT_RANGE.1);
    match target {
        "shift_r_x" => settings.shift_r_x = clamped,
        "shift_r_y" => settings.shift_r_y = clamped,
        "shift_g_x" => settings.shift_g_x = clamped,
        "shift_g_y" => settings.shift_g_y = clamped,
        "shift_b_x" => settings.shift_b_x = clamped,
        "shift_b_y" => settings.shift_b_y = clamped,
        _ => {
            return Err(unknown_target(
                "channel-shift",
                target,
                CHANNEL_SHIFT_MODULATION_TARGETS,
            ))
        }
    }
    Ok(())
}

/// Overwrite one routed flow-feedback knob with a mapped value. Clamps mirror
/// `FlowFeedbackSettings::validate` (one-sided where validate is one-sided);
/// the amounts share the channel-shift pixel range.
pub fn apply_flow_feedback_modulation(
    settings: &mut FlowFeedbackSettings,
    target: &str,
    value: f32,
) -> Result<(), RenderError> {
    match target {
        "carrier_amount" => settings.carrier_amount = value.clamp(SHIFT_RANGE.0, SHIFT_RANGE.1),
        "feedback_amount" => settings.feedback_amount = value.clamp(SHIFT_RANGE.0, SHIFT_RANGE.1),
        "feedback_mix" => settings.feedback_mix = value.clamp(0.0, 1.0),
        "decay" => settings.decay = value.max(0.0),
        "structure_mix" => settings.structure_mix = value.max(0.0),
        _ => {
            return Err(unknown_target(
                "flow-feedback",
                target,
                FLOW_FEEDBACK_MODULATION_TARGETS,
            ))
        }
    }
    Ok(())
}

/// Overwrite one routed fluid-advect knob with a mapped value. Clamps mirror
/// `FluidAdvectSettings::validate` (advect/detail non-negative, reinject in
/// `[0, 1]`); where validate only requires finiteness, the shared pixel range
/// bounds the value so a runaway `scale·envelope` can never abort a frame's
/// own validate call (clamp-never-error).
pub fn apply_fluid_advect_modulation(
    settings: &mut FluidAdvectSettings,
    target: &str,
    value: f32,
) -> Result<(), RenderError> {
    match target {
        "advect" => settings.advect = value.clamp(0.0, SHIFT_RANGE.1),
        "turbulence_scale" => settings.turbulence_scale = value.clamp(SHIFT_RANGE.0, SHIFT_RANGE.1),
        "turbulence_speed" => settings.turbulence_speed = value.clamp(SHIFT_RANGE.0, SHIFT_RANGE.1),
        "detail" => settings.detail = value.clamp(0.0, SHIFT_RANGE.1),
        "reinject" => settings.reinject = value.clamp(0.0, 1.0),
        _ => {
            return Err(unknown_target(
                "fluid-advect",
                target,
                FLUID_ADVECT_MODULATION_TARGETS,
            ))
        }
    }
    Ok(())
}

/// Overwrite one routed two-source/optical-flow advect knob with a mapped
/// value. `advect` may legally go negative (validate only requires finiteness
/// — a reversed flow), so it takes the full pixel range; `reinject` mirrors
/// validate's `[0, 1]`.
pub fn apply_fluid_advect_two_source_modulation(
    settings: &mut FluidAdvectTwoSourceSettings,
    target: &str,
    value: f32,
) -> Result<(), RenderError> {
    match target {
        "advect" => settings.advect = value.clamp(SHIFT_RANGE.0, SHIFT_RANGE.1),
        "reinject" => settings.reinject = value.clamp(0.0, 1.0),
        _ => {
            return Err(unknown_target(
                "fluid-advect-two-source",
                target,
                FLUID_ADVECT_TWO_SOURCE_MODULATION_TARGETS,
            ))
        }
    }
    Ok(())
}

/// Overwrite one routed palette-quantize knob with a mapped value. `levels`
/// is the first integer target (clamped to `[2, 256]`, then rounded per the
/// contracted rule); `mode` selects posterize/palette by variant index
/// (`kmeans` is excluded — unimplemented variants must stay unreachable).
pub fn apply_palette_quantize_modulation(
    settings: &mut PaletteQuantizeSettings,
    target: &str,
    value: f32,
) -> Result<(), RenderError> {
    match target {
        "levels" => settings.levels = integer_knob(value, LEVELS_RANGE),
        "mode" => settings.mode = enum_knob(value, &QUANTIZE_MODE_VARIANTS),
        _ => {
            return Err(unknown_target(
                "palette-quantize",
                target,
                PALETTE_QUANTIZE_MODULATION_TARGETS,
            ))
        }
    }
    Ok(())
}

/// Overwrite one routed rutt-etra knob with a mapped value. All three targets
/// clamp (never error); `line_pitch` / `line_thickness` follow the contracted
/// integer rule, so an envelope can never drive the render below the
/// validate-legal minimum of 1.
pub fn apply_rutt_etra_modulation(
    settings: &mut RuttEtraSettings,
    target: &str,
    value: f32,
) -> Result<(), RenderError> {
    match target {
        "displacement_depth" => {
            settings.displacement_depth =
                value.clamp(DISPLACEMENT_DEPTH_RANGE.0, DISPLACEMENT_DEPTH_RANGE.1)
        }
        "line_pitch" => settings.line_pitch = integer_knob(value, LINE_PITCH_RANGE),
        "line_thickness" => settings.line_thickness = integer_knob(value, LINE_THICKNESS_RANGE),
        _ => {
            return Err(unknown_target(
                "rutt-etra",
                target,
                RUTT_ETRA_MODULATION_TARGETS,
            ))
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_route_grammar() {
        let route = parse_modulation_route("threshold_high=audio-onset:0.6,0.3").unwrap();
        assert_eq!(route.target, "threshold_high");
        assert_eq!(route.source, ModulationSource::AudioOnset);
        assert_eq!(route.scale, 0.6);
        assert_eq!(route.offset, 0.3);
    }

    #[test]
    fn parses_defaults_and_negative_parameters() {
        let route = parse_modulation_route("strength=audio-rms").unwrap();
        assert_eq!(route.scale, 1.0);
        assert_eq!(route.offset, 0.0);

        let route = parse_modulation_route("shift_b_x=flow:-24").unwrap();
        assert_eq!(route.source, ModulationSource::Flow);
        assert_eq!(route.scale, -24.0);
        assert_eq!(route.offset, 0.0);

        let route = parse_modulation_route(" strength = luma : 2 , -0.5 ").unwrap();
        assert_eq!(route.target, "strength");
        assert_eq!(route.source, ModulationSource::Luma);
        assert_eq!(route.scale, 2.0);
        assert_eq!(route.offset, -0.5);
    }

    #[test]
    fn parse_rejects_malformed_specs() {
        for spec in [
            "strength",                  // no '='
            "=audio-rms",                // empty target
            "strength=woble",            // unknown source
            "strength=audio-rms:x",      // bad scale
            "strength=audio-rms:1,y",    // bad offset
            "strength=audio-rms:inf,0",  // non-finite
            "strength=audio-rms@linear", // unknown sampling suffix
            "strength=audio-rms:1,0@",   // empty sampling suffix
            "strength=audio-rms@hold@",  // suffix must be terminal
        ] {
            assert!(
                parse_modulation_route(spec).is_err(),
                "expected '{spec}' to be rejected"
            );
        }
    }

    #[test]
    fn parses_named_modulator_prefix() {
        // Bare source reads the default modulator (None) — the pre-slice path.
        let route = parse_modulation_route("strength=audio-rms:0.5,0.25").unwrap();
        assert_eq!(route.modulator, None);

        let route = parse_modulation_route("strength=drums.audio-rms:0.5,0.25@smooth").unwrap();
        assert_eq!(route.modulator.as_deref(), Some("drums"));
        assert_eq!(route.source, ModulationSource::AudioRms);
        assert_eq!(route.sampling, Some(ModulationSampling::Smooth));

        let route = parse_modulation_route("shift_r_x= cam2 . flow :24").unwrap();
        assert_eq!(route.modulator.as_deref(), Some("cam2"));
        assert_eq!(route.source, ModulationSource::Flow);
        assert_eq!(route.scale, 24.0);

        // Unset modulator serializes to the pre-slice JSON shape (no key).
        let unnamed = parse_modulation_route("strength=audio-rms").unwrap();
        assert!(!serde_json::to_string(&unnamed)
            .unwrap()
            .contains("modulator"));

        for spec in [
            "strength=.audio-rms",     // empty modulator name
            "strength=drums.wobble",   // unknown source behind a valid name
            "strength=drums.rms.late", // source names contain no '.'
        ] {
            assert!(
                parse_modulation_route(spec).is_err(),
                "expected '{spec}' to be rejected"
            );
        }
    }

    #[test]
    fn parses_per_route_sampling_suffix() {
        // No suffix inherits the render-level sampling (None).
        let route = parse_modulation_route("strength=audio-rms:0.5,0.25").unwrap();
        assert_eq!(route.sampling, None);

        let route = parse_modulation_route("strength=audio-rms:0.5,0.25@smooth").unwrap();
        assert_eq!(route.sampling, Some(ModulationSampling::Smooth));
        assert_eq!(route.scale, 0.5);
        assert_eq!(route.offset, 0.25);

        // The suffix composes with the short forms too.
        let route = parse_modulation_route("strength=audio-rms@hold").unwrap();
        assert_eq!(route.sampling, Some(ModulationSampling::Hold));
        assert_eq!(route.scale, 1.0);

        let route = parse_modulation_route("strength=luma:2 @ smooth").unwrap();
        assert_eq!(route.sampling, Some(ModulationSampling::Smooth));
        assert_eq!(route.scale, 2.0);

        // Unset sampling serializes to the pre-slice JSON shape (no key), so
        // existing checkpoints and manifests stay byte-identical.
        let unsuffixed = parse_modulation_route("strength=audio-rms:0.5,0.25").unwrap();
        let json = serde_json::to_string(&unsuffixed).unwrap();
        assert!(!json.contains("sampling"));
        let decoded: ModulationRoute = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, unsuffixed);
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let routes = vec![
            parse_modulation_route("strength=audio-rms").unwrap(),
            parse_modulation_route("strength=luma").unwrap(),
        ];
        assert!(validate_route_targets(&routes).is_err());
        assert!(validate_route_targets(&routes[..1]).is_ok());
    }

    #[test]
    fn hold_sampling_steps_and_holds_ends() {
        let samples = vec![(0.0, 0.1), (1.0, 0.5), (2.0, 0.9)];
        let hold = |t| sample_envelope(&samples, t, ModulationSampling::Hold);
        assert_eq!(hold(-1.0), 0.1); // before first: first value
        assert_eq!(hold(0.0), 0.1);
        assert_eq!(hold(0.99), 0.1); // steps, no interpolation
        assert_eq!(hold(1.0), 0.5);
        assert_eq!(hold(5.0), 0.9); // after last: last value
        assert_eq!(
            sample_envelope(&[], 1.0, ModulationSampling::Hold),
            0.0,
            "empty envelope yields zero"
        );
    }

    #[test]
    fn smooth_sampling_interpolates_between_brackets() {
        let samples = vec![(0.0, 0.0), (2.0, 1.0)];
        let smooth = |t| sample_envelope(&samples, t, ModulationSampling::Smooth);
        assert_eq!(smooth(-1.0), 0.0);
        assert_eq!(smooth(0.0), 0.0);
        assert!((smooth(1.0) - 0.5).abs() < 1e-6);
        assert!((smooth(1.5) - 0.75).abs() < 1e-6);
        assert_eq!(smooth(2.0), 1.0);
        assert_eq!(smooth(9.0), 1.0);
    }

    #[test]
    fn peak_normalize_is_relative_and_zeroes_silence() {
        let mut samples = vec![(0.0, 0.25), (1.0, 0.5)];
        peak_normalize(&mut samples);
        assert_eq!(samples[0].1, 0.5);
        assert_eq!(samples[1].1, 1.0);

        let mut silent = vec![(0.0, 0.0), (1.0, 0.0)];
        peak_normalize(&mut silent);
        assert!(silent.iter().all(|&(_, v)| v == 0.0));
    }

    #[test]
    fn modulated_value_applies_affine_mapping() {
        let route = parse_modulation_route("strength=luma:0.5,0.25").unwrap();
        let samples = vec![(0.0, 1.0)];
        let value = modulated_value(&route, &samples, 0.0, ModulationSampling::Hold);
        assert!((value - 0.75).abs() < 1e-6);
    }

    #[test]
    fn apply_functions_clamp_to_declared_ranges() {
        let mut retro = RetroStaticSettings::default();
        apply_retro_static_modulation(&mut retro, "strength", 7.0).unwrap();
        assert_eq!(retro.strength, 1.0);
        apply_retro_static_modulation(&mut retro, "strength", -3.0).unwrap();
        assert_eq!(retro.strength, 0.0);

        let mut sort = PixelSortSettings::default();
        apply_pixel_sort_modulation(&mut sort, "threshold_low", 0.9).unwrap();
        apply_pixel_sort_modulation(&mut sort, "threshold_high", -1.0).unwrap();
        assert_eq!(sort.threshold_low, 0.9);
        assert_eq!(sort.threshold_high, 0.0);
        assert!(
            sort.threshold_low > sort.threshold_high,
            "passthrough frame is legal"
        );

        let mut shift = ChannelShiftSettings::default();
        apply_channel_shift_modulation(&mut shift, "shift_g_y", 99999.0).unwrap();
        assert_eq!(shift.shift_g_y, 4096.0);
    }

    #[test]
    fn apply_functions_reject_unknown_targets() {
        let mut retro = RetroStaticSettings::default();
        assert!(apply_retro_static_modulation(&mut retro, "real_bpp", 1.0).is_err());
        let mut sort = PixelSortSettings::default();
        assert!(apply_pixel_sort_modulation(&mut sort, "max_span", 1.0).is_err());
        let mut shift = ChannelShiftSettings::default();
        assert!(apply_channel_shift_modulation(&mut shift, "strength", 1.0).is_err());
        let mut quantize = PaletteQuantizeSettings::default();
        assert!(apply_palette_quantize_modulation(&mut quantize, "dither", 1.0).is_err());
    }

    #[test]
    fn flow_feedback_targets_clamp_to_validate_ranges() {
        let mut feedback = FlowFeedbackSettings {
            carrier_amount: 12.0,
            feedback_amount: 24.0,
            feedback_mix: 0.72,
            decay: 0.995,
            iterations: 1,
            structure_mix: 0.0,
            structure_mode: Default::default(),
        };
        apply_flow_feedback_modulation(&mut feedback, "feedback_mix", 1.5).unwrap();
        assert_eq!(feedback.feedback_mix, 1.0);
        apply_flow_feedback_modulation(&mut feedback, "feedback_mix", -0.5).unwrap();
        assert_eq!(feedback.feedback_mix, 0.0);
        apply_flow_feedback_modulation(&mut feedback, "decay", -1.0).unwrap();
        assert_eq!(feedback.decay, 0.0);
        apply_flow_feedback_modulation(&mut feedback, "decay", 1.5).unwrap();
        assert_eq!(
            feedback.decay, 1.5,
            "decay clamp is one-sided like validate"
        );
        apply_flow_feedback_modulation(&mut feedback, "structure_mix", -3.0).unwrap();
        assert_eq!(feedback.structure_mix, 0.0);
        apply_flow_feedback_modulation(&mut feedback, "carrier_amount", 99999.0).unwrap();
        assert_eq!(feedback.carrier_amount, 4096.0);
        apply_flow_feedback_modulation(&mut feedback, "feedback_amount", -99999.0).unwrap();
        assert_eq!(feedback.feedback_amount, -4096.0);
        // Every clamped combination stays legal for the render.
        feedback.validate().unwrap();
        // structure_mode is deliberately not a target (backend-invalid variants).
        assert!(apply_flow_feedback_modulation(&mut feedback, "structure_mode", 1.0).is_err());
        assert!(apply_flow_feedback_modulation(&mut feedback, "iterations", 1.0).is_err());
    }

    #[test]
    fn fluid_advect_targets_clamp_to_validate_ranges() {
        let mut fluid = FluidAdvectSettings::default();
        apply_fluid_advect_modulation(&mut fluid, "advect", -5.0).unwrap();
        assert_eq!(fluid.advect, 0.0);
        apply_fluid_advect_modulation(&mut fluid, "advect", 99999.0).unwrap();
        assert_eq!(fluid.advect, 4096.0);
        apply_fluid_advect_modulation(&mut fluid, "reinject", 1.5).unwrap();
        assert_eq!(fluid.reinject, 1.0);
        apply_fluid_advect_modulation(&mut fluid, "reinject", -0.5).unwrap();
        assert_eq!(fluid.reinject, 0.0);
        apply_fluid_advect_modulation(&mut fluid, "detail", -1.0).unwrap();
        assert_eq!(fluid.detail, 0.0);
        // Scale/speed only require finiteness — negatives pass through.
        apply_fluid_advect_modulation(&mut fluid, "turbulence_scale", -0.02).unwrap();
        assert_eq!(fluid.turbulence_scale, -0.02);
        apply_fluid_advect_modulation(&mut fluid, "turbulence_speed", 0.25).unwrap();
        assert_eq!(fluid.turbulence_speed, 0.25);
        // Every clamped combination stays legal for the render.
        fluid.validate().unwrap();
        // seed is a structural field, not a knob.
        assert!(apply_fluid_advect_modulation(&mut fluid, "seed", 1.0).is_err());

        let mut two_source = FluidAdvectTwoSourceSettings::default();
        // advect may legally go negative (a reversed flow).
        apply_fluid_advect_two_source_modulation(&mut two_source, "advect", -2.5).unwrap();
        assert_eq!(two_source.advect, -2.5);
        apply_fluid_advect_two_source_modulation(&mut two_source, "advect", 99999.0).unwrap();
        assert_eq!(two_source.advect, 4096.0);
        apply_fluid_advect_two_source_modulation(&mut two_source, "reinject", 2.0).unwrap();
        assert_eq!(two_source.reinject, 1.0);
        two_source.validate().unwrap();
        assert!(
            apply_fluid_advect_two_source_modulation(&mut two_source, "turbulence_scale", 1.0)
                .is_err(),
            "single-source-only knobs are not two-source targets"
        );
    }

    #[test]
    fn enum_targets_select_variants_by_index_in_declared_order() {
        let mut sort = PixelSortSettings::default();
        // Boundary values, rounding, and the tie rule on the 2-variant knobs.
        apply_pixel_sort_modulation(&mut sort, "direction", 0.0).unwrap();
        assert_eq!(sort.direction, SortDirection::Asc);
        apply_pixel_sort_modulation(&mut sort, "direction", 0.4).unwrap();
        assert_eq!(sort.direction, SortDirection::Asc);
        apply_pixel_sort_modulation(&mut sort, "direction", 0.5).unwrap();
        assert_eq!(sort.direction, SortDirection::Desc);
        apply_pixel_sort_modulation(&mut sort, "axis", 99.0).unwrap();
        assert_eq!(sort.axis, SortAxis::Col, "clamps to the last variant");
        apply_pixel_sort_modulation(&mut sort, "axis", -99.0).unwrap();
        assert_eq!(sort.axis, SortAxis::Row, "clamps to the first variant");

        // The 5-variant filter knob: full declared order is reachable.
        let mut retro = RetroStaticSettings::default();
        let expected = [
            (0.0, ScanlineFilter::None),
            (1.0, ScanlineFilter::Sub),
            (2.0, ScanlineFilter::Up),
            (2.5, ScanlineFilter::Average), // tie away from zero
            (4.0, ScanlineFilter::Paeth),
        ];
        for (value, filter) in expected {
            apply_retro_static_modulation(&mut retro, "filter", value).unwrap();
            assert_eq!(retro.filter, filter, "filter at {value}");
        }

        // `mode` clamps to palette; the unimplemented kmeans is unreachable.
        let mut quantize = PaletteQuantizeSettings::default();
        apply_palette_quantize_modulation(&mut quantize, "mode", 1.0).unwrap();
        assert_eq!(quantize.mode, QuantizeMode::Palette);
        apply_palette_quantize_modulation(&mut quantize, "mode", 9999.0).unwrap();
        assert_eq!(
            quantize.mode,
            QuantizeMode::Palette,
            "kmeans must be unreachable"
        );
    }

    #[test]
    fn integer_levels_clamp_then_round_ties_away_from_zero() {
        let mut quantize = PaletteQuantizeSettings::default();
        // Clamp at both ends of [2, 256].
        apply_palette_quantize_modulation(&mut quantize, "levels", -5.0).unwrap();
        assert_eq!(quantize.levels, 2);
        apply_palette_quantize_modulation(&mut quantize, "levels", 9999.0).unwrap();
        assert_eq!(quantize.levels, 256);
        // Nearest integer, ties away from zero.
        apply_palette_quantize_modulation(&mut quantize, "levels", 4.4).unwrap();
        assert_eq!(quantize.levels, 4);
        apply_palette_quantize_modulation(&mut quantize, "levels", 4.5).unwrap();
        assert_eq!(quantize.levels, 5);
        // The off case is reachable: 255.5 rounds to the passthrough 256.
        apply_palette_quantize_modulation(&mut quantize, "levels", 255.5).unwrap();
        assert_eq!(quantize.levels, 256);
        // Continuity identity in integer form: scale 0, offset K == --levels K.
        apply_palette_quantize_modulation(&mut quantize, "levels", 7.0).unwrap();
        assert_eq!(quantize.levels, 7);
    }

    #[test]
    fn rutt_etra_targets_clamp_to_declared_ranges() {
        let mut rutt = RuttEtraSettings::default();
        // displacement_depth clamps at both ends of [-512, 512]; the flat off
        // case (0) and negative (downward) pushes stay reachable.
        apply_rutt_etra_modulation(&mut rutt, "displacement_depth", 9999.0).unwrap();
        assert_eq!(rutt.displacement_depth, 512.0);
        apply_rutt_etra_modulation(&mut rutt, "displacement_depth", -9999.0).unwrap();
        assert_eq!(rutt.displacement_depth, -512.0);
        apply_rutt_etra_modulation(&mut rutt, "displacement_depth", 0.0).unwrap();
        assert_eq!(rutt.displacement_depth, 0.0);

        // line_pitch: integer rule over [1, 256] — clamp at both ends, then
        // round to nearest with ties away from zero.
        apply_rutt_etra_modulation(&mut rutt, "line_pitch", -5.0).unwrap();
        assert_eq!(rutt.line_pitch, 1);
        apply_rutt_etra_modulation(&mut rutt, "line_pitch", 9999.0).unwrap();
        assert_eq!(rutt.line_pitch, 256);
        apply_rutt_etra_modulation(&mut rutt, "line_pitch", 8.4).unwrap();
        assert_eq!(rutt.line_pitch, 8);
        apply_rutt_etra_modulation(&mut rutt, "line_pitch", 8.5).unwrap();
        assert_eq!(rutt.line_pitch, 9, "tie rounds away from zero");

        // line_thickness: integer rule over [1, 64].
        apply_rutt_etra_modulation(&mut rutt, "line_thickness", 0.0).unwrap();
        assert_eq!(rutt.line_thickness, 1);
        apply_rutt_etra_modulation(&mut rutt, "line_thickness", 9999.0).unwrap();
        assert_eq!(rutt.line_thickness, 64);
        apply_rutt_etra_modulation(&mut rutt, "line_thickness", 2.5).unwrap();
        assert_eq!(rutt.line_thickness, 3, "tie rounds away from zero");

        // Every clamped combination stays legal for the render.
        rutt.validate().unwrap();

        // mono is a flag, not a modulation target.
        assert!(apply_rutt_etra_modulation(&mut rutt, "mono", 1.0).is_err());
        assert!(apply_rutt_etra_modulation(&mut rutt, "depth", 1.0).is_err());
    }

    #[test]
    fn source_flag_requirements_partition_the_sources() {
        for source in [
            ModulationSource::AudioRms,
            ModulationSource::AudioOnset,
            ModulationSource::AudioCentroid,
            ModulationSource::Luma,
            ModulationSource::Flow,
        ] {
            assert_ne!(source.needs_audio(), source.needs_frames());
            assert_eq!(ModulationSource::parse(source.name()), Some(source));
        }
    }

    #[test]
    fn lfo_needs_no_media() {
        let source = ModulationSource::Lfo {
            shape: LfoShape::Square,
            rate_hz: 2.0,
            phase: 0.0,
        };
        assert!(!source.needs_audio());
        assert!(!source.needs_frames());
    }

    #[test]
    fn lfo_shape_formulas_are_pinned_at_quarter_points() {
        // milestone doc, "Semantics" — changing any of these changes rendered
        // frames.
        let cases: [(LfoShape, [f64; 4]); 4] = [
            (LfoShape::Sine, [0.0, 0.5, 1.0, 0.5]),
            (LfoShape::Triangle, [0.0, 0.5, 1.0, 0.5]),
            (LfoShape::Saw, [0.0, 0.25, 0.5, 0.75]),
            (LfoShape::Square, [0.0, 0.0, 1.0, 1.0]),
        ];
        for (shape, expected) in cases {
            for (p, want) in [0.0, 0.25, 0.5, 0.75].into_iter().zip(expected) {
                let got = shape.evaluate(p);
                assert!(
                    (got - want).abs() < 1e-9,
                    "{shape:?} at p={p}: got {got}, want {want}"
                );
            }
        }
    }

    #[test]
    fn lfo_phase_wraps_via_floor_based_fract() {
        // phase 1.25 must behave identically to phase 0.25 (fract(x) =
        // x - x.floor()) at every t.
        let wrapped = parse_modulation_route("depth=lfo(saw,1,1.25)").unwrap();
        let base = parse_modulation_route("depth=lfo(saw,1,0.25)").unwrap();
        for t in [0.0, 0.5, 1.0, 3.3] {
            let a = modulated_value(&wrapped, &[], t, ModulationSampling::Hold);
            let b = modulated_value(&base, &[], t, ModulationSampling::Hold);
            assert_eq!(a, b, "phase 1.25 must equal phase 0.25 at t={t}");
        }
    }

    #[test]
    fn parses_lfo_source_full_grammar_and_defaults() {
        let route = parse_modulation_route("displacement_depth=lfo(sine)").unwrap();
        assert_eq!(
            route.source,
            ModulationSource::Lfo {
                shape: LfoShape::Sine,
                rate_hz: 1.0,
                phase: 0.0,
            }
        );
        assert_eq!(route.scale, 1.0);
        assert_eq!(route.offset, 0.0);
        assert_eq!(route.modulator, None);

        let route = parse_modulation_route("depth=lfo(saw,0.5,0.25):64,-32@smooth").unwrap();
        assert_eq!(
            route.source,
            ModulationSource::Lfo {
                shape: LfoShape::Saw,
                rate_hz: 0.5,
                phase: 0.25,
            }
        );
        assert_eq!(route.scale, 64.0);
        assert_eq!(route.offset, -32.0);
        assert_eq!(route.sampling, Some(ModulationSampling::Smooth));

        // Round trip: spec_text() composes with target/scale/offset/suffix
        // through the exact same parser (the milestone's Trap 2 pin).
        let respec = format!("depth={}:64,-32@smooth", route.source.spec_text());
        assert_eq!(respec, "depth=lfo(saw,0.5,0.25):64,-32@smooth");
        let reparsed = parse_modulation_route(&respec).unwrap();
        assert_eq!(reparsed, route);
    }

    #[test]
    fn parses_lfo_source_with_named_modulator_dot_unambiguously() {
        // Trap 1: the named-modulator dot-split must not eat the '.' inside
        // `lfo(sine,0.5)`.
        let route = parse_modulation_route("depth=lfo(sine,0.5)").unwrap();
        assert_eq!(route.modulator, None);
        assert_eq!(
            route.source,
            ModulationSource::Lfo {
                shape: LfoShape::Sine,
                rate_hz: 0.5,
                phase: 0.0,
            }
        );
    }

    #[test]
    fn parse_rejects_malformed_lfo_specs() {
        for spec in [
            "depth=lfo(wobble)",     // unknown shape
            "depth=lfo(sine,0)",     // rate must be > 0
            "depth=lfo(sine,-1)",    // negative rate
            "depth=lfo(sine,inf)",   // non-finite rate
            "depth=lfo(sine,1,nan)", // non-finite phase
            "depth=lfo(sine,1,2,3)", // too many params
            "depth=lfo(sine",        // malformed (no closing paren)
            "depth=wob.lfo(sine)",   // named prefix on an lfo source
        ] {
            assert!(
                parse_modulation_route(spec).is_err(),
                "expected '{spec}' to be rejected"
            );
        }
    }

    #[test]
    fn lfo_sampling_suffix_is_a_documented_no_op() {
        let unsuffixed = parse_modulation_route("depth=lfo(sine,0.5):64,-32").unwrap();
        let held = parse_modulation_route("depth=lfo(sine,0.5):64,-32@hold").unwrap();
        let smoothed = parse_modulation_route("depth=lfo(sine,0.5):64,-32@smooth").unwrap();
        for t in [0.0, 0.3, 0.77, 1.5] {
            let a = modulated_value(&unsuffixed, &[], t, ModulationSampling::Hold);
            let b = modulated_value(&held, &[], t, ModulationSampling::Smooth);
            let c = modulated_value(&smoothed, &[], t, ModulationSampling::Hold);
            assert_eq!(a, b, "@hold must equal unsuffixed, byte-equal, at t={t}");
            assert_eq!(a, c, "@smooth must equal unsuffixed, byte-equal, at t={t}");
        }
    }

    #[test]
    fn lfo_source_serializes_as_an_object_with_exact_literals() {
        let source = ModulationSource::Lfo {
            shape: LfoShape::Sine,
            rate_hz: 0.5,
            phase: 0.25,
        };
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(
            json,
            r#"{"lfo":{"shape":"sine","rate_hz":0.5,"phase":0.25}}"#
        );
        let decoded: ModulationSource = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, source);

        // Pre-slice unit variants stay bare strings — byte-identical
        // checkpoints/manifests/queue JSON.
        assert_eq!(
            serde_json::to_string(&ModulationSource::AudioRms).unwrap(),
            "\"audio-rms\""
        );
    }

    #[test]
    fn lfo_scale_zero_offset_k_is_continuity_identity_with_a_constant() {
        // milestone doc, acceptance criterion 2: target=lfo(sine,1):0,K byte-
        // identical to passing constant K directly, on rutt-etra
        // displacement_depth.
        let route = parse_modulation_route("displacement_depth=lfo(sine,1):0,150").unwrap();
        let mut rutt = RuttEtraSettings::default();
        for t in [0.0, 0.1, 0.37, 5.5] {
            let value = modulated_value(&route, &[], t, ModulationSampling::Hold);
            assert_eq!(value, 150.0, "scale 0 must ignore t entirely, t={t}");
            apply_rutt_etra_modulation(&mut rutt, "displacement_depth", value).unwrap();
            assert_eq!(rutt.displacement_depth, 150.0);
        }
    }
}
