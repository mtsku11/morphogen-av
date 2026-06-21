# CLI & Codebase Reference

Exhaustive command catalog and key-path map for Morphogen AV. `CLAUDE.md` holds
the invariants and the short list of everyday commands; this file is the full
reference. Keep it current when commands or module responsibilities change.

## Key Commands

- `cargo test` - run the Rust workspace tests.
- `cargo run -p morphogen-cli -- init-example /tmp/morphogen-example.morphogen.json` - write an example project.
- `cargo run -p morphogen-cli -- inspect-project /tmp/morphogen-example.morphogen.json` - validate and summarize a project.
- `cargo run -p morphogen-cli -- project-register-proxy /tmp/morphogen-example.morphogen.json --source-role modulator --frame-dir /tmp/proxy/source-a/frames --audio /tmp/proxy/source-a/audio.wav --analysis-cache audio_rms=/tmp/proxy/source-a/rms.json` - record ingested proxy media and analysis-cache references onto a project source (`--source-id` is available for projects with multiple sources of a role; `--analysis-cache kind=path` is repeatable and re-registering the same cache id replaces it).
- `cargo run -p morphogen-cli -- render-test /tmp/morphogen-test.png` - render the synthetic CPU reference PNG.
- `cargo run -p morphogen-cli -- metal-render-test /tmp/morphogen-metal-test.png` - render the synthetic flow-displacement fixture through Metal on macOS.
- `cargo run -p morphogen-cli -- render-two-source /path/to/source-a.png /path/to/source-b.png /tmp/morphogen-two-source.png --amount 16` - render a real two-image CPU displacement.
- `cargo run -p morphogen-cli -- render-granular-mosaic-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-granular-output --grain-size 24 --rearrangement 0.5 --variation 0.1 --rms-cache /tmp/source-a-rms.json --onset-cache /tmp/source-a-onsets.json --stft-cache /tmp/source-a-stft.json --rms-variation-scale 0.6 --onset-rearrangement-scale 0.4 --centroid-grain-size-scale 12 --frame-rate 24 --grain-cache-dir /tmp/morphogen-grain-cache --max-frames 120 --backend metal` - render a deterministic grain sequence where Source A luminance selects material and cached Source A audio descriptors control variation, rearrangement, and grain size at each frame time.
- `--selection rgb` (default `luma`) on `render-granular-mosaic`, `render-granular-mosaic-sequence`, and `queue-add-granular-mosaic-sequence` switches grain matching from 1-D mean luminance to multimodal nearest-neighbor on mean RGB (`multimodal_nearest_grain_cpu_v1`); writes a `grain_color_descriptors.json` sidecar and tags selection/provenance with the multimodal algorithm id.
- `cargo run -p morphogen-cli -- render-granular-mosaic-pool-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-pool-output --grain-size 16 --rearrangement 1 --variation 0 --audio-weight 1 --modulator-rms-cache /tmp/source-a-rms.json --carrier-rms-cache /tmp/source-b-rms.json --frame-rate 24 --grain-cache-dir /tmp/morphogen-pool-cache` - step 6b joint-AV path (`pooled_av_nearest_grain_cpu_v1`, CPU-only): builds a whole-clip temporal grain pool from every Source B frame, each grain carrying its frame-time RMS, and selects per output tile by combined `[mean_color | audio]` distance against Source A's frame-time RMS query (`--audio-weight` scales the audio dim). `rearrangement` is a cross-frame value blend (0 = carrier, 1 = selected grain). RMS caches are optional but both-or-neither; omit them for color-only matching across time. Writes/reuses a `grain_pool_descriptors.json` sidecar keyed on the whole carrier set.
- `cargo run -p morphogen-cli -- render-frame-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-output-frames --amount 16 --flow-cache-dir /tmp/morphogen-flow-cache --max-frames 120` - render paired extracted frame directories with per-frame flow cache sidecars.
- `cargo run -p morphogen-cli -- render-frame-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-output-frames --amount 16 --rms-modulator-wav /tmp/source-a.wav --frame-rate 12 --rms-amount-scale 24` - modulate sequence displacement amount from a WAV RMS envelope.
- `cargo run -p morphogen-cli -- render-feedback-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-feedback-output --flow-source optical-flow --carrier-amount 1.5 --feedback-amount 2 --feedback-mix 0.72 --decay 0.995 --output-bit-depth 16 --temporal-supersampling 2 --max-frames 120 --frame-rate 24 --backend metal` - render deterministic A-modulates-B temporal feedback; `--stop-after-frame` proves resume and `--reset-at-frame 48` restarts feedback at a selected output frame.
- `cargo run -p morphogen-cli -- render-feedback-sequence /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-feedback-output --flow-source optical-flow --feedback-amount 2 --carrier-amount 1.5` - the feedback flow field defaults to temporal Lucas-Kanade optical flow between consecutive modulator frames (`--flow-source luminance` restores the single-frame gradient field). Optical-flow vectors are true pixel motion, so use small amounts. Frame zero (and any reset frame) uses a zero field. The flow source is recorded as the contract's `flow_algorithm`, so changing it invalidates an existing checkpoint.
- `cargo run -p morphogen-cli -- render-feedback-sequence ... --structure-mix 0.6 [--structure-mode multiscale]` - re-inject the carrier high-frequency band so high `--feedback-mix` morphs into regenerating structure instead of flat fog. `single-scale` (default) has CPU/Metal parity; `multiscale` is CPU-only and currently marginal on real footage (see [BACKLOG.md](BACKLOG.md)).
- `cargo run -p morphogen-cli -- export-audio-stem /tmp/source.wav /tmp/stem.wav --gain 1.0` - write a 32-bit float WAV stem through the Rust audio path.
- `cargo run -p morphogen-cli -- cache-stft /tmp/source.wav /tmp/source-stft.json --fft-size 1024 --hop-size 256 --window hann` - write an inspectable STFT magnitude cache sidecar.
- `cargo run -p morphogen-cli -- cache-onsets /tmp/source.wav /tmp/source-onsets.json --fft-size 1024 --hop-size 256 --window hann` - write an inspectable onset-strength cache sidecar.
- `cargo run -p morphogen-cli -- cache-rms /tmp/source.wav /tmp/source-rms.json --window-size 2048 --hop-size 512` - write an inspectable RMS-envelope analysis cache sidecar (also generated automatically during app-side media ingest).
- `cargo run -p morphogen-cli -- cache-synthetic-flow /tmp/morphogen-flow-cache --width 64 --height 64` - write a versioned flow cache sidecar.
- `cargo run -p morphogen-cli -- cache-luminance-flow /path/to/source-a.png /tmp/morphogen-luma-flow-cache --width 256 --height 256` - write a luminance-gradient flow cache sidecar.
- `cargo run -p morphogen-cli -- queue-init /tmp/morphogen-render-queue.json` - create a persisted offline render queue.
- `cargo run -p morphogen-cli -- queue-run-test /tmp/morphogen-render-queue.json /tmp/morphogen-render-output --stop-after-frame` - checkpoint a queued test job after writing the PNG frame.
- `cargo run -p morphogen-cli -- queue-run-test /tmp/morphogen-render-queue.json /tmp/morphogen-render-output` - execute or resume the first queued/running test job into a PNG sequence plus WAV bundle.
- `cargo run -p morphogen-cli -- queue-add-frame-sequence /tmp/morphogen-frame-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-frame-output --amount 16 --max-frames 120 --frame-rate 24` - queue a real two-source frame-sequence displacement job with source/cache provenance.
- `cargo run -p morphogen-cli -- queue-add-frame-sequence /tmp/morphogen-frame-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-frame-output --amount 16 --backend metal` - queue a frame-sequence job that renders on the Metal backend with a per-frame CPU parity check (`--backend` also works on `render-frame-sequence`; defaults to `cpu`).
- `cargo run -p morphogen-cli -- queue-run-frame-sequence /tmp/morphogen-frame-queue.json` - execute the next queued two-source frame-sequence job into a ProRes-ready render bundle. A failure records a durable `failed` status plus reason on the job rather than leaving it `running`.
- `cargo run -p morphogen-cli -- queue-add-granular-mosaic-sequence /tmp/morphogen-granular-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-granular-output --grain-size 24 --rearrangement 1 --variation 0.35 --seed 42 --max-frames 120 --frame-rate 24 --backend metal` - persist a ProRes-ready granular image-sequence job with grain-cache provenance and a CPU parity-gated Metal backend.
- `cargo run -p morphogen-cli -- queue-run-granular-mosaic-sequence /tmp/morphogen-granular-queue.json` - execute the next queued granular mosaic job.
- `cargo run -p morphogen-cli -- queue-add-granular-mosaic-pool-sequence /tmp/morphogen-granular-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-granular-output --grain-size 16 --rearrangement 1 --variation 0 --audio-weight 1 --modulator-rms-cache /tmp/source-a-rms.json --carrier-rms-cache /tmp/source-b-rms.json --max-frames 120 --frame-rate 24` - persist a step-6b temporal-grain-pool (joint-AV) granular job; writes a ProRes-ready bundle with a `grain_pool_descriptors.json` sidecar and a `frame_sequence_granular_mosaic_pool` manifest carrying the pooled algorithm id, `audio_weight`, and RMS-cache provenance. CPU-only.
- `cargo run -p morphogen-cli -- queue-run-granular-mosaic-pool-sequence /tmp/morphogen-granular-queue.json` - execute the next queued temporal-grain-pool job.
- `cargo run -p morphogen-cli -- queue-add-feedback-sequence /tmp/morphogen-feedback-queue.json /tmp/source-a-frames /tmp/source-b-frames /tmp/morphogen-feedback-output --carrier-amount 1.5 --feedback-amount 2 --feedback-mix 0.72 --decay 0.995 --backend metal --flow-source optical-flow` - persist a resumable temporal feedback job with source/cache provenance (`--flow-source` defaults to `optical-flow`; queue jobs serialized before optical flow existed default to `luminance` for backward compatibility).
- `cargo run -p morphogen-cli -- queue-run-feedback-sequence /tmp/morphogen-feedback-queue.json` - execute the next queued feedback job into a ProRes-ready bundle with a verified float state checkpoint.
- `cargo run -p morphogen-cli -- queue-cancel /tmp/morphogen-frame-queue.json job-0001` - cancel a queued or running job so the runner skips it.
- `cargo run -p morphogen-cli -- probe /path/to/media.mov` - probe media with optional external FFprobe.
- `cargo run -p morphogen-cli -- extract-frames /path/to/media.mov /tmp/morphogen-frames --fps 12 --max-frames 120` - extract PNG frames with optional external FFmpeg.
- `cargo run -p morphogen-cli -- extract-audio /path/to/media.mov /tmp/morphogen-audio.wav --sample-rate 48000 --max-duration-seconds 10` - extract a WAV with optional external FFmpeg.
- `swift build` - build the SwiftUI macOS app shell.
- `swift test` - run Swift-side macOS app service tests.
- `swift run MorphogenMacApp` - run the SwiftUI macOS app shell.

