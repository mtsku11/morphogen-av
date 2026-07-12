//! Faux-fluid dye advection — a single-source, stateful, per-pixel feedback advection
//! that ports the *Faux Fluid Sim* shadertoy look (procedural-turbulence variant).
//!
//! Unlike [`crate::fluid_mosaic`], which moves discrete colour **tiles** as particles,
//! this effect transports **continuous pixels**: it keeps a "dye" image and, every frame,
//! pushes each pixel along a procedural velocity field, then bleeds a little of the
//! current source frame back in. There are no tiles and no particles — the picture
//! itself becomes liquid and marbles (folds, stretches, swirls) like ink in water.
//!
//! Per frame `n` (with `D` the dye buffer, `S_n` the current source frame), the dye is
//! advanced in **N substeps** (the v3 fix for the "echo ring" striations: at one step per
//! frame, every reinjected source copy landed a full `advect` pixels from the previous
//! one and the stack read as concentric rings; N substeps of `advect/N` land the copies
//! ≤ ~1.5 px apart, where the bilinear filter fuses them into a continuum — exactly the
//! reference shader's regime, whose per-frame velocity is only a few pixels). Each substep:
//! 1. **Velocity field** `v(p, t)` — the analytic curl `(∂ψ/∂y, -∂ψ/∂x)` of a streamfunction
//!    `ψ` built to match the reference shader's *character*: large coherent vortices that
//!    emerge and slowly re-form, not a busy wobble. `ψ` is **3D gradient (Perlin) noise**
//!    (splitmix-hashed gradients, GPU-safe — not the shader's `sin()`-hashing) with a
//!    **dominant low-frequency octave** (the big vortices) plus a `detail`-weighted octave
//!    at 2× frequency (the shader weights its small turbulence only `0.1`). Time is the
//!    noise's **3rd axis**; substep times interpolate within the frame. An optional
//!    animated sinusoidal domain **warp** (the shader's `QuakeLavaUV` analog) folds the
//!    detail octave so material creases instead of winding forever. The curl is
//!    divergence-free, so the dye is transported without sources/sinks.
//! 2. **Semi-Lagrangian advection** — `D'[p] = bilinear(D, p - v · advect/N)`: each output
//!    pixel reads the dye that was *upstream*, so the colour flows downstream along `v`.
//!    An optional **diffuse** weight mixes in a 3×3 binomial blur of the upstream sample —
//!    the faux viscosity the shader gets for free from its texture filtering (and the
//!    in-effect cure for moiré when the source carries near-Nyquist detail).
//! 3. **Source reinjection (the "frame refresh")** — `D'[p] = mix(D'[p], S_n[p], r_sub)`
//!    with `r_sub = 1 − (1 − reinject)^(1/N)` so N substeps compound to exactly
//!    `reinject`. An optional **reinject_blotch** weight modulates the rate by an
//!    animated sparse blotch mask (the shader's `pow(mask, 5.5) · 0.05` patches), so
//!    fresh source appears in soft moving islands instead of as a full-frame layer.
//!
//! Frame zero is the source frame verbatim (`D = S_0`), the prior-frame state consumed is
//! the dye buffer (RGBA32F), and that buffer is the checkpoint representation — a stateful
//! temporal node. Deterministic: splitmix64 hashing, fixed per-frame time, no wall clock.
//!
//! [`relief_shade_cpu`] is the port of the shader's *Image* pass (faux-normal relief
//! lighting) as a **display-only** adapter: apply it to the frame you show/save, never to
//! the dye state that is carried forward.
//!
//! Continuity identities (the off cases for an off-vs-on readout), holding at any substep
//! count:
//! - `reinject == 1.0` with `reinject_blotch == 0` ⇒ output is `S_n` verbatim every frame
//!   (no fluid at all).
//! - `advect == 0.0` with `reinject == 0.0` and `diffuse == 0` ⇒ output is the previous
//!   dye unchanged (the field never displaces anything) — a pure hold of frame zero.

use serde::{Deserialize, Serialize};

use crate::cpu_reference::flow_displace_cpu;
use crate::sampler::sample_bilinear_clamped;
use crate::vortex_field::{reinjection_blotch_mask, steady_vortex_velocity_warped};
use crate::{FlowField, ImageBufferF32, RenderError};

/// Algorithm identifier for the CPU reference. Bump when the velocity model, advection
/// scheme, or reinjection changes so stale caches/checkpoints invalidate.
/// v3: substep integration (echo-ring fix), blotch reinjection, domain warp, diffusion.
pub const FLUID_ADVECT_ALGORITHM: &str = "fluid_advect_curl_noise_cpu_v3";

