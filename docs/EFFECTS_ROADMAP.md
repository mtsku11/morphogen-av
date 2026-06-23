# Effects Roadmap

## Flow Feedback and Advection (Completed Milestone)

- Modulator input: Source A motion fields or luminance gradients.
- Carrier input: Source B and previous output frame.
- Output: feedback trails pushed by modulator motion.
- Cached analysis: flow fields and masks; render checkpoints hold the previous float output state.
- First MVP version: one feedback buffer, temporal pyramidal Lucas-Kanade flow, fixed frame order, verified float checkpoints, reset frames, and CPU/Metal parity.
- Future high-quality version: occlusion-aware flow, float feedback chains, higher temporal integration, and high-bit-depth ProRes export.

## Optical-Flow Advection

- Modulator input: Source A video frames.
- Carrier input: Source B video frames or a flow-feedback state.
- Output: carrier pixels advected through A-derived flow.
- Cached analysis: dense optical flow fields and image pyramids.
- First MVP version: replace the current spatial luminance-gradient signal in feedback with a deterministic two-frame flow estimator.
- Future high-quality version: multiscale optical flow with temporal smoothing and Metal acceleration.

## Video Vocoder

- Modulator input: Source A luminance, edge maps, spectral bands, or motion descriptors.
- Carrier input: Source B color, texture, or frequency bands.
- Output: carrier decomposed and reweighted by modulator descriptors.
- Cached analysis: luminance pyramids, edge maps, spectral descriptors.
- First MVP version: luma-band gain routing. **Landed** (CPU + CLI + parity-gated
  Metal + queue + SwiftUI) — see `docs/VIDEO_VOCODER_MILESTONE.md`. Ships two
  modes: **`match`** (default, histogram-specification tonal-envelope transfer —
  the stronger headline look) and **`gain`** (per-band luma-histogram gain).
- Future high-quality version: multiband spatial-frequency analysis with GPU kernels.

## AV Granular Mosaicing

- Modulator input: Source A visual/audio descriptors.
- Carrier input: Source B audiovisual grains.
- Output: recomposed audiovisual grains selected by descriptor similarity.
- Cached analysis: grain indexes, RMS, onset maps, color/luma descriptors.
- First MVP version: fixed-size visual tiles selected by Source A luma, with deterministic variation and paired PNG-frame sequence output.
- Future high-quality version: multimodal nearest-neighbor grain scheduling.

## Spectral Audio Cross-Synthesis

- Modulator input: Source A audio spectrum.
- Carrier input: Source B audio spectrum.
- Output: B spectrum shaped by A.
- Cached analysis: STFT, spectral centroid, onset maps.
- First MVP version: RMS or centroid controls a simple filter/gain path.
- Future high-quality version: phase-vocoder cross-synthesis with Accelerate-backed FFT.

## Audio-to-Video Descriptor Routing

- Modulator input: Source A audio descriptors.
- Carrier input: Source B video parameters.
- Output: video parameters modulated by RMS, centroid, or onsets.
- Cached analysis: RMS envelopes, onset strength, spectral descriptors.
- First MVP version: RMS controls displacement amount. **Landed** (CPU + CLI +
  parity-gated Metal + queue + SwiftUI) — see `docs/AUDIO_VIDEO_ROUTE_MILESTONE.md`.
  A's peak-normalized RMS envelope drives the per-frame displacement amount fed
  to the existing flow displace; uniform field, `amount 0` = passthrough.
- Future high-quality version: sample-accurate descriptor curves routed into render nodes.

## Video-to-Audio Descriptor Routing

- Modulator input: Source A visual descriptors.
- Carrier input: Source B audio parameters.
- Output: audio transformed by motion, brightness, edge density, or depth.
- Cached analysis: luminance, edge maps, optical flow, depth maps.
- First MVP version: frame-luma controls gain or pan. **Landed** (CPU + CLI +
  queue + SwiftUI; CPU-only — audio is not a GPU target) — see
  `docs/VIDEO_AUDIO_ROUTE_MILESTONE.md`. Source A's peak-normalized per-frame
  mean luma drives B's amplitude (`gain`) or equal-power stereo pan (`pan`);
  `amount 0` = byte-identical Source B passthrough.
