# Deferred Work — Build Handoff

Handoff catalog of every deferred item across the project, written for a fresh
agent session picking one up cold. Compiled 2026-07-04 against the
**cargo 546 / swift 117** baseline (see `STATUS.md`). Sources: `BACKLOG.md`
"Next", `RECOMMENDATIONS.md`, each `*_MILESTONE.md` "Deferred" section, and
`/memory/`.

## How to use this doc (read first)

1. **Contract first.** Before building any item, write or extend its
   `docs/*_MILESTONE.md` contract with acceptance criteria (CLAUDE.md workflow
   §1). This doc gives you the shape, entry points, and traps — the contract is
   where acceptance gets pinned.
2. **Baseline before touching anything**: `cargo test --workspace` and
   `swift test`; record the counts, report deltas.
3. **CPU reference → Metal → queue → SwiftUI**, in that order. Never expose a
   feature in queue/SwiftUI before its CPU path is proven.
4. **Verify with pixels + a number**: off-vs-on on a readout fixture
   (`--variation 0` for pool readouts), Read frames, report the
   `scripts/frame-delta.py` / cross-delta number.
5. `/checkpoint` after each verified slice (local commit, source only, never
   push). Never commit the untracked `scripts/solitaire-cascade-prototype.py`
   or `shader-port-ideas/`.
6. Record non-obvious findings in `/memory/`, not prose docs. Check the memory
   index first — most effects have a topic file with tuning numbers and traps.
7. Items in **Tier 2 are user-gated**: ask (AskUserQuestion) before building.
   Tier 4 items need new evidence before they're worth building at all.

---

## Tier 1 — Build-ready (no user gate needed)

Ordered by payoff ÷ effort, roughly following `RECOMMENDATIONS.md`.

### 1.1 Modulation-target registries for the unregistered effects

**What:** The newest effects register no modulation targets. Missing:
field-particles, cascade collage, cascade trails, dispersion blend
(`disperse.rs`), coagulated blend, fluid mosaic. (Registered today:
retro-static, pixel-sort, channel-shift, palette-quantize, rutt-etra,
flow-feedback, fluid-advect ×3, datamosh — see
`crates/morphogen-render/src/modulation.rs:454` onward.)

**Shape:** Per effect: a `*_MODULATION_TARGETS` const + clamped apply fn in
`modulation.rs` (or CLI-side in `modulate.rs` when the settings struct is
CLI-local — the datamosh precedent), the standard `--modulate` flag set on the
render command, then queue route persistence (serde-default fields, add-time
validation via the apply-fn probe, rejection persists nothing), then SwiftUI
mod slots (`ModulationSlotRow`, defaulted range params for pixel-unit knobs).
For coagulated: `coagulation_strength` / `edge_hardness` first
(`coagulation_strength=audio-rms` is nearly free per RECOMMENDATIONS §5).

**Rules that are contract, not style** (from `MODULATION_MATRIX_MILESTONE.md`
+ memory `modulation-matrix`):
- Clamp, never error; per-frame settings copy.
- Integer rule: clamp range THEN round, ties away from zero. Enum rule: the
  integer rule over declared variant order; unimplemented variants excluded.
- Zero routes ⇒ byte-identical to the unmodulated path (pin it).
- Slot ranges must fit pixel-unit knobs (±8 default is invisible).
- Structural knobs (seed, block size, backend-changing enums) are NOT targets.
- Stateful effects: routes join the checkpoint contract (feedback/datamosh
  precedent) or, where no checkpoint path exists (fluid advect), routes are
  printed provenance only.

**Acceptance per effect:** continuity identity (`scale 0, offset K` ≡ the
constant knob, byte-identical), unknown-target rejection, off-vs-on readout
with a delta number + Read frames.

### 1.2 Audiovisual granular grains (audio resynthesis)

**What:** Grains currently *select* by audio but output only pixels. Make each
pooled grain also emit its source-frame audio window, overlap-added into an
output WAV beside the frames — the effect becomes a true AV granulator
(RECOMMENDATIONS Part 1 §2; "audiovisual grains" is in the effect's own
roadmap entry).

**Where:** `crates/morphogen-render/src/granular_mosaic.rs` (pool path,
`pooled_av_nearest_grain_cpu_v2`), `crates/morphogen-audio` (WAV I/O, windows),
CLI `render-granular-mosaic-pool-sequence`. The pool already stores
`(frame_index, origin_x, origin_y)` per grain — frame_index → carrier-audio
sample offset is the mapping.

