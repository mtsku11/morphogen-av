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

- **S1 — CPU field sim + seed + checkpoint.** The Gray-Scott core, frame-zero
  declaration, RGBA32F checkpoint, anchors A2/A3/A4. No composite yet (dump
  the raw field as PNG for eyeballing). Id `morphogenesis_cpu_v1`.
- **S2 — composite + CLI.** `render-morphogenesis-sequence <source-b> <out>`
  with pattern-mix/displace/hue, presets, anchor A1, acceptance 2–3.
- **S3 — coupling.** B→(f,k) param maps; register the five modulation targets
  (checkpoint contract joins here — routed settings enter the fingerprint);
  acceptance 4.
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
