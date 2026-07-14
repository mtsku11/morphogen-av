# Controlled Datamosh / Motion-Vector Reuse Milestone

## Goal

The roadmap's **"flow-field reuse on decoded float frames"** MVP for Controlled
Datamosh / Motion-Vector Reuse (`docs/EFFECTS_ROADMAP.md`). Source A is the
**motion source**; Source B is the **carrier material**. Output is B's content
repeatedly pushed by A's optical-flow deltas — the signature datamosh
**"bloom/melt"** look, where a held carrier frame smears under motion that never
belonged to it.

This is the deterministic, parity-gated **flow-reuse tier 1** (see
`/memory/datamosh-real-vs-simulated.md`): real melt/bloom/motion-transfer on
decoded RGBA32F frames, *in the datamosh family* but not the authentic
macroblock/bitstream artifact (which would break determinism-first and is the
deferred FFglitch tier). The pixel transform is the existing, already-parity-gated
**flow displace** (`flow_displace_cpu` / `flow_displace_metal`); the only new
logic is the **recursive accumulation + keyframe-refresh policy**.

## Model (the new logic)

A **stateful temporal node**. Source A's per-frame optical flow
(`flowA[i]` = Lucas-Kanade from `A[i-1]` to `A[i]`, the same estimator the
flow-feedback path uses) advects the *previous output*, not the carrier — so the
carrier content is "frozen" from the last keyframe and melts under A's motion:

```
out[0] = B[0]                                   # frame-zero: the first carrier frame
for i >= 1:
    if is_keyframe(i):  out[i] = B[i]           # "keep" / I-frame refresh (snap back to B)
    else:               out[i] = flow_displace(out[i-1], flowA[i], amount)   # P-frame delta
```

`is_keyframe(i) = (i == 0) || (K >= 1 && i % K == 0)`, where `K = keyframe_interval`:

- **`K = 1`** ⇒ every frame is a keyframe ⇒ output **byte-identical to Source B**
  (the natural "off" / passthrough identity).
- **`K = N` (small)** ⇒ periodic snap-back to B every `N` frames — the "pulse" look.
- **`K = 0`** ⇒ only frame 0 is a keyframe; B[0] accumulates *all* of A's motion ⇒
  maximal melt/bloom.

`amount` (`--amount`, default `1.0`) scales A's flow per advection step (motion
intensity). `amount = 0` freezes the held keyframe (motion has no effect between
keyframes; with `K = 1` it is still exact B passthrough).

### Stateful-node declarations (invariant)

- **Frame-zero behavior:** `previous_output: None` ⇒ returns the carrier frame
  unchanged (`B[0]`).
- **Prior-frame state consumed:** the previous output frame as **RGBA32F**
  (unquantized), never a display PNG. Between keyframes only this buffer + A's
  flow are read; the carrier is *not* re-sampled.
- **Checkpoint representation:** the previous-output RGBA32F buffer (the render
  loop carries it in memory; the existing `write_flow_feedback_state` /
  `read_flow_feedback_state` RGBA32F serializers are the disk rep when
  resume/checkpoint exposure lands — deferred from this MVP slice).
- **Cache invalidation:** changing the algorithm id, A/B inputs, `keyframe_interval`,
  `amount`, or backend changes the output and must invalidate stale caches.

## Initial Scope

- CPU reference in `morphogen-render` (`datamosh.rs`): `datamosh_bloom_frame_cpu`
  (carrier, `Option<&previous_output>`, flow, `is_keyframe`, amount) delegating to
  `flow_displace_cpu` for the advection branch, with focused synthetic tests. No
  new pixel math — the transform is the proven displace; the new logic is the
  keep/advect branch + frame-zero.
- `render-datamosh-sequence` CLI: `--modulator-dir` (A PNG sequence),
  `--carrier-dir` (B PNG sequence), `--output-dir` (out PNG sequence),
  `--keyframe-interval` (`K`, default `0` = melt; `1` = passthrough),
  `--amount` (default `1.0`), `--backend cpu|metal` (parity-gated), `--max-frames`.
  Per-frame optical flow `A[i-1]→A[i]` via `pyramidal_lucas_kanade_flow_cpu`
  (reusing the flow-feedback idiom); recursion carries RGBA32F in memory.
- Output is a PNG frame sequence following Source B (dimensions, frame count =
  common prefix of A and B with the cap).
- Queue task + macOS Render-panel exposure follow once the CPU + CLI + Metal
  slice is proven (a `frame_sequence`-style video job, like the audio→video route).

## Algorithm Identifier