**Shape:** CPU-only (audio is never a GPU target here). Slice 1: deterministic
OLA resynthesis — per output frame, the selected grains' carrier-audio windows
(Hann, Σw² normalization floor — reuse the `stft_complex.rs` weighted-OLA
conventions) mixed at their tile times; new algorithm id for the audio
artifact only (video path byte-identical — pin it). Slice 2: queue + manifest
provenance. SwiftUI last.

**Traps (memory):** per-grain carrier audio on the per-frame grid is a no-op —
the pool (grains drawn across time) is exactly what makes this meaningful
(`granular-audio-needs-temporal-pool`). Write fixture WAVs as 16-bit PCM;
stdlib-side verification can't read hound's float WAVs. Off-vs-on audio proof
= RMS/length compare + a spectral check against a chirp fixture
(`/fixture` can generate chirp WAVs).

### 1.3 Coagulated flow blend — slices 2–4

**What:** Only Slice 1 (CPU ownership field) landed. Remaining per the
contract sketch in memory `coagulated-flow-blend`: Slice 2 flow advection of
the ownership field, Slice 3 jitter/smear, Slice 4 Metal port. First *mutual*
two-source effect (both sources contribute material).

**Where:** `crates/morphogen-render/src/coagulate.rs`,
`RenderCoagulatedBlendSequence` (args.rs:1393). No queue task yet — add it
after Slice 3, before/with Metal.

**Look guidance (memory):** bias=0 is subtle; push bias/strength/edge for the
bold glitch. Verify each slice with the off-vs-on + cross-delta discipline.

### 1.4 Cascade collage — A→B cross-synth seam

**What:** CPU ref + CLI + queue + SwiftUI have ALL landed (algo is at v7,
block compositing). The one remaining build slice (memory
`cascade-collage-effect` slice ⑥): swap the per-shape palette for a **Source B
sampler** (tile colour from B at the origin cell), then drive `morph_rate` /
`scrib_amp` from **Source A analysis** — deferred originally so footage colour
wouldn't erode the flat faces; contract that tension explicitly.

**Where:** `crates/morphogen-render/src/cascade_collage.rs`, contract
`docs/CASCADE_COLLAGE_MILESTONE.md`.

**Hard user decisions already made (do NOT relitigate):**
- **Metal port: decided CPU-only** (user, 2026-06-30) — per-pixel gather over
  ~400 stamps is likely slower than the fast CPU and the scribbled-edge float
  thresholds make the 1/255 parity gate fragile. Do not re-attempt without a
  new ask.
- **Shared torn seams / strata: tried and REJECTED** (was algo v8, reverted to
  v7). Do not re-attempt without a new ask.

**Traps (memory `cascade-collage-effect`):** cascade AWAY from the morphing
edge or it's buried; the reassembly trap — a tile sampling B at its own screen
position rebuilds the original video (give sample origins ≠ draw positions);
Screen ~0.8 is the block-blend winner over the dark floor.

### 1.5 Fluid colour-sort mosaic — curated-preset queue job

**What:** The open decision in BACKLOG (Fluid Advect Family §3). Resolution
already recommended (RECOMMENDATIONS Part 1 §3): a **curated-preset** queue
job — a handful of named looks — rather than exposing the ~15-knob raw API.

**Where:** `crates/morphogen-render/src/fluid_mosaic.rs`,
`RenderFluidMosaicSequence` (args.rs:1258). Per-slice tuning numbers live in
memory `fluid-colour-sort-mosaic` — the presets should be picked from those
proven settings (cluster-blob, dispersion band, turbulence, vortex).

**Hard constraint:** MOSAIC STAYS CPU — the sequential sim is parity-hostile;
do not attempt a Metal port. Queue task persists the preset name + resolved
knobs; add→run byte-identical to direct with the same knobs.

### 1.6 Edge-density modulation source

**What:** A new video descriptor `edge-density` (Sobel/gradient magnitude per
frame, normalized) joining `luma`/`flow` as a `--modulator-frames` source.
Cheap, purely deterministic. Deferred from `VIDEO_AUDIO_ROUTE_MILESTONE.md`;
RECOMMENDATIONS Part 2 §D.

**Where:** envelope extraction in `crates/morphogen-cli/src/modulate.rs`
(beside the luma/flow extractors), grammar in
`crates/morphogen-render/src/modulation.rs`. Everything downstream (sampling,
named modulators, sidecar cache, checkpoint fingerprints) generalizes over the
source enum — the LFO slice proved the pattern.

