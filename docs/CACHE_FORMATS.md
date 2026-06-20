# Cache Formats

Cache files are reusable sidecars derived from source media, analysis settings, and algorithm versions. The project stores a manifest; cache files should remain regenerable.

## Optical Flow Fields

Dense per-frame vector fields with width, height, coordinate convention, frame index, source timing, and algorithm version.

The first implemented cache format is a single-frame `flow_field_f32` directory written by `morphogen-render`:

- `manifest.json` stores `version`, `kind`, `algorithm`, `width`, `height`, coordinate-space notes, vector-unit notes, and frame file records.
- `frame_000000.flowf32` stores `MGFLW001`, little-endian `u32` width/height, then little-endian `f32` x/y vectors in row-major order.
- Coordinates are output pixel coordinates. Vectors are pixel offsets before the render node's `amount` scale is applied.
- The current CLI can write synthetic swirl flow caches, luminance-gradient flow caches, and frame-sequence cache roots where each rendered frame has a `frame_000000/`, `frame_000001/`, etc. subdirectory containing a v1 single-frame cache. Multi-frame source timing and provenance are next.

## Masks

Float or binary masks used for routing, compositing, selective displacement, and effect isolation.

## Depth Maps

Per-frame depth or disparity maps, with model/backend provenance and normalization metadata.

## Audio RMS

Windowed RMS envelopes with sample rate, window size, hop size, channel policy, and time mapping.

## Audio STFT

The first implemented audio spectral cache is an inspectable JSON sidecar written by `morphogen cache-stft`:

- `cache_format: "stft_magnitude_v1"`
- sample rate, source channel count, channel mix policy, FFT size, hop size, window type, and bin count
- frame records with index, start time in seconds, and magnitude bins

It stores magnitudes only. Future formats should add complex spectra, phase convention, binary chunk storage for large media, and provenance linking back to the source asset and analysis settings.

## Onset Maps

The first implemented onset cache is an inspectable JSON sidecar written by `morphogen cache-onsets`:

- `cache_format: "onset_strength_v1"`
- source cache format, sample rate, and hop size
- frame records with index, start time in seconds, and positive spectral-flux strength

Future formats should add thresholded onset events, smoothing settings, confidence values, and provenance linking back to the STFT cache or source WAV.

## Spectral Descriptors

Centroid, bandwidth, rolloff, flatness, and future descriptors over audio windows.

## Grain Indexes

Audio/video grain start times, durations, descriptors, media provenance, and cache version.

## Render Job Metadata

Render settings, frame ranges, output targets, cache dependencies, resume checkpoints, and status.

## Temporal Feedback Checkpoints

Feedback state is render state, not reusable analysis. The implemented feedback bundle writes `checkpoint.json` beside immutable `state/feedback_frame_*.rgba32f` files after every frame. The checkpoint references the last acknowledged state file, so an interrupted write cannot replace the state required by the prior checkpoint. The JSON checkpoint contains contract version, next frame index, node settings, reset frame, input frame checksums, source/cache provenance, state path, and state descriptor. The binary state file stores `MGFDBK01`, little-endian version, width, height, FNV-1a-64 checksum, then unquantized row-major RGBA32F pixels. PNG or ProRes outputs are export artifacts and are never read back to resume a deterministic job. A changed input fingerprint, settings value, analysis producer, or state checksum rejects the checkpoint.

## Frame Provenance

Frame-level records of source inputs, analysis cache versions, node graph hash, and render kernel versions. This is important for reproducibility and debugging.
