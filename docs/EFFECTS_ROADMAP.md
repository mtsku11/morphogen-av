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
- First MVP version: frame-luma controls gain or pan.
- Future high-quality version: time-resampled visual descriptors drive spectral audio processing.

## Controlled Datamosh / Motion-Vector Reuse

- Modulator input: Source A motion vectors or optical flow.
- Carrier input: Source B compressed or decoded frames.
- Output: B frames warped by reused or remapped motion.
- Cached analysis: future motion-vector data, optical flow fields.
- First MVP version: flow-field reuse on decoded float frames.
- Future high-quality version: codec-aware motion-vector extraction and controlled remapping.

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
