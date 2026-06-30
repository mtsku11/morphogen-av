# Cascade Collage Milestone — scribbled-edge tile cascade

## Origin & Goal

Reference look (glitch art, Reddit r/AbstractArt): a **collage of a few large,
mostly-straight-edged tiles** that fold into one another like strata of a single
cliff face. The fine lines in the image are not internal texture and not pixel
sorting — they are the **stacked edges of a tile re-stamped many times** in a small
cascade, each copy morphing very subtly. Some edges are clean and straight; one
edge of each tile is **scribbled** — a complex warbling hand-drawn line. The solid
tile faces are the open, line-free spaces.

No existing effect produces this. `cascade_trails` shares only the word "cascade":
it advects **square** source patches along a vector field into a **persistent
cross-frame accumulator**. This effect is different in geometry (rect/L polygons
with a scribbled edge), motion (a fixed per-step offset, not a flow field), morph
(per-step hue gradient + edge extend/shorten + scribble re-draw), and state
(**stateless** single-frame composite, no checkpoint). It is therefore a **new
effect**, not an extension of `cascade_trails`.

This was reverse-engineered through a Python prototype
(`scripts/`/scratchpad, `make_multicascade.py`) and confirmed by the user before
this contract was written. The prototype is the look reference; this milestone
reproduces the **mechanic** deterministically in the engine.

---

## Mechanic (the cascade)

A frame is composited by stamping a small set of **shapes** onto a canvas
pre-filled with a backstop `background` colour (a coloured floor so any uncovered
gap is obvious, never black). Each shape is stamped **N times in one frame** — the
cascade — with last-writer-wins, so only thin slivers of earlier copies survive at
the edges. Those surviving slivers are the fine lines.

For each shape, for `step` in `0..steps`:

- **Position**: `ox = cx + dx*step`, `oy = cy + dy*step` (a fixed linear offset; the
  cascade direction is chosen *away from* the scribbled edge so that edge stays
  exposed by later copies).
- **Per-step morph** (the "subtle iteration"):
  - **Scribble phase** advances with `step` (and `frame`), so the warbling edge
    slowly re-draws.
  - **Straight notch edge** extends/shortens: `grow = edge_grow * sin(step*k + frame*k)`.
  - **Hue gradient across the cascade**: `hue = base_hue + hue_spread * step/(steps-1)`
    — each stacked copy is a slightly different hue, so the cascade reads as a hue
    ramp (per the user's "each tile in the cascade a different hue").
- **Colour**: HSV(`hue` + global per-frame rotation, `sat`, `val * brightness_osc`)
  → linear RGB. Each shape has its own `base_hue` so the shapes are different
  colours.
- **Rasterize** the shape in tile-local `(u,v) = (x-ox, y-oy)`:
  - `Rect` — 4 straight edges; one edge (`Left|Right|Top|Bottom`) is scribbled:
    that edge's bound is offset by `scribble(t, …)` (t = `v` for L/R edges, `u` for
    T/B edges).
  - `L` — outer box minus a notched corner (6 edges); the notch's vertical edge is
    scribbled (`nu = notch_u + scribble(v)`), its horizontal edge is straight and
    carries the `grow` morph (`nv = notch_v + grow`). `notch_right`/`notch_bottom`
    pick which corner is removed.

`scribble(t, seed, phase, amp)` = `amp * (0.55*sin(t*0.05 + phase*0.7)
+ 0.30*(vnoise1(t*0.14+phase)-0.5)*2 + 0.18*(vnoise1(t*0.45+phase*1.7)-0.5)*2)`
— a slow swing + mid wobble + fine jitter. `vnoise1` is 1-D smoothstep value noise
on a splitmix64 lattice (same hash family as `block_collage`/`fluid_advect`, so the
future MSL port reuses the established `splitmix64` precedent).

Deterministic: stamp order is fixed (shape index, then step), all randomness is
splitmix64 of integer lattice coords. Stateless: a frame depends only on
`(settings, frame)` — no prior-frame state, no checkpoint.

---

## Knobs (`CascadeCollageSettings`)

Global: `background` (RGB floor), `shapes: Vec<CascadeShape>`, `scrib_amp_scale`
(global scribble multiplier; **0 ⇒ all edges straight**), `morph_rate` (per-frame
phase advance; 0 ⇒ frames don't drift), `frame_hue_rate` (per-frame global hue
rotation in turns; 0 ⇒ no per-frame colour change), `bright_osc`
(per-step brightness oscillation amplitude), `seed`.