/// Hard cap on explicit `substeps` (validation bound).
pub const FLUID_ADVECT_MAX_SUBSTEPS: u32 = 64;
/// Auto substep sizing: split `advect` into steps of at most this many pixels, the spacing
/// at which successive reinjection layers fuse under bilinear filtering instead of reading
/// as separate rings.
const AUTO_SUBSTEP_TARGET_PX: f32 = 1.5;
/// Cap on the auto-derived substep count (an explicit `substeps` may go up to
/// [`FLUID_ADVECT_MAX_SUBSTEPS`]).
const AUTO_SUBSTEP_CAP: u32 = 16;
/// Blotch-mask lattice frequency relative to `turbulence_scale` — patches a couple of
/// times smaller than the big vortices, like the reference shader's dot masks.
const BLOTCH_SCALE_RATIO: f32 = 2.0;

/// Settings for [`fluid_advect_frame_cpu`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FluidAdvectSettings {
    /// Advection distance per frame (pixels) — how far the dye is pushed along the
    /// velocity field each step. `0` holds the dye in place (the field never displaces).
    pub advect: f32,
    /// Spatial frequency of the dominant vortex octave (lattice cells per pixel). Smaller
    /// ⇒ larger vortices. The size of the big coherent swirls.
    pub turbulence_scale: f32,
    /// Temporal evolution rate of the velocity field per frame (how fast vortices form,
    /// drift, and re-form). Smaller ⇒ slower, calmer flow.
    pub turbulence_speed: f32,
    /// Weight of the fine (2× frequency) detail octave relative to the dominant vortex
    /// octave. The reference shader uses `0.1`; raising it adds finer structure (and, past
    /// a point, the busy "wobble"); `0` is pure large vortices.
    pub detail: f32,
    /// Fraction of the current source frame bled back into the dye each frame, in
    /// `[0, 1]` — the "frame refresh". `0` lets the dye smear freely with no fresh
    /// content (frame zero dissolves into pure flow); `1` shows the source verbatim
    /// (no fluid). Small values (~0.05–0.15) keep the video present while it marbles.
    pub reinject: f32,
    /// Seed for the deterministic turbulence field.
    pub seed: u64,
    /// Integration substeps per frame. `0` (the default) sizes automatically so each
    /// substep displaces at most ~1.5 px (capped at 16); an explicit `1..=64` forces a
    /// count. `1` reproduces the v2 single-step behaviour — and its echo-ring artifact.
    #[serde(default)]
    pub substeps: u32,
    /// Blend in `[0, 1]` between uniform reinjection (`0`, the default) and reinjection
    /// modulated by an animated sparse blotch mask (`1` — the reference shader's patchy
    /// "source bleeds back in islands" behaviour). Because the mask is mostly near zero,
    /// high blotch values sharply lower the *effective* refresh rate — raise `reinject`
    /// to compensate. The `reinject == 1` verbatim identity only holds at `0`.
    #[serde(default)]
    pub reinject_blotch: f32,
    /// Amplitude (detail-lattice cells) of the animated sinusoidal domain warp on the
    /// detail octave — the shader's `QuakeLavaUV` fold. `0` (default) = unwarped; has no
    /// effect when `detail == 0` (the big vortices stay steady by design).
    #[serde(default)]
    pub warp: f32,
    /// Faux-viscosity weight in `[0, 1]`: mixes a 3×3 binomial blur into the upstream
    /// dye sample each substep. `0` (default) = sharp; small values (~0.1–0.3) suppress
    /// moiré from near-Nyquist source detail (scanlines, hard blocks).
    #[serde(default)]
    pub diffuse: f32,
}

impl Default for FluidAdvectSettings {
    fn default() -> Self {
        Self {
            advect: 12.0,
            turbulence_scale: 0.008,
            turbulence_speed: 0.06,
            detail: 0.1,
            reinject: 0.05,
            seed: 0,
            substeps: 0,
            reinject_blotch: 0.0,
            warp: 0.0,
            diffuse: 0.0,
        }
    }
}

