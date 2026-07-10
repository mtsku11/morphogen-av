# Morphogenesis Relief Shading (Track B1) — the 3D look

Status: **IN PROGRESS.** Baseline at slice start: cargo 733/0, swift 152/0,
clippy clean, `cargo fmt --check` dirty on 8 pre-existing files (zero new
diffs allowed). Builds on
[MORPHOGENESIS_FIELD_VIEW_MILESTONE.md](MORPHOGENESIS_FIELD_VIEW_MILESTONE.md)
(a7d9c8e) and the plan in
[MORPHOGENESIS_EXPANSION_HANDOFF.md](MORPHOGENESIS_EXPANSION_HANDOFF.md)
(Track B, B1). Ground rules as ever (see handoff doc — session-limit
finish-inline pattern, 64MiB CLI thread, CPU-only, presets not raw numbers,
real-footage readouts).

## Design (from the handoff, restated precisely)

Treat the V field as a height map and light it with a directional lamp,
per-pixel, deterministically:

```
n  = normalize(-dV/dx * height, -dV/dy * height, 1)      // surface normal
l  = (cos(el)·cos(az), cos(el)·sin(az), sin(el))          // light direction
diffuse  = max(0, n·l)
specular = max(0, reflect(-l, n)·(0,0,1))^shininess
lit = ambient + (1-ambient)·diffuse + spec_strength·specular
```

- Compute ∇V at **sim resolution** (reuse the existing gradient — the
  displace pass already differentiates V), light it there, then **upsample
  the lit result** to carrier resolution (declared: shading-before-upsample
  is smoother than upsample-then-shade; this is the recommendation, pin it).
- **Field view:** output = `V_greyscale * lit` (or a declared blend — the
  agent picks one mixing rule, documents it in code comments only if
  non-obvious, and pins it in a test). The B/W field becomes an embossed
  membrane.
- **Composite view:** `lit` modulates the pattern layer (multiply onto the
  existing `pattern_mix` contribution, or additively per the "closes the
  dark-footage gap" requirement below — the agent must verify growth is
  visible on the near-black cello stage with shade on, not just pick
  whichever is easiest).
- ambient is a fixed constant (not user-exposed) chosen so shade=0 is exact
  passthrough and shade=1 is fully lit-only; pick a reasonable ambient
  (e.g. 0.3) and pin it.

## Knobs

`--shade <0..1>` (blend/strength, default 0 = off, continuity anchor),
`--shade-height <f32>` (gradient→normal scale, default empirically chosen on
the cello field render), `--shade-azimuth <turns, 0..1>` (light rotation),
`--shade-elevation <0..0.25 turns>` (light height above horizon),
`--shade-specular <0..1>` (specular strength), `--shade-shininess <f32>`
(pinned reasonable default, e.g. 16.0, not necessarily user-exposed as a
mod target).

## Mod targets

`shade`, `shade_azimuth`, `shade_height` join
`MORPHOGENESIS_MODULATION_TARGETS` (clamp-never-error, same convention as
existing targets). `shade_azimuth = lfo(saw, 0.1)` is the hero readout — a
light orbiting the pattern.

## Anchors (falsifiable)

- **RS1 (identity):** `--shade 0` (and the flag absent) byte-identical to
  pre-slice renders in both output views. Existing tests stay green.
- **RS2 (azimuth mirror):** rendering the same frame at azimuth `az` and
  `az + 0.5` (180° flip) mirrors which side is highlighted vs shadowed —
  assert via a spot-check (e.g. sum of lit values on left half vs right half
  swaps sign of (left-right) difference, or an equivalent falsifiable
  pixel-region comparison). Proves the lighting math actually rotates.
- **RS3 (dark-footage gap closes):** re-render the ORIGINAL showcase
  composite settings that read as "just a hue change" (hue color mode, no
  displace, near-black cello footage) with `--shade` on. Read the frames.
  Growth must show visible relief structure (highlight/shadow edges) where
  it was previously invisible. This is the falsifiable proof the user's
  original complaint is fixed — not just a numeric delta, Read the PNGs.
- **RS4 (off-vs-on delta + Read):** field view, `shade=0` vs `shade=0.8`,
  frame-delta.py cross-comparison (non-zero) + Read both frames side by
  side.
- **RS5 (checkpoint):** interrupt+resume byte-identical with shade active;
  changing `--shade` (or azimuth/height) on an existing checkpoint dir
  refuses resume; legacy (pre-slice) checkpoints resume fine with shade
  defaulted to 0 (serde-default).
- **RS6 (queue):** add→run byte-identical with `--shade` + azimuth/height
  set; SwiftUI token-sequence tests for the three new knobs; no-op arg
  arrays (shade absent/0) byte-identical to pre-slice arrays.

## Acceptance criteria

RS1–RS6 as tests/smokes; clippy clean; zero new fmt diffs (8 pre-existing
dirty files, confirm count unchanged); baseline cargo 733 / swift 152 → delta
reported; no `unwrap()`. Final deliverable (orchestrator, not the agent): a
6s clip — field view + shade + `shade_azimuth = lfo(saw, 0.1)` on the cello
fixture, audio-muxed — sent via SendUserFile.

## Build plan

Single slice (machinery is model-agnostic and already built): core
gradient→lighting fn in `morphogenesis.rs` + wiring into both output views +
CLI flags + contract field + mod targets + queue field + SwiftUI knobs +
tests + the RS3 dark-footage re-render + the RS4/hero readout.
