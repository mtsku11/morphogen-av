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

## Colour-Group Dispersion Blend

The **content-advecting** sibling of the coagulation blend. Coagulation composites A
and B *in place* behind a moving ownership mask (a moving-edge dissolve); this path
advects the image **content itself**, per block, so colour-grouped tiles physically
flow, shatter, and intermix. The target look: crisp glitch tiles of both sources that
first flow together along a directional current, then break apart and disperse from
their groups (perpetual churn).

- Modulator/Carrier: Source A + Source B frames (matched dims); A's optical flow is
  the directional current.
- Output: both sources sampled at a per-block displaced coordinate and blended by the
  (also displaced) colour-group ownership field, so A-tiles and B-tiles interleave.
- Cached/stateful: the colour-group ownership field (reused from the coagulation
  effect) and a per-block content-offset field, both carried frame-to-frame.
- First MVP version: **Landed** (CPU + CLI `render-dispersion-blend-sequence`). A
  stateful per-block offset accumulates `coherent·current + dispersion·scatter`
  (damped/bounded); `dispersion` ramps `0→1` then churns. Block size = fine tiles;
  frame-zero starts in place. Algorithm id `colour_group_dispersion_blend_cpu_v1`.
  Locked v1 forks: directional-current-then-scatter, perpetual churn, fine tiles.
- Future high-quality version: a spatially-varying **dispersion band** (concentrate the
  shatter along a transition curve, like a glitch wipe), selectable/synthetic flow
  fields (turbulence, radial, custom vector fields), transition and disperse-then-
  re-form arcs, coarse/mixed tile scales, and a parity-gated Metal composite.

## Fluid Colour-Sort Mosaic

The **relocation** effect, where coagulation and dispersion keep each tile roughly
where it started. Tiles of *both* sources become crisp **particles** that are sorted
by colour into screen-filling domains, then advected by a fluid current so the colour
groups flow, fold, and intermix — the source footage is unrecognisable throughout
(target reference: the marcscully.com fluid background). User-locked forks: emergent
self-sorting, hybrid "crisp tiles ride a fluid", uniform tile size.

- Modulator/Carrier: Source A + Source B frames (matched dims); the simulation is
  seeded from the first frame of each and runs self-contained.
- Output: a particle set rendered as crisp colour tiles (painter order A-then-B,
  uncovered = black).
- Cached/stateful: per-tile positions + velocities carried frame-to-frame; fixed
  per-tile mean colour + colour bin.
