# Status

Session-resume checkpoint. Update at the end of any working session so a fresh
session (or a fresh agent) can pick up in seconds. Keep it short; durable detail
lives in `docs/`, cross-session findings live in `/memory/`.

_Last updated: 2026-06-24_

## Baseline (verified)

- `cargo test --workspace`: **288 passing across 7 crates, 0 failing.**
  One benign warning (`block v0.1.6` transitive dep, future-Rust deprecation).
- `swift test`: **47 passing, 0 failing** (Swift shell + service tests).
- Tree clean as of the experimental bitstream-datamosh commit. Manual-testing
  clips (`cello.mp4`, `cello2.mp4`, `harp.mp4`) are gitignored, not tracked.

## What just landed

- **Fluid Colour-Sort Mosaic — steady-vortex flow mode (Slice 9, CPU + CLI).** The
  perfected faux-fluid vortex field, now driving the mosaic *tiles* as a new opt-in flow
  (the user asked to "add as a new mode" — analytic fluid + value-noise turbulence stay).
  Extracted the steady-vortex field into a shared `vortex_field.rs`
  (`steady_vortex_velocity`); `fluid_advect` refactored to call it (verified
  byte-identical). `--vortex-flow > 0` adds that velocity to each tile so colour domains
  flow and swirl along persistent vortices; four serde-defaulted knobs; algo id v8 → v9;
  `vortex_flow 0` skips the call ⇒ byte-identical to v8. **Tuning:** a *steady, coherent*
  field advects all tiles the same way, so past ~0.4 it sweeps tiles out of their domains
  faster than cohesion refills → black voids; sweet spot ≈0.2–0.3 (domains swirl while
  staying space-filling). The mosaic is discrete tiles held by cohesion, so it can't be
  pushed as hard as the continuous dye. **Off-vs-on** (harp/cello, vortex 0.25 scale
  0.006): frame 0 byte-identical, cross-delta ≈42/255 f30 → ≈46/255 f59. New unit test.
  See [[fluid-colour-sort-mosaic]].

- **Faux-fluid dye advection — NEW effect (`fluid_advect.rs`, CPU + CLI).** A separate,
  single-source effect that ports the *Faux Fluid Sim* shadertoy **pixel** behaviour —
  built after the mosaic's turbulence knob (Slice 8) read as "≈off" because the mosaic is
  a tile/particle system and the blocky sorted look dominates. This is a **continuous
  per-pixel feedback advection**: a dye buffer is advected semi-Lagrangian-style (sample
  the previous frame at `p − v·advect` via `sample_bilinear_clamped`) along the same
  divergence-free curl-of-value-noise velocity field, and a little of the current source
  frame is bled back in each frame (`--reinject` = the "frame refresh"). The video becomes
  liquid and marbles — no tiles, no particles (Read-confirmed on harp: the figure
  dissolves into swirling dye trails). **Velocity field reworked to match the shader
  (id v1 → v2)** after feedback that it was "wobbly" vs the shader's flowing swirls:
  switched value noise → **3D gradient (Perlin) noise** (round vortices) and — the key
  fix — made the big-vortex octave **steady** (a fixed noise z-slice) so the dye flows
  along its streamlines and **spirals into the vortex centres** over frames (that
  accumulation *is* the swirl; an evolving field only wobbles in place). Only a 0.1
  `--detail` octave drifts. Defaults retuned so material wraps several times: advect 12,
  reinject 0.05 (lower = dye persists/spirals more), turbulence-speed 0.06. Frame-delta
  3.46 → 2.19 (calmer); Read-confirmed the same vortices persist across frames while dye
  spirals through them. Stateful temporal node: frame 0 = source verbatim,
  prior state = RGBA32F dye buffer (the checkpoint). CLI `render-fluid-advect-sequence`
  (single source). **Levers:** `--advect` (flow strength), `--reinject` in [0,1] (0 = pure
  smear, 1 = source verbatim, ~0.08 marble). Continuity identities unit-tested (reinject 1
  ⇒ source; advect 0 + reinject 0 ⇒ hold previous). **Readout** (harp, advect 6 reinject
  0.08): output frame 0 = source (cross-delta 0.000), source-vs-fluid ~14.5/255 steady,
  within-sequence frame-delta 3.45/255 (continuously flowing). Workspace 282 → 287 (+5
  tests). Deferred variants: optical-flow-driven, two-source A→B, a discrete carrier
  (tiles/particles on this field), Metal. See [[faux-fluid-advect]].

- **Fluid Colour-Sort Mosaic — faux-fluid turbulence (Slice 8, CPU + CLI).** Ported the
  *faux-fluid* shadertoy look. The analytic fluid field is a regular swirl lattice; with
  `--turbulence > 0` a curl-of-value-noise streamfunction is **added** to it, giving the
  flow organic, evolving, multi-scale currents. Two octaves of value noise built on the
  existing splitmix `hash01` (GPU-safe — the reference shaders' own `sin()`-hashing is
  flagged accuracy-dependent), lattices drifting in different directions so the field
  *evolves* not just translates; the velocity is the analytic curl `(∂/∂y, -∂/∂x)` by
  central finite difference, **divergence-free by construction** (so the tuned force
  balance is preserved), normalized by `--turbulence-scale` so amplitude reads in pixels.
  Shares the dispersion band's `fluid_gain`. Three serde-defaulted settings, algo id
  bumped `…_v7` → `…_v8`; `turbulence 0` early-returns a zero contribution ⇒ off path
  **byte-identical** to v7. **Tuning finding:** amplitude is in the same pixel units as
  `fluid_strength` (≈0.5) — sweet spot ≈**0.6** (coherent domains, organic currents);
  overdriving (≈6) reproduces the boil-to-confetti failure mode *globally* (what the
  dispersion band does locally). **Off-vs-on readout** (harp→cello, `--turbulence 0.6`):
  frame 0 byte-identical (turbulence is advance-time only), cross-delta ≈23/255 by frame 5
  → ≈41/255 by frame 59; Read confirms coherent marbling along irregular currents, not
  boil. Workspace 281 → 282. New unit test (turbulence perturbs both tiles, off is
  byte-identical regardless of scale/speed, deterministic). See
  [[fluid-colour-sort-mosaic]]. Metal port deferred.

