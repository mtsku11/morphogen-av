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
--modulate "<target>=<source>[:<scale>[,<offset>]][@hold|@smooth]"   (repeatable)
--modulator-audio <wav>        required iff any audio-* route is present
--modulator-frames <dir>       required iff any luma/flow route is present
--modulation-sampling hold|smooth   (default hold; the per-render default)
```

Examples:

```
--modulate "strength=audio-rms"                  # retro-static breathes with loudness
--modulate "threshold_high=audio-onset:0.6,0.3"  # sort bursts on onsets
--modulate "shift_r_x=flow:24" --modulate "shift_b_x=flow:-24"   # RGB split tracks motion
--modulate "feedback_mix=audio-rms:0.75@smooth"  # this route interpolates; others hold
```

- `scale` defaults to `1`, `offset` to `0`; both accept negatives.
- The optional terminal `@hold`/`@smooth` overrides `--modulation-sampling`
  for that route only; no suffix inherits the render-level default (the exact
  pre-suffix behaviour). The suffix is part of the route, so it persists on
  queue jobs, round-trips through queue-run, and joins a stateful effect's
  checkpoint contract like every other route field. An unknown suffix is a
  hard error before rendering.
- Two routes to the **same target** are a hard error (ambiguous intent), raised
  before any frame renders.
- An unknown target name for the effect, an unknown source name, or a route
  whose required modulator flag is missing are hard errors before rendering.
- **Readout trap:** hold and smooth are byte-identical when the output frame
  times land exactly on the envelope's sample grid (e.g. fps 4 against the
  62.5 ms RMS hop at 8192 Hz — 0.25 s is a multiple). Prove `@smooth` with a
  frame rate that does not divide the hop grid.

### Envelope sidecars (`--modulation-cache-dir`)

Extracted **luma/flow** envelopes are per-frame analysis over the whole
modulator clip (flow runs Lucas-Kanade per consecutive pair — the dominant
cost); audio envelopes are cheap and not cached. `--modulation-cache-dir <dir>`
(direct CLI, every modulatable command) persists each extracted envelope as a
reusable sidecar (`envelope_luma.json` / `envelope_flow.json`) carrying the
algorithm id (`modulation_envelope_{luma,flow}_v1`), the sampling convention
(fps + sample count), and the modulator-frames content fingerprint. A sidecar
is reused only on a full algorithm/fps/fingerprint match; any mismatch
regenerates and overwrites it. `serde_json` round-trips finite floats exactly,
so a cache hit is byte-identical to a fresh extraction. Like the flow cache,
the sidecar never joins a render's contract — it only skips recomputation —
so resume/checkpoint semantics are unaffected. Queue jobs render uncached for
now.

## Targets (stateless effects)

| Effect command | Target keys | Clamp range |
|---|---|---|
| `render-retro-static-sequence` | `strength` | `[0, 1]` |
| `render-pixel-sort-sequence` | `threshold_low`, `threshold_high` | `[0, 1]` |
| `render-channel-shift-sequence` | `shift_r_x`, `shift_r_y`, `shift_g_x`, `shift_g_y`, `shift_b_x`, `shift_b_y` | `[-4096, 4096]` |
| `render-palette-quantize-sequence` | `levels` (integer) | `[2, 256]`, then rounded |

Clamping (not erroring) is deliberate: an envelope must never abort a render
mid-sequence. `settings.validate()` still runs on the post-clamp value each
frame. If modulation drives pixel-sort's `threshold_low` above `threshold_high`,
that frame is the effect's own documented passthrough case — not an error.

### Integer targets — the contracted rounding rule

An integer knob applies the same affine mapping and clamp as a float knob,
then converts with **round to nearest, ties away from zero** (`f32::round`):
`knob = round(clamp(envelope(t)·scale + offset, lo, hi)) as int`. The order
(clamp, then round) and the tie rule are part of the contract — changing
either changes which frames flip value.

- Clamp bounds are integers, so clamp-then-round can never leave the range.
- The continuity identity holds in integer form: `scale 0, offset K` (integer
  `K`) is byte-identical to passing `--levels K` directly.
- The knob's **off case stays reachable**: an envelope driving palette-quantize
  `levels` to 256 produces that frame's documented byte-identical passthrough —
  deliberate, the integer analogue of pixel-sort's legal empty-mask frame.

### Enum targets — the contracted variant-index rule

An enum knob is an integer knob over its variant list: variants get indices
`0..N-1` in the **declared order below** (which is contract), and the mapped
value selects `variants[round(clamp(envelope(t)·scale + offset, 0, N−1))]`
under the same clamp-then-round, ties-away-from-zero rule.

| Effect | Target | Variant order (index 0 → N−1) |
|---|---|---|
| `render-pixel-sort-sequence` | `direction` | `asc`, `desc` |
| `render-pixel-sort-sequence` | `axis` | `row`, `col` |
| `render-retro-static-sequence` | `filter` | `none`, `sub`, `up`, `average`, `paeth` |
| `render-palette-quantize-sequence` | `mode` | `posterize`, `palette` |

- **Unimplemented variants are excluded.** Palette-quantize `kmeans` renders
  an error, so it is not in the modulatable list — clamp-never-error extends
  to enum selection: an envelope must not be able to drive an effect into an
  erroring variant.
- The continuity identity holds by index: `scale 0, offset K` is
  byte-identical to passing variant `K`'s CLI value directly.
- **Range trap:** a `[0, 1]` envelope at the default `scale 1` only spans
  indices 0 and 1. Sweeping an N-variant knob end-to-end needs
  `scale ≈ N−1` (e.g. `filter=luma:4` to reach `paeth`).

### Stateful targets — routes join the checkpoint contract

On a stateful temporal effect, frame `N`'s output depends on the **whole knob
history** `0..N` (each frame's state update consumes that frame's knobs), not
just frame `N`'s values. That is deterministic — the envelope is a pure
function of (modulator media, analysis algorithm, scale/offset, sampling,
fps) — but it makes the route configuration part of the state contract:

1. **Per-frame application point.** The per-frame settings copy is
   overwritten at the top of each frame's state update; the same modulated
   settings feed every knob consumer inside that frame (render, supersample).
   Clamp-never-error as everywhere: clamps mirror the settings' `validate`
   ranges, one-sided where validate is one-sided.
2. **The modulation config joins the sequence contract.** The checkpoint's
   contract block gains a serde-defaulted `modulation` field: the resolved
   routes (canonical order), sampling mode, envelope fps, and a
   **content fingerprint of the modulator media** (same fnv1a64 scheme as the
   source fingerprints — a path alone would let edited media silently change
   the envelope mid-resume). Contract equality already gates resume, so any
   change to routes, sampling, fps, or modulator content refuses to resume
   with the existing "settings changed; start with a new output directory"
   error. Pre-slice checkpoints deserialize to `modulation: None` ==
   unmodulated, so they stay resumable by unmodulated renders.
3. **Resume reproducibility.** Resuming at frame `K` re-extracts the envelope
   from the (fingerprint-pinned) modulator media and samples it at the same
   absolute frame indices, so an interrupted-and-resumed render is
   byte-identical to an uninterrupted one — this is the acceptance test.
4. **Analysis caches are unaffected.** Routes modulate *render* knobs; flow
   fields and other analysis sidecars are functions of the sources only, so
   flow-cache reuse rules do not change under modulation.
5. **Target class restriction:** only per-frame-consumed knobs are
   modulatable. Knobs that select a code path with backend restrictions
   (feedback `structure_mode` — multiscale is CPU-only) or restructure the
   sequence (datamosh `keyframe_interval`) stay excluded: an envelope must
   not drive a render into an erroring or contract-breaking configuration.

First stateful effect: **flow feedback** (`render-feedback-sequence`, direct
CLI). Targets and clamps:

| Target | Clamp | Notes |
|---|---|---|
| `carrier_amount` | `[-4096, 4096]` | px flow gain (shift-range precedent) |
| `feedback_amount` | `[-4096, 4096]` | px flow gain; also feeds supersampling |
| `feedback_mix` | `[0, 1]` | the cliff lever |
| `decay` | `max(0)` | one-sided, mirrors validate |
| `structure_mix` | `max(0)` | one-sided, mirrors validate |

**Datamosh** (`render-datamosh-sequence`, direct CLI) follows the same
checkpoint-contract rules (its contract gains the identical serde-defaulted
`modulation` block). Its apply function lives CLI-side (`modulate.rs`) because
`DatamoshSequenceSettings` is itself a CLI-side struct — that placement is the
contract answer to the deferred "apply fn placement" question. Targets, all
clamped `[0, 4096]` (mirroring the command's own finite/non-negative
validation; the upper bound keeps a runaway `scale·envelope` finite):

| Target | Notes |
|---|---|
| `amount` | per-step flow gain (0 freezes the held frame) |
| `residual_gain` | fine-motion haze injection |
| `residual_decay` | residual accumulator decay |
| `refresh_threshold` | per-block keep/drop threshold |

Two datamosh-specific rules:

- **Tier activation is re-evaluated per frame** from the routed knobs
  (`residual_gain > 0`, `refresh_threshold > 0`, each with `block_size >= 2`),
  so an envelope can enable its tier per frame — e.g. a residual haze that
  pulses with audio over a plain block base. A zero-gain frame takes the plain
  block path and **holds** the residual accumulator untouched (neither decays
  nor injects) until the tier re-activates or a keyframe clears it. Without
  routes the per-frame flags equal the base flags — the exact unmodulated path.
- **Excluded knobs:** `keyframe_interval` (restructures the sequence),
  `block_size` (selects the bloom-vs-macroblock code path and the tier state
  topology), `vector_remix`/`remix_seed`/`preset`/`scanline_smear`/
  `codec_engrave` (structural mode/seed selections).

**Fluid advect** (`render-fluid-advect-sequence`,
`render-fluid-advect-two-source-sequence`,
`render-optical-flow-advect-sequence`, direct CLI) is stateful — each frame's
dye update consumes that frame's knobs — but has **no checkpoint/resume path**,
so only the per-frame application rule applies; the resolved routes are printed
provenance, not a persisted contract. The two-source and optical-flow commands
share one settings struct and apply function. Targets (clamps mirror validate;
where validate only requires finiteness the shared `[-4096, 4096]` range
bounds the value):

| Command | Targets |
|---|---|
| single-source | `advect` `[0,4096]`, `turbulence_scale` `[±4096]`, `turbulence_speed` `[±4096]`, `detail` `[0,4096]`, `reinject` `[0,1]` |
| two-source / optical-flow | `advect` `[±4096]` (negative = reversed flow), `reinject` `[0,1]` |

`seed` is excluded (structural, like `remix_seed`).

All three effects sample envelopes against `--modulation-fps` (the stateless
default, 12) — unlike feedback, none of these commands has a `--frame-rate` of
its own.

**Queue/SwiftUI exposure (landed).** All five stateful queue tasks (feedback,
datamosh, the three fluid-advect variants) persist routes via the same
serde-defaulted core fields as the stateless tasks: queue-add validates before
persisting (probe through the effect's apply fn), queue-run rebuilds the spec
strings so it shares the direct code path (add→run byte-identical, smoke), and
manifests gain the `modulation` block only when routes exist. The envelope time
base on the queue path is the **job's `frame_rate`** — for datamosh, whose task
has no frame rate, the manifest's fixed 30 fps (a direct render matches with
`--modulation-fps 30`). The feedback/datamosh checkpoint contracts carry the
routes through the queue path unchanged. SwiftUI panels follow the slice-3
mod-slot pattern; the fluid panel shares one slot set across its three runs —
Procedural Fluid consumes all six slots, A-to-B/Self-Flow only the flow-advect
and reinject slots (their commands have no turbulence targets).

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
3. **SwiftUI — LANDED.** Per-knob **mod slots** rather than a free-form route
   list: each modulatable target on the retro-static (strength) and pixel-sort
   (threshold low/high) panel sections gets a source picker (Off = no route,
   so duplicate-target routes are impossible by construction) plus scale/offset
   steppers, with shared modulator WAV/frames pickers and a hold/smooth
   sampling picker that appear only when a slot is active. The bridge appends
   the `--modulate` flag set to `queue-add-…` (no routes ⇒ no flags = the
   exact unmodulated path) and validates finiteness + modulator-media presence
   app-side before dispatch; the CLI's add-time validation remains the
   authority. Argument tests pin the route spec formatting, the no-route
   omission, and the missing-media rejection.
4. **Integer targets (CPU + CLI) — LANDED.** Palette-quantize `levels` joins
   the registry under the contracted rounding rule above;
   `render-palette-quantize-sequence` gains the standard `--modulate` flag set.
   Direct CLI only — palette-quantize has no queue task or SwiftUI section yet,
   so those exposures are their own later slices (channel-shift precedent).
5. **Enum targets (CPU + CLI) — LANDED.** Pixel-sort `direction`/`axis`,
   retro-static `filter`, palette-quantize `mode` join the registries under
   the contracted variant-index rule above. Because the queue path validates
   and applies routes through the same per-effect apply functions, enum routes
   persist on the existing pixel-sort/retro-static queue tasks with no queue
   changes. SwiftUI mod slots for enum targets are deferred (the slot UI's
   scale/offset steppers need an enum-aware presentation).
6. **Palette-quantize queue task — LANDED.** `frame_sequence_palette_quantize`
   render-job task plus `queue-add`/`queue-run-palette-quantize-sequence`,
   channel-shift precedent: routes validated at add time through the same
   apply function (nothing persists on rejection), `mode` persisted as a
   string label like retro-static's `filter`, queue-run rebuilds spec strings
   so it shares the direct code path (add→run byte-identical, smoke-tested
   with a `levels` + `mode` route pair).
7. **Palette-quantize SwiftUI panel — LANDED.** Backend picker (sticky,
   Metal default — both modes parity-gated, no CPU-only mode), mode picker,
   posterize-only levels stepper, and a slice-3 mod slot on the integer
   `levels` target with wide ranges. The enum `mode` target deliberately has
   **no** mod slot — the enum-aware slot presentation is still the deferred
   design decision.
8. **SwiftUI enum mod slots — LANDED.** `EnumModulationSlotRow` presents an
   enum knob's slot as **From → To variant pickers** instead of scale/offset
   steppers: envelope 0 selects From, envelope 1 selects To, and
   `enumModulationMapping` emits the equivalent affine route
   (`offset = fromIndex`, `scale = toIndex − fromIndex`) over the option
   enum's declared case order — which must mirror the contract variant table
   (pinned by a test). From == To emits `scale 0` (constant override =
   continuity identity); reversed and partial sweeps fall out naturally.
   Slots: retro-static `filter`, pixel-sort `direction`/`axis`,
   palette-quantize `mode`.
9. **Later:** stateful-effect targets, per-route sampling, envelope caching
   as analysis sidecars, multiple modulators per render.

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