impl FluidAdvectSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        for (name, value) in [
            ("advect", self.advect),
            ("turbulence_scale", self.turbulence_scale),
            ("turbulence_speed", self.turbulence_speed),
            ("detail", self.detail),
            ("reinject", self.reinject),
            ("reinject_blotch", self.reinject_blotch),
            ("warp", self.warp),
            ("diffuse", self.diffuse),
        ] {
            if !value.is_finite() {
                return Err(RenderError::InvalidCoagulationSettings(format!(
                    "{name} must be finite"
                )));
            }
        }
        if self.detail < 0.0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "detail must be non-negative".to_string(),
            ));
        }
        if self.advect < 0.0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "advect must be non-negative".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.reinject) {
            return Err(RenderError::InvalidCoagulationSettings(
                "reinject must be in [0, 1]".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.reinject_blotch) {
            return Err(RenderError::InvalidCoagulationSettings(
                "reinject_blotch must be in [0, 1]".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.diffuse) {
            return Err(RenderError::InvalidCoagulationSettings(
                "diffuse must be in [0, 1]".to_string(),
            ));
        }
        if self.warp < 0.0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "warp must be non-negative".to_string(),
            ));
        }
        if self.substeps > FLUID_ADVECT_MAX_SUBSTEPS {
            return Err(RenderError::InvalidCoagulationSettings(format!(
                "substeps must be at most {FLUID_ADVECT_MAX_SUBSTEPS}"
            )));
        }
        Ok(())
    }

    /// The substep count actually run per frame: the explicit `substeps` when non-zero,
    /// otherwise sized so each substep displaces at most ~1.5 px (capped at 16).
    pub fn effective_substeps(&self) -> u32 {
        if self.substeps > 0 {
            self.substeps
        } else {
            let auto = (self.advect / AUTO_SUBSTEP_TARGET_PX).ceil() as u32;
            auto.clamp(1, AUTO_SUBSTEP_CAP)
        }
    }

    /// Per-substep reinjection rate: `substeps` applications compound to exactly
    /// `reinject` (`1 − (1 − r_sub)^N = r`). Exactly `reinject` when `substeps <= 1`.
    pub fn per_substep_reinject(&self, substeps: u32) -> f32 {
        if substeps <= 1 {
            self.reinject
        } else {
            1.0 - (1.0 - self.reinject).powf(1.0 / substeps as f32)
        }
    }

    /// The blotch-mask lattice frequency (cells per pixel) derived from
    /// `turbulence_scale` — the single source of truth shared with the Metal dispatch.
    pub fn blotch_lattice_scale(&self) -> f32 {
        self.turbulence_scale * BLOTCH_SCALE_RATIO
    }

    /// The field-evaluation time for substep `substep` (0-based) of `substeps` at
    /// `frame_index`: interpolates within the frame and ends exactly at
    /// `frame_index * turbulence_speed`, so one substep reproduces the v2 timeline.
    pub fn substep_time(&self, frame_index: u32, substep: u32, substeps: u32) -> f32 {
        let n = substeps.max(1) as f32;
        (frame_index as f32 - 1.0 + (substep as f32 + 1.0) / n) * self.turbulence_speed
    }
}

/// Advance the dye one frame. `previous` is the dye buffer carried from the prior frame;
/// `None` (frame zero) returns the source frame verbatim. `source` is the current video
/// frame (sampled for reinjection and used to size the output).
pub fn fluid_advect_frame_cpu(
    source: &ImageBufferF32,
    previous: Option<&ImageBufferF32>,
    frame_index: u32,
    settings: FluidAdvectSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;

    let Some(previous) = previous else {
        // Frame zero: the dye is seeded from the source frame verbatim.
        return Ok(source.clone());
    };

    if previous.width != source.width || previous.height != source.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "previous dye is {}x{}, source is {}x{}",
            previous.width, previous.height, source.width, source.height
        )));
    }

    // The documented off-case: full uniform reinjection is the source verbatim. Made
    // explicit so the identity is byte-exact at any substep count (the per-substep lerp
    // is only ULP-close), and mirrored by the Metal wrapper.
    if settings.reinject >= 1.0 && settings.reinject_blotch == 0.0 {
        return Ok(source.clone());
    }

    let substeps = settings.effective_substeps();
    let step = settings.advect / substeps as f32;
    let reinject = settings.per_substep_reinject(substeps);

    let mut dye = advect_substep(
        previous,
        source,
        settings.substep_time(frame_index, 0, substeps),
        step,
        reinject,
        settings,
    )?;
    for substep in 1..substeps {
        dye = advect_substep(
            &dye,
            source,
            settings.substep_time(frame_index, substep, substeps),
            step,
            reinject,
            settings,
        )?;
    }
    Ok(dye)
}

