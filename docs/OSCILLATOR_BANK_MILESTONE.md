# Oscillator Bank Milestone — deterministic video oscillators (source-less generators)

Tier 5.2 of `docs/DEFERRED_WORK_HANDOFF.md`. Contract written 2026-07-07.

## Origin & Goal

A synth has oscillators; today the app makes users bring footage. This milestone
adds `generate-frames <preset> <output_dir>` — a bank of deterministic pattern
oscillators writing **ordinary PNG frame dirs**, so every existing effect, mod
route, queue job, and chain stage consumes the output unchanged. An oscillator
driving Rutt-Etra displacement or feeding fluid advect IS classic video
synthesis; the app becomes playable with zero footage.

Precedent: `cascade_collage.rs` is the proven source-less generator — splitmix64
hashing, continuity-identity discipline (all drift through explicit rate knobs,
rate 0 ⇒ frame 0 forever, pinned by test), stateless `(settings, frame) → image`.

## Non-goals (MVP)

- **No Metal.** Generation is cheap; CPU determinism is the ground truth anyway.
- **No queue task / SwiftUI panel.** A later slice puts presets in the source
  panel (generate-to-proxy-dir behind the scenes). Not this milestone.
- **No modulation targets.** The generators have rate/phase/scale knobs but are
  not `--modulate` targets in the MVP (a generator is an upstream *source*; the
  matrix modulates *effects*). Revisit only if a real patch demands it.

## Mechanic

New module `crates/morphogen-render/src/generators.rs`. One pure function per
preset: `(settings, frame_index) → ImageBufferF32` (RGBA, alpha 1.0). Stateless:
a frame depends only on `(settings, frame)`; no prior-frame state, no checkpoint.

**The phase law (the core invariant).** Every preset's animation is driven by a
single scalar phase computed in **f64**:

```
phase(frame) = phase0 + rate * frame as f64
```

No accumulation, no per-frame increment — recompute-from-index, the same
no-drift-by-construction rule as the preview loop. This makes the **phase-drift
anchor** hold exactly (below).

All hash randomness (plasma noise lattice) is splitmix64 over integer lattice
coordinates seeded by `seed`, same family as `block_collage`/`fluid_advect`/
`cascade_collage`.

## Presets (4)

All presets render in linear RGB into `ImageBufferF32`; pixel centre sampling at
`(x + 0.5, y + 0.5)`; f64 math for the pattern functions, converted to f32 at
store time. Exact formulas below are contract — pin them with value tests.

1. **`scan-bars`** — vertical bars scrolling horizontally.
   `v = 0.5 + 0.5 * sin(TAU * (x_norm * scale + phase))` where
   `x_norm = (x + 0.5) / width`. Greyscale (`r = g = b = v`). `scale` = bar
   count across the frame; `rate` = bars scrolled per frame.

2. **`radial`** — concentric rings breathing outward from centre.
   `d = hypot(px - cx, py - cy) / (min(w, h) / 2)` (pixel-centre coords, centre
   at `(w/2, h/2)`), `v = 0.5 + 0.5 * sin(TAU * (d * scale - phase))`.
   Greyscale. Positive rate ⇒ rings travel outward.

3. **`plasma`** — classic two-layer drifting interference + hash-noise shimmer:
   ```
   n  = vnoise2(x_norm * scale * 4 + phase * 0.7, y_norm * scale * 4 - phase * 0.9)   // [0,1]
   v  = ( sin(TAU * (x_norm * scale + phase))
        + sin(TAU * (y_norm * scale * 0.83 - phase * 1.13))
        + sin(TAU * (d_center * scale * 1.31 + phase * 0.57))
        + (2.0 * n - 1.0) * 0.8 ) / 3.8 * 0.5 + 0.5, clamped to [0,1]
   ```
   where `vnoise2` is 2-D smoothstep value noise on a splitmix64 lattice
   (`seed`-keyed), `d_center` as in `radial`. Colour: hue = `v` as HSV turns
   (sat 0.7, val = `0.35 + 0.65 * v`) → linear RGB, reusing the existing HSV
   helper convention from cascade_collage.

