//! Scribbled-edge tile cascade — a collage of a few large, mostly-straight-edged
//! tiles (rect = 4 edges, L = 6 edges), each **re-stamped many times in one frame**
//! at a fixed per-step offset (last-writer-wins). Only thin slivers of earlier
//! copies survive at the edges, so the stacked edges become the fine lines; the
//! solid tile faces are the open, line-free spaces. One edge of each tile is a
//! **scribbled** warbling line; each cascade copy morphs subtly (scribble re-draws,
//! the straight notch edge extends/shortens, the hue ramps across the cascade).
//!
//! Unlike [`crate::cascade_trails`] (square patches advected by a flow field into a
//! persistent cross-frame accumulator), this is a **stateless** single-frame
//! composite: a frame depends only on `(settings, frame)` — no prior-frame state,
//! no checkpoint. The cascade is the in-frame re-stamping, not a cross-frame trail.
//!
//! Determinism: stamp order is fixed (shape index, then step); the scribble noise is
//! 1-D smoothstep value noise on a splitmix64 lattice (same hash family as
//! [`crate::block_collage`]/[`crate::fluid_advect`], so the future MSL port reuses
//! the established `splitmix64` precedent).
//!
//! Continuity identity (off-vs-on readouts):
//! - `scrib_amp_scale == 0.0` ⇒ every scribbled edge is straight.
//! - `morph_rate == 0.0` and `frame_hue_rate == 0.0` ⇒ all frames are byte-identical
//!   to frame 0 (no per-frame drift).
//!
//! See `docs/CASCADE_COLLAGE_MILESTONE.md` for the contract and acceptance criteria.

use serde::{Deserialize, Serialize};

use crate::{ImageBufferF32, RenderError};

/// Algorithm identifier — bump when the rasterizer, scribble formulation, morph, or
/// colour model changes so stale caches/checkpoints invalidate.
pub const CASCADE_COLLAGE_ALGORITHM: &str = "cascade_collage_scribble_cpu_v1";

/// Tile geometry: a plain rectangle (4 straight edges) or an L (outer box minus a
/// notched corner = 6 straight edges).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShapeKind {
    #[default]
    Rect,
    L,
}

/// Which single edge of the tile is the scribbled warbling line. For `Rect` one of
/// the four straight sides; for `L` the `Notch` (the notch's vertical edge).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScribbleEdge {
    Left,
    #[default]
    Right,
    Top,
    Bottom,
    Notch,
}

/// One cascading shape. Spatial fractions are of the canvas (0..1) unless noted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CascadeShape {
    /// Start centre (fraction of canvas).
    pub cx: f32,
    pub cy: f32,
    /// Half-extents (fraction of canvas).
    pub hw: f32,
    pub hh: f32,
    pub kind: ShapeKind,
    /// Notch corner offset (fraction of canvas) — L only.
    pub notch_u: f32,
    pub notch_v: f32,
    /// Which corner is removed (L only): `notch_right` picks the +u side, else -u;
    /// `notch_bottom` picks the +v side, else -v.
    pub notch_right: bool,
    pub notch_bottom: bool,
    /// Which edge is scribbled.
    pub scrib: ScribbleEdge,
    /// Per-step cascade offset, in pixels (direction chosen *away from* the scribbled
    /// edge so it stays exposed).
    pub dx: f32,
    pub dy: f32,
    /// Number of cascade copies stamped per frame.
    pub steps: u32,
    /// Base hue (turns, 0..1), saturation and value (0..1) — each shape a colour.
    pub base_hue: f32,
    pub sat: f32,
    pub val: f32,
    /// Scribble amplitude in pixels (before the global `scrib_amp_scale`).
    pub scrib_amp: f32,
    /// Hue ramp across the cascade (turns from first to last copy) — each stacked
    /// copy a slightly different hue.
    pub hue_spread: f32,
    /// Straight notch edge extend/shorten amplitude (fraction of `hh`).
    pub edge_grow: f32,
}