/// One integration substep: advect the dye upstream along the (optionally warped) vortex
/// field by `step` pixels, optionally diffuse the sample, then bleed in `reinject` of the
/// current source frame (optionally gated by the animated blotch mask).
fn advect_substep(
    previous: &ImageBufferF32,
    source: &ImageBufferF32,
    time: f32,
    step: f32,
    reinject: f32,
    settings: FluidAdvectSettings,
) -> Result<ImageBufferF32, RenderError> {
    ImageBufferF32::from_fn(source.width, source.height, |x, y| {
        let xf = x as f32;
        let yf = y as f32;
        let (vx, vy) = steady_vortex_velocity_warped(
            settings.seed,
            xf,
            yf,
            time,
            settings.turbulence_scale,
            settings.detail,
            settings.warp,
        );
        // Semi-Lagrangian: read the dye that was upstream so colour flows downstream.
        let sx = xf - vx * step;
        let sy = yf - vy * step;
        let mut advected = sample_bilinear_clamped(previous, sx, sy);
        if settings.diffuse > 0.0 {
            let blurred = binomial_blur_sample(previous, sx, sy);
            for channel in 0..4 {
                advected[channel] +=
                    (blurred[channel] - advected[channel]) * settings.diffuse;
            }
        }
        let src = source.pixel(x, y).unwrap_or([0.0; 4]);
        let mut r = reinject;
        if settings.reinject_blotch > 0.0 {
            let mask = reinjection_blotch_mask(
                settings.seed,
                xf,
                yf,
                time,
                settings.blotch_lattice_scale(),
            );
            // mix(1, mask, blotch): 0 keeps uniform reinjection, 1 is fully patchy.
            r *= 1.0 + (mask - 1.0) * settings.reinject_blotch;
        }
        [
            advected[0] + (src[0] - advected[0]) * r,
            advected[1] + (src[1] - advected[1]) * r,
            advected[2] + (src[2] - advected[2]) * r,
            advected[3] + (src[3] - advected[3]) * r,
        ]
    })
}

/// 3×3 binomial (1-2-1)²/16 blur of the bilinear samples around `(x, y)` — the faux
/// viscosity mixed in by `diffuse`.
fn binomial_blur_sample(image: &ImageBufferF32, x: f32, y: f32) -> [f32; 4] {
    const TAPS: [(f32, f32, f32); 9] = [
        (-1.0, -1.0, 1.0),
        (0.0, -1.0, 2.0),
        (1.0, -1.0, 1.0),
        (-1.0, 0.0, 2.0),
        (0.0, 0.0, 4.0),
        (1.0, 0.0, 2.0),
        (-1.0, 1.0, 1.0),
        (0.0, 1.0, 2.0),
        (1.0, 1.0, 1.0),
    ];
    let mut sum = [0.0f32; 4];
    for (dx, dy, weight) in TAPS {
        let sample = sample_bilinear_clamped(image, x + dx, y + dy);
        for channel in 0..4 {
            sum[channel] += sample[channel] * weight;
        }
    }
    for channel in &mut sum {
        *channel /= 16.0;
    }
    sum
}

/// Reference-shader Image-pass relief lighting as a **display-only** adapter: faux height
/// `max(min(r, g), b)`, a normal from its finite-difference gradient (strength 2.0, slope
/// response `|g|^0.6`, sample offset proportional to resolution), a fixed light from
/// `(1, 1, 1)`, and `colour · (max(dot(n, l), 0) · 1.2 + 0.3)`, mixed in by `strength`
/// (`0` = untouched, `1` = fully lit). Alpha passes through. Deviation from the shader:
/// its bird-image-specific tonemap colour grade is omitted. Never feed the shaded frame
/// back as dye state — light would compound frame over frame.
pub fn relief_shade_cpu(
    image: &ImageBufferF32,
    strength: f32,
) -> Result<ImageBufferF32, RenderError> {
    const NORMAL_STRENGTH: f32 = 2.0;
    const NORMAL_TWEAK: f32 = 0.6;
    const NORMAL_OFFSET: f32 = 0.002;
    let strength = strength.clamp(0.0, 1.0);
    if strength == 0.0 {
        return Ok(image.clone());
    }
    // The shader offsets by 0.002 of the frame height on both axes (its horizontal UV
    // offset is pre-divided by the aspect ratio), never less than one pixel here.
    let offset = ((NORMAL_OFFSET * image.height as f32).round() as i64).max(1);
    let height_at = |x: i64, y: i64| -> f32 {
        let cx = x.clamp(0, image.width as i64 - 1) as u32;
        let cy = y.clamp(0, image.height as i64 - 1) as u32;
        let p = image.pixel(cx, cy).unwrap_or([0.0; 4]);
        p[0].min(p[1]).max(p[2])
    };
    // The shader's light direction (1, 1, 1) normalized.
    let light = 1.0 / 3.0f32.sqrt();
    ImageBufferF32::from_fn(image.width, image.height, |x, y| {
        let xi = x as i64;
        let yi = y as i64;
        let ddx = (height_at(xi + offset, yi) - height_at(xi - offset, yi)) * NORMAL_STRENGTH;
        // Shader "north" is up; our +y runs down, so north is y - offset.
        let ddy = (height_at(xi, yi - offset) - height_at(xi, yi + offset)) * NORMAL_STRENGTH;
        let mapped_x = ddx.abs().powf(NORMAL_TWEAK) * ddx.signum();
        let mapped_y = ddy.abs().powf(NORMAL_TWEAK) * ddy.signum();
        let mapped_z = (1.0 - mapped_x * mapped_x - mapped_y * mapped_y).max(0.0).sqrt();
        let lit = ((mapped_x + mapped_y + mapped_z) * light).max(0.0) * 1.2 + 0.3;
        let lit = lit.max(0.0);
        let p = image.pixel(x, y).unwrap_or([0.0; 4]);
        let shade = |c: f32| c + (c * lit - c) * strength;
        [shade(p[0]), shade(p[1]), shade(p[2]), p[3]]
    })
}

