# Morphogen AV Agent Notes

## Purpose

Morphogen AV is a Mac-first experimental audiovisual cross-synthesis app. The current phase is early media-ingest development: preserve the architecture for a native Rust core, Metal renderer, analysis cache, SwiftUI node graph UI, and deterministic offline render queue while extending the small working CPU reference render path.

## Key Commands

- `cargo test` - run the Rust workspace tests.
- `cargo run -p morphogen-cli -- init-example /tmp/morphogen-example.morphogen.json` - write an example project.
- `cargo run -p morphogen-cli -- inspect-project /tmp/morphogen-example.morphogen.json` - validate and summarize a project.
- `cargo run -p morphogen-cli -- render-test /tmp/morphogen-test.png` - render the synthetic CPU reference PNG.
- `cargo run -p morphogen-cli -- metal-render-test /tmp/morphogen-metal-test.png` - render the synthetic flow-displacement fixture through Metal on macOS.
- `cargo run -p morphogen-cli -- render-two-source /path/to/source-a.png /path/to/source-b.png /tmp/morphogen-two-source.png --amount 16` - render a real two-image CPU displacement.
- `cargo run -p morphogen-cli -- render-frame-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-output-frames --amount 16 --flow-cache-dir /tmp/morphogen-flow-cache --max-frames 120` - render paired extracted frame directories with per-frame flow cache sidecars.
- `cargo run -p morphogen-cli -- render-frame-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-output-frames --amount 16 --rms-modulator-wav /tmp/source-a.wav --frame-rate 12 --rms-amount-scale 24` - modulate sequence displacement amount from a WAV RMS envelope.
- `cargo run -p morphogen-cli -- export-audio-stem /tmp/source.wav /tmp/stem.wav --gain 1.0` - write a 32-bit float WAV stem through the Rust audio path.
- `cargo run -p morphogen-cli -- cache-stft /tmp/source.wav /tmp/source-stft.json --fft-size 1024 --hop-size 256 --window hann` - write an inspectable STFT magnitude cache sidecar.
- `cargo run -p morphogen-cli -- cache-onsets /tmp/source.wav /tmp/source-onsets.json --fft-size 1024 --hop-size 256 --window hann` - write an inspectable onset-strength cache sidecar.
- `cargo run -p morphogen-cli -- cache-synthetic-flow /tmp/morphogen-flow-cache --width 64 --height 64` - write a versioned flow cache sidecar.
- `cargo run -p morphogen-cli -- queue-init /tmp/morphogen-render-queue.json` - create a persisted offline render queue.
- `cargo run -p morphogen-cli -- queue-run-test /tmp/morphogen-render-queue.json /tmp/morphogen-render-output --stop-after-frame` - checkpoint a queued test job after writing the PNG frame.
- `cargo run -p morphogen-cli -- queue-run-test /tmp/morphogen-render-queue.json /tmp/morphogen-render-output` - execute or resume the first queued/running test job into a PNG sequence plus WAV bundle.
- `cargo run -p morphogen-cli -- queue-add-frame-sequence /tmp/morphogen-frame-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-frame-output --amount 16 --max-frames 120 --frame-rate 24` - queue a real two-source frame-sequence displacement job with source/cache provenance.
- `cargo run -p morphogen-cli -- queue-add-frame-sequence /tmp/morphogen-frame-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-frame-output --amount 16 --backend metal` - queue a frame-sequence job that renders on the Metal backend with a per-frame CPU parity check (`--backend` also works on `render-frame-sequence`; defaults to `cpu`).
- `cargo run -p morphogen-cli -- queue-run-frame-sequence /tmp/morphogen-frame-queue.json` - execute the next queued two-source frame-sequence job into a ProRes-ready render bundle. A failure records a durable `failed` status plus reason on the job rather than leaving it `running`.
- `cargo run -p morphogen-cli -- queue-cancel /tmp/morphogen-frame-queue.json job-0001` - cancel a queued or running job so the runner skips it.
- `swift build` - build the SwiftUI macOS app shell.
- `swift test` - run Swift-side macOS app service tests.
- `swift run MorphogenMacApp` - run the SwiftUI macOS app shell.

