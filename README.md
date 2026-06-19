# Morphogen AV

Morphogen AV is a Mac-first experimental audiovisual cross-synthesis app. It is designed around two loaded audiovisual sources:

- Source A: modulator / analysis source
- Source B: carrier / material source
- Output: B transformed by motion, audio, spectral, temporal, or structural analysis derived from A

The long-term goal is an audiovisual modular synthesizer where compatible analysis signals can modulate visual and audio parameters: optical-flow advection, displacement, feedback, video vocoder processing, spectral and granular recomposition, convolutional blending, controlled motion-vector reuse, and descriptor-based routing.

## Current Status

This repository is an initial production-quality scaffold with small deterministic vertical slices. The Rust CLI can create an example project, inspect project JSON, probe media through optional FFprobe, extract proxy media, render a synthetic PNG, render real still-image and frame-sequence A-modulates-B displacement through the CPU reference pipeline, modulate visual displacement amount from an audio RMS envelope, write 32-bit float WAV stems, write flow-analysis cache sidecars, write STFT magnitude and onset-strength JSON sidecars, persist a simple offline render queue, and execute or resume a deterministic queued test job into an image-sequence plus WAV bundle. The Metal crate can compile and submit the flow-displacement kernel on macOS when a Metal device is available. The SwiftUI app shell can select source files, probe media through AVFoundation with FFprobe fallback, check a VideoToolbox ProRes export plan, run the dev queue test job, export the queue output bundle directly to ProRes `.mov` with its first WAV stem muxed as audio, export arbitrary PNG frame directories to ProRes `.mov`, decode selected source first frames into Metal textures for preview diagnostics, display decoded source thumbnails, and invoke the local CLI for dev-only project creation/loading and CPU reference rendering.

## What Works Now

- Serializable project schema and timeline/sample alignment helpers in `morphogen-core`.
- Typed node-port compatibility checks for known analysis outputs and render parameters in `morphogen-core`.
- Optional external FFmpeg/FFprobe command wrappers in `morphogen-media`.
- Portable audio buffer, WAV loading/export, RMS envelope, spectral centroid, STFT magnitude cache, and onset-strength scaffolding in `morphogen-audio`.
- Float RGBA CPU image buffers, flow fields, bilinear sampling, luminance-gradient flow generation, flow displacement, and versioned flow cache sidecars in `morphogen-render`.
- A checked-in tiny golden fixture for CPU flow-displacement output.
- Metal backend placeholders plus a first flow-displacement compute kernel in `morphogen-metal`.
- Rust-side Metal dispatch planning, shader preflight, and macOS runtime submission for the flow-displacement kernel.
- CLI commands for project initialization, project inspection, probing, extraction, cache generation, queue persistence, and render testing.
- Dev queue execution that writes `frames/frame_000000.png`, `audio/main.wav`, `checkpoint.json`, and `manifest.json` for a deterministic test job.
- Minimal native SwiftUI macOS shell titled "Morphogen AV".
- AppKit-backed file picking for Source A and Source B.
- AVFoundation media probing in the SwiftUI shell, with FFprobe fallback through the dev CLI bridge.
- First-frame AVFoundation decode into a CoreVideo pixel buffer and Metal texture.
- App-side preview-frame probe that reports decoded frame dimensions, presentation time, Metal texture format, and displays a decoded source thumbnail.
- VideoToolbox ProRes encoder discovery and export-plan probing in the SwiftUI shell.
- ProRes 422 HQ `.mov` export from a selected PNG frame directory through AVAssetWriter with VideoToolbox encoder selection.
- Direct ProRes 422 HQ `.mov` export from the render queue output bundle's `frames/` directory with the first WAV stem muxed as a PCM audio track.
- Dev-only Swift-to-CLI bridge for `init-example`, `inspect-project`, `probe`, and `render-test`.

## Intentional Placeholders

- The SwiftUI shell does not link Rust directly yet; it can invoke the local CLI during development.
- Metal runtime integration is not wired into the CLI or SwiftUI shell yet.
- The source preview surface is a first-frame diagnostic thumbnail, not a realtime timeline or node-graph preview yet.
- Real optical flow, depth, masks, EXR output, and production render-queue execution are future work.
- FFmpeg is optional and external; if it is missing, media commands return a clear error.
- Arbitrary frame-directory VideoToolbox export is still video-only; multi-stem muxing and high-bit-depth pixel-buffer paths are future work.

## Prerequisites

- macOS, preferably Apple Silicon.
- Stable Rust toolchain with `cargo` on PATH.
- Swift 5.9 or newer via Xcode command line tools.
- Optional: user-installed `ffmpeg` and `ffprobe` for media probe and extraction commands.

## Rust

Run tests:

```sh
cargo test
```

Create and inspect an example project:

```sh
cargo run -p morphogen-cli -- init-example /tmp/morphogen-example.morphogen.json
cargo run -p morphogen-cli -- inspect-project /tmp/morphogen-example.morphogen.json
```

Render a synthetic PNG using the CPU reference flow-displacement pipeline:

```sh
cargo run -p morphogen-cli -- render-test /tmp/morphogen-test.png
```

Render a real two-source CPU displacement from extracted or generated image frames:

```sh
cargo run -p morphogen-cli -- render-two-source /path/to/source-a.png /path/to/source-b.png /tmp/morphogen-two-source.png --amount 16
```

Render paired frame sequences from extracted frame directories:

```sh
cargo run -p morphogen-cli -- render-frame-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-output-frames --amount 16 --flow-cache-dir /tmp/morphogen-flow-cache --max-frames 120
```

Use a modulator WAV RMS envelope to vary visual displacement amount over the output sequence:

```sh
cargo run -p morphogen-cli -- render-frame-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-output-frames --amount 16 --rms-modulator-wav /tmp/source-a.wav --frame-rate 12 --rms-amount-scale 24
```

Write flow-analysis cache sidecars:

```sh
cargo run -p morphogen-cli -- cache-synthetic-flow /tmp/morphogen-flow-cache --width 64 --height 64
cargo run -p morphogen-cli -- cache-luminance-flow /path/to/source-a.png /tmp/morphogen-luma-flow-cache --width 256 --height 256
cargo run -p morphogen-cli -- render-two-source /path/to/source-a.png /path/to/source-b.png /tmp/morphogen-two-source.png --flow-cache-dir /tmp/morphogen-flow-cache
```

Write an inspectable STFT magnitude cache from a WAV:

```sh
cargo run -p morphogen-cli -- cache-stft /tmp/morphogen-audio.wav /tmp/morphogen-stft.json --fft-size 1024 --hop-size 256 --window hann
```

Write an onset-strength cache from a WAV:

```sh
cargo run -p morphogen-cli -- cache-onsets /tmp/morphogen-audio.wav /tmp/morphogen-onsets.json --fft-size 1024 --hop-size 256 --window hann
```

Create and inspect a persisted offline render queue:

```sh
cargo run -p morphogen-cli -- queue-init /tmp/morphogen-render-queue.json
cargo run -p morphogen-cli -- queue-add-test /tmp/morphogen-render-queue.json --project-path /tmp/morphogen-example.morphogen.json
cargo run -p morphogen-cli -- queue-inspect /tmp/morphogen-render-queue.json
cargo run -p morphogen-cli -- queue-run-test /tmp/morphogen-render-queue.json /tmp/morphogen-render-output --stop-after-frame
cargo run -p morphogen-cli -- queue-run-test /tmp/morphogen-render-queue.json /tmp/morphogen-render-output
```

Probe media with optional external FFprobe:

```sh
cargo run -p morphogen-cli -- probe /path/to/media.mov
```

Extract analysis-friendly proxies with optional external FFmpeg:

```sh
cargo run -p morphogen-cli -- extract-frames /path/to/media.mov /tmp/morphogen-frames --fps 12 --max-frames 120
cargo run -p morphogen-cli -- extract-audio /path/to/media.mov /tmp/morphogen-audio.wav --sample-rate 48000
```

Export a 32-bit float WAV stem through the Rust audio path:

```sh
cargo run -p morphogen-cli -- export-audio-stem /tmp/morphogen-audio.wav /tmp/morphogen-stem.wav --gain 1.0
```

## SwiftUI macOS Shell

Build and run the native app shell:

```sh
swift build
swift run MorphogenMacApp
```

Run Swift-side service tests:

```sh
swift test
```

The source buttons open native file pickers. Create Test Project writes an example `.morphogen.json` through `morphogen-cli init-example`, Open Project validates a selected project through `morphogen-cli inspect-project`, Probe Sources uses AVFoundation first and falls back to `morphogen-cli probe`, Probe Preview Frames decodes selected source first frames into Metal textures and reports dimensions/timing, Check ProRes probes a VideoToolbox ProRes 422 HQ export plan, Run Queue Test creates and executes a deterministic dev queue bundle at `/tmp/morphogen-render-output/job-0001`, Export Queue ProRes MOV converts that bundle's `frames/` directory into a `.mov` and muxes its first WAV stem without selecting folders manually, Export Frame Directory MOV keeps the lower-level arbitrary PNG sequence exporter available, and the CPU reference render button invokes `morphogen-cli render-test /tmp/morphogen-test.png`. The CLI bridge calls require `cargo` on PATH; FFprobe is optional fallback for media inspection.

## Future Direction

The next engineering task is adding configurable frame-rate/profile controls to the ProRes export panel. Metal is the intended production backend, while CPU rendering remains the deterministic reference path for tests and offline correctness.