/// Algorithm identifier for the two-source CPU reference. Bump when the advection scheme
/// or the reinjection changes so stale caches/checkpoints invalidate.
pub const FLUID_ADVECT_TWO_SOURCE_ALGORITHM: &str = "fluid_advect_two_source_cpu_v1";

/// Settings for [`fluid_advect_two_source_frame_cpu`] — the mutual two-source variant where
/// Source A's optical-flow motion drives the field that advects Source B's colour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FluidAdvectTwoSourceSettings {
    /// Strength applied to Source A's per-pixel flow when advecting the dye, in flow units
    /// (A's flow is already expressed in output pixels per frame). `1.0` moves the dye
    /// exactly with A's measured motion; higher amplifies it; `0` holds the dye in place.
    pub advect: f32,
    /// Fraction of the current Source B frame bled back into the dye each frame, in
    /// `[0, 1]` — the "frame refresh". `0` lets B smear freely along A's motion with no
    /// fresh content; `1` shows B verbatim (no fluid). Small values (~0.05–0.15) keep B
    /// present while A's motion reshapes it.
    pub reinject: f32,
}

impl Default for FluidAdvectTwoSourceSettings {
    fn default() -> Self {
        Self {
            advect: 1.0,
            reinject: 0.08,
        }
    }
}

impl FluidAdvectTwoSourceSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if !self.advect.is_finite() {
            return Err(RenderError::InvalidCoagulationSettings(
                "advect must be finite".to_string(),
            ));
        }
        if !self.reinject.is_finite() || !(0.0..=1.0).contains(&self.reinject) {
            return Err(RenderError::InvalidCoagulationSettings(
                "reinject must be in [0, 1]".to_string(),
            ));
        }
        Ok(())
    }
}

