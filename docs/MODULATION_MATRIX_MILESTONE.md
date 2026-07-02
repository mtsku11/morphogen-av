# Modulation Matrix Milestone — typed analysis signals routed onto effect knobs

## Goal

The long-term target (CLAUDE.md) is an audiovisual modular synthesizer where
**typed analysis signals modulate visual/audio params**. Today every route is a
bespoke effect pair (RMS→displacement, luma→gain/pan/filter), and each new pair
costs its own CLI command, queue task, and SwiftUI section. This milestone
builds the generic layer: a **modulation route** binds one analysis descriptor
(the modulator signal) to one numeric effect knob (the target), with an affine
mapping and deterministic time resampling. N sources × M knobs stop being N×M
bespoke features.

A route is the modular-synth patch cable. The effect keeps its algorithm and
identifier; what changes is that a knob's value becomes a per-frame function of
the modulator instead of a constant.

---

## Model

```
descriptor extraction        mapping                 application
(WAV or PNG frames)  ──►  value·scale + offset  ──►  settings.<knob> = clamp(range, v)
     one envelope           per route                per frame, before render
 (time, value) samples
```

- **Envelope**: a sparse `(time_seconds, value)` series produced once per
  source, deterministically, from the modulator media. Values are normalized to
  `[0, 1]` per the conventions below.
- **Route**: `{ target knob, source, scale, offset }`. The mapped value is
  `envelope(t) * scale + offset`, clamped to the target's declared range.
- **Sampling**: the envelope is evaluated at each output frame's time
  (`frame_index / frame_rate`) with `hold` (step, default) or `smooth` (linear
  interpolation) — the same two modes as the video-audio route's
  `DescriptorEnvelope::resample`, applied per frame instead of per audio sample.
- **Application**: the effect's settings struct is copied, routed knobs are
  overwritten, and the ordinary per-frame render function runs. The effect code
  itself is untouched and unaware of modulation.

## Descriptor sources (slice 1)

| Source name     | Media               | Extraction                                        | Normalization |
|-----------------|---------------------|---------------------------------------------------|---------------|
| `audio-rms`     | `--modulator-audio` | `rms_envelope` (window 2048, hop 512)             | peak-normalized (**relative**) |
| `audio-onset`   | `--modulator-audio` | `stft_magnitude_cache` → `onset_strength_from_stft` (fft 2048, hop 512, Hann) | peak-normalized (**relative**) |
| `audio-centroid`| `--modulator-audio` | per-STFT-frame `spectral_centroid_from_magnitudes` | / Nyquist (**absolute**) |
| `luma`          | `--modulator-frames`| per-frame mean Rec.709 luma (`build_luma_samples`) | already `[0,1]` (**absolute**) |
| `flow`          | `--modulator-frames`| mean temporal Lucas-Kanade magnitude (`build_flow_magnitude_samples`; frame 0 = 0) | peak-normalized (**relative**) |

**Relative-normalization trap** (established by the video-audio route): a
peak-normalized envelope always spans up to 1.0 regardless of absolute level —
a quiet clip modulates as hard as a loud one, and a silent/static modulator
(peak ≤ 0) yields an all-zero envelope, not an error. Readout fixtures must
span quiet→loud / still→moving.

## Route grammar (CLI)

```
--modulate "<target>=<source>[:<scale>[,<offset>]]"     (repeatable)
--modulator-audio <wav>        required iff any audio-* route is present
--modulator-frames <dir>       required iff any luma/flow route is present
--modulation-sampling hold|smooth   (default hold, applies to all routes)
```

Examples:

```
--modulate "strength=audio-rms"                  # retro-static breathes with loudness
--modulate "threshold_high=audio-onset:0.6,0.3"  # sort bursts on onsets
--modulate "shift_r_x=flow:24" --modulate "shift_b_x=flow:-24"   # RGB split tracks motion
```

- `scale` defaults to `1`, `offset` to `0`; both accept negatives.
- Two routes to the **same target** are a hard error (ambiguous intent), raised
  before any frame renders.
- An unknown target name for the effect, an unknown source name, or a route
  whose required modulator flag is missing are hard errors before rendering.

## Targets (slice 1 — stateless effects, float knobs only)

| Effect command | Target keys | Clamp range |
|---|---|---|
| `render-retro-static-sequence` | `strength` | `[0, 1]` |
| `render-pixel-sort-sequence` | `threshold_low`, `threshold_high` | `[0, 1]` |
| `render-channel-shift-sequence` | `shift_r_x`, `shift_r_y`, `shift_g_x`, `shift_g_y`, `shift_b_x`, `shift_b_y` | `[-4096, 4096]` |

