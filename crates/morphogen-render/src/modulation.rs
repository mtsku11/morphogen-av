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
    ChannelShiftSettings, PaletteQuantizeSettings, PixelSortSettings, QuantizeMode, RenderError,
    RetroStaticSettings, ScanlineFilter, SortAxis, SortDirection,
};

/// Which analysis descriptor drives a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
}

impl ModulationSource {
    /// The CLI spelling (`audio-rms`, `audio-onset`, `audio-centroid`,
    /// `luma`, `flow`).
    pub fn name(self) -> &'static str {
        match self {
            ModulationSource::AudioRms => "audio-rms",
            ModulationSource::AudioOnset => "audio-onset",
            ModulationSource::AudioCentroid => "audio-centroid",
            ModulationSource::Luma => "luma",
            ModulationSource::Flow => "flow",
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
}

fn default_scale() -> f32 {
    1.0
}

/// Parse the CLI route grammar `<target>=<source>[:<scale>[,<offset>]]`.
pub fn parse_modulation_route(spec: &str) -> Result<ModulationRoute, RenderError> {
    let bad = |detail: &str| {
        RenderError::InvalidModulationRoute(format!(
            "'{spec}': {detail} (expected <target>=<source>[:<scale>[,<offset>]])"
        ))
    };
    let (target, rest) = spec.split_once('=').ok_or_else(|| bad("missing '='"))?;
    let target = target.trim();
    if target.is_empty() {
        return Err(bad("empty target"));
    }
    let (source_name, params) = match rest.split_once(':') {
        Some((source, params)) => (source, Some(params)),
        None => (rest, None),
    };
    let source_name = source_name.trim();
    let source = ModulationSource::parse(source_name).ok_or_else(|| {
        bad(&format!(
            "unknown source '{source_name}' (available: audio-rms, audio-onset, audio-centroid, luma, flow)"
        ))
    })?;
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

/// Posterize levels range: 2 = harshest, 256 = the documented byte-identical
/// passthrough (deliberately reachable by modulation).
const LEVELS_RANGE: (f32, f32) = (2.0, 256.0);

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
            "strength",                 // no '='
            "=audio-rms",               // empty target
            "strength=woble",           // unknown source
            "strength=audio-rms:x",     // bad scale
            "strength=audio-rms:1,y",   // bad offset
            "strength=audio-rms:inf,0", // non-finite
        ] {
            assert!(
                parse_modulation_route(spec).is_err(),
                "expected '{spec}' to be rejected"
            );
        }
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
}
