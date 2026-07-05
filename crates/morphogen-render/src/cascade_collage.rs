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

use crate::{sample_bilinear_clamped, ImageBufferF32, RenderError};

/// Algorithm identifier — bump when the rasterizer, scribble formulation, morph, or
/// colour model changes so stale caches/checkpoints invalidate.
pub const CASCADE_COLLAGE_ALGORITHM: &str = "cascade_collage_scribble_cpu_v7";

/// Algorithm identifier for the A→B cross-synth mode: Source A drives texture/motion,
/// Source B supplies per-shape origin-cell colour (replaces the HSV palette).
pub const CASCADE_COLLAGE_B_SAMPLER_ALGORITHM: &str = "cascade_collage_b_sampler_cpu_v8";

/// Lifts the small per-pixel footage gradients (Sobel magnitude ~0.05–0.3) into
/// visible contour lines, so `edge_detect ≈ 1` already exposes lines on the face.
const EDGE_DETECT_GAIN: f32 = 5.0;

/// Which single OUTER edge of the tile is the scribbled warbling line (or `None`).
/// Notches carry their own scribble via [`Notch::scrib`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScribbleEdge {
    #[default]
    None,
    Left,
    Right,
    Top,
    Bottom,
}

/// Maximum notches per shape (fixed so [`CascadeShape`] stays `Copy`). 4 allows a
/// plus/cross (all four corners notched).
pub const MAX_NOTCHES: usize = 4;

/// How each block (a shape + its cascade, rendered to its own layer) is composited
/// onto the blocks below — lets blocks merge instead of hard-occluding.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    /// Replace (with `block_opacity`): at opacity 1 this is the hard last-writer look.
    #[default]
    Normal,
    /// `dst * src` — overlaps darken; blocks tint through one another.
    Multiply,
    /// `1-(1-dst)(1-src)` — overlaps brighten; neon lines glow where blocks cross.
    Screen,
    /// `(dst+src)/2` — even mix; blocks read as equal translucent panes.
    Average,
    /// `max(dst,src)` — keeps the brighter of the two; merges glow without washing out.
    Lighten,
}

/// An axis-aligned rectangle subtracted from a tile, in local fractions of canvas
/// (relative to the tile centre). Subtracting notches yields rectilinear shapes
/// (L, T, U, plus, staircase) whose every corner is 90° or 270°. `scrib` wobbles the
/// notch's interior edges (the ones inside the outer box).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Notch {
    pub u0: f32,
    pub u1: f32,
    pub v0: f32,
    pub v1: f32,
    pub scrib: bool,
}

/// One cascading shape: an outer rectangle minus up to [`MAX_NOTCHES`] notches.
/// Spatial fractions are of the canvas (0..1) unless noted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CascadeShape {
    /// Start centre (fraction of canvas).
    pub cx: f32,
    pub cy: f32,
    /// Half-extents (fraction of canvas).
    pub hw: f32,
    pub hh: f32,
    /// Texture-mode sample origin (fraction of source) — the centre of the source crop
    /// this tile carries. Distinct from `cx,cy` so each tile shows a *different* region
    /// of the footage (a collage); set equal to `cx,cy` to reassemble the source.
    pub src_cx: f32,
    pub src_cy: f32,
    /// Rectangular notches subtracted from the outer box (first `notch_count` used).
    pub notches: [Notch; MAX_NOTCHES],
    pub notch_count: u8,
    /// Which outer edge is scribbled (notches scribble via their own flag).
    pub outer_scrib: ScribbleEdge,
    /// Per-step cascade offset, in pixels (direction chosen *away from* the exposed
    /// edges so they stay visible).
    pub dx: f32,
    pub dy: f32,
    /// Number of cascade copies stamped per frame.
    pub steps: u32,
    /// Base hue (turns, 0..1), saturation and value (0..1) — each shape a colour.
    pub base_hue: f32,
    pub sat: f32,
    pub val: f32,
    /// Hue (turns, 0..1) of this tile's neon EDGE lines (the cascade seams).
    pub edge_hue: f32,
    /// Scribble amplitude in pixels (before the global `scrib_amp_scale`).
    pub scrib_amp: f32,
    /// Per-copy hue variation amplitude (turns) — each cascade copy a different hue
    /// within ±`hue_spread`, quantized to `hue_steps`. Makes every block multi-coloured.
    pub hue_spread: f32,
    /// Notch interior-edge extend/shorten amplitude over the cascade (fraction of `hh`).
    pub edge_grow: f32,
}

