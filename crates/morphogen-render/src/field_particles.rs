//! Discrete-carrier advection — coloured **particles** that ride the shared steady-vortex
//! velocity field ([`crate::vortex_field`]).
//!
//! Where [`crate::fluid_advect`] transports a *continuous* dye buffer and
//! [`crate::fluid_mosaic`] moves tiles under cohesion/repulsion *forces*, this effect is the
//! third option the user asked about ("tiles flowing or tiny particles?"): a dense grid of
//! discrete coloured points, each seeded from a source cell, that simply **flow along the
//! field's streamlines** — no inter-particle forces, no colour sorting. They stream out of
//! flat regions and pile into the vortex centres, tracing the flow as a discrete carrier.
//!
//! Per frame the particle positions integrate the field (forward Euler):
//! `p ← p + v(p, t) · advect`, where `v` is the steady curl-noise vortex velocity. The frame
//! is then rendered by splatting each particle as a `particle_size × particle_size` square of
//! its colour onto a black canvas, in fixed particle-index order (last writer wins on overlap
//! — deterministic). Colours are sampled once from the seed frame, so the particles carry the
//! frame-zero image as they flow (live colour re-sampling is a deferred variant).
//!
//! Stateful temporal node: frame zero is the initial grid (the particle state is the
//! checkpoint — positions + colours, never a re-read PNG); each later frame advances that
//! state. Deterministic: the field hashing is splitmix64 and the splat order is fixed.
//!
//! **Live colour (opt-in).** By default a particle's colour is frozen at seed time, so a video
//! does not play through. [`refresh_field_particle_colors`] re-samples each particle's colour
//! from its *origin cell* in the current source frame (the [`crate::fluid_mosaic`] live-refresh
//! semantics): the video's colour at the particle's birthpoint is carried to wherever the
//! particle has flowed, so the current frame plays through, displaced by the accumulated flow.
//!
//! Continuity identities (the off cases for an off-vs-on readout): `advect == 0.0` ⇒ the
//! particles never move, so every frame renders the same initial grid (a posterised source).
//! Frame zero is identical for any `advect` (the advance never runs at frame zero). Refreshing
//! colours from the seed frame is a no-op (the colour already came from it), so live colour is
//! byte-identical until the source frame changes.

use serde::{Deserialize, Serialize};

use crate::vortex_field::steady_vortex_velocity;
use crate::{ImageBufferF32, RenderError};

/// Algorithm identifier for the CPU reference. Bump when the integration scheme, the field
/// model, or the splat/render changes so stale caches/checkpoints invalidate. `v2` adds the
/// opt-in live-colour refresh (byte-identical to `v1` when colours are not refreshed).
pub const FIELD_PARTICLES_ALGORITHM: &str = "field_particles_vortex_cpu_v2";

/// Settings for the discrete-carrier particle advection.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FieldParticleSettings {
    /// Grid spacing in pixels — one particle is seeded per `spacing × spacing` cell. Smaller
    /// ⇒ denser carrier (more, finer particles).
    pub spacing: u32,
    /// Edge length (pixels) of the square each particle splats. Equal to `spacing` ⇒ the
    /// frame-zero grid tiles the canvas exactly; larger ⇒ overlapping, blobbier carrier.
    pub particle_size: u32,
    /// Field strength applied to the velocity per frame (pixels). `0` holds the particles on
    /// the static grid; higher ⇒ they flow further along the field each step.
    pub advect: f32,
    /// Vortex frequency of the field (lattice cells per pixel). Smaller ⇒ larger vortices.
    pub turbulence_scale: f32,
    /// Temporal drift rate of the field's fine detail octave per frame (the big vortices stay
    /// steady so particles can spiral into them).
    pub turbulence_speed: f32,
    /// Weight of the fine detail octave relative to the steady big vortices (`0` = pure large
    /// vortices).
    pub detail: f32,
    /// When `true`, each particle's colour is re-sampled from its origin cell in the current
    /// source frame every frame (via [`refresh_field_particle_colors`]) so the video plays
    /// through the flowing carrier. When `false` (default), colours stay frozen at seed time.
    #[serde(default)]
    pub live_color: bool,
    /// Seed for the deterministic field.
    pub seed: u64,
}

impl Default for FieldParticleSettings {
    fn default() -> Self {
        Self {
            spacing: 8,
            particle_size: 8,
            advect: 6.0,
            turbulence_scale: 0.008,
            turbulence_speed: 0.06,
            detail: 0.1,
            live_color: false,
            seed: 0,
        }
    }
}

