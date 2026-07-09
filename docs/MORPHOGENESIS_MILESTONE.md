# Morphogenesis — Reaction-Diffusion Engine (the wild one)

Status: **PLANNED — handoff for a future build session.** Written 2026-07-05 at
the user's request ("plan a build for a wild, experimental feature"). Nothing
implemented. The [`DEFERRED_WORK_HANDOFF.md`](DEFERRED_WORK_HANDOFF.md) ground
rules apply (baseline first, CPU-first, pixels+number verification,
`/checkpoint` per slice).

## Why this one

The app is called **Morphogen** and has no morphogenesis in it. Reaction-
diffusion (Gray-Scott) is *the* morphogenesis algorithm — Turing's chemical
model of how spots, stripes, coral, and dividing-cell patterns self-organize.
As an effect: the footage grows a **living skin** — patterns nucleate on the
carrier, crawl, split, and heal, steered in real time by the modulator's audio.
Nothing else in the catalog generates *emergent structure over time*; every
existing effect transforms what's there. This one grows something new.

It is also a perfect invariant citizen: a **stateful temporal node** exactly
like flow_feedback/datamosh — f32 field state, declared frame-zero seeding,
unquantized RGBA32F checkpoint, fixed-order gather-only stencil math ⇒
deterministic by construction.

## Mechanic

Two chemical fields U, V (f32 grids) at a sim resolution (`--sim-scale`,
default half the frame; bilinear upsample at composite). Per output frame, run
`--substeps` (default 12) of Gray-Scott with a 5-point Laplacian:

```
dU = Du·∇²U − U·V² + f·(1 − U)
dV = Dv·∇²V + U·V² − (f + k)·V
```

fixed raster order, **clamped edges** (declared; not toroidal — footage has a
frame), U,V clamped to [0,1] after every substep (Gray-Scott is stiff; the
clamp is part of the algorithm, not a safety net — declare it in the id).
Defaults from the known-alive band: `Du=0.16, Dv=0.08, f=0.037, k=0.060, dt=1.0`
(coral growth). Most of (f,k) space is dead (uniform grey) — this is the #1
look trap; ship named presets (`coral`, `mitosis` f=0.0367/k=0.0649, `worms`,
`spots`) rather than making the user prospect raw numbers.

**Footage coupling (what makes it cross-synthesis, not a screensaver):**

- **Seed (frame zero, declared):** V seeded where B frame 0's luma crosses
  `--seed-threshold`, plus splitmix64 speckle at a fixed seed knob; U starts
  at 1. Patterns therefore *nucleate on the subject*.
- **B → parameter maps (per frame):** B's luma shifts (f,k) locally along a
  declared line segment in parameter space, scaled by `--param-map-strength`.
  Bright and dark regions literally grow *different pattern species*.
- **A → global steering via the existing mod matrix:** register `feed`,
  `kill`, `param_map_strength`, `pattern_mix`, `displace` as modulation
  targets (clamp-never-error, stateful → checkpoint contract). Then
  `feed = audio-rms @smooth` makes the chemistry breathe with the soundtrack —
  no bespoke audio code at all.