- Forces (the whole look is their balance): **local same-colour cohesion** (pull to
  the local mean of nearby same-colour tiles → phase separation into domains) +
  **stiff colour-blind repulsion** (incompressible pressure that keeps the sheet
  space-filling so domains can't contract into voids) + a deterministic
  divergence-free **fluid curl field** (gentle, so domains marble rather than boil to
  gas) + tiny jitter. A warmup *settle* runs cohesion+repulsion before frame zero so
  the first frame is already colour-grouped.
- First MVP version: **Landed** (CPU + CLI `render-fluid-mosaic-sequence`). Algorithm
  id `fluid_mosaic_colour_sort_cpu_v1`; off case (`--cohesion 0 --repulsion 0
  --fluid-strength 0 --jitter 0 --settle-iterations 0`) = source grids overlaid.
- **Texture-carrying tiles: Landed** (algorithm id bumped to
  `fluid_mosaic_colour_sort_cpu_v2`). Each tile carries its source cell's original
  pixel patch; with `carry_texture` on (the default) the render paints that patch
  (nearest-sampled into the tile square) so footage grain survives instead of a flat
  mean colour. Sorting/cohesion still key on the mean colour, so the temporal motion
  is byte-identical to flat — `--flat-tiles` is the off case (v1 look) and isolates
  exactly the texture (off-vs-on cross-delta ≈9.3/255).
- **Adaptive (varying) tile sizes: Landed** (algorithm id bumped to
  `fluid_mosaic_colour_sort_cpu_v3`). With `--adaptive-tiles`, a quadtree subdivides
  each `tile_size` cell down toward `--min-tile-size` wherever local colour variance
  exceeds `--subdivide-threshold`, so flat regions stay coarse and detailed regions
  go fine. Repulsion targets each pair's average size (**floored at `repulsion_radius`**
  so small tiles can't over-pack and collapse into voids), and the render paints
  largest-tiles-first (stable, so uniform output is unchanged). Off by default;
  omitting `--adaptive-tiles` is the off case (off-vs-on cross-delta ≈46/255).
- **Live per-frame colour refresh (render-only): Landed** (algorithm id bumped to
  `fluid_mosaic_colour_sort_cpu_v4`). With `--live-refresh`, each tile remembers its
  source-origin cell and re-samples its painted colour/patch from the **current**
  source frame every frame, so the two videos play through the flowing mosaic. The
  simulation (positions and the frozen frame-zero colour bins that drive sorting) is
  untouched — render-only — so the force balance is unchanged and toggling refresh
  isolates exactly the live content. Off by default; omitting `--live-refresh` is the
  off case (frame 0 byte-identical; off-vs-on cross-delta grows as the clips play,
  ≈9.7/255 by frame 59). Sources cycle if the render outlasts a clip.
- **Sim-driving live re-sort: Landed** (algorithm id bumped to
  `fluid_mosaic_colour_sort_cpu_v5`). With `--live-resort`, the per-frame re-sample also
  **re-bins** each tile from the current source frame, so the cohesion force (which keys
  on the bin) follows the live colour and colour domains **migrate to track the video**
  rather than staying frozen in their frame-zero grouping. Positions/velocities carry
  forward; the force balance still holds (coverage stays space-filling — no boil to
  confetti, no black voids — confirmed by Reading frame 59). Off by default; the off case
  is render-only `--live-refresh` (bins frozen). Frame 0/1 byte-identical, off-vs-on
  cross-delta grows as the re-binned cohesion steers domains apart (≈22/255 by frame 30,
  ≈28/255 by frame 59 — ~3× the render-only refresh because positions, not just painted
  pixels, diverge).
- **Cluster-blob layout: Landed** (algorithm id bumped to
  `fluid_mosaic_colour_sort_cpu_v6`). With `--cluster-blob`, cohesion pulls each tile
  toward its colour bin's **global** centroid (the mean position of *all* same-colour
  tiles) instead of the local same-colour mean, so each colour gathers into a single
  compact **blob** rather than phase-separating into screen-filling domains in place;
  stiff repulsion still keeps a blob a disc, not a point. `cohesion_radius` is ignored
  for the cohesion pull in this mode (the reach is global). Off by default (local
  cohesion is the off case). Caveat: spatially-uniform colours share a near-identical
  centroid (the canvas centre), so the blobs separate only when each colour is
  spatially concentrated in the source. Off-vs-on readout (a fixture with red split
  into two discs + a blue disc): cluster-blob merges the two red discs into one blob at
  red's global centroid while local cohesion keeps them as two domains — cross-delta
  ≈57.8/255 at frame 0 (the settle pass already diverges) settling to ≈49.5/255.
- **Spatially-varying dispersion band: Landed** (algorithm id bumped to
  `fluid_mosaic_colour_sort_cpu_v7`). With `--dispersion-band > 0`, a soft-edged
  vertical band whose centre sweeps across the canvas (`--band-width`, `--band-speed`,
  `--band-start`) amplifies each in-band tile's jitter + fluid advection during the
  per-frame advance, so colour domains boil apart into scattered confetti where the
  wipe currently sits while the rest of the mosaic stays coherent — failure-mode #3
  ("high fluid + jitter → boil to gas") turned into a spatially-confined, opt-in
  glitch-wipe. Advance-time only (the warmup settle is untouched), so cohesion
  re-gathers the scattered tiles behind the sweep (disperse-then-re-form). Off by
  default (`--dispersion-band 0`); frame 0 is byte-identical (the band acts only on
  advance) and the off-vs-on cross-delta grows from 0 as the wipe scatters tiles
  (≈16/255 by frame 11, ≈50/255 by frame 47 on the harp/cello clip).
- Next: a parity-gated Metal port for the fluid mosaic.