/// Settings for the scribbled-edge tile cascade.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CascadeCollageSettings {
    /// Backstop fill (linear RGB) — a coloured floor so any uncovered gap is obvious,
    /// never black. The default composition covers it entirely.
    pub background: [f32; 3],
    /// The shapes, stamped in order (later shapes draw on top).
    pub shapes: Vec<CascadeShape>,
    /// Global scribble amplitude multiplier. `0` ⇒ all edges straight.
    pub scrib_amp_scale: f32,
    /// Per-frame phase advance for the morph (scribble re-draw, edge grow, brightness).
    /// `0` ⇒ frames do not drift.
    pub morph_rate: f32,
    /// Per-frame global hue rotation (turns). `0` ⇒ no per-frame colour change.
    pub frame_hue_rate: f32,
    /// Per-step brightness oscillation amplitude (0..1).
    pub bright_osc: f32,
    /// Deterministic seed for the scribble noise.
    pub seed: u64,
}

impl Default for CascadeCollageSettings {
    /// The validated 4-shape quadrant composition: magenta L (TL), orange rect (TR),
    /// teal rect (BL), purple L (BR), each cascading outward toward its corner with
    /// its scribbled edge facing the centre.
    fn default() -> Self {
        Self {
            background: [0.118, 0.047, 0.157],
            shapes: vec![
                // magenta — TL, L, notch toward centre (BR), cascade up-left
                CascadeShape {
                    cx: 0.30,
                    cy: 0.30,
                    hw: 0.42,
                    hh: 0.42,
                    kind: ShapeKind::L,
                    notch_u: 0.10,
                    notch_v: 0.08,
                    notch_right: true,
                    notch_bottom: true,
                    scrib: ScribbleEdge::Notch,
                    dx: -1.20,
                    dy: -1.20,
                    steps: 55,
                    base_hue: 0.90,
                    sat: 0.94,
                    val: 0.80,
                    scrib_amp: 12.0,
                    hue_spread: 0.05,
                    edge_grow: 0.06,
                },
                // orange — TR, rect, LEFT edge (toward centre) scribbled, cascade up-right
                CascadeShape {
                    cx: 0.70,
                    cy: 0.30,
                    hw: 0.42,
                    hh: 0.42,
                    kind: ShapeKind::Rect,
                    notch_u: 0.0,
                    notch_v: 0.0,
                    notch_right: false,
                    notch_bottom: false,
                    scrib: ScribbleEdge::Left,
                    dx: 1.20,
                    dy: -1.20,
                    steps: 55,
                    base_hue: 0.07,
                    sat: 0.95,
                    val: 0.93,
                    scrib_amp: 11.0,
                    hue_spread: 0.05,
                    edge_grow: 0.06,
                },
                // teal — BL, rect, RIGHT edge (toward centre) scribbled, cascade down-left
                CascadeShape {
                    cx: 0.30,
                    cy: 0.70,
                    hw: 0.42,
                    hh: 0.42,
                    kind: ShapeKind::Rect,
                    notch_u: 0.0,
                    notch_v: 0.0,
                    notch_right: false,
                    notch_bottom: false,
                    scrib: ScribbleEdge::Right,
                    dx: -1.20,
                    dy: 1.20,
                    steps: 55,
                    base_hue: 0.47,
                    sat: 0.90,
                    val: 0.66,
                    scrib_amp: 12.0,
                    hue_spread: 0.05,
                    edge_grow: 0.06,
                },
                // purple — BR, L, notch toward centre (TL), cascade down-right
                CascadeShape {
                    cx: 0.70,
                    cy: 0.70,
                    hw: 0.42,
                    hh: 0.42,
                    kind: ShapeKind::L,
                    notch_u: -0.10,
                    notch_v: -0.08,
                    notch_right: false,
                    notch_bottom: false,
                    scrib: ScribbleEdge::Notch,
                    dx: 1.20,
                    dy: 1.20,
                    steps: 55,
                    base_hue: 0.78,
                    sat: 0.80,
                    val: 0.66,
                    scrib_amp: 11.0,
                    hue_spread: 0.05,
                    edge_grow: 0.06,
                },
            ],
            scrib_amp_scale: 1.0,
            morph_rate: 0.12,
            frame_hue_rate: 0.0,
            bright_osc: 0.12,
            seed: 71,
        }
    }
}

