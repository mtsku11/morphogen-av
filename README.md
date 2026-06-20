# Morphogen AV

Morphogen AV is a Mac-first experimental audiovisual cross-synthesis app. It is designed around two loaded audiovisual sources:

- Source A: modulator / analysis source
- Source B: carrier / material source
- Output: B transformed by motion, audio, spectral, temporal, or structural analysis derived from A

The long-term goal is an audiovisual modular synthesizer where compatible analysis signals can modulate visual and audio parameters: optical-flow advection, displacement, feedback, video vocoder processing, spectral and granular recomposition, convolutional blending, controlled motion-vector reuse, and descriptor-based routing.

## Current Status

This repository is an initial production-quality scaffold with deterministic vertical slices. The Rust CLI can create and inspect project JSON, probe and extract proxy media through optional FFmpeg tools, render synthetic and real A-modulates-B displacement and temporal-feedback sequences, produce reusable analysis sidecars, and persist resumable offline render queues with timing and source/cache provenance. The Metal crate compiles and submits flow displacement plus the one-pass feedback/advection kernel on macOS, gated against the CPU reference. The SwiftUI app shell can select movie sources, extract them to PNG/WAV proxies, submit the proxy frames as a persisted queue job, inspect decoded source previews, and export completed queue bundles to ProRes `.mov`.

## What Works Now

- Serializable project schema and timeline/sample alignment helpers in `morphogen-core`.
- Typed node-port compatibility checks for known analysis outputs and render parameters in `morphogen-core`.
- Optional external FFmpeg/FFprobe command wrappers in `morphogen-media`.
- Portable audio buffer, WAV loading/export, RMS envelope, spectral centroid, STFT magnitude cache, and onset-strength scaffolding in `morphogen-audio`.
- Float RGBA CPU image buffers, flow fields, bilinear sampling, luminance-gradient flow generation, flow displacement, and versioned flow cache sidecars in `morphogen-render`.
- Deterministic temporal flow feedback with explicit frame-zero/reset semantics, CPU/Metal parity, and resumable RGBA32F state checkpoints.
- A checked-in tiny golden fixture for CPU flow-displacement output.
- Metal backend placeholders plus a first flow-displacement compute kernel in `morphogen-metal`.
- Rust-side Metal dispatch planning, shader preflight, and macOS runtime submission for the flow-displacement kernel.
- CLI Metal validation command for the synthetic flow-displacement fixture on macOS.
- CLI commands for project initialization, project inspection, probing, extraction, cache generation, queue persistence, and render testing.
- Dev queue execution that writes `frames/frame_000000.png`, `audio/main.wav`, `checkpoint.json`, and `manifest.json` with frame/sample timing metadata for a deterministic test job, and persists the same output contract on the queued job.
- Minimal native SwiftUI macOS shell titled "Morphogen AV".
- AppKit-backed file picking for Source A and Source B.
- AVFoundation media probing in the SwiftUI shell, with FFprobe fallback through the dev CLI bridge.
- First-frame AVFoundation decode into a CoreVideo pixel buffer and Metal texture.
- App-side preview-frame probe that reports decoded frame dimensions, presentation time, Metal texture format, and displays a decoded source thumbnail.
- VideoToolbox ProRes encoder discovery and export-plan probing in the SwiftUI shell.
- Configurable ProRes `.mov` export from a selected PNG frame directory through AVAssetWriter with VideoToolbox encoder selection.
- SwiftUI controls for extracting selected movies into paired PNG/WAV proxy directories through the dev CLI bridge, then recording their RMS/STFT sidecars on the active project.
- SwiftUI controls for choosing Source A/Source B frame directories and submitting a real two-source frame-sequence CPU render as a persisted queue job.
- Queue manifests contain the modulator/carrier directories and the generated flow-cache provenance; completed bundles export directly to configurable ProRes `.mov`.
- Direct configurable ProRes `.mov` export from the render queue output bundle's `frames/` directory with the first WAV stem muxed as a PCM audio track.
- Dev-only Swift-to-CLI bridge for project commands, media proxy extraction, queue submission, and CPU reference rendering.