- **Fluid Colour-Sort Mosaic — spatially-varying dispersion band (Slice 7, CPU + CLI).**
  The destabilizing forces made spatially *local*: `--dispersion-band > 0` adds a
  soft-edged vertical band whose centre sweeps across the canvas
  (`--band-width`/`--band-speed`/`--band-start`) and amplifies each in-band tile's
  jitter + fluid advection during the per-frame **advance** — so colour domains boil
  apart into scattered confetti where the wipe sits while the rest stays coherent. This
  is the effect's documented failure-mode #3 ("high fluid + jitter → boil to gas")
  turned into a spatially-confined, opt-in glitch-wipe. `dispersion_band_weight` is a
  pure smoothstep-falloff function of `(x, frame)` with a toroidal wrap so the sweep
  loops; the modulation lives entirely in `advance_fluid_mosaic` (the warmup **settle is
  untouched**, so behind the sweep cohesion re-gathers the scattered tiles —
  disperse-then-re-form). Four serde-defaulted settings, algo id bumped `…_v6` → `…_v7`;
  `dispersion_band 0` multiplies the fluid gain by exactly 1.0 and adds 0 jitter ⇒ the
  off path is byte-identical. **Off-vs-on readout** (harp→cello, band 6, width 0.3,
  sweep 0.02/frame): **frame 0 byte-identical** (band is advance-time only), cross-delta
  growing monotonically from 0 (≈16/255 by frame 11, ≈50/255 by frame 47) as the wipe
  scatters tiles; Read confirms shatter localized to the band + its trailing wake,
  coherent domains ahead of it. Workspace 280 → 281. New unit test (2-tile state: the
  in-band tile moves ≥1.5× farther with the band on, the out-of-band tile byte-identical,
  off-path geometry-independent, deterministic). See [[fluid-colour-sort-mosaic]]. Metal
  port deferred.

- **Fluid Colour-Sort Mosaic — cluster-blob layout (Slice 6, CPU + CLI).** The
  deferred alternate layout. `--cluster-blob` swaps the cohesion *target*: each tile is
  pulled toward its colour bin's **global** centroid (precomputed once per force pass by
  `global_bin_centroids`) instead of the local same-colour mean, so each colour gathers
  into one compact blob rather than phase-separating into screen-filling domains. This is
  the effect's documented failure-mode #1 ("global-centroid → collapse to points") turned
  into an opt-in feature; stiff repulsion still keeps a blob a disc, not a point. Setting
  `cluster_blob: bool` serde-default false, algo id bumped `…_v5` → `…_v6`; the
  local-cohesion branch is untouched so the off path is byte-identical. **Centralization
  caveat** (why it isn't the default): spatially-uniform colours share a near-identical
  centroid (the canvas centre), so blobs only separate when each colour is spatially
  concentrated. **Off-vs-on readout** (fixture: red split into two discs + a blue disc,
  cohesion amplified, fluid/jitter off): cluster-blob merges the two red discs into one
  blob at red's global centroid while local cohesion keeps them as two domains (Read
  confirmed); cross-delta **57.8/255 at frame 0** (the *settle pass* already runs the
  cluster force, so frame 0 diverges — unlike refresh/resort which are frame-0-identical)
  settling to **49.5/255**. Workspace 279 → 280. New unit test builds a 2-tile state
  directly: local force ~0 across the gap, cluster pulls both to the midpoint by exactly
  dist·cohesion. See [[fluid-colour-sort-mosaic]]. Metal port deferred.

