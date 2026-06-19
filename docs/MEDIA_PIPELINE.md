# Media Pipeline

The first CLI workflows may call user-installed `ffmpeg` and `ffprobe`, but those binaries are optional and external. The repository does not vendor FFmpeg, and FFmpeg is not the only long-term media strategy.

## External FFmpeg Helper

The media crate builds testable command specifications for:

- media probing with `ffprobe`
- proxy frame extraction
- WAV extraction for audio analysis

If the binary is missing, callers receive a clear missing-binary error.

The CLI entry points are `morphogen probe`, `morphogen extract-frames`, and `morphogen extract-audio`. Extraction writes ordinary files on disk so early analysis and render jobs can be inspected before the full AVFoundation/VideoToolbox backend exists.

## Long-Term Mac Path

The long-term Mac backend should use:

- AVFoundation for asset inspection, user-facing media handling, and decode coordination.
- CoreMedia for timing.
- CoreVideo for pixel buffers and `CVMetalTextureCache` handoff into Metal textures.
- VideoToolbox for hardware decode and encode.
- Metal textures for GPU processing.

## Probe and Extraction

Media probe collects stream, duration, resolution, sample rate, and codec information. The SwiftUI shell uses AVFoundation for source probing and falls back to FFprobe through the CLI bridge when needed. A first-frame AVFoundation decode helper can produce a BGRA CoreVideo pixel buffer and hand it to the Metal texture bridge for future previews. Extraction workflows create analysis-friendly proxies: image sequences for visual analysis and WAV files for portable audio descriptors.

## Analysis Cache

Cache files should be sidecars keyed by source media, analysis settings, and versioned algorithms. Cached analysis is reusable across render jobs and should be safe to regenerate.

## Final Render Strategy

The archival default should be image sequence plus WAV stems because it is inspectable, resumable, and easy to validate. Future output targets include EXR sequences, ProRes files, 16-bit PNG/TIFF-style workflows, and high-quality float intermediates.

The first ProRes path is scoped as a Mac-only export layer: build a deterministic frame list, convert PNG frames into CoreVideo BGRA pixel buffers, encode through AVAssetWriter with VideoToolbox encoder selection, and mux to `.mov`. It currently operates on a selected PNG frame directory from the SwiftUI shell; the next step is connecting it to persisted offline render-queue output bundles and adding audio muxing.