/// Advance the two-source dye one frame. Source B (`carrier_b`) is the material whose colour
/// flows; `flow_a` is Source A's optical flow (the modulator's motion), already sized to B's
/// dimensions and in B's pixel units. `previous` is the dye buffer carried from the prior
/// frame; `None` (frame zero) returns B verbatim (there is no prior A frame to derive motion
/// from). The dye is advected along A's flow via the parity-gated [`flow_displace_cpu`], then
/// a fraction of the current B frame is bled back in (the "frame refresh").
///
/// Continuity identities (the off cases for an off-vs-on readout):
/// - `reinject == 1.0` ⇒ output is B verbatim every frame (no fluid at all).
/// - `advect == 0.0` with `reinject == 0.0` ⇒ output is the previous dye unchanged (A's
///   motion never displaces anything) — a pure hold of frame zero.
pub fn fluid_advect_two_source_frame_cpu(
    carrier_b: &ImageBufferF32,
    previous: Option<&ImageBufferF32>,
    flow_a: &FlowField,
    settings: FluidAdvectTwoSourceSettings,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;

    let Some(previous) = previous else {
        // Frame zero: the dye is seeded from Source B verbatim.
        return Ok(carrier_b.clone());
    };

    if previous.width != carrier_b.width || previous.height != carrier_b.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "previous dye is {}x{}, carrier B is {}x{}",
            previous.width, previous.height, carrier_b.width, carrier_b.height
        )));
    }
    if flow_a.width != carrier_b.width || flow_a.height != carrier_b.height {
        return Err(RenderError::IncompatibleInputs(format!(
            "Source A flow is {}x{}, carrier B is {}x{}",
            flow_a.width, flow_a.height, carrier_b.width, carrier_b.height
        )));
    }

    // Advect the dye along Source A's motion (the same parity-gated displace the rest of the
    // graph uses; a future Metal port is flow_displace_metal + the reinject composite).
    let advected = flow_displace_cpu(previous, flow_a, settings.advect)?;

    let r = settings.reinject;
    ImageBufferF32::from_fn(carrier_b.width, carrier_b.height, |x, y| {
        let a = advected.pixel(x, y).unwrap_or([0.0; 4]);
        let b = carrier_b.pixel(x, y).unwrap_or([0.0; 4]);
        [
            a[0] + (b[0] - a[0]) * r,
            a[1] + (b[1] - a[1]) * r,
            a[2] + (b[2] - a[2]) * r,
            a[3] + (b[3] - a[3]) * r,
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(width: u32, height: u32) -> ImageBufferF32 {
        // A horizontal colour ramp so advection visibly relocates content.
        ImageBufferF32::from_fn(width, height, |x, _| {
            let t = x as f32 / (width.max(2) - 1) as f32;
            [t, 1.0 - t, 0.5, 1.0]
        })
        .expect("ramp")
    }

    #[test]
    fn frame_zero_is_the_source_verbatim() {
        let src = ramp(16, 16);
        let out =
            fluid_advect_frame_cpu(&src, None, 0, FluidAdvectSettings::default()).expect("f0");
        assert_eq!(out.pixels, src.pixels);
    }

    #[test]
    fn full_reinject_returns_the_source() {
        // reinject 1 ⇒ output is the current source verbatim, no fluid.
        let src = ramp(16, 16);
        let prev = ImageBufferF32::from_fn(16, 16, |_, _| [0.0, 0.0, 0.0, 1.0]).expect("prev");
        let settings = FluidAdvectSettings {
            reinject: 1.0,
            ..FluidAdvectSettings::default()
        };
        let out = fluid_advect_frame_cpu(&src, Some(&prev), 3, settings).expect("on");
        assert_eq!(out.pixels, src.pixels);
    }

    #[test]
    fn zero_advect_zero_reinject_holds_the_previous() {
        // No displacement and no fresh content ⇒ the dye is held unchanged.
        let src = ramp(16, 16);
        let prev = ImageBufferF32::from_fn(16, 16, |x, y| {
            [
                (x as f32 * 0.05).fract(),
                (y as f32 * 0.03).fract(),
                0.25,
                1.0,
            ]
        })
        .expect("prev");
        let settings = FluidAdvectSettings {
            advect: 0.0,
            reinject: 0.0,
            ..FluidAdvectSettings::default()
        };
        let out = fluid_advect_frame_cpu(&src, Some(&prev), 5, settings).expect("hold");
        assert_eq!(out.pixels, prev.pixels);
    }

    #[test]
    fn advection_displaces_the_dye() {
        // With advection on, the flow must move dye off the held image.
        let src = ramp(64, 64);
        let prev = ramp(64, 64);
        let settings = FluidAdvectSettings {
            advect: 8.0,
            reinject: 0.0,
            ..FluidAdvectSettings::default()
        };
        let out = fluid_advect_frame_cpu(&src, Some(&prev), 4, settings).expect("flow");
        assert_ne!(out.pixels, prev.pixels, "advection should relocate content");
    }

    #[test]
    fn is_deterministic() {
        let src = ramp(32, 32);
        let prev = ramp(32, 32);
        let settings = FluidAdvectSettings {
            substeps: 3,
            reinject_blotch: 0.5,
            warp: 1.0,
            diffuse: 0.2,
            ..FluidAdvectSettings::default()
        };
        let a = fluid_advect_frame_cpu(&src, Some(&prev), 7, settings).expect("a");
        let b = fluid_advect_frame_cpu(&src, Some(&prev), 7, settings).expect("b");
        assert_eq!(a.pixels, b.pixels);
    }

    #[test]
    fn substep_reinjection_compounds_to_the_frame_rate() {
        // With no displacement, N substeps of the compound rate must equal one full-rate
        // lerp: prev + (src - prev) * reinject, within float tolerance.
        let src = ramp(16, 16);
        let prev = ImageBufferF32::from_fn(16, 16, |x, y| {
            [(x as f32 * 0.07).fract(), (y as f32 * 0.11).fract(), 0.8, 1.0]
        })
        .expect("prev");
        let settings = FluidAdvectSettings {
            advect: 0.0,
            reinject: 0.3,
            substeps: 4,
            ..FluidAdvectSettings::default()
        };
        let out = fluid_advect_frame_cpu(&src, Some(&prev), 2, settings).expect("out");
        for ((o, p), s) in out.pixels.iter().zip(&prev.pixels).zip(&src.pixels) {
            for channel in 0..4 {
                let expected = p[channel] + (s[channel] - p[channel]) * 0.3;
                assert!(
                    (o[channel] - expected).abs() < 1e-5,
                    "compound reinjection drifted: {} vs {expected}",
                    o[channel]
                );
            }
        }
    }

    #[test]
    fn substep_count_changes_the_advected_path() {
        // The whole point of substeps: the integration path (and the ring layering)
        // differs between one big step and several small ones.
        let src = ramp(64, 64);
        let prev = ramp(64, 64);
        let one = FluidAdvectSettings {
            advect: 8.0,
            substeps: 1,
            ..FluidAdvectSettings::default()
        };
        let four = FluidAdvectSettings { substeps: 4, ..one };
        let a = fluid_advect_frame_cpu(&src, Some(&prev), 4, one).expect("one");
        let b = fluid_advect_frame_cpu(&src, Some(&prev), 4, four).expect("four");
        assert_ne!(a.pixels, b.pixels);
    }

    #[test]
    fn auto_substeps_track_the_advect_distance() {
        let mut settings = FluidAdvectSettings {
            substeps: 0,
            ..FluidAdvectSettings::default()
        };
        settings.advect = 0.0;
        assert_eq!(settings.effective_substeps(), 1);
        settings.advect = 4.0;
        assert_eq!(settings.effective_substeps(), 3);
        settings.advect = 12.0;
        assert_eq!(settings.effective_substeps(), 8);
        settings.advect = 100.0;
        assert_eq!(settings.effective_substeps(), 16, "auto count is capped");
        settings.substeps = 2;
        assert_eq!(settings.effective_substeps(), 2, "explicit count wins");
    }

    #[test]
    fn warp_is_invisible_without_detail() {
        // The warp folds only the detail octave; with detail 0 the field is untouched.
        let src = ramp(48, 48);
        let prev = ramp(48, 48);
        let flat = FluidAdvectSettings {
            detail: 0.0,
            warp: 0.0,
            ..FluidAdvectSettings::default()
        };
        let warped = FluidAdvectSettings { warp: 3.0, ..flat };
        let a = fluid_advect_frame_cpu(&src, Some(&prev), 3, flat).expect("flat");
        let b = fluid_advect_frame_cpu(&src, Some(&prev), 3, warped).expect("warped");
        assert_eq!(a.pixels, b.pixels);

        let with_detail = FluidAdvectSettings {
            detail: 0.5,
            warp: 3.0,
            ..FluidAdvectSettings::default()
        };
        let unwarped_detail = FluidAdvectSettings {
            warp: 0.0,
            ..with_detail
        };
        let c = fluid_advect_frame_cpu(&src, Some(&prev), 3, with_detail).expect("warp+detail");
        let d = fluid_advect_frame_cpu(&src, Some(&prev), 3, unwarped_detail).expect("detail");
        assert_ne!(c.pixels, d.pixels, "warp must fold the detail octave");
    }

    #[test]
    fn blotch_makes_reinjection_patchy() {
        // With no displacement, uniform reinjection lerps every pixel by the same rate;
        // the blotch mask must vary that rate spatially.
        let src = ramp(48, 48);
        let prev = ImageBufferF32::from_fn(48, 48, |_, _| [0.0, 0.0, 0.0, 1.0]).expect("prev");
        let uniform = FluidAdvectSettings {
            advect: 0.0,
            reinject: 0.5,
            reinject_blotch: 0.0,
            // Blotch patches at the default scale are larger than this fixture, so use
            // a coarse-enough lattice that the 48px window spans mask variation.
            turbulence_scale: 0.05,
            ..FluidAdvectSettings::default()
        };
        let patchy = FluidAdvectSettings {
            reinject_blotch: 1.0,
            ..uniform
        };
        let a = fluid_advect_frame_cpu(&src, Some(&prev), 2, uniform).expect("uniform");
        let b = fluid_advect_frame_cpu(&src, Some(&prev), 2, patchy).expect("patchy");
        assert_ne!(a.pixels, b.pixels);
    }

    #[test]
    fn diffuse_softens_the_advected_sample() {
        let src = ramp(48, 48);
        let prev = ImageBufferF32::from_fn(48, 48, |x, _| {
            // Alternating columns: any blur changes the sample.
            if x % 2 == 0 {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            }
        })
        .expect("prev");
        let sharp = FluidAdvectSettings {
            advect: 0.0,
            reinject: 0.0,
            diffuse: 0.0,
            ..FluidAdvectSettings::default()
        };
        let soft = FluidAdvectSettings {
            diffuse: 0.5,
            ..sharp
        };
        let a = fluid_advect_frame_cpu(&src, Some(&prev), 2, sharp).expect("sharp");
        let b = fluid_advect_frame_cpu(&src, Some(&prev), 2, soft).expect("soft");
        assert_eq!(a.pixels, prev.pixels, "sharp hold is the identity");
        assert_ne!(b.pixels, prev.pixels, "diffuse must soften the hold");
    }

    #[test]
    fn new_knobs_are_validated() {
        let src = ramp(8, 8);
        let prev = ramp(8, 8);
        for settings in [
            FluidAdvectSettings {
                substeps: FLUID_ADVECT_MAX_SUBSTEPS + 1,
                ..FluidAdvectSettings::default()
            },
            FluidAdvectSettings {
                reinject_blotch: 1.5,
                ..FluidAdvectSettings::default()
            },
            FluidAdvectSettings {
                diffuse: -0.1,
                ..FluidAdvectSettings::default()
            },
            FluidAdvectSettings {
                warp: -1.0,
                ..FluidAdvectSettings::default()
            },
        ] {
            assert!(fluid_advect_frame_cpu(&src, Some(&prev), 1, settings).is_err());
        }
    }

    #[test]
    fn relief_shade_zero_strength_is_identity() {
        let image = ramp(32, 32);
        let out = relief_shade_cpu(&image, 0.0).expect("shade");
        assert_eq!(out.pixels, image.pixels);
    }

    #[test]
    fn relief_shade_lights_slopes_and_keeps_alpha() {
        // A height step must produce different lighting on its two sides.
        let image = ImageBufferF32::from_fn(32, 32, |x, _| {
            if x < 16 {
                [0.2, 0.2, 0.2, 0.75]
            } else {
                [0.9, 0.9, 0.9, 0.75]
            }
        })
        .expect("step");
        let out = relief_shade_cpu(&image, 1.0).expect("shade");
        assert_ne!(out.pixels, image.pixels, "shading must change a sloped image");
        for pixel in &out.pixels {
            assert_eq!(pixel[3], 0.75, "alpha must pass through");
        }
        let again = relief_shade_cpu(&image, 1.0).expect("shade again");
        assert_eq!(out.pixels, again.pixels, "shading must be deterministic");
    }

    fn zero_flow(width: u32, height: u32) -> FlowField {
        FlowField::new(width, height, vec![[0.0, 0.0]; (width * height) as usize]).expect("flow")
    }

    fn uniform_flow(width: u32, height: u32, vector: [f32; 2]) -> FlowField {
        FlowField::new(width, height, vec![vector; (width * height) as usize]).expect("flow")
    }

    #[test]
    fn two_source_frame_zero_is_carrier_b() {
        let b = ramp(16, 16);
        let flow = zero_flow(16, 16);
        let out = fluid_advect_two_source_frame_cpu(
            &b,
            None,
            &flow,
            FluidAdvectTwoSourceSettings::default(),
        )
        .expect("f0");
        assert_eq!(out.pixels, b.pixels);
    }

    #[test]
    fn two_source_full_reinject_returns_carrier_b() {
        // reinject 1 ⇒ output is the current B verbatim, no fluid.
        let b = ramp(16, 16);
        let prev = ImageBufferF32::from_fn(16, 16, |_, _| [0.0, 0.0, 0.0, 1.0]).expect("prev");
        let flow = uniform_flow(16, 16, [3.0, -2.0]);
        let settings = FluidAdvectTwoSourceSettings {
            reinject: 1.0,
            ..FluidAdvectTwoSourceSettings::default()
        };
        let out = fluid_advect_two_source_frame_cpu(&b, Some(&prev), &flow, settings).expect("on");
        assert_eq!(out.pixels, b.pixels);
    }

    #[test]
    fn two_source_zero_advect_zero_reinject_holds_previous() {
        // No displacement and no fresh content ⇒ the dye is held unchanged, even if A moved.
        let b = ramp(16, 16);
        let prev = ImageBufferF32::from_fn(16, 16, |x, y| {
            [
                (x as f32 * 0.05).fract(),
                (y as f32 * 0.03).fract(),
                0.25,
                1.0,
            ]
        })
        .expect("prev");
        let flow = uniform_flow(16, 16, [4.0, 4.0]);
        let settings = FluidAdvectTwoSourceSettings {
            advect: 0.0,
            reinject: 0.0,
        };
        let out =
            fluid_advect_two_source_frame_cpu(&b, Some(&prev), &flow, settings).expect("hold");
        assert_eq!(out.pixels, prev.pixels);
    }

    #[test]
    fn two_source_flow_advects_the_dye() {
        // With A's flow non-zero and advection on, the dye must move off the held image.
        let b = ramp(64, 64);
        let prev = ramp(64, 64);
        let flow = uniform_flow(64, 64, [5.0, 0.0]);
        let settings = FluidAdvectTwoSourceSettings {
            advect: 1.0,
            reinject: 0.0,
        };
        let out =
            fluid_advect_two_source_frame_cpu(&b, Some(&prev), &flow, settings).expect("flow");
        assert_ne!(
            out.pixels, prev.pixels,
            "A's motion should relocate B's dye"
        );
    }

    #[test]
    fn two_source_dimension_mismatch_errors() {
        let b = ramp(16, 16);
        let prev = ramp(16, 16);
        let flow = zero_flow(8, 8);
        let result = fluid_advect_two_source_frame_cpu(
            &b,
            Some(&prev),
            &flow,
            FluidAdvectTwoSourceSettings::default(),
        );
        assert!(result.is_err(), "mismatched flow dimensions should error");
    }
}