- **Controlled Datamosh — REAL bitstream mosh, P-frame "bloom" (experimental,
  non-deterministic CLI).** The authentic codec-artifact tier — mangles the
  *compressed stream* (not decoded float frames) so the decoder itself produces the
  glitch. New standalone `datamosh-bitstream` CLI subcommand: ffmpeg encodes the
  input to a P-frame-only AVI/MPEG-4 (LGPL `mpeg4` encoder, **no GPL dep**), pure-Rust
  RIFF surgery (`crates/morphogen-media/src/avi.rs`) duplicates a chosen P-frame's
  compressed chunk `--duplicate-count` times so its motion vectors re-bloom on
  redecode, ffmpeg decodes to PNGs. **Explicit invariant carve-out** (this tier was
  always gated on one): lives OUTSIDE the deterministic render graph — **no
  RenderJobTask / queue / SwiftUI, no parity gate**; output is **not bit-reproducible**
  by design (a `datamosh_bitstream.json` sidecar records params + ffmpeg version +
  `deterministic: false`). The AVI surgery itself *is* deterministic + unit-tested on
  synthetic byte buffers (`--duplicate-count 0` = exact identity / off case). Id
  `datamosh_bitstream_pframe_dup_experimental_v1`. **Off-vs-on (look check, not a
  determinism proof):** 2s `testsrc2`, P-frame 5, count 0 (48 frames) vs 30 (78
  frames) — the duplicated frames bloom/melt (rainbow diagonal dissolves, clock digits
  smear into macroblock glitches, blocky codec decay); frame-to-frame delta 5.982 →
  4.081 /255. Workspace 272 → 279. See [[datamosh-real-vs-simulated]],
  [[datamosh-bitstream-pframe-bloom]]. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh — per-block keep/drop pseudo-keyframes (full vertical
  slice).** The patchy "some macroblocks refresh, some rot" half of the aesthetic,
  completing the codec-simulated block tier. After the recursive advect, each
  macroblock whose **mean-motion magnitude** is below `--block-refresh-threshold`
  "keeps" — it snaps back to the carrier `B[i]` (an intra/I-block refresh) — while
  busier blocks are denied refresh and keep rotting. **Content-driven** like a
  codec's intra-block map (not injected noise): calm blocks refresh, busy blocks
  smear, so the trail behind a moving subject **self-erases** (calm regions snap
  back to clean `B`) leaving the smear only at the subject's current position. A
  per-block composite over the *output* of the parity-gated displace, so **Metal
  came free again** (the Metal refresh path renders, per-frame gate passing); a
  refreshed block also **clears its residual accumulator** (intra-block reset).
  `--block-refresh-threshold` on `render-datamosh-sequence` + queue + a macOS Block
  Refresh stepper. Continuity: `threshold 0` ≡ the block/residual path
  (byte-identical); a threshold above the largest block motion ≡ a whole-frame
  keyframe (carrier verbatim, accumulator cleared); `block_size ≤ 1` ≡ bloom. New id
  `flow_reuse_datamosh_block_refresh_cpu_v1` via `datamosh_algorithm(block_size,
  residual_gain, refresh_threshold)` — **only** for blocks ≥ 2px **and** threshold >
  0 (a separate id, precedence refresh > residual > block > bloom, no id bump); job
  field `serde(default)` (=0 ≡ off). **Off-vs-on readout** (bouncing-square A over a
  static stripe+dot B, block 16, full melt): refresh off (`threshold 0`) vs on
  (`threshold 1.0`) cross-sequence delta grows **0 → 31.6/255** (frame 0 identical =
  both `B[0]`); frames Read — off = a cumulative smear everywhere the square has
  been, on = the diagonal stripes stay clean (trail self-erases) with the smear only
  at the square's current position. Workspace 265 → 272; Swift 46 → 47. See
  [[datamosh-codec-block-tier]]. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh — block-residual accumulation tier (full vertical slice).**
  The quantization-noise half of the macroblock aesthetic. Quantizing A's flow to a
  block mean discards the intra-block detail (`residual = flow − block_mean`); this
  tier accumulates it in a **per-pixel residual flow buffer** (`accum = accum·decay
  + residual`) and re-injects it (`effective = block_mean + accum·gain`) into the
  advecting flow, so macroblocks slide coherently **and** shed a trailing
  fine-motion haze. Still a **pure flow→flow transform** (`datamosh_residual_flow`),
  so the displace stays the existing parity-gated kernel and **Metal came free
  again** (no new kernel — the Metal render ran the residual path, per-frame gate
  passing). `--residual-gain` / `--residual-decay` on `render-datamosh-sequence` +
  queue + two macOS steppers. Continuity: `gain 0` short-circuits to the block path
  (byte-identical); `gain 1` first P-frame ≡ the smooth bloom (raw-flow) displace;
  `block_size ≤ 1` ≡ bloom (residual is a no-op without quantization). New id
  `flow_reuse_datamosh_block_residual_cpu_v1` via `datamosh_algorithm(block_size,
  residual_gain)` — **only** for blocks ≥ 2px **and** gain > 0 (a separate id, no
  block-id bump); job fields `serde(default)` (=0 ≡ off). **Off-vs-on readout**
  (high-motion bouncing-square A over a static stripe+dot B, block 16, full melt):
  residual off (`gain 0`) vs on (`gain 1, decay 0.9`) cross-sequence delta grows
  **0 → 33.8/255** (frame 0 identical = both `B[0]`); frames Read — the coherent
  macroblock slide gains a divergent streaky haze (stripes smear, the dot drags
  into a comet). Workspace 258 → 265; Swift 45 → 46. See
  [[datamosh-codec-block-tier]]. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh — codec-simulated ("block") tier (full vertical slice).**
  The first deferred datamosh tier: A's per-frame optical flow is **quantized to a
  coarse `block_size`×`block_size` grid** (one mean motion vector per block) before
  the recursive advection, so whole macroblocks slide coherently — the chunky
  "real datamosh" look vs the smooth per-pixel bloom. The only new pixel logic is
  `quantize_flow_to_blocks` (a pure flow→flow transform); the heavy displace is
  still the existing parity-gated kernel, so **Metal came free — no new kernel**.
  `--block-size` knob on `render-datamosh-sequence` + queue + a macOS Macroblock
  Size stepper; `block_size ≤ 1` ≡ the smooth bloom path (byte-identical), so the
  resolved algorithm id (`datamosh_algorithm`) is the new
  `flow_reuse_datamosh_block_cpu_v1` **only for blocks ≥ 2px**. Job field is
  `serde(default)` (=0 ≡ smooth) so legacy datamosh jobs keep their meaning.
  **Off-vs-on readout** (high-motion bouncing-square A over a static stripe+dot B):
  smooth (block 1) vs blocky (block 16) cross-sequence delta grows **0 → 35.9/255**
  (frame 0 identical = both `B[0]`); frames Read — block 16 melts into large
  coherent wavy warps (16px regions slide together) where block 1 shatters into
  per-pixel speckle. Workspace 250 → 258; Swift 44 → 45. See
  [[datamosh-codec-block-tier]]. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh / Motion-Vector Reuse — full vertical slice (CPU + CLI +
  Metal + queue + SwiftUI).** The roadmap's "flow-field reuse on decoded float
  frames" MVP, A→B, the deterministic flow-reuse tier (real melt/bloom, *in the
  datamosh family* but not the authentic macroblock/bitstream artifact — see
  [[datamosh-real-vs-simulated]]). A **stateful temporal node**: Source A's
  per-frame Lucas-Kanade optical flow (`A[i-1]→A[i]`, reusing
  `pyramidal_lucas_kanade_flow_cpu`) repeatedly advects Source B's *previous
  output* (the carrier is frozen from the last keyframe and smears under A's
  motion). **Recursive accumulate + keyframe refresh** (the chosen model of three):
  `out[0]=B[0]`; `is_datamosh_keyframe(i,K)` ⇒ snap back to `B[i]`, else
  `flow_displace(out[i-1], flowA[i], amount)`. `--keyframe-interval` `1` = exact B
  passthrough, `N` = pulse, `0` = full melt from B[0]; `--amount` scales the flow.
  CPU core `datamosh.rs` (`datamosh_bloom_frame_cpu`, 6 tests, algorithm id
  `flow_reuse_datamosh_bloom_cpu_v1`) delegates the advect branch to the
  parity-gated `flow_displace_cpu`. The recursion carries the previous output as
  **RGBA32F in memory** (unquantized internal state; disk checkpoint/resume
  deferred — the `write_flow_feedback_state` serializers exist to reuse).
  `render-datamosh-sequence` CLI + parity-gated `--backend metal` (reuses
  `flow_displace_metal`, gated per-frame). Persisted `frame_sequence_datamosh`
  queue job (backend serde-default CPU; queue-add/run, manifest carries
  algorithm + keyframe_interval/amount/backend) + a macOS Render-panel section
  (A/B/output pickers, keyframe-interval + amount steppers, CPU/Metal backend).
  **Off-vs-on readout** (high-motion A square over a static stripe+dot B fixture,
  `scripts/make-datamosh-fixture.py` + `scripts/dm-cross-delta.py`): interval 1 =
  **0.000/255** passthrough vs B; interval 0 melts **0 → 17.06/255** as B[0]
  accumulates A's rightward motion (frames Read — the dot stretches, stripes drag).
  **Metal nuance:** per-frame parity gate passes, but the end-to-end Metal sequence
  is **not** byte-identical to CPU (max drift **0.013/255**) because the recursion
  compounds sub-epsilon float diffs across frames — same accepted pattern as the
  recursive `flow_feedback` Metal path; Metal is byte-reproducible across runs
  (determinism-first holds per-backend). Queue add→run byte-identical to direct
  (smoke test pins it + the manifest knobs). Workspace 243 → 250; Swift 42 → 44.
  MVP feature-complete. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Video-to-Audio Descriptor Routing — HQ tier (3 vertical slices; CPU + CLI +
  queue + SwiftUI, CPU-only).** The three deferred axes of the MVP, built
  incrementally. **(1) Optical-flow descriptor** (`--descriptor flow`): per-frame
  mean Lucas-Kanade flow magnitude (motion) instead of mean luma, reusing the
  parity-gated `lucas_kanade_flow_cpu`; frame 0 = 0 (no prior frame). The gain/pan
  routes were made descriptor-neutral (`descriptor_gain_route`/`_pan_route` take
  arbitrary `(time,value)` samples); the algorithm id is composed in core
  (`video_audio_route_algorithm_id`) as `{descriptor}_{mapping}_route_cpu_v1` —
  **luma ids byte-unchanged**, flow added `flow_gain/flow_pan/flow_filter`.
  **(2) Filter target** (`--mode filter --filter-type lowpass|highpass`): the
  descriptor sweeps a one-pole cutoff on B, reusing a `one_pole_filter_sweep`
  factored out of `centroid_filter_cross_synth` (cross-synth's f64 path
  byte-unchanged). **(3) Time-resampled curves** (`--sampling hold|smooth`):
  `hold` steps (default, byte-identical to the MVP), `smooth` linearly
  interpolates between frames — centralized in `DescriptorEnvelope::resample`,
  shared by gain/pan/filter. New core enums `VideoAudioRouteDescriptor` /
  `VideoAudioRouteFilterType` / `VideoAudioRouteSampling` (all serde-defaulted to
  the MVP meaning) + task fields; manifest records descriptor/filter_type/sampling;
  Render-panel pickers (descriptor, filter-type shown in filter mode, envelope).
  **Off-vs-on readouts:** flow→gain (moving-square fixture) OFF flat 0.5, ON tracks
  motion 0.00→0.11→0.22→0.32→0.43→0.50; luma→lowpass (HF-content metric) OFF flat
  0.9999, ON 0.00 (closed) →0.92 (open); hold-vs-smooth (coarse ramp) max
  consecutive-sample jump 0.1255 (staircase) vs 0.000126 (~1000× smoother).
  Queue add→run byte-identical to direct (3 smoke tests: luma-pan/flow-gain-smooth/
  filter-highpass). Workspace 236 → 243; Swift unchanged at 42 (tests extended).
  Contract: `docs/VIDEO_AUDIO_ROUTE_MILESTONE.md`.