## Important Paths

- `crates/morphogen-core/src/project.rs` - project schema, example project creation, validation.
- `crates/morphogen-core/src/graph.rs` - node graph and modulation route model.
- `crates/morphogen-core/src/timeline.rs` - frame/time/sample alignment helpers.
- `crates/morphogen-core/src/render_job.rs` - render-node and render-job task definitions.
- `crates/morphogen-core/src/render_queue.rs` - offline render queue persistence.
- `crates/morphogen-render/src/cpu_reference.rs` - deterministic CPU reference render operations.
- `crates/morphogen-render/src/granular_mosaic.rs` - deterministic Source A luma to Source B grain-selection renderer.
- `crates/morphogen-render/src/grain_cache.rs` - validated grain descriptor and selection cache sidecars.
- `crates/morphogen-render/src/feedback_state.rs` - versioned, checksummed RGBA32F feedback-state checkpoints.
- `crates/morphogen-render/src/flow_cache.rs` - versioned single-frame flow-analysis sidecar format.
- `crates/morphogen-render/src/luminance_flow.rs` - first deterministic modulator-derived flow signal.
- `crates/morphogen-render/src/optical_flow.rs` - temporal pyramidal Lucas-Kanade optical flow.
- `crates/morphogen-render/src/sampler.rs` - bilinear sampling and border behavior.
- `crates/morphogen-audio/src/rms.rs` - portable RMS envelope analysis.
- `crates/morphogen-audio/src/stft.rs` - portable STFT magnitude cache reference implementation.
- `crates/morphogen-audio/src/onset.rs` - portable onset-strength reference implementation.
- `crates/morphogen-audio/src/spectral.rs` - spectral centroid analysis.
- `crates/morphogen-media/src/ffmpeg.rs` - optional external FFmpeg/FFprobe command construction.
- `crates/morphogen-metal/src/` - Metal device/pipeline/texture ownership and kernel dispatch.
- `crates/morphogen-metal/shaders/` - `.metal` compute kernels (flow displace, advect feedback, granular mosaic).
- `crates/morphogen-cli/src/main.rs` - first working engine validation path.
- `apps/macos/Sources/MorphogenMacApp/` - native SwiftUI shell.
- `apps/macos/Sources/MorphogenMacApp/Services/RustBridgePlaceholder.swift` - dev-only Swift-to-CLI bridge.
- `apps/macos/Sources/MorphogenMacApp/Models/AppState.swift` - source proxy extraction and persisted queue submission.
- `apps/macos/Sources/MorphogenMacApp/Services/MediaFilePicker.swift` - AppKit file picking used by source slots.
- `apps/macos/Sources/MorphogenMacApp/Services/AppleMediaProbePlaceholder.swift` - AVFoundation media probing with FFprobe fallback.
- `apps/macos/Sources/MorphogenMacApp/Services/CoreVideoMetalTextureBridge.swift` - CoreVideo pixel-buffer to Metal texture bridge helper.
- `apps/macos/Sources/MorphogenMacApp/Services/AVFoundationFrameTextureBridge.swift` - first-frame AVFoundation decode into CoreVideo pixel buffers and Metal textures.
- `apps/macos/Sources/MorphogenMacApp/Services/SourcePreviewFrameProbe.swift` - app-side decoded-frame Metal texture probe summaries and thumbnails.
- `apps/macos/Sources/MorphogenMacApp/Services/RenderQueueOutputBundle.swift` - Swift-side resolver mapping render queue bundles to ProRes-ready sequences and WAV stems.
- `apps/macos/Sources/MorphogenMacApp/Services/VideoToolboxProResExportPlan.swift` - VideoToolbox ProRes encoder discovery and export-plan spike.
- `apps/macos/Sources/MorphogenMacApp/Services/ProResImageSequenceExporter.swift` - PNG image-sequence to ProRes `.mov` export through AVAssetWriter.
- `apps/macos/Sources/MorphogenMacApp/Services/ProjectFilePanel.swift` - AppKit project open/save panels.