**Neon edge tint** (the reference's "colour lives in the edges" treatment): the tile
*face* shows footage/palette, but a band of width `edge_width` px along every tile
boundary (straight, scribbled, and L-notch edges) is blended by `edge_strength`
toward a neon colour — per-shape `edge_hue` at global `edge_sat`/`edge_val`. Stacked
by the cascade, the surviving edge slivers become glowing coloured lines.
`edge_strength = 0` ⇒ edges show footage (the off case).

Per shape (`CascadeShape`, all spatial values are **fractions of canvas** unless
noted): `cx, cy` start centre; `hw, hh` half-extents; `kind` (`Rect|L`);
`notch_u, notch_v` notch corner + `notch_right, notch_bottom` (L only); `scrib`
(`Left|Right|Top|Bottom|Notch`); `dx, dy` per-step offset **in pixels**; `steps`;
`base_hue, sat, val`; `scrib_amp` (pixels, pre-`scrib_amp_scale`); `hue_spread`
(turns across the cascade); `edge_grow` (notch grow amplitude, fraction of `hh`).

`Default` is the validated 4-shape quadrant composition: magenta L (top-left),
orange rect (top-right), teal rect (bottom-left), purple L (bottom-right), each
cascading outward toward its corner with its scribbled edge facing centre.

---

## Acceptance Criteria

- **A1 Determinism** — identical `(settings, frame)` ⇒ byte-identical output. *(test)*
- **A2 Full coverage** — the default composition produces **zero** pixels equal to
  `background` (no black/gap). This is the "fill the screen" criterion as a unit
  test. *(test)*
- **A3 Static identity** — all per-frame drift (scribble phase, edge grow,
  brightness oscillation, hue rotation) is routed through `morph_rate` /
  `frame_hue_rate`, so with `morph_rate=0` and `frame_hue_rate=0` every frame is
  byte-identical to frame 0 (the cascade is fixed). *(test)*
- **A4 Scribble off-vs-on** — `scrib_amp_scale=0` (straight edges) vs default
  (scribbled) differ; cross-frame delta > 0, scribble visible on Read. *(visual + test)*
- **A5 Morph drift** — with `morph_rate>0`, frame K differs from frame 0;
  `frame-delta.py` > 0. *(visual)*
- **A6 Per-tile / per-step hue** — each shape a distinct base hue; within a cascade
  the hue ramps by `hue_spread`. *(visual)*
- **A7 Library hygiene** — no `unwrap()` in lib; errors via
  `RenderError::InvalidCascadeCollageSettings`. *(review)*
- **A8 CPU = ground truth** — Metal kernel (later slice) gated frame-by-frame
  within `METAL_CPU_PARITY_EPSILON`. *(later)*

---

## Build Order (slices)

1. **CPU reference + tests** (DONE) — `cascade_collage.rs`, lib export, error
   variant, A1–A4 tests.
2. **CLI readout** (DONE) — `render-cascade-collage-sequence` (source-less
   generator: `--width --height --frames` + knobs) so off-vs-on (A4) and morph drift
   (A5) can be Read.
3. **Source texture** (DONE) — tiles carry a **source-video crop** centred on each
   shape's home position (texture + colour from the video), re-sampled per frame so
   the video plays through. `render_cascade_collage_frame` takes `source:
   Option<&ImageBufferF32>`; `--source-dir` enables it (output = source dims). Palette
   stays the no-source fallback. Optional `hue_spread`/`frame_hue_rate` rotate the
   sampled hue; `bright_osc` shades. Verified: solid-source covered pixel == source
   colour; 0 background; determinism (texture tests).
4. **Face look-dev + high-level params** (DONE) — face hue-tint, quantized per-copy
   hue variation, rectilinear multi-notch tiles (T/U/plus/staircase), edge-detect that
   exposes footage contours as neon lines, and CLI knobs `--tile-scale` (size),
   `--detail-tiles` (amount), `--hue-rotate` (colour) over the default composition.
5. **Metal kernel** — **DECIDED CPU-ONLY** (user, 2026-06-30). The effect is a
   per-pixel gather over ~400 stamps (8 tiles × ~50 copies), each needing full
   scribble/notch/colour eval: O(w·h·stamps) is likely **slower** than the
   bbox-limited CPU (already ~0.2 s/frame @640×360), and the `sin`-based scribbled hard
   edges make boundary pixels flip on sub-ULP CPU↔MSL `sin` differences, tripping the
   strict 1/255 frame gate (workable only via fragile validate-then-trust, or by
   replacing scribble's `sin` with hash-only noise = a look change). Determinism (the
   #1 invariant) is CPU-only regardless. So this effect stays CPU, like `fluid_mosaic`.
6. **Queue + SwiftUI** (next) — `RenderJobTask` variant, queue add/run.
7. **A→B morph drive** — drive `morph_rate`/`scrib_amp` from **Source A** analysis
   (luma/flow) so A modulates B's cascade. (Single-source B texture is slice 3; this
   adds the A side.)

---

## Non-goals (this milestone)

- No persistent cross-frame accumulator (that is `cascade_trails`' identity).
- No rotation of shapes (axis-aligned rect/L only) — the look does not need it.
- No A→B sourcing yet (slice 5); the MVP is a deterministic procedural generator.