## Intentional Placeholders

- The SwiftUI shell does not link Rust directly yet; it can invoke the local CLI during development.
- Metal runtime integration is wired into a CLI validation command but not into the SwiftUI shell yet.
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

Render the synthetic fixture through Metal on macOS:

```sh
cargo run -p morphogen-cli -- metal-render-test /tmp/morphogen-metal-test.png
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

Queue a real two-source frame-sequence render into a ProRes-ready bundle:

```sh
cargo run -p morphogen-cli -- queue-init /tmp/morphogen-frame-queue.json
cargo run -p morphogen-cli -- queue-add-frame-sequence /tmp/morphogen-frame-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-frame-output --amount 16 --max-frames 120 --frame-rate 24
cargo run -p morphogen-cli -- queue-run-frame-sequence /tmp/morphogen-frame-queue.json
```

Render a temporal feedback bundle. The output contains `frames/`, generated flow-cache sidecars, `checkpoint.json`, and immutable unquantized `state/feedback_frame_*.rgba32f` resume buffers:

```sh
cargo run -p morphogen-cli -- render-feedback-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-feedback-output --carrier-amount 12 --feedback-amount 24 --feedback-mix 0.72 --decay 0.995 --max-frames 120 --frame-rate 24 --backend metal
```

`--stop-after-frame` writes a resumable checkpoint; rerun the same command to continue. `--reset-at-frame 48` makes that output frame use the documented frame-zero behavior before feedback continues. The queue variant persists the same contract:

```sh
cargo run -p morphogen-cli -- queue-add-feedback-sequence /tmp/morphogen-feedback-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-feedback-output --backend metal
cargo run -p morphogen-cli -- queue-run-feedback-sequence /tmp/morphogen-feedback-queue.json
```

Probe media with optional external FFprobe:

```sh
cargo run -p morphogen-cli -- probe /path/to/media.mov
```

Extract analysis-friendly proxies with optional external FFmpeg:

```sh
cargo run -p morphogen-cli -- extract-frames /path/to/media.mov /tmp/morphogen-frames --fps 12 --max-frames 120
cargo run -p morphogen-cli -- extract-audio /path/to/media.mov /tmp/morphogen-audio.wav --sample-rate 48000 --max-duration-seconds 10
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

The source buttons open native file pickers. Create Test Project writes an example `.morphogen.json` through `morphogen-cli init-example`, Open Project validates a selected project through `morphogen-cli inspect-project`, Probe Sources uses AVFoundation first and falls back to `morphogen-cli probe`, and Probe Preview Frames decodes selected source first frames into Metal textures. Proxy Output and Extract Source Proxies use `morphogen-cli extract-frames` and `extract-audio` to write each selected source as PNG frames plus a 32-bit float WAV. The WAV duration matches the requested proxy-frame span, keeping the generated RMS/STFT cache size bounded and its timing aligned with the frame sequence; those generated frame directories become the Source A and Source B sequence inputs. Run Two-Source Sequence appends a `frame_sequence_flow_displace` job to a persisted queue and executes it into a bundle containing `frames/`, optional `cache/flow/`, manifest, and checkpoint files. Both sequence export actions can write the completed frames to ProRes `.mov`; Export Queue ProRes MOV also understands any audio stems in the bundle. The CLI bridge calls require `cargo` on PATH; FFmpeg is optional but required for media proxy extraction.

## Future Direction

The first deterministic flow-feedback milestone is complete: Source A's luminance-gradient signal repeatedly moves and blends Source B with the previous output frame, with CPU/Metal parity and float-state resume. The next quality step is to replace that spatial gradient with a cached, deterministic temporal optical-flow analysis while preserving the same temporal render contract. Details are in [Flow Feedback Milestone](docs/FLOW_FEEDBACK_MILESTONE.md).