**Composite (out):** `out = B` reshaped by the V field two ways, both with
strength knobs: `--pattern-mix` colourizes V into the frame (V→luma-preserving
tint toward `--pattern-hue`, or `inherit` = local B colour so growths take the
footage's own palette), and `--displace` pushes B's pixels along ∇V
(chemotaxis smear — the footage gets *eaten along the growth fronts*).

## Off / identity anchors

- **A1 (passthrough):** `--pattern-mix 0 --displace 0` ⇒ output byte-identical
  to B, regardless of what the field is doing underneath.
- **A2 (frozen field):** `--substeps 0` ⇒ the field stays exactly the frame-zero
  seed for every frame (composite may still show it; the *field* is constant —
  assert on the checkpoint bytes).
- **A3 (dead chemistry):** `--param-map-strength 0` with a dead (f,k) pair ⇒
  field converges to uniform; composite delta → 0 over time (falsifiable decay
  test).
- **A4 (resume):** interrupt mid-render, resume from the RGBA32F checkpoint
  (U,V in R,G; B,A spare) ⇒ byte-identical to an uninterrupted run. Changed
  settings/algorithm id/inputs refuse (stale-checkpoint invariant).

## Acceptance criteria

1. Anchors A1–A4 as automated tests; determinism smoke test (two fresh runs
   byte-identical).
2. **Aliveness is proven, not asserted:** unit test that the default preset's
   field variance grows from seed and stays in a nontrivial band over 60
   frames (dead-parameter trap made falsifiable).
3. Off-vs-on render on a real-footage fixture with `--variation`-free defaults:
   Read frames at t=0/mid/end + `frame-delta.py` numbers — the on-case delta
   must show sustained temporal evolution (patterns move), not a static
   overlay.
4. Mod-matrix route (`feed = lfo(sine, 0.1)`) renders and visibly pulses the
   growth (within-on delta + Read frames; sparse-structure deltas may be
   non-monotonic — the rutt-etra precedent — so pair number with pixels).
5. No `unwrap()`; `thiserror`; baseline counts + delta per slice.

## Build plan (slices — /checkpoint each)

- **S1 — CPU field sim + seed + checkpoint. DONE (2026-07-08,** Sonnet build,
  orchestrator-verified: cargo 664 → **674/0**, clippy clean, zero new fmt
  diffs, no `unwrap()`. Anchors A2/A3/A4 + determinism + per-preset aliveness
  variance band + seeding rules all pinned; checkpoint reuses
  `feedback_state.rs`'s RGBA32F codec verbatim (declared DRY deviation — the
  JSON contract discriminates staleness) with feedback's refusal wording.
  Debug scaffold `render-morphogenesis-field` (S2's composite command
  supersedes/absorbs it) writes V-field PNGs + manifest + checkpoint. Visual:
  coral on a wide-ring radial carrier — seed rings bead into nodules (t=15),
  elongate into tendrils (t=30), full labyrinthine coral by t=59; field
  frame-delta **2.488/255** sustained over 60 frames. Preset values pinned:
  mitosis f=.0367/k=.0649 (contract), spots f=.030/k=.062 (atlas), worms
  f=.062/k=.061 (the common .058/.065 is alive but too slow for the 60-frame
  window — empirically tuned). **Trap recorded in memory
  (`morphogenesis-reaction-diffusion`):** seed geometry has a critical size
  independent of (f,k) — thin seeds at half-res sim decay to V=0 by ~frame 15
  and mimic dead chemistry; check seed feature width in sim-pixels before
  blaming the parameters. Speckle density 0.2% (1% drowned the aliveness
  metric).)
- **S2 — composite + CLI. DONE (2026-07-08,** Sonnet build,
  orchestrator-verified: cargo 674 → **683/0**, clippy clean, zero new fmt
  diffs. `render-morphogenesis-sequence` with `--pattern-mix` (default 0.85) /
  `--displace` / `--pattern-hue` + `--pattern-color-mode hue|inherit` (inherit
  = growth takes the local B colour); displace gathers at `x − displace·∇V`
  (central differences at sim res, bilinear upsample). Composite knobs JOIN
  the checkpoint contract (composite reads B every frame ⇒ changed knobs or
  any changed carrier frame refuses resume; whole-dir source fingerprint).
  A1 pinned at unit + CLI level, and re-verified live on real footage at the
  pixel level (ffmpeg rgb24 md5 identical — raw byte cmp is the known
  RGB↔RGBA encoding trap). Acceptance 3 on cello.mp4: off≡source; on = coral
  nucleating on the cellist's face/hands at t=0 → travelling-front stipple
  through the shirt at t=29 → full labyrinthine coral carpet across the hall
  floor at t=59; off-vs-on cross-delta sustained 2.9–5.6/255 across the run
  (non-monotonic per the rutt-etra precedent — the mid-run dip is the seed
  patches thinning into fronts). `render-morphogenesis-field` kept as the
  raw-field debug view.)