- High-quality version: **Landed** (CPU + CLI + queue + SwiftUI) — three axes:
  an **optical-flow magnitude** descriptor (`--descriptor flow`, motion instead of
  brightness, reusing the Lucas-Kanade estimator), a **filter** audio target
  (`--mode filter`, the descriptor sweeps a one-pole LP/HP cutoff, sharing
  cross-synth's filter core), and **time-resampled descriptor curves**
  (`--sampling smooth`, linear interpolation vs the hold-last step). Still
  deferred: edge-density descriptor, pitch/playback-rate target (bit-repro risk),
  depth (no pipeline), and phase-vocoder spectral processing.

## Controlled Datamosh / Motion-Vector Reuse

- Modulator input: Source A motion vectors or optical flow.
- Carrier input: Source B compressed or decoded frames.
- Output: B frames warped by reused or remapped motion.
- Cached analysis: future motion-vector data, optical flow fields.
- First MVP version: flow-field reuse on decoded float frames.
  **Landed** (CPU + CLI + parity-gated Metal + queue + SwiftUI) — see
  `docs/DATAMOSH_MILESTONE.md`. Recursive flow-reuse "bloom/melt": Source A's
  per-frame Lucas-Kanade flow repeatedly advects Source B's previous output;
  `--keyframe-interval` snaps back to B (`1` = passthrough, `0` = full melt,
  `N` = pulse), `--amount` scales the flow. A stateful temporal node carrying the
  previous output as RGBA32F; the advect step reuses the parity-gated
  `flow_displace`. Algorithm id `flow_reuse_datamosh_bloom_cpu_v1`.
- Codec-*simulated* ("block") tier: **Landed** (CPU + CLI + free parity-gated Metal
  + queue + SwiftUI) — `--block-size` quantizes A's flow to a coarse block grid
  (one mean vector per block) before the advect, so whole macroblocks slide
  coherently. `block_size ≤ 1` ≡ the smooth bloom path; id
  `flow_reuse_datamosh_block_cpu_v1` for blocks ≥ 2px.
- Block-residual accumulation: **Landed** (CPU + CLI + free parity-gated Metal +
  queue + SwiftUI) — `--residual-gain`/`--residual-decay` accumulate the
  intra-block motion discarded by quantization in a per-pixel buffer and re-inject
  it (a fine-motion haze atop the macroblock slide). `gain 0` ≡ block path; id
  `flow_reuse_datamosh_block_residual_cpu_v1` (blocks ≥ 2px and gain > 0).
- Per-block keep/drop pseudo-keyframes: **Landed** (CPU + CLI + free parity-gated
  Metal + queue + SwiftUI) — `--block-refresh-threshold`: macroblocks whose mean
  motion is below the threshold snap back to the carrier `B[i]` (intra-block
  refresh, content-driven like a codec's intra map) while busier blocks rot, so the
  smear trail self-erases behind a moving subject. `threshold 0` ≡ block/residual
  path; threshold above max block motion ≡ a whole-frame keyframe; id
  `flow_reuse_datamosh_block_refresh_cpu_v1` (blocks ≥ 2px and threshold > 0). This
  completes the codec-simulated block tier.
- Real bitstream mosh (P-frame "bloom"): **Landed as an experimental,
  non-deterministic CLI** (`datamosh-bitstream`) — the authentic codec-artifact
  tier, kept inside an explicit invariant carve-out. ffmpeg encodes the input to a
  P-frame-only AVI/MPEG-4 (LGPL encoder, no GPL dep); pure-Rust RIFF surgery
  (`morphogen-media/src/avi.rs`) duplicates a chosen P-frame's compressed chunk so
  its motion vectors re-bloom on redecode; ffmpeg decodes to PNGs. Lives OUTSIDE the
  deterministic render graph (no RenderJobTask/queue/SwiftUI, no parity gate); output
  is not bit-reproducible by design (a sidecar records params + ffmpeg version).
  Algorithm id `datamosh_bitstream_pframe_dup_experimental_v1`.
- Future high-quality version: codec-aware motion-vector extraction and controlled
  remapping. Remaining deferred ops on the bitstream path: I-frame removal
  (transition/void mosh) and motion-transfer (likely FFglitch). The richer FFglitch
  vocabulary stays deferred behind the same carve-out.

## Convolutional Audio/Video Blending

- Modulator input: Source A impulse, spectrum, or image kernel.
- Carrier input: Source B audio or image.
- Output: convolved audio or spatial image blend.
- Cached analysis: kernels, spectra, frame provenance.
- First MVP version: tiny direct convolution for audio or image kernels.
  **Landed (image + audio)** (CPU + CLI + queue + SwiftUI; image adds a
  parity-gated Metal kernel) — see `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.
  *Image:* each Source A frame is box-downsampled into a normalized K×K luma
  kernel; Source B's frame is directly convolved with it and blended by `amount`
  (`amount 0` = passthrough). Algorithm id `image_kernel_convolution_blend_cpu_v1`.
  *Audio:* Source A is an impulse response, L1-normalized to a mono IR, and each
  Source B channel is convolved with it (convolution-reverb-style), blended
  wet/dry by `amount` (`amount 0` = passthrough; the wet tail extends the
  output). CPU-only. Algorithm id `impulse_response_convolution_blend_cpu_v1`.
  The audio HQ tier — **FFT convolution** (`--method fft`, a pure-Rust radix-2
  FFT gated against the direct path within 1e-4) and **IR resampling**
  (`--resample-impulse`, opt-in deterministic Lanczos) — is **landed** (CPU +
  CLI + queue + SwiftUI); see `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.
  The HQ image/audio modes are also **landed**: **per-channel colour kernels**
  (`--kernel-mode color`, a parity-gated `convolution_blend_color` Metal kernel,
  algorithm id `image_color_kernel_convolution_blend_cpu_v1`) and **per-channel /
  true-stereo IRs** (`--ir-mode per-channel`, algorithm id
  `per_channel_impulse_response_convolution_blend_cpu_v1`), both across CPU + CLI
  + queue + SwiftUI. The image Metal kernel already handles large K (no cap).
- Future high-quality version: tiled large-K Metal (perf only); separable image
  kernels.

## Scanline / Rutt-Etra Style Carrier Modulation

- Modulator input: Source A luminance, depth, or audio envelope.
- Carrier input: Source B frame or generated scanline mesh.
- Output: displaced scanline geometry or rasterized carrier.
- Cached analysis: luminance maps, depth maps, RMS envelopes.
- First MVP version: luma-derived vertical displacement.
- Future high-quality version: Metal mesh or compute-driven line rendering with temporal supersampling.

## Descriptor-Coagulated Flow Blend (proposed — experimental, deterministic)

The first **mutual** two-source effect: instead of A merely *modulating* B, pixels
from **both** sources are mangled together. Cells of the screen group into irregular
**coagulated patches** by descriptor similarity (colour, luma, texture, motion,
optionally audio), and those patches **advect, smear, and collide** over time through
a vector field. The target look is extreme experimental glitch — pixelated,
datamosh-adjacent, fluid, unstable, patchy, temporally alive — explicitly *not* an
even crossfade, checkerboard, or uniform mosaic.

- Modulator input: Source A frames + descriptors (colour, luma, texture, optical-flow
  magnitude, optionally audio RMS/centroid).
- Carrier input: Source B frames + the same descriptor set, **plus** the previous
  frame's ownership field (stateful temporal node).
- Output: B and A interleaved as moving clumps of visual material, with hard/dirty
  patch edges, block jitter, and history smear.
- Cached analysis: per-cell colour/texture descriptors (reuses the granular-mosaic
  descriptor set), Lucas-Kanade flow fields for A and/or B, and the advected
  ownership/mixture field checkpoint (RGBA32F, like the flow-feedback state).

**Model — a low-resolution ownership/mixture field.** At patch resolution (cells of
`patch_size` px, the same grid shape as `quantize_flow_to_blocks`), each cell holds a
mixture weight `w ∈ [0,1]` (`0` = all B, `1` = all A). Per frame:
1. **Descriptors** — per-cell `mean_color [3]` + `texture [2]` (luma variance,
   gradient magnitude) for A and B, reusing the granular-mosaic feature extraction;
   motion/audio dims appended later.
2. **Coagulate** — update `w` toward whichever source's descriptor is more similar to
   a seeded per-cell target, then run spatial-coherence relaxation passes (a cell is
   pulled toward its neighbourhood's ownership) so patches *clump* instead of forming a
   checkerboard, with seeded randomness breaking uniformity. Deterministic PCG/splitmix
   keyed by `(seed, frame, cell)` — no wall-clock.
3. **Advect** — drift the ownership field through A-flow, B-flow, mixed flow, or
   synthetic turbulence, reusing the parity-gated `flow_displace` backward warp on the
   field stored in an `ImageBufferF32` channel, so patches smear and collide.
4. **Composite** — per output pixel, sample the upsampled advected `w`, bilinearly
   sample A and B, and blend; `edge_hardness` controls soft lerp vs a noise-dithered
   hard threshold (dirty edges), `block_jitter` adds per-cell sub-pixel offset.
5. **Smear (optional)** — feed the composite through the existing
   `flow_feedback_frame_cpu` history/structure machinery for trails.

- First MVP version (Slice 1 — deterministic CPU, single frame, no advection):
  **Landed** (CPU + CLI). Per-cell A/B descriptors → seeded + coherence-relaxed
  ownership field → hard/soft composite. Continuity identity: `coagulation_strength 0`
  (with `randomness 0`, `bias 0`) ⇒ `w ≡ 0` ⇒ Source B verbatim (the off case for the
  off-vs-on readout). Algorithm id `descriptor_coagulated_flow_blend_cpu_v1`.
- Temporal advection (Slice 2 — stateful): **Landed** (CPU + CLI). The ownership
  field is carried frame-to-frame and advected each frame by a chosen flow —
  `--advect-source {a-flow|b-flow|mixed|turbulence}` × `--advect-amount` — by packing
  the field into an `ImageBufferF32` channel and reusing the parity-gated
  `flow_displace` warp (advection comes free). `--refresh` blends the advected history
  toward the fresh descriptor field (`1` = re-seed every frame ≡ Slice 1; `0` = the
  field only advects). Frame-zero = descriptors only; the prior state is the
  unquantized field carried in memory (never a display PNG). `advect_amount 0` +
  `refresh 1` is byte-identical to Slice 1.
- Dirty edges + history smear (Slice 3): **Landed** (CPU + CLI). `--block-jitter`
  applies a per-cell coherent sub-block offset to the ownership-field lookup (whole
  blocks of the patch boundary shift, ragged/datamosh-y; `0` = clean grid).
  `--smear`/`--smear-decay` hold a decayed fraction of the previous output into each
  frame, leaving RGB trails as patches move (alpha stays from the composite so the
  blend stays opaque; `smear 0` = no trail). Both continuity-safe at `0`.
- Metal composite (Slice 4): **Landed** (parity-gated). `--backend metal` runs the
  per-pixel composite (block jitter + bilinear field sample + dithered hard/soft edge
  blend + A/B lerp) as a `coagulated_composite` Metal kernel, gated against the CPU
  `composite_with_field` per frame (tolerance `1/255`). Compiled with fast-math
  disabled and the splitmix64 hash replicated in MSL so the hard-edge threshold
  decision matches the CPU bit-for-bit. The ownership-field build/advance (cheap,
  iterative, neighbour-coupled) stays CPU; advection already rode the parity-gated
  `flow_displace`. This completes the effect's first full CPU→Metal vertical.
- Future high-quality version: curl-noise turbulence advection, multi-class ownership
  (more than two sources / hybrid phases), motion- and audio-driven coagulation, and a
  Metal field-build/advance kernel (the remaining CPU stage) gated against the CPU
  reference.
- Future high-quality version: curl-noise turbulence advection, multi-class ownership
  (more than two sources / hybrid phases), motion- and audio-driven coagulation, and a
  Metal field-update kernel gated against the CPU reference.