impl FieldParticleSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.spacing == 0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "spacing must be greater than zero".to_string(),
            ));
        }
        if self.particle_size == 0 {
            return Err(RenderError::InvalidCoagulationSettings(
                "particle_size must be greater than zero".to_string(),
            ));
        }
        for (name, value) in [
            ("advect", self.advect),
            ("turbulence_scale", self.turbulence_scale),
            ("turbulence_speed", self.turbulence_speed),
            ("detail", self.detail),
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
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Particle {
    x: f32,
    y: f32,
    /// The grid cell the particle was seeded from — the origin re-sampled for live colour.
    home_x: u32,
    home_y: u32,
    color: [f32; 4],
}

/// The stateful particle carrier — the checkpoint representation. Positions are floats that
/// integrate the field over frames; colours are fixed at seed time.
#[derive(Debug, Clone, PartialEq)]
pub struct ParticleField {
    width: u32,
    height: u32,
    particles: Vec<Particle>,
}

impl ParticleField {
    /// Number of particles in the carrier.
    pub fn particle_count(&self) -> usize {
        self.particles.len()
    }
}

/// Seed the particle grid from the source frame (frame zero). One particle per `spacing` cell,
/// positioned at the cell origin with the source colour sampled there.
pub fn initialize_field_particles(
    source: &ImageBufferF32,
    settings: FieldParticleSettings,
) -> Result<ParticleField, RenderError> {
    settings.validate()?;

    let mut particles = Vec::new();
    let mut gy = 0u32;
    while gy < source.height {
        let mut gx = 0u32;
        while gx < source.width {
            let color = source.pixel(gx, gy).unwrap_or([0.0; 4]);
            particles.push(Particle {
                x: gx as f32,
                y: gy as f32,
                home_x: gx,
                home_y: gy,
                color,
            });
            gx += settings.spacing;
        }
        gy += settings.spacing;
    }

    Ok(ParticleField {
        width: source.width,
        height: source.height,
        particles,
    })
}

/// Advance every particle one frame along the field: `p ← p + v(p, t) · advect`, with
/// `t = frame_index · turbulence_speed`. Positions are clamped to the canvas so particles
/// swept past an edge ride along it rather than vanishing.
pub fn advance_field_particles(
    field: &mut ParticleField,
    frame_index: u32,
    settings: FieldParticleSettings,
) -> Result<(), RenderError> {
    settings.validate()?;

    if settings.advect == 0.0 {
        // The particles never move — keep the state byte-identical (the off case).
        return Ok(());
    }

    let time = frame_index as f32 * settings.turbulence_speed;
    let max_x = (field.width.max(1) - 1) as f32;
    let max_y = (field.height.max(1) - 1) as f32;
    for particle in &mut field.particles {
        let (vx, vy) = steady_vortex_velocity(
            settings.seed,
            particle.x,
            particle.y,
            time,
            settings.turbulence_scale,
            settings.detail,
        );
        particle.x = (particle.x + vx * settings.advect).clamp(0.0, max_x);
        particle.y = (particle.y + vy * settings.advect).clamp(0.0, max_y);
    }

    Ok(())
}

/// Re-sample each particle's colour from its origin cell in the current source frame (the
/// live-colour refresh — the [`crate::fluid_mosaic`] live-refresh semantics). Positions are
/// untouched: only the colour each flowing particle carries updates, so the video plays
/// through the carrier. A no-op against the seed frame (colours already came from it).
pub fn refresh_field_particle_colors(field: &mut ParticleField, source: &ImageBufferF32) {
    let max_x = source.width.saturating_sub(1);
    let max_y = source.height.saturating_sub(1);
    for particle in &mut field.particles {
        let sx = particle.home_x.min(max_x);
        let sy = particle.home_y.min(max_y);
        particle.color = source.pixel(sx, sy).unwrap_or(particle.color);
    }
}

/// Render the carrier: splat each particle as a `particle_size` square of its colour onto a
/// black canvas, in fixed particle-index order (last writer wins on overlap).
pub fn render_field_particles(
    field: &ParticleField,
    settings: FieldParticleSettings,
) -> ImageBufferF32 {
    let width = field.width;
    let height = field.height;
    let mut pixels = vec![[0.0, 0.0, 0.0, 1.0]; (width as usize) * (height as usize)];
    let size = settings.particle_size.max(1);

    for particle in &field.particles {
        // Round to the nearest pixel for the splat origin; the square extends `size` to the
        // right/down so a `size == spacing` grid tiles the canvas at frame zero.
        let px = particle.x.round() as i64;
        let py = particle.y.round() as i64;
        for dy in 0..size as i64 {
            let y = py + dy;
            if y < 0 || y >= height as i64 {
                continue;
            }
            for dx in 0..size as i64 {
                let x = px + dx;
                if x < 0 || x >= width as i64 {
                    continue;
                }
                pixels[(y as usize) * (width as usize) + x as usize] = particle.color;
            }
        }
    }

    ImageBufferF32::new(width, height, pixels)
        .expect("particle canvas dimensions are valid by construction")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32) -> ImageBufferF32 {
        ImageBufferF32::from_fn(width, height, |x, y| {
            let u = x as f32 / (width.max(2) - 1) as f32;
            let v = y as f32 / (height.max(2) - 1) as f32;
            [u, v, 1.0 - u, 1.0]
        })
        .expect("gradient")
    }

    #[test]
    fn initial_grid_count_matches_spacing() {
        let src = gradient(32, 32);
        let settings = FieldParticleSettings {
            spacing: 8,
            ..FieldParticleSettings::default()
        };
        let field = initialize_field_particles(&src, settings).expect("init");
        // 32 / 8 = 4 columns and 4 rows.
        assert_eq!(field.particle_count(), 16);
    }

    #[test]
    fn zero_advect_holds_the_grid_byte_identical() {
        // advect 0 ⇒ the particles never move, so every frame renders the same grid.
        let src = gradient(48, 48);
        let settings = FieldParticleSettings {
            advect: 0.0,
            ..FieldParticleSettings::default()
        };
        let mut field = initialize_field_particles(&src, settings).expect("init");
        let frame0 = render_field_particles(&field, settings);
        advance_field_particles(&mut field, 5, settings).expect("advance");
        let frame5 = render_field_particles(&field, settings);
        assert_eq!(frame0.pixels, frame5.pixels);
    }

    #[test]
    fn advection_moves_the_particles() {
        let src = gradient(48, 48);
        let settings = FieldParticleSettings {
            advect: 6.0,
            ..FieldParticleSettings::default()
        };
        let mut field = initialize_field_particles(&src, settings).expect("init");
        let frame0 = render_field_particles(&field, settings);
        for index in 1..=8 {
            advance_field_particles(&mut field, index, settings).expect("advance");
        }
        let frame8 = render_field_particles(&field, settings);
        assert_ne!(frame0.pixels, frame8.pixels, "the field should relocate particles");
    }

    #[test]
    fn frame_zero_is_independent_of_advect() {
        // The advance never runs at frame zero, so the initial grid is identical for any
        // advect — the basis for a frame-zero-byte-identical off-vs-on readout.
        let src = gradient(40, 40);
        let off = FieldParticleSettings {
            advect: 0.0,
            ..FieldParticleSettings::default()
        };
        let on = FieldParticleSettings {
            advect: 9.0,
            ..FieldParticleSettings::default()
        };
        let off_frame =
            render_field_particles(&initialize_field_particles(&src, off).expect("init"), off);
        let on_frame =
            render_field_particles(&initialize_field_particles(&src, on).expect("init"), on);
        assert_eq!(off_frame.pixels, on_frame.pixels);
    }

    #[test]
    fn is_deterministic() {
        let src = gradient(40, 40);
        let settings = FieldParticleSettings::default();
        let mut a = initialize_field_particles(&src, settings).expect("a");
        let mut b = initialize_field_particles(&src, settings).expect("b");
        for index in 1..=6 {
            advance_field_particles(&mut a, index, settings).expect("a advance");
            advance_field_particles(&mut b, index, settings).expect("b advance");
        }
        assert_eq!(
            render_field_particles(&a, settings).pixels,
            render_field_particles(&b, settings).pixels
        );
    }

    #[test]
    fn refresh_against_seed_frame_is_a_no_op() {
        // Colours already came from the seed frame, so refreshing against it changes nothing
        // (the basis for live colour being byte-identical until the source frame moves).
        let src = gradient(24, 24);
        let settings = FieldParticleSettings::default();
        let field = initialize_field_particles(&src, settings).expect("init");
        let mut refreshed = field.clone();
        refresh_field_particle_colors(&mut refreshed, &src);
        assert_eq!(field, refreshed);
    }

    #[test]
    fn refresh_updates_colour_from_the_current_frame() {
        // A changed source frame ⇒ the particles pick up the new colour at their origin cell.
        let seed = gradient(24, 24);
        let settings = FieldParticleSettings::default();
        let mut field = initialize_field_particles(&seed, settings).expect("init");
        let before = render_field_particles(&field, settings);
        let next = ImageBufferF32::from_fn(24, 24, |_, _| [0.9, 0.1, 0.4, 1.0]).expect("next");
        refresh_field_particle_colors(&mut field, &next);
        let after = render_field_particles(&field, settings);
        assert_ne!(before.pixels, after.pixels, "live colour should track the new frame");
    }
}
