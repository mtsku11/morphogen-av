# Audio Pipeline

Audio analysis starts with portable Rust implementations so tests can define expected behavior before platform acceleration is added.

## Buffer Model

`AudioBufferF32` stores interleaved floating-point samples with explicit channel count, sample rate, and frame count. Offline jobs should operate on deterministic frame and sample ranges.

## WAV Stems

The Rust audio path can load WAV files and write 32-bit float WAV stems. The first CLI export path reads a WAV, applies optional gain, and writes a deterministic stem; later render jobs should replace this with graph-owned stem buses and sample-accurate scheduling.

## RMS

RMS envelopes provide a simple, reliable descriptor for audio-to-video and audio-to-audio modulation. The first implementation is portable and windowed. The CLI can load a modulator WAV and use the RMS envelope to add frame-addressed displacement amount during `render-frame-sequence`; this is the first working audio-to-video routing slice.

## Spectral Centroid

Spectral centroid estimates brightness from a magnitude spectrum. The initial implementation is a small deterministic DFT-based reference suitable for tests, not a production FFT path.

## STFT

The STFT module has a deterministic portable magnitude-cache implementation. It mixes channels by mean, applies a configurable Hann, Hamming, or rectangular window, computes naive DFT magnitudes, and serializes an inspectable `stft_magnitude_v1` JSON sidecar through `morphogen cache-stft`. This is a correctness/cache-shape reference, not a production FFT path. Future work should add complex spectra, phase policy, inverse transforms, binary cache storage, and an Accelerate/vDSP backend.

## Onset Strength

The first onset detector computes positive spectral flux from consecutive STFT magnitude frames and serializes an inspectable `onset_strength_v1` JSON sidecar through `morphogen cache-onsets`. It is intended as a deterministic scalar control curve for routing transients into visual or audio parameters. Future work should add normalization, smoothing, adaptive thresholding, and sample-accurate event extraction.

## Convolution

Convolution starts as a documented skeleton and simple direct implementation for tiny buffers. Production convolution should move to FFT-backed processing.

## Granular Analysis

Future granular analysis should cache grain indexes, descriptors, envelope data, and source provenance so visual and audio grains can be recomposed together.

## Accelerate/vDSP

Accelerate/vDSP should eventually power FFT, STFT, convolution, spectral centroid, onset detection, and phase-vocoder/cross-synthesis work on Apple Silicon.

## Offline Goals

Offline render jobs should support sample-accurate scheduling, WAV and stem export, analysis cache reuse, and deterministic alignment between audio descriptors and video frames.