Clamping (not erroring) is deliberate: an envelope must never abort a render
mid-sequence. `settings.validate()` still runs on the post-clamp value each
frame. If modulation drives pixel-sort's `threshold_low` above `threshold_high`,
that frame is the effect's own documented passthrough case — not an error.

Deferred target classes: integer knobs (palette-quantize `levels` — needs a
contracted rounding rule), enum knobs, and **stateful effects** (datamosh,
feedback, fluid advect — per-frame knob changes alter state evolution, so the
route config must join the checkpoint-invalidation contract first).

## Determinism & continuity

- Identical modulator media + routes + settings ⇒ identical envelopes ⇒
  identical per-frame settings ⇒ bit-reproducible output. No RNG anywhere.
- **Off case:** zero `--modulate` flags takes the exact pre-existing code path —
  byte-identical to a render before this milestone existed.
- **Continuity identity:** `scale 0` pins the knob to `clamp(offset)` — a
  constant-knob render, byte-identical to passing that constant directly.
- Effect algorithm identifiers are **unchanged** (the per-frame math is the
  same function of (frame, settings)); what a modulated render must record is
  the route set. Direct CLI prints the resolved routes; manifest persistence is
  the queue slice.

## Relationship to the core node-graph `ModulationRoute`

`morphogen-core/src/graph.rs` already defines a schema-level `ModulationRoute`
(`from_node/from_output → to_node/to_parameter`, `amount`) as part of the
project node graph — a data model with **no execution engine yet**. This
milestone's `morphogen_render::modulation::ModulationRoute` is the flat,
executable form: one implicit modulator node, one implicit effect node,
`to_parameter` = the target knob, `amount` generalized to `scale/offset`. When
the queue slice lands, the persisted form should either reuse or explicitly
mirror the core type so the graph model stays the single long-term home
(today's flat route is its degenerate two-node case).

## Slices

1. **CPU + CLI (this slice).** `modulation.rs` in `morphogen-render` (source
   enum, route struct, envelope sampling, per-effect target registry + apply),
   unit tests, `--modulate` wiring on the three commands above, off-vs-on
   readout. Core serde mirrors are *not* added yet (house precedent: core
   mirrors land with the queue slice).
2. **Queue — LANDED.** Routes persist on `frame_sequence_retro_static` and
   `frame_sequence_pixel_sort` as `modulation_routes` (a flat
   `RenderJobModulationRoute` in core, serde-default empty so pre-slice jobs
   keep their meaning) plus `modulator_audio_path` / `modulator_frames_directory`
   / `modulation_sampling`; envelope times sample against the job's
   `frame_rate` (no separate fps knob on the queue path).
   `queue-add-…` gains the same `--modulate` flags and **fails fast** — route
   grammar, duplicate/unknown targets, and missing modulator flags are all
   rejected before the job persists. `queue-run` reconstructs the CLI route
   specs from the persisted routes (`f32` `Display` round-trips exactly) so it
   shares the direct render's code path byte-for-byte; the manifest gains a
   `modulation` block (routes, modulator paths, sampling, fps) **only when
   routes exist**, so unmodulated manifests keep the pre-slice format.
   Verified: add→run byte-identical to the direct modulated render + manifest
   assertions (smoke), pre-slice job JSON deserializes unmodulated (core test).
   *Channel-shift has no queue task at all yet — adding one (with routes) is
   its own vertical slice, not part of route persistence.*
3. **SwiftUI.** Route editor on the render panel (target picker filtered by
   effect, source picker, scale/offset steppers).
4. **Later:** integer/enum targets, stateful-effect targets, per-route sampling,
   envelope caching as analysis sidecars, multiple modulators per render.

## Acceptance criteria (slice 1)

- Unit tests: hold vs smooth sampling; clamp at both range ends; unknown
  target/source and duplicate-target errors; empty route list is a no-op;
  deterministic double-run equality.
- Off-vs-on readout on a fixture (moving frames + level-ramping WAV, rendered
  `--modulate` off vs on): off is byte-identical to pre-milestone output; on
  shows the knob tracking the envelope with a growing/ebbing
  `frame-delta.py`/cross-delta number **and** Read frames confirming the look.
- `cargo test --workspace` green from the 431 baseline (+ new tests), clippy
  clean, `swift test` untouched at 64.