- **Video-to-Audio Descriptor Routing — full vertical slice (CPU + CLI + queue +
  SwiftUI; CPU-only).** The roadmap's "frame-luma controls gain or pan" MVP, the
  cross-modal mirror of Audio-to-Video routing (there A's audio shaped B's video;
  here A's *video* shapes B's *audio*). Source A's **peak-normalized per-frame
  mean Rec.709 luma** envelope (hold-last by frame time at `--fps`) drives Source
  B's WAV: **`gain`** = luma scales B's amplitude (`out = B·lerp(1,luma,amount)`,
  the shape of `rms_gain_cross_synth`); **`pan`** = luma drives an equal-power
  stereo pan of mono-mixed B (`pan=(2·luma−1)·amount`, dark→left, bright→right,
  output 2-channel). CPU-only (audio has no Metal target). The luma is computed
  by the CLI (which owns image decoding) and handed to `morphogen-audio` as raw
  `(time,luma)` samples, keeping the audio crate image-decoupled (the symmetric
  decoupling `audio_route.rs` keeps from audio). `video_route.rs` in
  morphogen-audio (`luma_gain_route` / `luma_pan_route`, 10 tests) +
  `render-video-audio-route` CLI + persisted `video_audio_route` queue job
  (core `VideoAudioRouteMode` enum serde-default Gain;
  `queue-add-/queue-run-video-audio-route` writing `audio/video_audio_route.wav`
  + a manifest carrying algorithm/mode/amount/fps) + a macOS Render-panel section
  (A frames / B WAV / output pickers, mode + amount + fps). Algorithm ids
  `luma_gain_route_cpu_v1` / `luma_pan_route_cpu_v1`. `amount 0` = byte-identical
  passthrough (mono B stays mono). **Off-vs-on readout** (8-frame dark→bright A,
  steady tone B, fps 8): gain off flat 0.354 RMS, on dark **0.035** / bright
  **0.330** (amplitude tracks A's luma ramp); pan off mono flat, on dark
  **L 0.349 / R 0.055** (left), bright **L 0.055 / R 0.349** (right). Queue add→run
  byte-identical to the direct render (smoke test pins it + the manifest knobs,
  pan mode). Workspace 223 → 236; Swift 40 → 42. MVP feature-complete. Contract:
  `docs/VIDEO_AUDIO_ROUTE_MILESTONE.md`.

- **CLI module split (behavior-preserving refactor).** The monolithic
  `crates/morphogen-cli/src/main.rs` (8127 lines) was decomposed into eight
  modules with no logic change — the `run()` dispatch body is unchanged:
  `error.rs` (CliError), `imaging.rs` (PNG/image/fingerprint leaf utils),
  `args.rs` (Cli/Commands + all `Cli*` value-enums + From impls + mode/algorithm
  helpers), `project.rs` (init/probe/extract/cache/inspect/proxy), `audio.rs`
  (cross-synth + impulse-convolution render & queue), `render.rs` (all direct
  `render_*` handlers + granular controls + provenance + feedback + shared render
  consts), `queue.rs` (queue add/run + manifests + checkpoints + bundle writers;
  depends one-directionally on render). `main.rs` is now **786 lines** (imports +
  `main` + `run` dispatch). Cross-module request structs got `pub(crate)` fields.
  Verified: cli tests 34/34, clippy clean, `cargo test --workspace` green
  (baseline unchanged). A new effect now adds its command to `args.rs`, render
  handler to `render.rs`, queue handler to `queue.rs` — bounded files, not a
  monolith.

- **Convolutional AV Blending (per-channel colour kernels + true-stereo IRs +
  large-K Metal verify) — three vertical slices.** The remaining deferred HQ
  items. **Colour kernels** (`--kernel-mode color`): a separate K×K kernel from
  each of A's R/G/B channels, applied channel-wise (chromatic structure transfer);
  parity-gated `convolution_blend_color` Metal kernel (three weight buffers), algo
  id `image_color_kernel_convolution_blend_cpu_v1`; CPU + CLI + queue + SwiftUI.
  Off-vs-on (luma vs colour, K=7): **mean 24/255, max 130**, 0 vs identical.
  **Per-channel IRs** (`--ir-mode per-channel`): each carrier channel convolved
  with its own IR from the matching A channel (cycling when counts differ),
  CPU-only, algo id `per_channel_impulse_response_convolution_blend_cpu_v1`; CPU +
  CLI + queue + SwiftUI. Off-vs-on (mono vs per-channel, stereo identity/smear IR):
  **max abs diff 0.48 (L) / 0.35 (R)**, 0 vs identical. **Large-K Metal:** the
  existing image kernel already convolves arbitrary odd K (no cap) — proved with a
  K=11 CPU + Metal parity test; a tiled perf kernel is deferred (not a correctness
  gap). Both new modes serde-default to luma/mono so existing jobs keep meaning.
  Workspace 208 → 223; Swift 38 → 40. Contract:
  `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Convolutional AV Blending (audio HQ tier: FFT method + IR resampling) —
  full vertical slice (CPU + CLI + queue + SwiftUI; CPU-only).** The two deferred
  audio items. **FFT** (`--method fft`): a new pure-Rust radix-2 Cooley-Tukey FFT
  (`morphogen-audio/src/fft.rs`, forward+inverse over f64, no new deps — the STFT
  is magnitude-only with no inverse) computes the per-channel convolution in the
  frequency domain; same transform as the direct `O(B·L)` loop, gated against it
  within `FFT_DIRECT_PARITY_EPSILON` (1e-4). **IR resampling**
  (`--resample-impulse`, opt-in): a deterministic 3-lobe Lanczos resampler maps
  A's IR to B's rate (L1 after resampling so the gain bound survives), instead of
  the default hard error on a rate mismatch. New `ConvolutionMethod` enum (audio +
  core), serde-default `method`/`resample_impulse` on the `audio_impulse_convolution`
  job, CLI flags on render/queue-add, manifest records both. Algorithm id
  unchanged (`impulse_response_convolution_blend_cpu_v1` — method is an
  implementation choice, the audio analogue of `backend`). **Off-vs-on readout:**
  FFT vs direct on a 400-tap IR/1000-sample carrier = **max abs diff 5.96e-8**
  (≪ 1e-4; identical length/RMS/peak — FFT *is* the direct path); resample off =
  hard error, on = a 24 kHz IR reconstructs the native-48 kHz IR result within
  **7.8e-6**. FFT+resample queue add→run byte-identical to the direct render
  (smoke test pins it + the manifest knobs). Workspace 198 → 208; Swift 37 → 38.
  Contract: `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Convolutional AV Blending (audio impulse) — full vertical slice (CPU + CLI +
  queue + SwiftUI; CPU-only, no Metal like the cross-synth).** The roadmap's
  "tiny direct convolution for audio kernels" MVP — the other half of
  Convolutional AV Blending. Source A is an **impulse response**: downmix to mono,
  optional `--max-impulse-samples` head-truncation, then **L1-normalize** (so
  `Σ|tap| = 1`, which bounds the wet path — no clip blow-up); a silent A falls
  back to a unit-impulse identity. Each Source B channel is convolved with that IR
  (reusing `convolve_mono`), blended wet/dry by `amount`; the output extends past
  B by `L − 1` (the reverb tail). `--amount 0` = exact B passthrough. New logic in
  `morphogen-audio/src/convolution.rs` (`impulse_convolution_blend`, 9 tests) +
  `render-audio-impulse-convolution` CLI + persisted `audio_impulse_convolution`
  queue task (add/run writing `audio/impulse_convolution.wav` + manifest knobs) +
  a macOS Render-panel section (A IR / B / output pickers, amount + max-IR
  steppers). Algorithm id `impulse_response_convolution_blend_cpu_v1`. **Off-vs-on
  readout (audio, not the image's cross-sequence trick):** a straight OFF
  (`--amount 0`) vs ON (`--amount 1`) WAV compare — ON is **longer by L − 1**
  (4800 → 5039 for a 240-tap IR) and a positive lowpass IR drops **RMS
  0.574 → 0.027** / peak 0.90 → 0.08 (L1-bounded), OFF byte-identical to B,
  deterministic re-render byte-identical, queue add→run byte-identical to the
  direct render (smoke test pins it + the manifest knobs). Workspace 186 → 198;
  Swift 34 → 37. Both MVP halves now landed. Contract:
  `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Convolutional AV Blending (image kernel) — full vertical slice (CPU + CLI +
  Metal + queue + SwiftUI).** The roadmap's "tiny direct convolution for image
  kernels" MVP, A→B and **spatial** (the first effect where A modulates B with a
  *kernel*, not a scalar). Each Source A frame is box-downsampled into a normalized
  K×K luma kernel (bright A regions = heavy taps; black A falls back to uniform);
  Source B's frame is directly convolved with it (centered, clamped border,
  correlation-style) and blended by `amount`. `--amount 0` (or `K=1`) = exact
  Source B passthrough. New `conv_blend.rs` in morphogen-render (`ConvolutionKernel`
  + `analyze_convolution_kernel_cpu` + `convolution_blend_cpu`, 7 tests) +
  parity-gated `convolution_blend` Metal kernel (new `.metal` + runtime fn +
  parity/preflight tests) + `render-convolutional-blend-sequence` CLI (A frames +
  B frames → PNG seq, `--kernel-size`/`--amount`/`--backend`) + persisted
  `frame_sequence_convolution_blend` queue job (backend serde-default CPU;
  queue-add/run writing a frames/ bundle + manifest carrying the convolution
  algorithm id + kernel_size/amount/backend) + a macOS Render-panel section
  (A/B pickers, kernel + amount steppers, CPU/Metal backend). Algorithm id
  `image_kernel_convolution_blend_cpu_v1`. **Off-vs-on readout is cross-sequence,
  not within-sequence** — a spatial blur on a static carrier is invisible to
  `frame-delta.py`; instead render `--amount 0` vs `--amount 1` (K=5) on a
  checkerboard carrier + gradient modulator and diff OFF vs ON frame 0: mean
  per-channel **91.5/255** (the 5×5 kernel collapses the Nyquist checkerboard
  toward gray — Read confirms), OFF deterministic across renders, CPU==Metal
  byte-identical, queue add→run byte-identical to the direct render (smoke test
  pins it + the manifest knobs). Workspace 173 → 186; Swift 32 → 34. MVP
  feature-complete for the image carrier. Contract:
  `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Audio-to-Video Descriptor Routing — full vertical slice (CPU + CLI + Metal +
  queue + SwiftUI).** The roadmap's "RMS controls displacement amount" MVP, A→B
  cross-modal (A's *audio* shapes B's *video*, the complement to the cross-synth's
  A-audio→B-audio). The only new logic is **routing**: A's peak-normalized RMS
  envelope, hold-last per output frame at `--fps`, becomes the scalar `amount`
  fed to the **existing, already-parity-gated** flow displace op over a uniform
  displacement field (`--shift-x/--shift-y`). `--amount 0` (or silent A) = exact
  Source B passthrough. Because the pixel transform is the proven
  `flow_displace_cpu`/`flow_displace_metal`, **Metal came nearly free** —
  `--backend metal` reuses the displace kernel, gated per-frame against CPU.
  `audio_route.rs` in morphogen-render (`RmsDisplacementEnvelope` +
  `uniform_displacement_field`, 7 tests) + `render-audio-video-route-sequence`
  CLI (WAV A + PNG-seq B → PNG seq) + persisted `frame_sequence_audio_video_route`
  queue job (backend serde-default CPU; queue-add/run writing a frames/ bundle +
  manifest carrying the routing algorithm id + every knob) + a macOS Render-panel
  section (Source A WAV / Source B frames / amount+shift steppers / CPU-Metal
  backend). Algorithm id `rms_displacement_route_cpu_v1`. Off-vs-on verified on a
  static-gradient readout: amount 0 frame-delta **0.000/255** (passthrough),
  ramped-A on **0.656/255** (displacement tracks the loud→quiet envelope),
  large-shift frame visibly displaced (Read); OFF deterministic, CPU==Metal
  byte-identical, queue add→run byte-identical to the direct render (smoke test
  pins it + the manifest knobs). Workspace 163 → 173; Swift 30 → 32. MVP
  feature-complete. Contract: `docs/AUDIO_VIDEO_ROUTE_MILESTONE.md`.

- **Spectral Audio Cross-Synthesis — full vertical slice (CPU + CLI + queue +
  SwiftUI).** The roadmap's "RMS or centroid controls a simple filter/gain path"
  MVP, A→B, **time-domain by constraint** (our STFT is magnitude-only with no
  inverse, so phase-vocoder resynthesis stays the deferred HQ tier). Two modes
  share the framing (output follows B; A's descriptor resolved by time-based
  hold-last; `amount=0` = byte-identical passthrough): **`gain`** = A's
  peak-normalized RMS envelope scales B's amplitude; **`filter`** = A's
  spectral-centroid envelope (normalized to Nyquist) sweeps a per-sample one-pole
  LP/HP cutoff on B. CPU-only (audio is not a GPU target — no Metal, nothing to
  parity-gate). `cross_synth.rs` in morphogen-audio (5 tests) +
  `render-spectral-cross-synth` CLI (WAV A + WAV B → WAV out) + persisted
  `audio_spectral_cross_synth` queue job (core enums `CrossSynthMode` /
  `CrossSynthFilterType` / `CrossSynthWindow`, all serde-defaulted;
  `queue-add-/queue-run-spectral-cross-synth` writing `audio/cross_synth.wav` +
  a manifest carrying every knob) + a macOS Render-panel section (mode/amount/
  filter-type + WAV pickers). Algorithm ids `rms_gain_cross_synth_cpu_v1` /
  `centroid_filter_cross_synth_cpu_v1`. Off-vs-on verified numerically (audio has
  no PNG): gain half-amplitude ratio **1.00 → 3.11** (output tracks A's
  loud→silent ramp); filter output centroid **5640 → 1962 Hz** (dark A lowpasses
  bright B). Queue add→run byte-identical to the direct render (both modes; smoke
  test pins it + the manifest knobs). Workspace 155 → 163; Swift 28 → 30. This
  effect is now feature-complete for the MVP. Contract:
  `docs/SPECTRAL_CROSS_SYNTH_MILESTONE.md`.

- **Video Vocoder — full vertical slice (CPU + CLI + Metal + queue + SwiftUI).**
  The roadmap's "luma-band gain routing" effect, built A→B. Two modes share the
  framing: **`match`** (default) = histogram specification (remap B's luma
  distribution onto A's via a 256-level CDF tone map — no neutral point, so it
  stays strong on real footage) and **`gain`** = per-band luma-histogram gain
  routing. Both preserve hue, clamp, and treat `amount=0` as a byte-identical
  passthrough. `render-video-vocoder[-sequence]` (CPU + parity-gated
  `--backend metal` for match), persisted `frame_sequence_video_vocoder` queue job
  (`queue-add-/queue-run-video-vocoder-sequence`, manifest carries mode/algorithm/
  bands/amount/backend), and a Render-panel section (mode/bands/amount/backend).
  **Why match over gain:** on harp→cello, gain reads as a timid grade (natural
  histograms keep `N·a_hist≈1`); match imposes A's whole tonal palette (lifts the
  dark cello frame onto harp's daylight palette) — chosen after a side-by-side
  prototype. Verified: amount=0 byte-identical (direct pixel sample); match
  off-vs-on routes correctly; Metal byte-identical to CPU on HD frames (0.0/255);
  queue add→run byte-identical to direct. Algorithm ids
  `luma_histogram_spec_vocoder_cpu_v1` (match) / `luma_band_gain_vocoder_cpu_v1`
  (gain). gain-mode Metal deferred (errors clearly). Workspace 142→155; Swift
  26→28. Contract: `docs/VIDEO_VOCODER_MILESTONE.md`.

- **Granular step 6b luma-variance + gradient texture dims (render/CLI + queue +
  SwiftUI):** the final 6b feature, landed as a full vertical slice. Each pooled
  grain now carries a 2-dim texture descriptor `[luma_variance,
  gradient_magnitude]` over its tile; `--texture-weight W` (0 = off) scales both
  dims in the per-tile nearest match, querying Source A's per-tile texture, so a
  smooth modulator region draws smooth carrier grains and a busy region draws busy
  ones. Off by default ⇒ byte-identical selection. The pool **algorithm id bumped
  v1 → v2** (descriptor schema changed), so stale v1 sidecars regenerate rather
  than read texture as zero. Plumbed through the persisted job (serde default 0),
  queue-add/run, manifest, and the Render panel (Texture Weight stepper). New
  render-crate test (texture breaks a mean-colour tie: a busy modulator query
  picks the checkerboard grain over the flat one; weight 0 leaves the tie). New
  `--readout texture` fixture mode (flat vs striped frames at equal mean colour);
  off-vs-on readout: OFF mean frame-delta **0.0/255** (colour tie pins to the flat
  grain), ON **48.0/255** with the output tracking the modulator's flat↔stripes
  texture demand (frames Read to confirm); `/parity` OK 8/8 (queue == direct,
  manifest carries `texture_weight`); smoke + Swift bridge tests pin the knob.
  Workspace 141 → 142; Swift unchanged at 26 (existing tests extended). **With
  this, granular step 6b is feature-complete — no algorithmic refinements remain.**
- **Granular step 6b spatial-origin coherence (render/CLI + queue + SwiftUI):**
  the spatial complement to frame coherence, landed as a full vertical slice.
  `--spatial-coherence-weight W` (0 = off) adds a second additive term to
  `TemporalCoherence`: a candidate grain whose origin differs from that tile's
  previous pick adds `W*min(dist_tiles,reach)/reach` to its squared feature
  distance (`dist_tiles` = Euclidean origin distance in grain-tile units, sharing
  `--coherence-reach`). Keeps a tile's pick from teleporting across the frame even
  on a nearby source frame. Off by default ⇒ byte-identical; with either coherence
  weight > 0 the scheduler engages (frame zero still a no-op). Plumbed through the
  persisted job (serde default 0), queue-add/run, manifest, and the Render panel
  (Spatial weight stepper sharing Reach). New render-crate test (spatial weight
  overturns the exact-colour grain toward the previous pick's origin; frame-zero
  no-op); `/parity` OK 4/4 with frame + spatial coherence (queue == direct);
  smoke + Swift bridge tests pin the knob. Workspace 140 → 141; Swift unchanged at
  26 (existing tests extended). With this, the last 6b algorithmic refinement
  remaining is luma-variance/gradient feature dims.
- **Granular step 6b pool-selection knobs — queue/SwiftUI exposure sweep:** the
  persisted `frame_sequence_granular_mosaic_pool` job now carries all four
  direct-render pool knobs — centroid (k=2) STFT caches, trailing pool window,
  anti-repeat (weight + cooldown), and temporal coherence (weight + reach). New
  schema fields are `#[serde(default)]` (off), so jobs serialized before this
  sweep keep their whole-clip / no-scheduler meaning.
  `queue-add-granular-mosaic-pool-sequence` gained the matching flags (same
  both-or-neither centroid validation + finite/non-negative weight checks as the
  direct path); `queue-run` threads them into the render request instead of the
  old hardcoded defaults; the bundle manifest + provenance record them. The macOS
  Render panel adds a Spectral Centroid (k=2) toggle (wires the STFT caches from
  proxy extraction, both-or-neither), a pool-window stepper, and anti-repeat /
  coherence weight+span steppers (span steppers disabled when weight = 0).
  Verified e2e: queue add→run with pool-window + anti-repeat + coherence engaged
  is byte-identical to the direct render with the same flags; extended pool queue
  smoke test asserts the knobs round-trip through task + manifest; 3 new Swift
  bridge tests pin the scheduling flags + centroid-cache args (Swift 23 → 26;
  Rust workspace unchanged at 140 — existing tests extended). With this, the last
  deferred 6b follow-on is closed; only spatial-origin coherence + luma-variance/
  gradient feature dims remain noted as algorithmic refinements.
- **Granular step 6b cross-frame scheduling — temporal coherence (render/CLI
  path):** the smooth-motion complement to anti-repeat. `--coherence-weight W`
  (0 = off) + `--coherence-reach R` (default 8) reward source-frame continuity:
  a candidate grain whose source frame differs from that **same tile's** previous
  pick by `delta` adds `W*min(delta,R)/R` to its squared feature distance (0 when
  unchanged, saturating at `W` once `delta>=R`). State is `prev_selection:
  Vec<Option<u32>>` (one global grain index per output tile) — serializable
  checkpoint rep. Frame zero has an empty history ⇒ byte-identical to
  non-scheduled (declared frame-zero behavior); composes additively with
  anti-repeat; Metal path unaffected (CPU-side selection). New render-crate test
  (coherence overturns color-nearest toward the previous pick's frame; frame-zero
  no-op). Verified e2e on solid-gray footage (rearrangement=1.0 ⇒ output color
  reveals source frame): alternating modulator → off jumps f0↔f3 every frame,
  on (W=5, R=1) holds f0 after an identical frame 0. Workspace 139 → 140.
  Queue/SwiftUI exposure deferred. Spatial-origin coherence deferred.
- **Granular step 6b cross-frame scheduling — anti-repeat (render/CLI path):**
  `--anti-repeat-weight W` (0 = off) + `--anti-repeat-cooldown C` (default 8)
  penalize grains used in recent output frames (penalty `W*(C-age)/C`, linear
  decay) to push temporal diversity. State is `last_used_frame: Vec<Option<u32>>`
  (serializable checkpoint rep). Frame zero has empty history ⇒ byte-identical to
  non-scheduled (declared frame-zero behavior); penalty reshapes only the
  nearest-match distance, Metal path unaffected (CPU-side selection). New
  render-crate test (penalty overturns color-nearest; frame-zero no-op). Verified
  e2e on a colorful carrier + static modulator: off = 1 distinct output frame,
  on = 3 distinct, frame 0 identical / frames 1–3 diverge. Render 53 → 54
  (workspace 139). Queue/SwiftUI exposure deferred.
- **Granular step 6b sliding-window pool scope (render/CLI path):**
  `--pool-window N` bounds each output frame to a trailing window of the last `N`
  carrier frames (`0` = whole-clip). Grains are frame-major, so a trailing window
  is a contiguous global-index slice — `PoolSelectionWindow::Trailing` is a
  selection-only filter (whole-clip sidecar stays reusable; Metal render path
  unaffected; `WholeClip` byte-identical to prior behavior). New render-crate test
  pins window membership. Verified e2e: `--pool-window 1` forces each output frame
  onto its own carrier frame (red→green→blue→white) vs the static whole-clip
  mosaic. Render tests 52 → 53 (workspace 138). Queue/SwiftUI exposure deferred.
- **Granular step 6b k>1 audio dims (render/CLI path):**
  `render-granular-mosaic-pool-sequence` accepts optional
  `--modulator-centroid-cache` / `--carrier-centroid-cache` (STFT caches)
  alongside RMS. The audio vector is `[rms?, centroid?]` (each descriptor
  independently both-or-neither across modulator/carrier), k=0..=2; one
  `audio_weight` scales every dim. CPU core was already k-generic; the Metal
  kernel is untouched (audio drives only CPU-side selection). New render-crate
  test proves a centroid dim flips selection vs RMS-only. Verified end-to-end: on
  a 4-frame solid-color carrier + constant-amplitude chirp (flat RMS, rising
  centroid), k=1 vs k=2 give different mosaics (k=1 frame0 mean greenish, k=2
  pulled to blue/white = higher-centroid frames). Render tests 51 → 52
  (workspace 137). Queue/SwiftUI centroid exposure deferred.
- **Granular step 6b Metal backend in queue + SwiftUI:** the persisted
  `frame_sequence_granular_mosaic_pool` job gained a `backend` field (serde
  default CPU). `queue-add-granular-mosaic-pool-sequence --backend metal` is
  parity-gated frame-by-frame in the run path and the manifest records the
  backend; the macOS Render panel exposes a CPU/Metal segmented selector for the
  pool job. Verified end-to-end: a Metal-backed queue run on generated 48×48
  footage rendered 4 frames (per-frame parity gate passed) with `backend: Metal`
  in the manifest. Swift tests 22 → 23; Rust workspace 136 (unchanged count).
- **Granular step 6b Metal render port (temporal grain pool):** a
  `granular_mosaic_pool` compute kernel renders the cross-frame pooled mosaic on
  the GPU — the whole-clip pool uploads as a 2D texture array (slice per frame),
  a flat grain-metadata buffer resolves each global pool index to
  `(frame_index, origin_x, origin_y)`, integer-nearest clamped sampling +
  `rearrangement` value-blend. `granular_mosaic_pool_metal` is parity-gated by a
  multi-frame runtime test; `render-granular-mosaic-pool-sequence --backend metal`
  gates every frame against the CPU reference before export (queue runs stay CPU).
  Verified on generated footage: Metal output byte-identical to CPU (PSNR inf,
  4 frames). Metal tests 11 → 13. SwiftUI/queue exposure of the Metal backend deferred.
- **Granular step 6b SwiftUI exposure (temporal grain pool):** the macOS Render
  panel gains a `Granular Mosaic — Temporal Pool` section (grain size,
  rearrangement, variation, seed, audio weight, Audio-Weighted RMS toggle). The
  dev bridge shells out to `queue-add-/queue-run-granular-mosaic-pool-sequence`;
  the toggle wires the RMS caches from source-proxy extraction (both-or-neither,
  color-only when off). 3 new bridge arg tests (Swift 19 → 22).
- **Granular step 6b queue task (temporal grain pool):** persisted
  `frame_sequence_granular_mosaic_pool` `RenderJob` variant +
  `queue-add-/queue-run-granular-mosaic-pool-sequence`. Writes a ProRes-ready
  bundle (frames + pool sidecar + `frame_sequence_granular_mosaic_pool` manifest
  carrying the pooled algorithm id, `audio_weight`, and RMS-cache provenance).
  Verified: queue add→run on real footage; queued frames are byte-identical to
  the direct render (determinism across the queue path). SwiftUI + Metal deferred.
- **Granular step 6b CLI wiring (temporal grain pool):** new
  `render-granular-mosaic-pool-sequence` subcommand renders the joint-AV pooled
  path end-to-end. `--audio-weight`, optional `--modulator-rms-cache` /
  `--carrier-rms-cache` (both-or-neither, RMS k=1), and a `grain_pool_descriptors.json`
  sidecar keyed on the whole carrier set. On real footage (harp→cello):
  audio-weighted vs audio-off selection differs in ~26% of pixels. CPU-only.
- **Granular step 6b CPU core (temporal grain pool, joint-AV selection):**
  `pooled_av_nearest_grain_cpu_v1`. Grains are drawn from across time (whole-clip
  pool); each carries its frame's carrier-audio descriptor, so audio is finally a
  real matching dimension. `analyze_grain_pool_cpu` / `select_grains_from_pool_cpu`
  (combined `[mean_color | audio]` weighted NN, scalar `audio_weight`) /
  `granular_mosaic_with_pool_selection_cpu` (rearrangement = cross-frame value
  blend). See milestone step 6b.
- **Granular step 6 (selection slice):** multimodal nearest-neighbor grain
  selection on mean RGB (`multimodal_nearest_grain_cpu_v1`), opt-in via
  `--selection rgb` on the direct, sequence, and queue CLI paths; persisted on
  granular queue jobs + provenance; new `grain_color_descriptors.json` sidecar.
  Selection is CPU-side so the Metal render path + parity gate are untouched.
  Verified end-to-end: rgb vs luma give different coherent mosaics; sidecars
  tagged correctly; algorithm-mismatch recompute works.
- (prior) Source A audio descriptors routed into granular-mosaic controls
  (RMS→variation, onset→rearrangement, centroid→grain-size).

## In flight

On `main` (local commits, not yet pushed). The **Video Vocoder** MVP is now
feature-complete end-to-end (CPU + CLI + parity-gated Metal for match mode + queue
job + SwiftUI). Granular step 6b remains feature-complete. The vocoder's
deferred items: gain-mode Metal port, a reusable Source-A luma-band histogram
sidecar (currently recomputed per frame), spatial-frequency (multiband) routing,
and the reverse/cross-clip look exploration. **Spectral Audio Cross-Synthesis**
is now a feature-complete MVP vertical slice (CPU + CLI + queue + SwiftUI, gain +
filter modes). Its deferred HQ tier is phase-vocoder cross-synthesis (needs a
complex-STFT + inverse + Accelerate-FFT path first). **Audio-to-Video Descriptor
Routing** (RMS→displacement) is now a feature-complete MVP vertical slice too
(CPU + CLI + parity-gated Metal + queue + SwiftUI); its deferred items are
spatially varying displacement fields (sine/radial/Source-A flow), other
descriptor targets (centroid→hue, onset→cut), and sample-accurate descriptor
curves (HQ tier). **Convolutional AV Blending** is now feature-complete across
both MVP halves, the audio HQ tier (FFT method + Lanczos IR resampling), **and
the HQ image/audio modes** — per-channel **colour kernels** (`--kernel-mode
color`, parity-gated Metal) and per-channel **true-stereo IRs** (`--ir-mode
per-channel`, CPU-only), each CPU + CLI + queue + SwiftUI. The image Metal kernel
already handles large K (no cap; proved by a K=11 parity test). Its only remaining
deferred items are a *tiled* large-K Metal kernel (perf only, not correctness) and
separable image kernels. **Video-to-Audio Descriptor Routing** is now feature-complete across the MVP
(luma → gain/pan) **and its HQ tier**: an optical-flow magnitude descriptor
(`--descriptor flow`), a filter audio target (`--mode filter`, LP/HP), and
time-resampled smooth descriptor curves (`--sampling smooth`) — each CPU + CLI +
queue + SwiftUI. Its only remaining deferred items are an edge-density descriptor
(near-free), a pitch/playback-rate target (bit-repro risk), depth (no pipeline),
and phase-vocoder spectral processing (gated on a complex-STFT + inverse, shared
with the cross-synth HQ tier). **Controlled Datamosh / Motion-Vector Reuse** is
now a feature-complete MVP vertical slice too (CPU + CLI + parity-gated Metal +
queue + SwiftUI) — recursive flow-reuse "bloom/melt" with a keyframe-interval
knob. Its deferred items are the codec-*simulated* mosh tier (16×16 block grid,
residual accumulation, pseudo-keyframes — visually closer, still deterministic),
the real bitstream/FFglitch tier (needs an invariant carve-out — see
[[datamosh-real-vs-simulated]]), a stateless motion-transfer mode, and disk
checkpoint/resume. With this the EFFECTS_ROADMAP MVPs are all landed; the next
work is HQ tiers / deferred follow-ons rather than a new unstarted effect.

## Candidate next steps

From `docs/BACKLOG.md` "Next" and `docs/EFFECTS_ROADMAP.md`:

1. **Granular step 6b remaining** — CPU core + CLI render path + pool sidecar +
   queue task + SwiftUI exposure + Metal render port (`--backend metal`,
   parity-gated) + Metal backend in queue/SwiftUI all landed. Deferred within 6b:
   k>1 audio dims (add centroid), sliding-window pool scope, and cross-frame
   scheduling (anti-repeat / temporal coherence).
2. **Next roadmap effect** — Video Vocoder (luma-band gain routing MVP) or
   Spectral Audio Cross-Synthesis (RMS/centroid filter path) are the natural
   next vertical slices.
3. **Deferred / low-priority** — Metal parity port for the multiscale
   structure-preserving morph, then its queue/SwiftUI exposure. Per the manual
   testing finding it's CPU-only and marginal on real footage; don't invest until
   a use case shows it mattering (see `docs/BACKLOG.md` + [[flow-feedback-levers]]).

## Known truths to respect

- Single-scale `--structure-mix` is the keeper for "beyond recognition" feedback;
  multiscale is correct-but-marginal. `--feedback-mix` is the dissolve cliff.
- Every new Metal kernel must parity-gate against the CPU reference before export.
