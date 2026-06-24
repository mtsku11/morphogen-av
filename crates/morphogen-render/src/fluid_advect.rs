//! Faux-fluid dye advection — a single-source, stateful, per-pixel feedback advection
//! that ports the *Faux Fluid Sim* shadertoy look (procedural-turbulence variant).
//!
//! Unlike [`crate::fluid_mosaic`], which moves discrete colour **tiles** as particles,
//! this effect transports **continuous pixels**: it keeps a "dye" image and, every frame,
//! pushes each pixel along a procedural velocity field, then bleeds a little of the
//! current source frame back in. There are no tiles and no particles — the picture
//! itself becomes liquid and marbles (folds, stretches, swirls) like ink in water.
//!
//! Per frame `n` (with `D` the dye buffer, `S_n` the current source frame):
//! 1. **Velocity field** `v(p, t)` — the analytic curl `(∂ψ/∂y, -∂ψ/∂x)` of a streamfunction
//!    `ψ` built to match the reference shader's *character*: large coherent vortices that
//!    emerge and slowly re-form, not a busy wobble. `ψ` is **3D gradient (Perlin) noise**
//!    (splitmix-hashed gradients, GPU-safe — not the shader's `sin()`-hashing) with a
//!    **dominant low-frequency octave** (the big vortices) plus a `detail`-weighted octave
//!    at 2× frequency (the shader weights its small turbulence only `0.1`). Time is the
//!    noise's **3rd axis**, so vortices form and dissolve smoothly in place; a coherent
//!    `x` drift scrolls them across the frame so they appear to emerge (the shader's
//!    `+ iTime` scroll). The curl is divergence-free, so the dye is transported without
//!    sources/sinks (no pooling to bright/black blobs).
//! 2. **Semi-Lagrangian advection** — `D'[p] = bilinear(D, p - v · advect)`: each output
//!    pixel reads the dye that was *upstream*, so the colour flows downstream along `v`.
//! 3. **Source reinjection (the "frame refresh")** — `D'[p] = mix(D'[p], S_n[p], reinject)`:
//!    a little of the current video frame is bled back in every frame, so the video plays
//!    through the flow and fresh content keeps the dye from washing out to fog.
//!
//! Frame zero is the source frame verbatim (`D = S_0`), the prior-frame state consumed is
//! the dye buffer (RGBA32F), and that buffer is the checkpoint representation — a stateful
//! temporal node. Deterministic: splitmix64 hashing, fixed per-frame time, no wall clock.
//!
//! Continuity identities (the off cases for an off-vs-on readout):
//! - `reinject == 1.0` ⇒ output is `S_n` verbatim every frame (no fluid at all).
//! - `advect == 0.0` with `reinject == 0.0` ⇒ output is the previous dye unchanged (the
//!   field never displaces anything) — a pure hold of frame zero.

use serde::{Deserialize, Serialize};

use crate::cpu_reference::flow_displace_cpu;
use crate::sampler::sample_bilinear_clamped;
use crate::vortex_field::steady_vortex_velocity;
use crate::{FlowField, ImageBufferF32, RenderError};

/// Algorithm identifier for the CPU reference. Bump when the velocity model, advection
/// scheme, or reinjection changes so stale caches/checkpoints invalidate.
pub const FLUID_ADVECT_ALGORITHM: &str = "fluid_advect_curl_noise_cpu_v2";

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
        Ok(())
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

    let time = frame_index as f32 * settings.turbulence_speed;

    ImageBufferF32::from_fn(source.width, source.height, |x, y| {
        let (vx, vy) = steady_vortex_velocity(
            settings.seed,
            x as f32,
            y as f32,
            time,
            settings.turbulence_scale,
            settings.detail,
        );
        // Semi-Lagrangian: read the dye that was upstream so colour flows downstream.
        let sx = x as f32 - vx * settings.advect;
        let sy = y as f32 - vy * settings.advect;
        let advected = sample_bilinear_clamped(previous, sx, sy);
        let src = source.pixel(x, y).unwrap_or([0.0; 4]);
        let r = settings.reinject;
        [
            advected[0] + (src[0] - advected[0]) * r,
            advected[1] + (src[1] - advected[1]) * r,
            advected[2] + (src[2] - advected[2]) * r,
            advected[3] + (src[3] - advected[3]) * r,
        ]
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
        let settings = FluidAdvectSettings::default();
        let a = fluid_advect_frame_cpu(&src, Some(&prev), 7, settings).expect("a");
        let b = fluid_advect_frame_cpu(&src, Some(&prev), 7, settings).expect("b");
        assert_eq!(a.pixels, b.pixels);
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