impl CascadeShape {
    fn notches(&self) -> &[Notch] {
        let n = (self.notch_count as usize).min(MAX_NOTCHES);
        &self.notches[..n]
    }
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
    /// Width (pixels) of the tinted neon edge band along every tile boundary.
    pub edge_width: f32,
    /// Blend toward the edge colour at the boundary, in [0, 1]. `0` ⇒ edges show
    /// footage/palette like the interior (the off case); `1` ⇒ pure neon lines.
    pub edge_strength: f32,
    /// Saturation and value of the neon edge colour (0..1) — high ⇒ glowing lines.
    pub edge_sat: f32,
    pub edge_val: f32,
    /// Blend the tile FACE toward its hue, in [0, 1]. The face keeps the footage's
    /// luma/texture but is colorized toward `base_hue` (+ per-step `hue_spread`), so a
    /// block reads as e.g. "blue" with each cascade copy a different blue. `0` ⇒ pure
    /// footage colour (off); `1` ⇒ fully colorized duotone.
    pub face_strength: f32,
    /// Saturation of the colorized face (0..1).
    pub face_sat: f32,
    /// Number of discrete hue levels for the per-copy variation. `<=1` ⇒ continuous.
    pub hue_steps: u32,
    /// Sobel edge-detect strength on the footage, in [0, 1+]. `0` ⇒ off; higher burns the
    /// video's own contours in as dark ink lines (adds to the geometric + scribble lines).
    pub edge_detect: f32,
    /// How each block composites onto the ones below.
    pub block_blend: BlendMode,
    /// Per-block composite opacity in [0, 1]. `1` ⇒ hard occlude (today's look);
    /// `<1` ⇒ blocks blend together / show through → a more unified image.
    pub block_opacity: f32,
    /// Deterministic seed for the scribble noise.
    pub seed: u64,
}

/// A notch (fractions of canvas, relative to tile centre).
fn notch(u0: f32, u1: f32, v0: f32, v1: f32, scrib: bool) -> Notch {
    Notch {
        u0,
        u1,
        v0,
        v1,
        scrib,
    }
}