- `flow_reuse_datamosh_bloom_cpu_v1` — the datamosh policy id recorded on the
  job/manifest. (The underlying pixel op is the existing `flow_displace`; this id
  names the recursive accumulation + keyframe-refresh policy.)

The per-frame optical flow reuses the existing `pyramidal_lucas_kanade_cpu_v1`
analysis (regenerable sidecar; no new analysis algorithm).

## Acceptance Criteria

1. **Passthrough identity.** `--keyframe-interval 1` ⇒ output byte-identical to
   Source B (every frame is a keyframe refresh).
2. **Melt transfer.** `--keyframe-interval 0` over a high-motion A ⇒ the held
   `B[0]` visibly smears/blooms under A's accumulated motion; a static A ⇒
   near-identity (no motion to apply).
3. **Determinism.** Identical A, B, interval, amount, backend ⇒ identical output
   frames.
4. **CPU/Metal parity.** `--backend metal` byte-identical to CPU within
   `METAL_CPU_PARITY_EPSILON`, gated frame-by-frame before export (inherited from
   the displace kernel's existing parity gate).
5. **Frame-zero behavior** declared and honored (`out[0] = B[0]`); no `unwrap()`
   in library code; errors via `RenderError` / `thiserror`.

## Verification (off-vs-on)

Render the same job **off** (`--keyframe-interval 1`, passthrough) vs **on**
(`--keyframe-interval 0`, full melt) on a **high-motion** A over a recognizable
static B, Read frames from both, and report the `scripts/frame-delta.py` number —
off ⇒ ~0 delta vs B (passthrough), on ⇒ a growing nonzero delta as the held frame
accumulates A's motion. The melt needs *motion in A*; a static A produces little
displacement (the analogue of the audio-route fixture needing a loud modulator).
A look without a number is unfalsifiable; a number without the pixels proves
nothing.

## Future Feature Verification Gate

Every new datamosh feature must ship with:

- deterministic unit and/or smoke coverage for the code path when it is inside the
  render graph;
- a clear invariant carve-out when it is real bitstream surgery and therefore
  non-deterministic;
- an off-vs-on render using representative moving footage, with the exact command
  and `scripts/frame-delta.py` readout recorded in the handoff;
- representative PNG frames or a contact sheet posted in the user-facing response
  so the output can be visually verified before the feature is treated as done.

`scripts/datamosh-contact-sheet.py` is the standing tool for that last point. It
renders the named destructive modes on the synthetic fixture and tiles sampled
frames into one labeled review sheet (pure-stdlib PNG + a built-in 5×7 font, no
deps), printing each deterministic mode's mean RGB cross-delta vs the PASSTHROUGH
baseline alongside the pixels. The canonical mode set it covers:

| Mode | Tier | Knobs |
| --- | --- | --- |
| PASSTHROUGH (baseline) | deterministic | `--keyframe-interval 1` (== Source B) |
| CODEC BLOOM | deterministic | `--keyframe-interval 0 --amount 1.0` |
| MACROBLOCK SLIDE | deterministic | `--keyframe-interval 0 --block-size 16` |
| STRUCTURED MELT | deterministic | `+ --residual-gain 1.0 --residual-decay 0.9` |
| MACROBLOCK ROT | deterministic | `+ --block-refresh-threshold 1.0` |
| P-FRAME BLOOM | bitstream (`--video`) | `--operation pframe-duplicate --duplicate-count N` |
| VOID MOSH | bitstream (`--video`) | `--operation remove-keyframe` |

The deterministic rows are byte-reproducible (a true regression baseline); the
bitstream rows need ffmpeg + a real clip and are flagged NON-DETERMINISTIC on the
sheet (no stable baseline, per the carve-out below). Run:
`scripts/datamosh-contact-sheet.py [out.png] [--video CLIP]`.

### Recursive-node Metal drift (known, accepted)

Because this is a *recursive* node (Metal's output feeds its own next frame), the
end-to-end Metal sequence is **not** byte-identical to CPU — the per-frame parity
gate passes, but sub-epsilon float differences compound. A 240-frame
sustained-motion check shows the CPU-vs-Metal mean RGB delta accumulating
**linearly** at **~0.00067/255 per frame** (systematic, not a plateau): <1/255
below ~1500 frames, ~6.7/255 at 10k frames. Metal is byte-reproducible run-to-run,
so determinism holds **per-backend**. Same accepted behavior as the recursive
`flow_feedback` Metal path. **Guidance:** for very long archival renders
(multi-thousand frames) prefer `--backend cpu`; otherwise Metal is a faithful
accelerated view. Not a correctness gap — byte-identity is the wrong goal for a
recursive node.

## Codec-simulated ("block") tier — LANDED

The first deferred tier shipped as a knob on the same command. A's per-frame flow
is **quantized to a coarse `block_size`×`block_size` grid** — one **mean** motion
vector per block (`quantize_flow_to_blocks`) — before the recursive advection, so
whole macroblocks slide coherently: the chunky "real datamosh" look rather than
the smooth per-pixel bloom warp. The only new pixel logic is the flow→flow
quantization; the advecting displace is still the existing parity-gated kernel, so
**Metal came free (no new kernel)** — quantize on CPU, displace on the gated GPU
path. `--block-size` (default `1`); `block_size ≤ 1` makes every pixel its own
block ⇒ **byte-identical to the smooth bloom path** (the continuity property).

- **Algorithm id:** `datamosh_algorithm(block_size)` resolves to
  `flow_reuse_datamosh_block_cpu_v1` **only for blocks ≥ 2px**, else the bloom id.
  Job field is `#[serde(default)]` (=`0` ≡ smooth) so legacy datamosh jobs keep
  their meaning. The manifest records `block_size` + the resolved id.
- **Off-vs-on readout:** high-motion bouncing-square A over a static stripe+dot B,
  smooth (`--block-size 1`) vs blocky (`--block-size 16`), full melt. Cross-sequence
  smooth-vs-blocky delta grows **0 → 35.9/255** (frame 0 identical, both `B[0]`);
  frames Read — block 16 melts into large coherent wavy warps (16px regions slide
  together) where block 1 shatters into per-pixel speckle (noisy per-pixel LK).
- **Whole tier now landed:** block-residual accumulation and per-block keep/drop
  pseudo-keyframes have both since landed (next sections).

## Block-residual accumulation tier — LANDED

The quantization-noise half of the macroblock aesthetic. Quantizing A's flow to a
block mean (above) **throws away** the intra-block detail `residual = flow −
block_mean`. This tier stops discarding it: a **per-pixel residual flow buffer**
accumulates the discarded sub-block motion across frames and re-injects it
(lagged) into the advecting flow, so macroblocks slide coherently *and* shed a
trailing haze of the fine motion the coarse grid couldn't represent. Like the
block tier it stays a **pure flow→flow transform**, so the advecting displace is
still the existing parity-gated kernel and **Metal comes free again** (quantize +
accumulate on CPU, displace on the gated GPU path).

Per-pixel, per P-frame (`q` = block mean, `f` = A's raw flow, `accum` = the new
state buffer):

```
resid[p]  = f[p] - q[p]                      # discarded intra-block detail
accum[p]  = accum[p]*residual_decay + resid[p]   # NEW per-pixel state (2ch)
flow[p]   = q[p] + accum[p]*residual_gain    # feed the parity-gated displace
```

- **State (invariant):** the per-pixel residual buffer is a second stateful
  channel alongside `previous_output`, carried as an unquantized 2-channel
  `FlowField` in memory. **Frame-zero and every keyframe reset it to zero** (an
  I-frame refresh clears accumulated residual, matching the snap-back to `B`).
- **Continuity knobs:**
  - `--residual-gain 0` ⇒ **byte-identical to the block path** (no residual
    re-injected; the function short-circuits to `datamosh_block_frame_cpu`).
  - `--residual-gain 1` on the first P-frame ⇒ `accum = resid`, so `flow = q +
    (f−q) = f` ⇒ that frame is byte-identical to the **smooth bloom** (raw-flow)
    displace; the residual only diverges from bloom once it accumulates over ≥ 2
    P-frames or `gain ≠ 1`.
  - `block_size ≤ 1` ⇒ `q = f` ⇒ `resid = 0` ⇒ accum stays zero ⇒ byte-identical
    to bloom regardless of gain (the residual is a no-op without quantization).
  - `--residual-decay` (default `0.9`) controls how long discarded motion lingers
    (`0` = one-frame kick, `→1` = long-lived drift).
- **Algorithm id:** `datamosh_algorithm(block_size, residual_gain)` resolves to
  `flow_reuse_datamosh_block_residual_cpu_v1` **only when blocks ≥ 2px and
  residual_gain > 0**, else the block / bloom id as before. A **separate** id (not
  a descriptor dim on the block id), so adding it does **not** bump the block id.
  Job fields are `#[serde(default)]` (gain/decay = `0` ≡ off) so legacy datamosh /
  block jobs keep their meaning.

### Acceptance criteria (residual)

1. **Block-path continuity.** `--residual-gain 0` ⇒ output byte-identical to the
   block path (same `block_size`).
2. **Bloom continuity.** `block_size ≤ 1` (any gain) ⇒ byte-identical to bloom.
3. **First-P-frame identity.** `--residual-gain 1` on the first P-frame ⇒
   byte-identical to the raw-flow bloom displace.
4. **Accumulation.** `accum` after a P-frame equals `prev_accum*decay + (f−q)`;
   keyframe / frame-zero ⇒ `accum = 0`.
5. **Determinism + Metal parity** inherited from the displace (per-frame gate);
   no `unwrap()` in library code.

### Verification (off-vs-on)

Block path (`--residual-gain 0`) vs residual on (`--residual-gain 1 --block-size
16` over high-motion A, full melt), Read frames from both, report
`scripts/frame-delta.py`. Off ⇒ identical to the block tier; on ⇒ the coherent
macroblock slide gains a divergent fine-motion haze that builds over frames.

## Per-block keep/drop pseudo-keyframes tier — LANDED

The patchy "some macroblocks refresh, some rot" half of the aesthetic — a keyframe
decision *per block* rather than per whole frame. After the recursive advect, each
macroblock whose **mean-motion magnitude** is below `--block-refresh-threshold`
"keeps": it snaps back to the carrier `B[i]` (an intra/I-block refresh) while
busier blocks are denied refresh and keep rotting under the reused flow.

The trigger is **content-driven**, like a codec's intra-block map (intra blocks are
inserted where inter-prediction fails ≈ high motion / new content), not injected
noise — the faithful simulation of the macroblock-refresh mechanism. "Calm blocks
refresh, busy blocks smear" yields the recognizable look where the trail behind a
moving subject **self-erases** (calm regions snap back to clean `B`) while the
subject's current position keeps smearing. Like the block / residual tiers it stays
a per-block composite over the *output* of the parity-gated displace, so **Metal
comes free again** (advect on the gated path, then a CPU composite identical across
backends).

Per P-frame (`q` = block mean = `quantize_flow_to_blocks`):

```
keeps(block) = |mean(q over block)| < refresh_threshold   # below ⇒ intra refresh
out[p]   = keeps(block(p)) ? B[i][p] : advected[p]         # composite
accum[p] = keeps(block(p)) ? 0       : accum[p]            # I-block clears residual
```

- **State (invariant):** the per-pixel residual accumulator (the residual tier's
  second stateful channel) is **cleared in every refreshed block** — an intra-block
  refresh discards that block's accumulated prediction state, matching the
  whole-frame keyframe reset. Frame-zero / keyframe are unchanged (carrier verbatim,
  accumulator zeroed); refresh only acts on a P-frame.
- **Continuity knobs:**
  - `--block-refresh-threshold 0` ⇒ **byte-identical to the block/residual path**
    (no block refreshes).
  - a threshold above the largest block motion ⇒ **every block refreshes** ⇒ the
    carrier verbatim with a cleared accumulator (byte-identical to a whole-frame
    keyframe).
  - `block_size ≤ 1` ⇒ the bloom path (refresh is a no-op without macroblocks, like
    residual).
- **Algorithm id:** `datamosh_algorithm(block_size, residual_gain, refresh_threshold)`
  resolves to `flow_reuse_datamosh_block_refresh_cpu_v1` **when blocks ≥ 2px and
  refresh_threshold > 0** (precedence refresh > residual > block > bloom — it names
  the most-specific active policy; the `residual_gain`/`residual_decay`/
  `block_refresh_threshold` knobs are recorded separately and carry the rest). A
  **separate** id, so it does not bump the block / residual ids. The job field is
  `#[serde(default)]` (= `0` ≡ off) so legacy datamosh / block / residual jobs keep
  their meaning.

### Acceptance criteria (refresh)

1. **Block/residual continuity.** `--block-refresh-threshold 0` ⇒ output
   byte-identical to the residual frame (same block_size/gain/decay).
2. **Keyframe continuity.** a threshold above every block's motion ⇒ the carrier
   verbatim with a cleared accumulator (≡ a whole-frame keyframe).
3. **Keep/rot.** calm blocks (mean motion below threshold) take the carrier; busy
   blocks take the advected content; refreshed blocks clear their accumulator.
4. **Bloom continuity.** `block_size ≤ 1` ⇒ the bloom path regardless of threshold.
5. **Determinism + Metal parity** inherited from the displace (per-frame gate); no
   `unwrap()` in library code.

### Verification (off-vs-on)

Block path (`--block-refresh-threshold 0`) vs refresh on (`--block-refresh-threshold
1.0 --block-size 16` over the bouncing-square A, full melt), Read frames from both,
report `scripts/dm-cross-delta.py`. Off ⇒ a cumulative smear everywhere the square
has been; on ⇒ the trail self-erases (calm blocks refresh to clean `B`) leaving the
smear only at the square's current position. Cross-delta grows **0 → 31.6/255** over
30 frames (frame 0 identical, both `B[0]`); the Metal refresh path renders (gate
passes ⇒ Metal free).

## Vector-remix tier (FFglitch MV sort/shuffle, deterministic) — LANDED (slice 1: CPU + CLI)

The deterministic "family look" of FFglitch's motion-vector sort/shuffle, **on the
optical-flow field rather than the codec bitstream** (chosen over an FFglitch
external dependency or a pure-Rust MPEG-4 MV codec — see the user decision). The
block-quantized flow *is* a per-block motion-vector grid (the same grid the block
tier builds), exactly FFglitch's "vector" unit, so a remix is a **permutation of
that block-MV grid** before the advection — pure flow→flow, so the displace stays
the parity-gated kernel and **Metal comes free again** (no new kernel).

`remix_block_vectors(flow, block_size, mode, seed)` (sharing a factored-out
`block_mean_grid` with `quantize_flow_to_blocks`):
- `--vector-remix sort` ⇒ reassign block MVs in **descending-magnitude** order along
  the raster scan (top-left blocks take the strongest motion) ⇒ motion pools
  coherently across the frame.
- `--vector-remix shuffle` (`--remix-seed`) ⇒ a deterministic seeded Fisher–Yates
  permutation ⇒ motion scrambles between blocks.
- Both are **pure permutations** of the existing block MVs (no new magnitudes
  invented), so total motion energy is preserved — only its spatial assignment moves.

- **Algorithm id:** `datamosh_algorithm(block_size, residual_gain, refresh_threshold,
  remix_mode)` gains a 4th arg; `remix_mode != None` **and** blocks ≥ 2px ⇒
  `flow_reuse_datamosh_vector_remix_cpu_v1` (most-specific, takes precedence). `none`
  or `block_size ≤ 1` ⇒ the prior precedence unchanged. In the render loop the remix
  computes `effective` itself (precedence over residual; refresh can still composite).
- **Continuity:** `--vector-remix none` ⇒ byte-identical to the block path;
  `block_size ≤ 1` ⇒ the bloom path (remix is a no-op without macroblocks).
- **Scope:** now a full vertical slice — CPU + CLI + persisted queue job +
  SwiftUI. The schema mirror `VectorRemixMode` lives in core (with `RenderBackend`/
  `KernelMode`); the persisted `frame_sequence_datamosh` job carries `vector_remix`
  (serde-default `None`) + `remix_seed` (serde-default `0`), so jobs serialized
  before this slice keep their id. `queue-add-datamosh-sequence` gained
  `--vector-remix`/`--remix-seed`; `queue-run` maps the core mode to the render enum
  (a free fn, orphan rule) and records both in the manifest. The macOS Render panel
  adds a Vector Remix picker + a Remix Seed stepper (shown for Shuffle).

### Acceptance criteria (vector remix)

1. **Block continuity.** `--vector-remix none` ⇒ byte-identical to the block path.
2. **Bloom continuity.** `block_size ≤ 1` ⇒ the bloom path regardless of mode.
3. **Permutation.** the remixed block MVs are exactly the original block MVs
   reordered (multiset preserved); `sort` is descending-magnitude.
4. **Determinism.** same seed ⇒ byte-identical; a different seed differs.
5. No `unwrap()` in library code.

### Verification (off-vs-on)

Datamosh fixture (bouncing-square A over a static stripe+dot B), block 16, full
melt. `none` vs `sort` cross-delta grows **0 → 70.9/255** over 8 frames (frame 0
identical, both `B[0]`); `none` vs `shuffle` (seed 42) grows **0 → ~37/255**
(non-monotonic — a scramble, not a pooling). Re-rendered `sort` byte-identical
(deterministic). Frames Read: `sort` redistributes the stripe displacement (the
strong-motion block reassigned toward the top-left), `shuffle` scatters it into a
different layout. The synthetic fixture concentrates motion in one band so the look
is subtle; a real moving clip with motion spread across the frame shows it stronger.

## Reusable Flow Sidecars, Resume, and Presets — LANDED

The deterministic datamosh path now shares the same offline-render discipline as
flow feedback: reusable analysis sidecars, validated resume state, and named
recipes for destructive looks.

- **Reusable Source A flow sidecars.** `render-datamosh-sequence --flow-cache-dir`
  reads/writes one temporal optical-flow sidecar per P-frame at
  `frame_000001/manifest.json` + `frame_000000.flowf32` (and so on). Sidecars use
  `pyramidal_lucas_kanade_cpu_v1`, the existing cache format v2, and Source A
  fingerprint validation; mismatched algorithm, dimensions, or source checksum
  regenerates rather than silently reusing stale flow.
- **Queued cache provenance.** `queue-add-datamosh-sequence` defaults the cache to
  `job-0001/cache/datamosh-flow` when no explicit cache is supplied. The queued
  job stores that path on `RenderJobTask::FrameSequenceDatamosh` and the output
  manifest records it under both `datamosh.flow_cache_directory` and
  `provenance.analysis_caches`.
- **Disk checkpoint / resume.** Direct datamosh renders write `checkpoint.json`
  plus unquantized `state/datamosh_output_frame_*.rgba32f` after every frame.
  Residual-mode renders also persist `state/datamosh_residual_frame_*` flow-cache
  directories. `--stop-after-frame` is the test hook; a subsequent identical
  command resumes from `next_frame_index`. The checkpoint rejects changed source
  provenance, settings, backend, job id, or unsafe relative state paths.
- **Curated destructive presets.** `--preset custom|codec-bloom|structured-melt|
  macroblock-rot|vector-shuffle|scanline-smear|codec-engrave` resolves to concrete
  deterministic settings before rendering. `custom` preserves the explicit knobs.
  `scanline-smear` follows the block/vector mosh with a flow-driven horizontal
  tear/debris pass: hard local edges reduce smear so the subject can survive while
  flatter regions break into lateral bands, chroma dashes, white specks, and black
  dropouts. `codec-engrave` layers on carrier-edge hatching, block stepping, RGB
  edge offsets, and micro-contrast so the readable subject itself gains the dense
  compressed surface detail visible in glitch stills. The persisted core job
  carries `DatamoshPreset` with `serde(default)`, queue manifests record the
  resolved settings, and SwiftUI exposes the preset picker beside Vector Remix.

### Acceptance Criteria (sidecars / resume / presets)

1. **Resume equivalence.** stop-after-one-frame + resume is byte-identical to an
   uninterrupted render for the same inputs/settings.
2. **Cache reuse.** a second render with the same Source A and cache directory
   reuses generated temporal-flow sidecars.
3. **Preset resolution.** the `vector-shuffle` preset resolves to the vector-remix
   algorithm with block size 16 and deterministic seed handling.
4. **Provenance.** queued datamosh jobs record the flow cache sidecar path and
   producer in output provenance.
5. No `unwrap()` in library code.

## Real bitstream mosh — P-frame bloom + keyframe removal — LANDED (experimental, non-deterministic)

The authentic codec-artifact tier, shipped as a **standalone experimental CLI**
(`datamosh-bitstream`) inside an explicit invariant carve-out. Unlike the simulated
tiers (which fake the look on decoded float frames), this mangles the *compressed
stream* so the decoder itself produces the artifacts.

**Pipeline.** ffmpeg encodes the input to a **P-frame-only AVI/MPEG-4** (one leading
I-frame, no B-frames, no audio — `-c:v mpeg4 -bf 0 -g 999999 -sc_threshold 0 -an`,
using ffmpeg's built-in **LGPL** mpeg4 encoder, *not* libxvid, so no GPL dependency);
pure-Rust RIFF surgery (`crates/morphogen-media/src/avi.rs::duplicate_p_frame`)
duplicates a chosen P-frame's compressed chunk `--duplicate-count` times so its
motion vectors re-apply on every redecode; ffmpeg decodes the mangled AVI to a PNG
sequence. The surgery rebuilds the `movi` list + `idx1` index (preserving the
encoder's offset convention via the first entry) and patches `avih.dwTotalFrames` /
`strh.dwLength`.

**The carve-out (3 invariants, explicit):**
- **Determinism** — output depends on the external ffmpeg codec (version/build), so
  it is **not bit-reproducible**. It lives OUTSIDE the deterministic render graph:
  no `RenderJobTask`, no queue, no SwiftUI; a `datamosh_bitstream.json` sidecar
  records params + ffmpeg version + `deterministic: false` for traceability.
- **CPU-ground-truth / Metal parity** — n/a; there is no render kernel, no GPU path,
  no parity gate.
- **FFmpeg external+optional / no GPL-only dep** — **honoured**: only the already-
  sanctioned external ffmpeg, the LGPL mpeg4 encoder, and our own Rust surgery. No
  FFglitch, no vendored codec.

**Determinism that *does* hold:** the AVI surgery is pure and unit-tested on
synthetic byte buffers (no ffmpeg needed) — `--duplicate-count 0` is the exact
identity (off case); duplication grows the chunk count by N, rebuilds the index, and
updates the frame-count headers. Algorithm id
`datamosh_bitstream_pframe_dup_experimental_v1`.

The first follow-up operation is also landed: `datamosh-bitstream --operation
remove-keyframe` removes the controlled substrate's leading keyframe so the
decoder starts from prediction data. This is the transition/void mosh variant:
frames decode from damaged reference state instead of a clean I-frame. The same
carve-out applies; the pure RIFF surgery is deterministic and unit-tested, while
the decoded look depends on the external ffmpeg codec. Algorithm id
`datamosh_bitstream_remove_keyframe_experimental_v1`.

**Verification (off-vs-on, look check not a determinism proof).** A 2s `testsrc2`
clip, `--p-frame-index 5`, off (`--duplicate-count 0`, a plain transcode = 48 frames)
vs on (`--duplicate-count 30` = 78 frames). Read frames: off is clean testsrc2; the
30 duplicated frames **bloom/melt** — the rainbow diagonal dissolves, the clock
digits smear into macroblock glitches, blocky codec decay scatters across the frame
(the real quantized-macroblock look the simulation only approximated). Mean
frame-to-frame delta **5.982 → 4.081 /255** (repeated identical-motion frames change
less per step than normal motion).

## Real bitstream mosh — motion transfer — LANDED (experimental, non-deterministic)

The classic "swap A's motion onto B's content" mosh, and — contrary to the
original "likely FFglitch" guess — achievable with the **same pure-Rust AVI chunk
surgery**, no FFglitch. Both clips are encoded to the P-frame-only MPEG-4
substrate; `avi.rs::transfer_motion` keeps the **carrier**'s (Source B) leading
I-frame (its appearance) and then replays the **modulator**'s (Source A) P-frames
(its motion vectors + residuals), so B's pixels are pushed around by motion that
never belonged to them. The carrier supplies the rebuilt headers (output inherits
its dimensions + `idx1` convention), so the modulator is encoded **scaled to the
carrier's size** (`encode_datamosh_avi_scaled`) — the macroblock grids must match,
or the spliced P-frames address a grid that no longer exists (`transfer_motion`
guards this with an `avi_dimensions` equality check).

- **CLI:** `datamosh-bitstream <MODULATOR> <OUT> --operation motion-transfer
  --carrier <CARRIER> [--carrier-keyframes N]`. For this op the positional `input`
  is the modulator (Source A, motion donor); `--carrier` is Source B. Missing
  `--carrier` is a clear error. `--carrier-keyframes` (default `1`) keeps that many
  leading carrier frames before the modulator's motion takes over (`1` = pure
  transfer = just the I-frame).
- **Algorithm id:** `datamosh_bitstream_motion_transfer_experimental_v1`. The
  sidecar records both `input` (modulator) and `carrier`, `carrier_keyframes`, and
  the usual `deterministic: false` + ffmpeg version.
- **Carve-out:** identical to P-frame bloom / keyframe removal — the surgery is
  deterministic + unit-tested (5 new `avi.rs` tests: splice order, carrier-keyframes
  retention, dimension-mismatch + no-P-frame rejection, `avi_dimensions`), but the
  decoded look depends on the external MPEG-4 codec, so it lives outside the render
  graph (no queue/SwiftUI/parity).
- **Verification (off-vs-on, look check).** Modulator A = `testsrc2` (strong,
  varied motion), carrier B = `mandelbrot` (a recognizable fractal), both 160×120
  @ 24fps, `--operation motion-transfer --carrier mandelbrot.mp4`. Frame 1 of the
  output is **byte-identical to the carrier** (the I-frame seed; cross-delta
  0.000); subsequent frames show the fractal's palette dragged and smeared by
  testsrc2's macroblock motion (testsrc2's moving structures bleed in as the
  vectors carve B's content). Frame-to-frame delta **8.83/255** (vs the plain
  carrier transcode's 3.94 — A's motion is more energetic than B's gentle zoom).
  Read-confirmed: B's appearance, A's motion.

## Real bitstream MV remix — pure-Rust MPEG-4 MV editing — LANDED 2026-07-14

The previously-deferred "true codec artifact" tier, green-lit 2026-07-13 after the
user asked for ffglitch-core's effects. FFglitch itself is a **GPL fork of FFmpeg**,
so extracting its code violates the no-vendored-FFmpeg / no-GPL invariant; instead
we implement the same signature effects with a **pure-Rust MPEG-4 Part 2 P-VOP
parser** that decodes, edits, and re-encodes the macroblock layer's motion vectors,
extending the existing `datamosh-bitstream` carve-out CLI.

**Scope.** Exactly the substrate the tier already standardizes on: FFmpeg's LGPL
`mpeg4` encoder output (`-c:v mpeg4 -bf 0 -g 999999 -sc_threshold 0 -an`, AVI).
That means MPEG-4 Simple Profile, rectangular VOL, progressive, no B-VOPs, single
video packet per VOP. The parser must **reject with a clear error** any syntax it
does not support: quarter-pel, GMC/sprite, data partitioning, reversible VLC,
interlace, resync markers mid-VOP, scalability, complexity estimation, short
header. Intra macroblocks inside P-VOPs (the encoder emits them when inter
prediction is poor) **must** be supported, including intra DC/AC coding and
`ac_pred_flag`. 1MV and 4MV (`+mv4`) inter macroblocks both supported.

**Module.** `crates/morphogen-media/src/mpeg4/` — bit reader/writer, VLC tables
(P/I-MCBPC, CBPY, MV, inter+intra TCOEF, DC size; values are ISO/IEC 14496-2
interoperability constants, independently implemented in Rust), VOL/VOP header
parse, full P-VOP macroblock-layer parse to structured MBs + re-emit. No
`unwrap()`; `thiserror` via `MediaError`.

**Operations** (new `datamosh-bitstream --operation` variants, each editing every
P-VOP's motion vectors then re-encoding differentials against recomputed median
predictors, clamped to the VOP's `fcode` range):
- `mv-zero` — all MVs → 0 (freeze/ghost drift; residuals keep painting).
- `mv-pan --mv-pan-x N --mv-pan-y N` — constant half-pel offset (the classic pan).
- `mv-scale --mv-scale F` — amplify (>1), dampen (<1), or invert (<0) motion.
- `mv-sink` — replace each MV with the running average of all MVs seen so far
  (ffglitch's "average motion" melt).
- `mv-sine --mv-sine-amp A --mv-sine-period P` — position-dependent sinusoidal
  warp across the macroblock grid.

**Acceptance criteria — all met.**
1. **Bit-exact round-trip**: `mpeg4::verify_roundtrip` re-emits every P-VOP of
   real FFmpeg fixtures byte-identical (plain, `+mv4`, `-mbd rd`; ≥ 60 P-VOPs
   each), gated in `cargo test` (skips cleanly without ffmpeg) alongside
   pure-Rust synthetic tests. **Key discovery:** FFmpeg's mpeg4 encoder
   slice-threads by default on multicore machines, so real files contain
   **video packets** (resync marker + mb_number + quant + HEC) mid-VOP — the
   parser/emitter handles them, including the packet-boundary predictor
   availability rules (`resync_mb_x` / first-slice-line) and regenerated
   pre-marker stuffing. Edited streams re-parse byte-exactly (self-consistency),
   and quarter-pel input is rejected with a clear error.
2. **Off-vs-on decode** (640×360 harp footage, 6 s, 143 P-VOPs, 18 144 MVs):
   within-sequence `frame-delta.py` off **0.262** → pan(6,2) **1.409**, sine
   **2.767**, zero **0.182** (motion frozen — *below* off, as it should be),
   sink **0.243**; same-index cross-delta vs off at frame 144: pan **23.8/255**
   (scene shredded into diagonal macroblock streaks, subject dissolved), sine
   **18.7** (painterly wavy warp), sink **6.7** (moving regions rainbow-melt,
   static field survives), zero **3.6** (ghost freeze). Read-confirmed.
3. Identity parameters (pan 0/0, scale 1.0) return the input verbatim —
   sidecar reports `changed_mvs: 0`.
4. Carve-out honoured: surgery deterministic + unit-tested, decode external
   (`deterministic: false` sidecar with an `mv_edit` params/counters block);
   queue add/run persists the knobs with serde defaults.

## Deferred

- **DCT-coefficient glitching** (ffglitch's rainbow-block noise): the same parser
  already walks TCOEF blocks, so a later slice can expose coefficient edits;
  deliberately out of scope for the MV milestone.
- **Stateless motion-transfer mode** — `out[i] = warp(B[i], flowA[i])` (content
  always fresh, no melt); a second mode if a use case shows it mattering.
- **Stateless motion-transfer mode** — `out[i] = warp(B[i], flowA[i])` (content
  always fresh, no melt); a second mode if a use case shows it mattering.