**Acceptance:** a fixture whose edge density ramps (e.g. increasing checker
frequency) while mean luma stays constant — proves the descriptor isn't luma
in disguise. Envelope sidecar gets its own algorithm id; content-change
invalidation pinned.

### 1.7 Drawn breakpoint envelopes as a modulation source

**What:** The other half of RECOMMENDATIONS Part 2 §B (LFOs landed). A
user-authored breakpoint curve file as a `ModulationSource` — deterministic,
media-free, exact per-frame evaluation.

**Shape:** Slice 1: a versioned JSON curve format (time/value breakpoints,
linear interp; document the out-of-range + single-point rules) + route grammar
`curve(<path>)`; content fingerprint joins stateful checkpoint contracts
(the named-modulator fingerprint precedent, NOT the LFO no-fingerprint
precedent — a file's content can change). Slice 2: queue. Slice 3: a minimal
SwiftUI editor (or file-picker-only first — ask the user which; the editor is
the only genuinely new UI machinery).

**Traps:** f32 JSON round-trip (`f32-json-roundtrip-test-trap`): use 0.5/0.25
in literal-asserting tests. Follow `lfo-modulation-sources` memory for the
parser (`spec_text()` for parameterized spellings; the `lfo(...)` dot trap in
the route parser — `curve(path)` with dots/slashes in the path will hit the
same parsing edge; decide the quoting rule in the contract).

---

## Tier 2 — User-gated (ask before building)

### 2.1 SwiftUI chain-builder panel *(the top open item in BACKLOG "Next")*

The open half of `EFFECT_CHAIN_MILESTONE.md` Slice 4. **Blocked on a UX
decision the user must make**: simple ordered stage list vs. a richer builder.
Ask with 2–3 concrete mockup options (AskUserQuestion previews). Technical
notes: the queue persists the resolved spec JSON document (not typed core
mirrors — memory `effect-chain`); nested modulation blocks because
deny_unknown_fields can't flatten; the bridge would emit `render-chain` /
`queue-add-chain` args. Once it exists, chain preview falls out of the
preview-loop machinery naturally (PREVIEW_LOOP deferred note).

### 2.2 LFO 6-panel SwiftUI sweep

Mechanical clone of the named-modulator 6-panel sweep: the LFO slot capability
(`ModulationSlotRow` opt-in, landed on Rutt-Etra) extended to the other
panels. **Gated on the user confirming the LFO look on real footage first**
(BACKLOG "Next"). Bridge needs zero changes — media guards key off source
strings (`lfo-modulation-sources`).

### 2.3 Rutt-Etra polish slices

From `RUTT_ETRA_TWO_SOURCE_MILESTONE.md` deferred list, all user-gated on
wanting more from the look:
- **HQ anti-aliased lines** — roadmap long-term tier; changes output ⇒ new
  algorithm id; the gather inversion must be re-derived for coverage-weighted
  spans (nontrivial — contract carefully).
- **Normalized-coordinate sampling of a differently-sized A** — only needed if
  a real use case escapes matching proxies.
- **A driving a second knob** (e.g. A luma → colour intensity too).
- **Depth-descriptor displacement** — blocked, see Tier 3.

### 2.4 Live preview scrubbing / streaming engine

The big one (PREVIEW_LOOP deferred; RECOMMENDATIONS Part 2 §C caveat). Needs
its own engine-first milestone: incremental re-render on knob change while
holding the same-engine invariant (preview = lower-fidelity view of the same
graph, never a fork). Do not start this as a side quest — it's a
multi-session milestone; get explicit user commitment.

### 2.5 Phase-vocoder extensions

From `PHASE_VOCODER_MILESTONE.md` deferred: mod-matrix routes on vocode knobs
(actually Tier-1-shaped — the standard registry slice — if the user wants it);
phase manipulation (time-stretch/pitch-shift); cepstral/LPC envelopes; A↔B
morphing. The traps that bit last time (memory `phase-vocoder-cross-synth`):
exclude zero-padded tail frames from A's envelope peak search; explicit
conjugate-symmetry mirroring; amount==0 short-circuits before the transform.

---

## Tier 3 — Blocked (prerequisite carve-out or design first)

### 3.1 Depth descriptor

Unlocks Rutt-Etra depth mode + parallax displacement, but Apple depth models
are **not bit-reproducible across OS versions** — violates determinism-first
unless given the sidecar-fingerprint carve-out treatment (deterministic
*given* a cached sidecar, the `datamosh-bitstream` precedent). The carve-out
contract must be written and user-approved **before** any implementation
(RECOMMENDATIONS Part 2 §D says: flag at milestone time, not discovery time).

### 3.2 Effect-chain graph / two-source stages

`EFFECT_CHAIN_MILESTONE.md` deferred: branching/merging via the core
`NodeGraph` typed ports; A→B effects mid-chain need a second input binding — a
graph problem, not a list problem. The spec `version` field exists for this.
Design-first; probably follows the chain-builder panel (2.1) since the UI
shapes the graph model the user actually wants. The datamosh stage type is
also "on demand" here.

---

## Tier 4 — Parked (do not build without new evidence)

Explicitly *not* recommended; the rationale is recorded and stands until a
concrete use case overturns it:

- **Multiscale structure-morph Metal/queue/SwiftUI** — mathematically correct,
  practically marginal on real footage (~1.5% mean diff, visually
  indistinguishable; mask degenerates on dense low-contrast footage). Stays a
  correct opt-in CPU-only path. See BACKLOG structure-morph note §5.
- **FFglitch integration / real bitstream MV remix** — the non-deterministic
  `datamosh-bitstream` carve-out already covers the authentic codec looks;
  a hard external dependency isn't worth the invariant cost
  (`DATAMOSH_MILESTONE.md` deferred + memory `datamosh-real-vs-simulated`).
- **Conv-blend tiled large-K Metal** — pure perf optimization over a
  parity-exact kernel; needs a *measured* too-slow render first.
- **Metal port of the fluid colour-sort mosaic** — parity-hostile sequential
  sim; permanently CPU (memory `fluid-colour-sort-mosaic`).
- **Cascade collage Metal port** — user decided CPU-only 2026-06-30 (gather
  over ~400 stamps likely slower than CPU; scribble thresholds parity-fragile).
- **Cascade collage torn seams / strata** — built, user rejected the look,
  reverted (v8 → v7). A new ask required before any retry.

---

## Later menu (small per-effect deferrals, build on demand)

One-liners kept for completeness; each has its contract's Deferred section as
the source of truth:

- **Video vocoder** (`VIDEO_VOCODER_MILESTONE.md`): hard/nearest-band
  membership (posterized vocoder aesthetic); spatial-frequency pyramid bands;
  audio-spectrum-driven bands (overlaps spectral cross-synth — keep separate).
- **Spectral cross-synth** (`SPECTRAL_CROSS_SYNTH_MILESTONE.md`): biquad /
  resonant filters; per-band spectral gain.
- **Audio→video route** (`AUDIO_VIDEO_ROUTE_MILESTONE.md`): spatially varying
  displacement fields (converges with optical-flow advection); more
  descriptor→target pairs (centroid→hue, onset→flash/cut).
- **Conv-blend** (`CONVOLUTIONAL_BLEND_MILESTONE.md`): separable image
  kernels; Source-A colour taps in luma mode.
- **LFO** (`LFO_MODULATION_MILESTONE.md`): LFO-on-LFO (modulated rate/phase),
  one-shot envelopes.
- **Preview loop** (`PREVIEW_LOOP_MILESTONE.md`): scrub bar; audio-command
  previews; Metal downscale (explicitly not wanted — preview utility stays
  CPU).
- **Granular pool**: nothing left in 6b (feature-complete per BACKLOG step
  16); a bounded *leading* window or other pool scopes only if asked.

---

## Cross-cutting reminders for whoever builds

- Algorithm-id discipline: any output-affecting change ⇒ new id; a descriptor
  schema change bumps ids too (`granular-texture-dims`: don't serde-default
  into a stale sidecar).
- Serde compatibility: new queue fields are `#[serde(default)]` (+
  `skip_serializing_if` when optional) so pre-slice queue JSON stays
  byte-identical — pin it with a test.
- PNG diff tools: `frame-delta.py` mis-decodes RGB-vs-RGBA pairs; use
  `dm-cross-delta.py` for cross-sequence datamosh-family comparisons; file
  `cmp` on PNGs is a known false negative.
- Stateful Metal effects: per-frame parity can pass while CPU≠Metal
  byte-identical over a sequence (sub-epsilon compounding,
  `datamosh-mvp-recursive-metal-drift`) — that is expected, don't "fix" it.
- Subagent builds: verify contract-named artifacts yourself (grep the output
  dir — the "mirror palette-quantize" manifest trap in `rutt-etra-scanline`);
  a deviations list is not evidence.