- **S3 — coupling. DONE (2026-07-09,** Sonnet build, orchestrator-verified:
  cargo 683 → **689/0**, clippy clean, zero new fmt diffs. B→(f,k) param map:
  a declared line segment centered EXACTLY on `settings`'s own `(feed, kill)`
  (`local_feed_kill`) — `param_map_strength == 0` delegates to the plain
  `advance_morphogenesis_frame` verbatim, so the continuity anchor is
  byte-identical by construction, not float coincidence. `feed`/`kill`
  registered on `MorphogenesisSettings`, `param_map_strength` alongside them;
  `pattern_mix`/`displace` stay on `MorphogenesisCompositeSettings` — one
  `apply_morphogenesis_modulation(settings, composite, target, value)` threads
  both. `--frame-rate` added to `render-morphogenesis-sequence` (the
  flow-feedback precedent: one timeline per stateful render, envelopes sample
  against it) plus the standard `--modulate`/`--modulator-*`/named-modulator
  args. Routes join `MorphogenesisSequenceContract` via `FeedbackModulationContract`
  reused verbatim (it's generic over which effect's routes it carries); both
  it and `MorphogenesisSettings.param_map_strength` are `#[serde(default)]` so
  pre-S3 checkpoints deserialize unmodulated and stay resumable — proven by a
  legacy-checkpoint resume test (mirrors
  `render_feedback_sequence_lfo_route_joins_checkpoint_contract`'s shape).
  **Trap found and fixed:** the first segment (opposite-sign deltas,
  `feed +0.05 / kill −0.02`) passed every unit test (uniform synthetic
  carriers) but silently killed the WHOLE field on real footage — a
  mostly-dark carrier pushes its majority of cells into a truly-dead
  `(feed, kill)` pair, and Gray-Scott's diffusion drags the small alive
  region down with it over 60 frames; a uniform-luma unit test can't see this
  because there's no dead majority to diffuse from. Fixed by making
  `feed`/`kill` shift with the SAME sign and empirically probing candidate
  segments with `render-morphogenesis-field` (bare aliveness, no compositing)
  before picking `feed +0.014 / kill +0.008`: both endpoints
  (`feed≈0.044,kill≈0.064` bright; `feed≈0.030,kill≈0.056` dark) stay alive at
  `param_map_strength == 1.0` (the visible-by-default value) on the cello
  footage. Acceptance 4 on the same cello frames: `feed = lfo(sine,0.1):0.02,0.03`
  at `--frame-rate 6` (60 frames = one full 10s LFO period) visibly pulses —
  dense floor/shirt growth expands into large blobs at the feed peak (t=5s,
  frame 30) and recedes to sparse speckle at the troughs (frame 0/59);
  within-on frame-delta 3.476/255 sustained, off-vs-on cross-delta grows
  0 (frame 0, shared seed) → 0.685 → 2.003 → 2.115 → 3.754/255 (frames
  15/30/45/59) as the knob history accumulates (non-monotonic, the rutt-etra
  precedent). The param-map on/off pair (same cello frames, coral defaults)
  shows bright (shirt) regions growing a finer stipple texture vs off's
  rounder blobs, and dark (floor) regions growing branchier/stringier shapes
  vs off's blobs — visibly different species; cross-delta grows 0 → 1.130 →
  1.859 → 2.074 → 3.132/255 (frames 0/15/30/45/59).
- **S4 — queue + SwiftUI panel** (established patterns; add→run
  byte-identical).
- **S5 (deferred-by-default) — Metal port.** Stencil gathers are
  parity-friendly, but 12 substeps/frame compounds sub-epsilon drift on a
  stateful node (the datamosh finding — per-frame parity can pass while
  CPU≠Metal bytes diverge; don't "fix" it). Gate per frame on the *composite*,
  expect non-byte-identical field state, and only build if CPU speed actually
  hurts (half-res sim may make it moot).

## Deferred (out of scope, listed so nobody scope-creeps)

- **Physarum/slime-mold agent tier** (the even wilder sibling — transport
  networks eating the footage; agent sims need a fixed-order deposit pass to
  stay deterministic; own contract when wanted).
- Multi-species RD (3+ chemicals, competing pattern ecologies).
- RD as a chain stage / composition scene (natural once
  [COMPOSITION_MILESTONE.md](COMPOSITION_MILESTONE.md) lands — a piece whose
  final movement *grows over* everything before it).
- Depth- or flow-seeded nucleation.
