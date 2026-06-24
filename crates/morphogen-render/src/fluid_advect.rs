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

use crate::sampler::sample_bilinear_clamped;
use crate::vortex_field::steady_vortex_velocity;
use crate::{ImageBufferF32, RenderError};

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
        let out = fluid_advect_frame_cpu(&src, None, 0, FluidAdvectSettings::default()).expect("f0");
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
            [(x as f32 * 0.05).fract(), (y as f32 * 0.03).fract(), 0.25, 1.0]
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
}