## Important Paths

- `crates/morphogen-core/src/project.rs` - project schema, example project creation, validation.
- `crates/morphogen-core/src/graph.rs` - node graph and modulation route model.
- `crates/morphogen-core/src/timeline.rs` - frame/time/sample alignment helpers.
- `crates/morphogen-render/src/cpu_reference.rs` - deterministic CPU reference render operations.
- `crates/morphogen-render/src/flow_cache.rs` - versioned single-frame flow-analysis sidecar format.
- `crates/morphogen-render/src/luminance_flow.rs` - first deterministic modulator-derived flow signal.
- `crates/morphogen-render/src/sampler.rs` - bilinear sampling and border behavior.
- `crates/morphogen-core/src/render_queue.rs` - offline render queue persistence.
- `crates/morphogen-audio/src/rms.rs` - portable RMS envelope analysis.
- `crates/morphogen-audio/src/stft.rs` - portable STFT magnitude cache reference implementation.
- `crates/morphogen-audio/src/onset.rs` - portable onset-strength reference implementation.
- `crates/morphogen-media/src/ffmpeg.rs` - optional external FFmpeg/FFprobe command construction.
- `crates/morphogen-cli/src/main.rs` - first working engine validation path.
- `apps/macos/Sources/MorphogenMacApp/` - native SwiftUI shell.
- `apps/macos/Sources/MorphogenMacApp/Services/RustBridgePlaceholder.swift` - dev-only Swift-to-CLI bridge.
- `apps/macos/Sources/MorphogenMacApp/Models/AppState.swift` - source proxy extraction and persisted two-source queue submission.
- `apps/macos/Sources/MorphogenMacApp/Services/MediaFilePicker.swift` - AppKit file picking used by source slots.
- `apps/macos/Sources/MorphogenMacApp/Services/AppleMediaProbePlaceholder.swift` - AVFoundation media probing with FFprobe fallback from app state.
- `apps/macos/Sources/MorphogenMacApp/Services/CoreVideoMetalTextureBridge.swift` - CoreVideo pixel-buffer to Metal texture bridge helper.
- `apps/macos/Sources/MorphogenMacApp/Services/AVFoundationFrameTextureBridge.swift` - first-frame AVFoundation decode into CoreVideo pixel buffers and Metal textures.
- `apps/macos/Sources/MorphogenMacApp/Services/SourcePreviewFrameProbe.swift` - app-side decoded-frame Metal texture probe summaries and thumbnail generation.
- `apps/macos/Sources/MorphogenMacApp/Services/RenderQueueOutputBundle.swift` - Swift-side resolver that maps render queue bundles to ProRes-ready frame sequences and WAV stems.
- `apps/macos/Sources/MorphogenMacApp/Services/VideoToolboxProResExportPlan.swift` - VideoToolbox ProRes encoder discovery and export-plan spike.
- `apps/macos/Sources/MorphogenMacApp/Services/ProResImageSequenceExporter.swift` - PNG image-sequence to ProRes `.mov` export through AVAssetWriter with VideoToolbox encoder selection and optional WAV audio muxing.
- `apps/macos/Sources/MorphogenMacApp/Services/ProjectFilePanel.swift` - AppKit project open/save panels used by the SwiftUI shell.
- `docs/CODEX_TASKS.md` - ordered follow-up backlog.

## Engineering Rules

- Keep offline deterministic rendering ahead of realtime preview work.
- Keep Metal as the production GPU target; do not add Vulkan, CUDA, WebGPU, or WGSL.
- Keep FFmpeg external and optional; do not vendor FFmpeg.
- Avoid GPL-only application dependencies.
- No `unwrap()` in library code except tests.
- Add focused tests for schema, render math, audio descriptors, media helpers, and CLI behavior.
- Prefer small, concrete vertical slices over broad abstractions.

## Context-Loading Order

1. `README.md`
2. `docs/ARCHITECTURE.md`
3. `docs/CODEX_TASKS.md`
4. Relevant crate or app files for the task at hand.
