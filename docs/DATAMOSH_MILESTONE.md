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
- **Still deferred within this tier:** block-residual accumulation and per-block
  keep/drop pseudo-keyframes (the residual/quantization-noise half of the
  macroblock aesthetic) — block-quantized motion is the visual core; residuals are
  an additive refinement if a use case shows it mattering.

## Deferred (not this slice)

- **Real bitstream mosh** (tier 3, FFglitch) — the only route to authentic
  artifacts; breaks determinism + CPU-parity + no-new-required-tool invariants.
  Needs an explicit invariant carve-out (see `/memory/datamosh-real-vs-simulated.md`).
- **Stateless motion-transfer mode** — `out[i] = warp(B[i], flowA[i])` (content
  always fresh, no melt); a second mode if a use case shows it mattering.
- **Disk checkpoint / resume** — the RGBA32F state serializers exist
  (`write_flow_feedback_state`); wiring the datamosh loop to resume mid-sequence
  lands after the MVP.
- **Reusable optical-flow sidecar** for A — the flow-feedback path already caches
  temporal flow; sharing that cache here is a later optimization.