4. **`gradient`** — a linear gradient sweeping its angle.
   `angle = TAU * phase`, `t = 0.5 + ((x_norm - 0.5) * cos(angle) + (y_norm - 0.5) * sin(angle))`,
   clamped [0,1]; greyscale `v = t`. `scale` is accepted but unused (uniform
   knob surface); document it. `rate` = revolutions per frame.

Preset choice rationale: scan-bars/radial are the classic video-synth test
signals (and ideal Rutt-Etra drivers); plasma exercises the hash-noise path;
gradient is the cheapest "spatial ramp" modulator for luma routes.

## CLI

```
generate-frames <preset> <output_dir>
  --width u32   (default 640)
  --height u32  (default 360)
  --frames u32  (default 48)
  --rate f32    (default 0.02)   # phase advance per frame
  --phase f32   (default 0.0)    # phase0
  --scale f32   (default 4.0)    # spatial frequency / pattern density
  --seed u64    (default 71)     # plasma noise lattice key (others ignore it; document)
```

Preset is a `ValueEnum` (`scan-bars|radial|plasma|gradient`). Validation:
width/height ≥ 1, frames ≥ 1, rate/phase/scale finite (`validate()` on the
settings struct, `RenderError`/`CliError` — **no `unwrap()`**). Frames written
as `frame_00000.png` etc., matching the existing sequence writers.

**Manifest is required** (the rutt-etra audit trap: do NOT mirror
palette-quantize's stdout-only convention): `manifest.json` in the output dir
with `algorithm`, `preset`, and every knob (width/height/frames/rate/phase/
scale/seed). Algorithm id: **one per preset** —
`oscillator_scan_bars_cpu_v1`, `oscillator_radial_cpu_v1`,
`oscillator_plasma_cpu_v1`, `oscillator_gradient_cpu_v1` (a preset IS the
algorithm; changing formulas bumps that preset's id).

## Anchors (falsifiable, each pinned by test)

1. **Rate-0 identity:** `--rate 0` ⇒ every frame byte-identical to frame 0
   (cascade-collage discipline).
2. **Phase-drift equivalence:** frame `k` rendered with `(rate r, phase p)` is
   byte-identical to frame `0` rendered with `(rate r, phase p + r*k)` — holds
   exactly because of the f64 phase law. Pin for at least scan-bars and plasma
   (worst case: the noise path). Note: compute `p + r*k` in f64 the same way
   the phase law does, passing it through the f32 knob would break exactness —
   the test constructs the equivalent phase via the same f64 expression.
3. **Two-run determinism:** identical invocation twice ⇒ byte-identical dirs.
4. **Seed sensitivity (plasma only):** different `--seed` ⇒ different frames;
   same seed ⇒ identical.
5. **Feeds the engine:** a generated dir is a valid input — smoke test renders
   `generate-frames scan-bars` then feeds it to `render-rutt-etra-sequence`
   and asserts frames render (count + dims), proving "just a frame dir".

## Acceptance criteria

- `cargo test --workspace` green; new unit tests for the anchors above +
  formula value pins (a handful of exact pixel values per preset at known
  (x, y, frame) — guards accidental formula drift).
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt` clean.
- **Visual proof:** render all 4 presets (small, e.g. 320×180×12 default knobs),
  Read at least 2 frames per preset and describe what is seen (bars/rings/
  plasma cells/gradient at the expected orientation and motion);
  `scripts/frame-delta.py` per preset: rate 0 ⇒ 0.000, default rate ⇒ a
  nonzero number, reported.
- Manifest present + fields pinned by test.
- No `unwrap()` outside tests; errors via existing error enums.

## Slices

Single slice (S1): module + 4 presets + CLI + manifest + anchors + visual
proof. Small by design; later exposure (SwiftUI source-panel picker, queue) is
deferred to its own slice when the user asks.