impl Default for CascadeCollageSettings {
    /// Layered composition: four large quadrant tiles (rect/L) guarantee full coverage,
    /// then four smaller many-sided rectilinear tiles (T, U, plus, staircase) stack on
    /// top to add sides, scribbled edges and colour variety.
    fn default() -> Self {
        // shared field defaults; per-shape fields overridden via struct update
        let base = CascadeShape {
            cx: 0.5,
            cy: 0.5,
            hw: 0.42,
            hh: 0.42,
            src_cx: 0.5,
            src_cy: 0.5,
            notches: [Notch::default(); MAX_NOTCHES],
            notch_count: 0,
            outer_scrib: ScribbleEdge::None,
            dx: 0.0,
            dy: 0.0,
            steps: 55,
            base_hue: 0.0,
            sat: 0.9,
            val: 0.8,
            edge_hue: 0.0,
            scrib_amp: 11.0,
            hue_spread: 0.14,
            edge_grow: 0.06,
        };
        Self {
            background: [0.118, 0.047, 0.157],
            shapes: vec![
                // ── coverage layer: 4 large quadrant tiles ───────────────────────────
                // magenta L (TL): notch the bottom-right corner toward centre
                CascadeShape {
                    cx: 0.30,
                    cy: 0.30,
                    src_cx: 0.45,
                    src_cy: 0.45,
                    notches: [
                        notch(0.10, 0.50, 0.08, 0.50, true),
                        Notch::default(),
                        Notch::default(),
                        Notch::default(),
                    ],
                    notch_count: 1,
                    dx: -1.20,
                    dy: -1.20,
                    base_hue: 0.90,
                    sat: 0.94,
                    val: 0.80,
                    edge_hue: 0.90,
                    scrib_amp: 12.0,
                    ..base
                },
                // orange rect (TR): scribbled LEFT edge toward centre
                CascadeShape {
                    cx: 0.70,
                    cy: 0.30,
                    src_cx: 0.30,
                    src_cy: 0.75,
                    outer_scrib: ScribbleEdge::Left,
                    dx: 1.20,
                    dy: -1.20,
                    base_hue: 0.07,
                    sat: 0.95,
                    val: 0.93,
                    edge_hue: 0.07,
                    ..base
                },
                // teal rect (BL): scribbled RIGHT edge toward centre
                CascadeShape {
                    cx: 0.30,
                    cy: 0.70,
                    src_cx: 0.70,
                    src_cy: 0.55,
                    outer_scrib: ScribbleEdge::Right,
                    dx: -1.20,
                    dy: 1.20,
                    base_hue: 0.47,
                    sat: 0.90,
                    val: 0.66,
                    edge_hue: 0.50,
                    scrib_amp: 12.0,
                    ..base
                },
                // purple L (BR): notch the top-left corner toward centre
                CascadeShape {
                    cx: 0.70,
                    cy: 0.70,
                    src_cx: 0.55,
                    src_cy: 0.25,
                    notches: [
                        notch(-0.50, -0.10, -0.50, -0.08, true),
                        Notch::default(),
                        Notch::default(),
                        Notch::default(),
                    ],
                    notch_count: 1,
                    dx: 1.20,
                    dy: 1.20,
                    base_hue: 0.78,
                    sat: 0.80,
                    val: 0.66,
                    edge_hue: 0.92,
                    ..base
                },
                // ── detail layer: 4 small many-sided tiles on top ────────────────────
                // blue T (notch both bottom corners)
                CascadeShape {
                    cx: 0.50,
                    cy: 0.34,
                    hw: 0.24,
                    hh: 0.20,
                    src_cx: 0.50,
                    src_cy: 0.40,
                    notches: [
                        notch(-0.30, -0.09, 0.04, 0.30, true),
                        notch(0.09, 0.30, 0.04, 0.30, true),
                        Notch::default(),
                        Notch::default(),
                    ],
                    notch_count: 2,
                    dx: 0.9,
                    dy: 1.1,
                    steps: 38,
                    base_hue: 0.60,
                    sat: 0.85,
                    val: 0.85,
                    edge_hue: 0.55,
                    ..base
                },
                // green U (notch the top middle)
                CascadeShape {
                    cx: 0.34,
                    cy: 0.60,
                    hw: 0.20,
                    hh: 0.20,
                    src_cx: 0.62,
                    src_cy: 0.50,
                    notches: [
                        notch(-0.10, 0.10, -0.30, 0.03, true),
                        Notch::default(),
                        Notch::default(),
                        Notch::default(),
                    ],
                    notch_count: 1,
                    dx: -1.0,
                    dy: 0.8,
                    steps: 38,
                    base_hue: 0.33,
                    sat: 0.82,
                    val: 0.80,
                    edge_hue: 0.33,
                    ..base
                },
                // yellow plus (notch all four corners)
                CascadeShape {
                    cx: 0.66,
                    cy: 0.56,
                    hw: 0.17,
                    hh: 0.17,
                    src_cx: 0.40,
                    src_cy: 0.30,
                    notches: [
                        notch(-0.30, -0.07, -0.30, -0.07, true),
                        notch(0.07, 0.30, -0.30, -0.07, true),
                        notch(-0.30, -0.07, 0.07, 0.30, true),
                        notch(0.07, 0.30, 0.07, 0.30, true),
                    ],
                    notch_count: 4,
                    dx: 1.0,
                    dy: -0.9,
                    steps: 34,
                    base_hue: 0.15,
                    sat: 0.92,
                    val: 0.92,
                    edge_hue: 0.15,
                    ..base
                },
                // red staircase (two stepped notches)
                CascadeShape {
                    cx: 0.50,
                    cy: 0.80,
                    hw: 0.26,
                    hh: 0.16,
                    src_cx: 0.30,
                    src_cy: 0.65,
                    notches: [
                        notch(0.05, 0.32, -0.20, 0.02, true),
                        notch(-0.20, 0.05, -0.20, -0.05, true),
                        Notch::default(),
                        Notch::default(),
                    ],
                    notch_count: 2,
                    dx: -0.8,
                    dy: 1.0,
                    steps: 34,
                    base_hue: 0.99,
                    sat: 0.90,
                    val: 0.85,
                    edge_hue: 0.99,
                    ..base
                },
            ],
            scrib_amp_scale: 1.0,
            morph_rate: 0.12,
            frame_hue_rate: 0.0,
            bright_osc: 0.12,
            edge_width: 2.5,
            edge_strength: 0.85,
            edge_sat: 1.0,
            edge_val: 1.0,
            face_strength: 0.55,
            face_sat: 0.85,
            hue_steps: 5,
            edge_detect: 0.0,
            block_blend: BlendMode::Normal,
            block_opacity: 1.0,
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

/// RGB in [0,1] → HSV (h in turns, s/v in [0,1]).
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max <= 0.0 { 0.0 } else { d / max };
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, v)
}

/// Rotate an RGB colour's hue by `shift` turns, preserving saturation/value.
fn hue_rotate(r: f32, g: f32, b: f32, shift: f32) -> (f32, f32, f32) {
    let (h, s, v) = rgb_to_hsv(r, g, b);
    hsv_to_rgb((h + shift).rem_euclid(1.0), s, v)
}

/// Combine a block-layer colour `src` over the canvas colour `dst` by `mode`, then
/// mix by `opacity`. `opacity == 1` + `Normal` reproduces hard last-writer occlusion.
fn composite(dst: [f32; 4], src: [f32; 3], mode: BlendMode, opacity: f32) -> [f32; 4] {
    let blended = match mode {
        BlendMode::Normal => src,
        BlendMode::Multiply => [dst[0] * src[0], dst[1] * src[1], dst[2] * src[2]],
        BlendMode::Screen => [
            1.0 - (1.0 - dst[0]) * (1.0 - src[0]),
            1.0 - (1.0 - dst[1]) * (1.0 - src[1]),
            1.0 - (1.0 - dst[2]) * (1.0 - src[2]),
        ],
        BlendMode::Average => [
            (dst[0] + src[0]) * 0.5,
            (dst[1] + src[1]) * 0.5,
            (dst[2] + src[2]) * 0.5,
        ],
        BlendMode::Lighten => [dst[0].max(src[0]), dst[1].max(src[1]), dst[2].max(src[2])],
    };
    [
        lerp(dst[0], blended[0], opacity),
        lerp(dst[1], blended[1], opacity),
        lerp(dst[2], blended[2], opacity),
        1.0,
    ]
}

// ─── Frame renderer ─────────────────────────────────────────────────────────────

/// Render one frame of the scribbled-edge tile cascade.
///
/// When `source` is `Some`, each tile carries a **crop of the source** centred on
/// its home position (texture + colour come from the video); output dimensions match
/// the source. When `source` is `None`, tiles are flat HSV palette colours and the
/// output is `width × height` (the source-less generator).
/// Render one frame of the cascade collage effect.
///
/// - `source` — Source A: when Some, tile faces are footage crops of A (texture mode).
/// - `carrier` — Source B: when Some (and `source` is None), tile face colour is sampled
///   from B at each shape's origin cell `(scx, scy)`, replacing the HSV palette.
///   Both None → palette-only (procedural generator mode).
pub fn render_cascade_collage_frame(
    width: u32,
    height: u32,
    source: Option<&ImageBufferF32>,
    carrier: Option<&ImageBufferF32>,
    settings: &CascadeCollageSettings,
    frame: u32,
) -> Result<ImageBufferF32, RenderError> {
    settings.validate()?;
    let (out_w, out_h) = match source {
        Some(s) => (s.width, s.height),
        None => (width, height),
    };
    let width = out_w;
    let height = out_h;
    let w = width as usize;
    let h = height as usize;
    let bg = [
        settings.background[0],
        settings.background[1],
        settings.background[2],
        1.0,
    ];
    let mut pixels = vec![
        bg;
        w.checked_mul(h).ok_or_else(|| {
            RenderError::InvalidCascadeCollageSettings("dimensions too large".into())
        })?
    ];

    let fw = width as f32;
    let fh = height as f32;
    let ff = frame as f32;

    let ew = settings.edge_width.max(0.0);
    let estr = settings.edge_strength;
    let fstr = settings.face_strength;
    let fsat = settings.face_sat;
    let edge_detect = settings.edge_detect.max(0.0);
    let block_blend = settings.block_blend;
    let block_opacity = settings.block_opacity.clamp(0.0, 1.0);
    // each block (a shape + its cascade) is stamped to its own layer, then composited
    // onto the canvas — so blocks can blend/merge instead of hard-occluding.
    let mut layer = vec![[0.0_f32; 4]; w * h];

    for (si, shape) in settings.shapes.iter().enumerate() {
        // reset this block's layer (alpha 0 = uncovered; rgb is set where covered)
        for px in layer.iter_mut() {
            px[3] = 0.0;
        }
        let ts = settings.seed ^ (si as u64).wrapping_mul(131);
        let cx = shape.cx * fw;
        let cy = shape.cy * fh;
        // texture sample origin (source px) — distinct from draw home so tiles collage
        let scx = shape.src_cx * fw;
        let scy = shape.src_cy * fh;
        let hw = shape.hw * fw;
        let hh = shape.hh * fh;
        let amp = shape.scrib_amp * settings.scrib_amp_scale;
        let steps = shape.steps.max(1);
        let notches = shape.notches();

        for step in 0..steps {
            let sf = step as f32;
            let ox = cx + shape.dx * sf;
            let oy = cy + shape.dy * sf;
            let phase = sf * 0.30 + ff * settings.morph_rate;
            let grow = shape.edge_grow * hh * (sf * 0.05 + ff * settings.morph_rate).sin();
            let osc = 0.5 + 0.5 * (sf * 0.6 + ff * settings.morph_rate).sin();
            let sh = 1.0 - settings.bright_osc + settings.bright_osc * osc;
            // per-copy hue variation: quantized pseudo-random in ±hue_spread, so EVERY
            // block (incl. red) reads as a mix of different hues across its cascade.
            let raw = hash1(ts ^ 0xA13F, step as i64);
            let lvl = if settings.hue_steps > 1 {
                (raw * settings.hue_steps as f32).floor() / (settings.hue_steps as f32 - 1.0)
            } else {
                raw
            };
            let hue_shift = (lvl - 0.5) * 2.0 * shape.hue_spread + settings.frame_hue_rate * ff;
            // palette-mode flat colour (used only when there is no source texture)
            let palette_col = {
                let hue = (shape.base_hue + hue_shift).rem_euclid(1.0);
                let v_eff = (shape.val * sh).clamp(0.0, 1.0);
                let (r, g, b) = hsv_to_rgb(hue, shape.sat, v_eff);
                [r, g, b, 1.0]
            };
            let edge_col = hsv_to_rgb(
                (shape.edge_hue + hue_shift).rem_euclid(1.0),
                settings.edge_sat,
                settings.edge_val,
            );
            let face_hue = (shape.base_hue + hue_shift).rem_euclid(1.0);

            let maxr = hw.max(hh) + amp.abs() + 4.0;
            let y0 = (oy - maxr).floor().max(0.0) as i64;
            let y1 = (oy + maxr).ceil().min(fh) as i64;
            let x0 = (ox - maxr).floor().max(0.0) as i64;
            let x1 = (ox + maxr).ceil().min(fw) as i64;

            for y in y0..y1 {
                let v = y as f32 - oy;
                let sc_v = scribble(v, ts, phase, amp);
                let row = y as usize * w;
                for x in x0..x1 {
                    let u = x as f32 - ox;
                    // outer box (one edge optionally scribbled)
                    let (bul, buh, bvl, bvh) = match shape.outer_scrib {
                        ScribbleEdge::None => (-hw, hw, -hh, hh),
                        ScribbleEdge::Right => (-hw, hw + sc_v, -hh, hh),
                        ScribbleEdge::Left => (-hw - sc_v, hw, -hh, hh),
                        ScribbleEdge::Top => (-hw, hw, -hh - scribble(u, ts, phase, amp), hh),
                        ScribbleEdge::Bottom => (-hw, hw, -hh, hh + scribble(u, ts, phase, amp)),
                    };
                    if !(u >= bul && u <= buh && v >= bvl && v <= bvh) {
                        continue;
                    }
                    let mut edge_dist = (u - bul).min(buh - u).min(v - bvl).min(bvh - v);
                    // subtract notches → rectilinear shape; track distance to every edge
                    let mut removed = false;
                    for nt in notches {
                        let mut nu0 = nt.u0 * fw;
                        let mut nu1 = nt.u1 * fw;
                        let mut nv0 = nt.v0 * fh;
                        let mut nv1 = nt.v1 * fh;
                        // wobble / morph the notch's INTERIOR edges (those inside the box)
                        if nt.scrib {
                            if nu0 > -hw && nu0 < hw {
                                nu0 += scribble(v, ts ^ 0x51, phase, amp);
                            }
                            if nu1 > -hw && nu1 < hw {
                                nu1 += scribble(v, ts ^ 0x52, phase, amp);
                            }
                            if nv0 > -hh && nv0 < hh {
                                nv0 += scribble(u, ts ^ 0x53, phase, amp);
                            }
                            if nv1 > -hh && nv1 < hh {
                                nv1 += scribble(u, ts ^ 0x54, phase, amp);
                            }
                        }
                        if nv0 > -hh && nv0 < hh {
                            nv0 += grow;
                        }
                        if u > nu0 && u < nu1 && v > nv0 && v < nv1 {
                            removed = true;
                            break;
                        }
                        // distance from this solid pixel to the notch rect (its boundary)
                        let dx = (nu0 - u).max(u - nu1).max(0.0);
                        let dy = (nv0 - v).max(v - nv1).max(0.0);
                        edge_dist = edge_dist.min((dx * dx + dy * dy).sqrt());
                    }
                    if removed {
                        continue;
                    }

                    // face: footage crop (A texture mode), B-sampled origin-cell colour
                    // (A→B cross-synth mode), or flat HSV palette (procedural mode).
                    let b_origin_col: [f32; 4] = if let Some(car) = carrier {
                        let s = sample_bilinear_clamped(car, scx, scy);
                        [s[0], s[1], s[2], 1.0]
                    } else {
                        palette_col
                    };
                    let (base, det_line) = match source {
                        Some(src) => {
                            let s = sample_bilinear_clamped(src, scx + u, scy + v);
                            let (r, g, b) = if hue_shift != 0.0 {
                                hue_rotate(s[0], s[1], s[2], hue_shift)
                            } else {
                                (s[0], s[1], s[2])
                            };
                            let foot = [
                                (r * sh).clamp(0.0, 1.0),
                                (g * sh).clamp(0.0, 1.0),
                                (b * sh).clamp(0.0, 1.0),
                            ];
                            // colorize toward the tile hue, keeping the footage luma
                            let face = if fstr > 0.0 {
                                let luma = 0.2126 * foot[0] + 0.7152 * foot[1] + 0.0722 * foot[2];
                                let (cr, cg, cb) = hsv_to_rgb(face_hue, fsat, luma);
                                [
                                    lerp(foot[0], cr, fstr),
                                    lerp(foot[1], cg, fstr),
                                    lerp(foot[2], cb, fstr),
                                    1.0,
                                ]
                            } else {
                                [foot[0], foot[1], foot[2], 1.0]
                            };
                            // edge-detect: Sobel on footage luma → a line amount that
                            // lights the contour up in the neon edge colour (EDGE_DETECT_GAIN
                            // lifts the small footage gradients into visible lines).
                            let det = if edge_detect > 0.0 {
                                let sx = scx + u;
                                let sy = scy + v;
                                let lat = |dx: f32, dy: f32| {
                                    let p = sample_bilinear_clamped(src, sx + dx, sy + dy);
                                    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
                                };
                                let gx = lat(1.0, 0.0) - lat(-1.0, 0.0);
                                let gy = lat(0.0, 1.0) - lat(0.0, -1.0);
                                let mag = (gx * gx + gy * gy).sqrt();
                                (edge_detect * EDGE_DETECT_GAIN * mag).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            (face, det)
                        }
                        None => (b_origin_col, 0.0),
                    };
                    // both the geometric edge band and the detected footage contours glow
                    // in the same neon edge colour → unified line language on the face.
                    let geo_t = if estr > 0.0 && ew > 0.0 && edge_dist < ew {
                        estr * (1.0 - edge_dist / ew)
                    } else {
                        0.0
                    };
                    let t = geo_t.max(det_line);
                    let color = if t > 0.0 {
                        [
                            lerp(base[0], edge_col.0, t),
                            lerp(base[1], edge_col.1, t),
                            lerp(base[2], edge_col.2, t),
                            1.0,
                        ]
                    } else {
                        base
                    };
                    layer[row + x as usize] = [color[0], color[1], color[2], 1.0];
                }
            }
        }
        // composite this block's layer onto the canvas
        for i in 0..pixels.len() {
            if layer[i][3] > 0.0 {
                let src = [layer[i][0], layer[i][1], layer[i][2]];
                pixels[i] = composite(pixels[i], src, block_blend, block_opacity);
            }
        }
    }

    ImageBufferF32::new(width, height, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [f32; 4]) -> ImageBufferF32 {
        ImageBufferF32::from_fn(w, h, |_, _| c).unwrap()
    }

    #[test]
    fn same_inputs_are_byte_identical() {
        let s = CascadeCollageSettings::default();
        let a = render_cascade_collage_frame(180, 240, None, None, &s, 3).unwrap();
        let b = render_cascade_collage_frame(180, 240, None, None, &s, 3).unwrap();
        assert_eq!(
            a, b,
            "A1: identical (settings, frame) must be byte-identical"
        );
    }

    #[test]
    fn default_composition_leaves_no_background() {
        let s = CascadeCollageSettings::default();
        let out = render_cascade_collage_frame(180, 240, None, None, &s, 0).unwrap();
        let bg = [s.background[0], s.background[1], s.background[2], 1.0];
        let gaps = out.pixels.iter().filter(|p| **p == bg).count();
        assert_eq!(
            gaps, 0,
            "A2: default composition must fully cover (no gaps)"
        );
    }

    #[test]
    fn no_per_frame_drift_is_static() {
        let s = CascadeCollageSettings {
            morph_rate: 0.0,
            frame_hue_rate: 0.0,
            ..Default::default()
        };
        let f0 = render_cascade_collage_frame(160, 200, None, None, &s, 0).unwrap();
        let f9 = render_cascade_collage_frame(160, 200, None, None, &s, 9).unwrap();
        assert_eq!(
            f0, f9,
            "A3: no per-frame drift ⇒ frames identical to frame 0"
        );
    }

    #[test]
    fn scribble_off_differs_from_on() {
        let on = CascadeCollageSettings::default();
        let off = CascadeCollageSettings {
            scrib_amp_scale: 0.0,
            ..Default::default()
        };
        let a = render_cascade_collage_frame(180, 240, None, None, &on, 0).unwrap();
        let b = render_cascade_collage_frame(180, 240, None, None, &off, 0).unwrap();
        let d = a.max_channel_difference(&b).expect("comparable");
        assert!(
            d > 0.0,
            "A4: straight-edge (off) must differ from scribbled (on)"
        );
    }

    #[test]
    fn texture_mode_uses_source_colour() {
        // Solid source + no hue rotation + no brightness oscillation ⇒ every covered
        // pixel is exactly the source colour (tiles carry source texture/colour).
        let src = solid(180, 240, [0.2, 0.6, 0.9, 1.0]);
        let s = CascadeCollageSettings {
            frame_hue_rate: 0.0,
            bright_osc: 0.0,
            edge_strength: 0.0, // no edge tint ⇒ face is pure source colour
            face_strength: 0.0, // no face colorize ⇒ face is pure source colour
            shapes: vec![CascadeShape {
                hue_spread: 0.0,
                ..CascadeCollageSettings::default().shapes[0]
            }],
            ..Default::default()
        };
        let out = render_cascade_collage_frame(0, 0, Some(&src), None, &s, 0).unwrap();
        assert_eq!(out.width, 180, "texture mode output matches source dims");
        // a covered pixel near the tile centre must equal the source colour
        let p = out.pixel(54, 72).unwrap();
        assert_eq!(
            p,
            [0.2, 0.6, 0.9, 1.0],
            "covered pixel carries source colour"
        );
    }

    #[test]
    fn edge_tint_off_differs_from_on() {
        let on = CascadeCollageSettings::default();
        let off = CascadeCollageSettings {
            edge_strength: 0.0,
            ..Default::default()
        };
        let a = render_cascade_collage_frame(180, 240, None, None, &on, 0).unwrap();
        let b = render_cascade_collage_frame(180, 240, None, None, &off, 0).unwrap();
        let d = a.max_channel_difference(&b).expect("comparable");
        assert!(
            d > 0.0,
            "neon edge tint (on) must differ from plain edges (off)"
        );
    }

    #[test]
    fn face_tint_off_differs_from_on() {
        // On a coloured source, colorizing the face toward a different tile hue must
        // change covered pixels vs pure footage.
        let src = solid(160, 200, [0.2, 0.7, 0.3, 1.0]); // green source
        let off = CascadeCollageSettings {
            face_strength: 0.0,
            ..Default::default()
        };
        let on = CascadeCollageSettings {
            face_strength: 0.8,
            ..Default::default()
        };
        let a = render_cascade_collage_frame(0, 0, Some(&src), None, &off, 0).unwrap();
        let b = render_cascade_collage_frame(0, 0, Some(&src), None, &on, 0).unwrap();
        let d = a.max_channel_difference(&b).expect("comparable");
        assert!(
            d > 0.0,
            "face colorize (on) must differ from pure footage (off)"
        );
    }

    #[test]
    fn edge_detect_off_differs_from_on() {
        // Source with a hard vertical edge so Sobel is non-zero; edge-detect must
        // darken those contours (changes covered pixels).
        let src = ImageBufferF32::from_fn(160, 200, |x, _| {
            if x < 80 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [1.0, 1.0, 1.0, 1.0]
            }
        })
        .unwrap();
        let off = CascadeCollageSettings {
            edge_detect: 0.0,
            face_strength: 0.0,
            ..Default::default()
        };
        let on = CascadeCollageSettings {
            edge_detect: 1.0,
            face_strength: 0.0,
            ..Default::default()
        };
        let a = render_cascade_collage_frame(0, 0, Some(&src), None, &off, 0).unwrap();
        let b = render_cascade_collage_frame(0, 0, Some(&src), None, &on, 0).unwrap();
        let d = a.max_channel_difference(&b).expect("comparable");
        assert!(
            d > 0.0,
            "edge-detect (on) must darken footage contours vs off"
        );
    }

    #[test]
    fn block_blend_off_differs_from_on() {
        // Translucent block compositing must change the overlap regions vs hard occlude.
        let off = CascadeCollageSettings {
            block_opacity: 1.0,
            ..Default::default()
        };
        let on = CascadeCollageSettings {
            block_opacity: 0.5,
            ..Default::default()
        };
        let a = render_cascade_collage_frame(180, 240, None, None, &off, 0).unwrap();
        let b = render_cascade_collage_frame(180, 240, None, None, &on, 0).unwrap();
        let d = a.max_channel_difference(&b).expect("comparable");
        assert!(
            d > 0.0,
            "block blend (opacity<1) must differ from hard occlude"
        );
    }

    #[test]
    fn texture_mode_is_deterministic() {
        let src = solid(160, 200, [0.5, 0.25, 0.75, 1.0]);
        let s = CascadeCollageSettings::default();
        let a = render_cascade_collage_frame(0, 0, Some(&src), None, &s, 2).unwrap();
        let b = render_cascade_collage_frame(0, 0, Some(&src), None, &s, 2).unwrap();
        assert_eq!(
            a, b,
            "texture mode must be byte-identical for identical inputs"
        );
    }

    #[test]
    fn b_sampler_differs_from_palette_mode() {
        // B-sampler (carrier Some) must produce different output from palette (None, None).
        let s = CascadeCollageSettings::default();
        // carrier: saturated green — visually very different from HSV palette
        let carrier = solid(180, 240, [0.0, 0.9, 0.1, 1.0]);
        let palette_out = render_cascade_collage_frame(180, 240, None, None, &s, 0).unwrap();
        let b_out = render_cascade_collage_frame(180, 240, None, Some(&carrier), &s, 0).unwrap();
        let d = palette_out
            .max_channel_difference(&b_out)
            .expect("comparable");
        assert!(
            d > 0.01,
            "B-sampler output must differ from palette output (d={d})"
        );
    }

    #[test]
    fn b_sampler_is_deterministic() {
        let s = CascadeCollageSettings::default();
        let carrier = solid(180, 240, [0.4, 0.6, 0.2, 1.0]);
        let a = render_cascade_collage_frame(180, 240, None, Some(&carrier), &s, 0).unwrap();
        let b = render_cascade_collage_frame(180, 240, None, Some(&carrier), &s, 0).unwrap();
        assert_eq!(a, b, "B-sampler must be byte-identical for identical inputs");
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