impl CascadeCollageSettings {
    pub fn validate(&self) -> Result<(), RenderError> {
        if self.shapes.is_empty() {
            return Err(RenderError::InvalidCascadeCollageSettings(
                "at least one shape is required".into(),
            ));
        }
        if self.scrib_amp_scale < 0.0 {
            return Err(RenderError::InvalidCascadeCollageSettings(
                "scrib_amp_scale must be >= 0".into(),
            ));
        }
        for (i, s) in self.shapes.iter().enumerate() {
            if s.steps == 0 {
                return Err(RenderError::InvalidCascadeCollageSettings(format!(
                    "shape {i}: steps must be >= 1"
                )));
            }
            if !(s.hw > 0.0 && s.hh > 0.0) {
                return Err(RenderError::InvalidCascadeCollageSettings(format!(
                    "shape {i}: hw and hh must be > 0"
                )));
            }
            if !(0.0..=1.0).contains(&s.sat) || !(0.0..=1.0).contains(&s.val) {
                return Err(RenderError::InvalidCascadeCollageSettings(format!(
                    "shape {i}: sat and val must be in [0, 1]"
                )));
            }
        }
        Ok(())
    }
}

// ─── Deterministic noise + colour helpers ───────────────────────────────────────

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Hash a 1-D integer lattice coordinate + seed to [0, 1].
fn hash1(seed: u64, i: i64) -> f32 {
    let h = splitmix(seed ^ (i as u64).wrapping_mul(0x9e3779b1));
    (h & 0xffff) as f32 / 65535.0
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 1-D smoothstep value noise.
fn vnoise1(t: f32, seed: u64) -> f32 {
    let it = t.floor();
    let i = it as i64;
    let ft = smoothstep(t - it);
    lerp(hash1(seed, i), hash1(seed, i + 1), ft)
}

/// Hand-scribble offset (pixels): slow swing + mid wobble + fine jitter, all drifting
/// with `phase` so the line slowly re-draws. `t` is the coordinate along the edge.
fn scribble(t: f32, seed: u64, phase: f32, amp: f32) -> f32 {
    let s = 0.55 * (t * 0.05 + phase * 0.7).sin()
        + 0.30 * (vnoise1(t * 0.14 + phase, seed ^ 0x21) - 0.5) * 2.0
        + 0.18 * (vnoise1(t * 0.45 + phase * 1.7, seed ^ 0x44) - 0.5) * 2.0;
    s * amp
}

/// HSV (h in turns, s/v in [0,1]) → RGB in [0,1].
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    if s <= 0.0 {
        return (v, v, v);
    }
    let h6 = (h - h.floor()) * 6.0;
    let i = h6.floor();
    let f = h6 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i as i32 % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

// ─── Frame renderer ─────────────────────────────────────────────────────────────

/// Render one frame of the scribbled-edge tile cascade at the given dimensions.
pub fn render_cascade_collage_frame(
    width: u32,
    height: u32,
    settings: &CascadeCollageSettings,
    frame: u32,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let w = width as usize;
    let h = height as usize;
    let bg = [
        settings.background[0],
        settings.background[1],
        settings.background[2],
        1.0,
    ];
    let mut pixels = vec![bg; w.checked_mul(h).ok_or_else(|| {
        RenderError::InvalidCascadeCollageSettings("dimensions too large".into())
    })?];

    let fw = width as f32;
    let fh = height as f32;
    let ff = frame as f32;

    for (si, shape) in settings.shapes.iter().enumerate() {
        let ts = settings.seed ^ (si as u64).wrapping_mul(131);
        let cx = shape.cx * fw;
        let cy = shape.cy * fh;
        let hw = shape.hw * fw;
        let hh = shape.hh * fh;
        let nu0 = shape.notch_u * fw;
        let nv0 = shape.notch_v * fh;
        let amp = shape.scrib_amp * settings.scrib_amp_scale;
        let steps = shape.steps.max(1);
        let denom = if steps > 1 { (steps - 1) as f32 } else { 1.0 };

        for step in 0..steps {
            let sf = step as f32;
            let ox = cx + shape.dx * sf;
            let oy = cy + shape.dy * sf;
            let phase = sf * 0.30 + ff * settings.morph_rate;
            let grow = shape.edge_grow * hh * (sf * 0.05 + ff * settings.morph_rate).sin();
            let hue = shape.base_hue
                + shape.hue_spread * (sf / denom)
                + settings.frame_hue_rate * ff;
            let osc = 0.5 + 0.5 * (sf * 0.6 + ff * settings.morph_rate).sin();
            let sh = 1.0 - settings.bright_osc + settings.bright_osc * osc;
            let v_eff = (shape.val * sh).clamp(0.0, 1.0);
            let (r, g, b) = hsv_to_rgb(hue.rem_euclid(1.0), shape.sat, v_eff);
            let col = [r, g, b, 1.0];

            let maxr = hw.max(hh) + amp.abs() + 4.0;
            let y0 = (oy - maxr).floor().max(0.0) as i64;
            let y1 = (oy + maxr).ceil().min(fh) as i64;
            let x0 = (ox - maxr).floor().max(0.0) as i64;
            let x1 = (ox + maxr).ceil().min(fw) as i64;

            for y in y0..y1 {
                let v = y as f32 - oy;
                // edge scribble for v-dependent edges (Right/Left/Notch)
                let sc_v = scribble(v, ts, phase, amp);
                let row = y as usize * w;
                for x in x0..x1 {
                    let u = x as f32 - ox;
                    let inside = match shape.kind {
                        ShapeKind::Rect => match shape.scrib {
                            ScribbleEdge::Right => {
                                u >= -hw && u <= hw + sc_v && v >= -hh && v <= hh
                            }
                            ScribbleEdge::Left => {
                                u >= -hw - sc_v && u <= hw && v >= -hh && v <= hh
                            }
                            ScribbleEdge::Top => {
                                let sc = scribble(u, ts, phase, amp);
                                u >= -hw && u <= hw && v >= -hh - sc && v <= hh
                            }
                            ScribbleEdge::Bottom => {
                                let sc = scribble(u, ts, phase, amp);
                                u >= -hw && u <= hw && v >= -hh && v <= hh + sc
                            }
                            ScribbleEdge::Notch => u >= -hw && u <= hw && v >= -hh && v <= hh,
                        },
                        ShapeKind::L => {
                            if !(u >= -hw && u <= hw && v >= -hh && v <= hh) {
                                false
                            } else {
                                let nu = nu0 + sc_v;
                                let nv = nv0 + grow;
                                let cu = if shape.notch_right { u >= nu } else { u <= nu };
                                let cv = if shape.notch_bottom { v >= nv } else { v <= nv };
                                !(cu && cv)
                            }
                        }
                    };
                    if inside {
                        pixels[row + x as usize] = col;
                    }
                }
            }
        }
    }

    ImageBufferF32::new(width, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_inputs_are_byte_identical() {
        let s = CascadeCollageSettings::default();
        let a = render_cascade_collage_frame(180, 240, &s, 3).unwrap();
        let b = render_cascade_collage_frame(180, 240, &s, 3).unwrap();
        assert_eq!(a, b, "A1: identical (settings, frame) must be byte-identical");
    }

    #[test]
    fn default_composition_leaves_no_background() {
        let s = CascadeCollageSettings::default();
        let out = render_cascade_collage_frame(180, 240, &s, 0).unwrap();
        let bg = [s.background[0], s.background[1], s.background[2], 1.0];
        let gaps = out.pixels.iter().filter(|p| **p == bg).count();
        assert_eq!(gaps, 0, "A2: default composition must fully cover (no gaps)");
    }

    #[test]
    fn no_per_frame_drift_is_static() {
        let s = CascadeCollageSettings {
            morph_rate: 0.0,
            frame_hue_rate: 0.0,
            ..Default::default()
        };
        let f0 = render_cascade_collage_frame(160, 200, &s, 0).unwrap();
        let f9 = render_cascade_collage_frame(160, 200, &s, 9).unwrap();
        assert_eq!(f0, f9, "A3: no per-frame drift ⇒ frames identical to frame 0");
    }

    #[test]
    fn scribble_off_differs_from_on() {
        let on = CascadeCollageSettings::default();
        let off = CascadeCollageSettings {
            scrib_amp_scale: 0.0,
            ..Default::default()
        };
        let a = render_cascade_collage_frame(180, 240, &on, 0).unwrap();
        let b = render_cascade_collage_frame(180, 240, &off, 0).unwrap();
        let d = a.max_channel_difference(&b).expect("comparable");
        assert!(d > 0.0, "A4: straight-edge (off) must differ from scribbled (on)");
    }

    #[test]
    fn validate_rejects_empty_and_zero_steps() {
        let empty = CascadeCollageSettings {
            shapes: vec![],
            ..Default::default()
        };
        assert!(empty.validate().is_err());

        let mut zero = CascadeCollageSettings::default();
        zero.shapes[0].steps = 0;
        assert!(zero.validate().is_err());
    }
}
