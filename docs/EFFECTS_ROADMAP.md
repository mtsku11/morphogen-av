# Effects Roadmap

## Optical-Flow Advection

- Modulator input: Source A video frames.
- Carrier input: Source B video frames.
- Output: carrier pixels advected through A-derived flow.
- Cached analysis: optical flow fields, image pyramids.
- First MVP version: synthetic or cached flow displaces carrier frames.
- Future high-quality version: multiscale deterministic optical flow with temporal smoothing and Metal acceleration.

## Flow Feedback

- Modulator input: Source A motion fields or luminance gradients.
- Carrier input: Source B and previous output frame.
- Output: feedback trails pushed by modulator motion.
- Cached analysis: flow fields, masks, frame provenance.
- First MVP version: one feedback buffer and synthetic flow.
- Future high-quality version: float feedback chains with temporal supersampling.

## Video Vocoder

- Modulator input: Source A luminance, edge maps, spectral bands, or motion descriptors.
- Carrier input: Source B color, texture, or frequency bands.
- Output: carrier decomposed and reweighted by modulator descriptors.
- Cached analysis: luminance pyramids, edge maps, spectral descriptors.
- First MVP version: luma-band gain routing.
- Future high-quality version: multiband spatial-frequency analysis with GPU kernels.

## AV Granular Mosaicing

- Modulator input: Source A visual/audio descriptors.
- Carrier input: Source B audiovisual grains.
- Output: recomposed audiovisual grains selected by descriptor similarity.
- Cached analysis: grain indexes, RMS, onset maps, color/luma descriptors.
- First MVP version: fixed-size visual tiles selected by luma.
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
- First MVP version: RMS controls displacement amount.
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
- Future high-quality version: FFT convolution and Metal spatial kernels.

## Scanline / Rutt-Etra Style Carrier Modulation

- Modulator input: Source A luminance, depth, or audio envelope.
- Carrier input: Source B frame or generated scanline mesh.
- Output: displaced scanline geometry or rasterized carrier.
- Cached analysis: luminance maps, depth maps, RMS envelopes.
- First MVP version: luma-derived vertical displacement.
- Future high-quality version: Metal mesh or compute-driven line rendering with temporal supersampling.
