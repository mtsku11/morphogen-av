# Morphogen AV

Morphogen AV is a Mac-first, deterministic audiovisual cross-synthesis
instrument. You load two sources — **Source A** (modulator/analysis) and
**Source B** (carrier/material) — and Source A's motion, audio, spectral, or
structural character reshapes Source B. The long-term target is an
audiovisual modular synthesizer: typed analysis signals (including
deterministic LFOs) patch onto any effect's knobs, effects chain together
into instruments, and everything renders bit-reproducibly offline first, with
a fast, real preview loop on top of the same engine.

It is not a toy demo. Every effect below is a deterministic CPU reference —
several with a Metal kernel gated frame-by-frame against that CPU
reference — driven by a real render queue with resumable checkpoints, wired
end to end through a native SwiftUI shell.

## Why it's built the way it is

- **Determinism first.** Offline rendering leads; the realtime preview is a
  lower-fidelity *view* of the same project graph, never a second engine.
  Identical inputs + settings ⇒ bit-reproducible output, every time.
- **CPU is ground truth.** Every Metal kernel is validated frame-by-frame
  against its CPU reference within a tight tolerance before it ships.
- **Stateful effects are resumable.** Temporal nodes (feedback, datamosh,
  chain stages) declare frame-zero behavior and checkpoint from an
  unquantized float state buffer — never a display PNG — so a render can
  stop and resume without drifting.
- **Analysis is reusable sidecar data.** Optical flow, RMS/onset/STFT,
  grain descriptors — all regenerable from source + settings, fingerprinted
  so a stale sidecar is never silently reused.

## The effect catalog

Two-source (A shapes B), single-source, and audio effects, each with a CPU
reference and — where marked **⚡** — a parity-gated Metal kernel.

**Motion & temporal**
- **Flow displacement** — Source A's optical flow (or luminance gradient)
  displaces Source B's pixels. ⚡
- **Flow feedback** — A's motion repeatedly smears B into its own previous
  output frame; float-state checkpoint, `--structure-mix` re-injects carrier
  detail so high feedback regenerates structure instead of flattening to fog. ⚡
- **Datamosh** (`render-datamosh-sequence`) — recursive flow-reuse "bloom" plus
  a full codec-*simulated* tier: block-quantized motion (macroblock slide),
  residual re-injection (fine-motion haze), per-block keep/drop refresh
  (content-aware self-erasing trails), and curated presets
  (`structured-melt`, `macroblock-rot`, `scanline-smear`, `codec-engrave`). ⚡
- **Real bitstream datamosh** (`datamosh-bitstream`) — an explicit,
  intentionally *non-deterministic* carve-out outside the render graph: pure-Rust
  RIFF surgery duplicates P-frame chunks or strips the keyframe for authentic
  codec-artifact bloom/void-mosh, decoded back through ffmpeg.
- **Fluid dye advection** — a source treated as continuous dye, advected
  through a divergence-free curl-noise vortex field (`render-fluid-advect-sequence`),
  through A's real optical flow onto B (`...-two-source-sequence`), or through
  its own motion (`render-optical-flow-advect-sequence`). ⚡
- **Field particles** — a grid of coloured particles rides the same steady
  vortex field, with optional live colour re-sampling from the playing source. ⚡
- **Rutt-Etra scanlines** — the classic analog-video-synth look: the frame
  redraws as sparse horizontal scanlines vertically displaced by luminance
  (or any modulator, including LFOs) — `displacement_depth=lfo(sine,0.5)` is
  the canonical demo.

**Structural & spatial**
- **Retro static** — simulated bit-depth banding/dither with a selectable
  error-diffusion filter.
- **Channel shift** — independent per-channel RGB pixel offsets, constant or
  driven per-row by Source A's optical flow.
- **Palette quantize** — posterize or a fixed neon palette, integer/enum
  modulation targets included.
- **Pixel sort** — threshold-bounded sort of contiguous runs, by row or column.
- **Block collage** — hard-cut NxN tile collage between A and B driven by a
  spatially-coherent noise field.
- **Convolutional blend** — Source A's frame becomes a normalized image
  kernel convolved with B (colour or luma kernels; ⚡), or A's audio becomes an
  L1-normalized impulse response reverbed onto B (mono or true-stereo, direct
  or FFT convolution).
- **Cascade collage / cascade trails** — a scribbled, morphing-edge tile
  cascade generator with per-step hue drift.
- **Descriptor-coagulated flow blend** — A and B *mutually* mangled: patches
  of the screen group by colour/texture similarity into an ownership field
  that advects, smears, and collides over time (the first true two-source
  mutual effect). ⚡
- **Colour-group dispersion blend** — the content-advecting sibling: image
  content itself (not just an ownership mask) flows, shatters, and disperses
  along A's optical-flow current.
- **Fluid colour-sort mosaic** — both sources' tiles phase-separate into
  colour domains via cohesion/repulsion forces, then advect through a fluid
  field — nine landed variants including adaptive tile sizes, live-refresh
  content, cluster-blob layout, a sweeping dispersion band, organic
  turbulence, and steady vortex flow.
- **Granular mosaic** — Source B recomposed as luma- or colour-matched
  visual grains selected by Source A, with a temporal grain-pool mode that
  matches grains across the whole clip by colour + texture + audio RMS.
- **Video vocoder** — B's tonal envelope reshaped to match A's (histogram
  specification, or per-band gain). ⚡
- **Effect chains** (`render-chain`) — compose any of the above into an
  ordered pipeline from one JSON spec: each stage's output feeds the next,
  one reproducible manifest, stateful stages checkpoint per-stage, and every
  stage can carry its own modulation routes.

**Audio & cross-domain**
- **Spectral cross-synthesis** — A's RMS drives B's amplitude, A's spectral
  centroid sweeps a filter on B, or — the headline mode — a real phase-vocoder
  imposes A's log-band spectral envelope onto B's spectrum through a complex
  inverse STFT, keeping B's own phase.
- **Audio-to-video routing** — A's RMS/onset/centroid drives B's visual
  displacement amount.
- **Video-to-audio routing** — A's luminance or optical-flow magnitude drives
  B's audio gain, stereo pan, or filter cutoff, with hold or smoothed
  envelope sampling.

## The modulation matrix — the modular-synth core

Nearly every knob above accepts `--modulate "<target>=<source>[:<scale>[,<offset>]][@hold|@smooth]"`
(repeatable) instead of a static value:

- **Analysis sources**: `audio-rms`, `audio-onset`, `audio-centroid` (from a
  modulator WAV), `luma`, `flow` (from a modulator frame sequence).
- **LFOs**: `lfo(sine|triangle|square|saw[,rate_hz[,phase]])` — a pure,
  media-free function of frame time. No modulator file needed at all.
- **Named modulators**: `<name>.<source>` lets different routes on the same
  render read different WAVs or frame sequences (`--named-modulator-audio
  name=path`, repeatable).
- **Per-route sampling** overrides the render's default hold/smooth envelope
  evaluation with a trailing `@hold`/`@smooth`.
- Values always **clamp, never error** — an envelope can never abort a render
  mid-sequence — and every stateful effect's checkpoint contract includes the
  active routes, so changing a route on resume is refused rather than
  silently drifting.

This whole surface is symmetric across the direct CLI, the persisted render
queue (`queue-add-*` validates and rejects before persisting; `queue-run-*`
is byte-identical to the direct render), and the SwiftUI panels.

## Deterministic rendering, the render queue, and analysis caches

Renders happen two ways: a **direct CLI command** for one-off/scripted runs,
or a **persisted offline queue** (`queue-init` / `queue-add-*` /
`queue-run-*`) that survives interruption, records source/cache provenance,
and produces a ProRes-ready output bundle (`frames/`, audio stems,
`manifest.json`, `checkpoint.json`). Stateful effects resume exactly:
`--stop-after-frame` writes a checksummed unquantized RGBA32F state buffer,
and re-running the same command continues from it — a changed input,
setting, or modulation route is detected and refused rather than silently
producing wrong output.

Analysis (optical flow, RMS/STFT/onset envelopes, grain descriptors) is
reusable sidecar data: each cache records its algorithm id, dimensions,
sampling convention, and a content fingerprint of its source, so a renderer
only reuses a sidecar that still matches and regenerates deterministically
otherwise.

## The realtime-ish preview loop

The SwiftUI shell's Quick Preview no longer renders eight static thumbnails —
it plays. Picking sources and hitting **Preview** downscales the source
proxies once (exact deterministic box-average, `downscale-frames`, default
quarter resolution), renders a few seconds of the selected effect through
the *same engine* (only the input paths change — every other render argument
is identical to a full-resolution render), and loops it in a real player
(play/pause, frame counter, scale/seconds controls). On an expensive effect
like flow feedback this is over **13x faster** than the old full-res preview,
so tuning an effect's look no longer means waiting on a full render.

## The SwiftUI macOS app

A native shell (not a wrapper): pick Source A/B with native file pickers,
and the app auto-extracts them to PNG/WAV proxies (AVFoundation, with an
FFprobe/FFmpeg fallback) and generates their RMS/STFT sidecars. A panel per
effect exposes its knobs, modulation slots (with per-slot named-modulator
and LFO options), and backend picker where relevant; every panel submits a
real persisted queue job and can preview it through the quarter-res loop
before committing to a full render. Completed queue bundles export directly
to configurable ProRes `.mov` via VideoToolbox, with any audio stems muxed
in.

*(A native SwiftUI chain-builder panel for the effect-chain JSON spec is the
one open design decision — everything else above is wired end to end.)*

## Project layout

```
crates/morphogen-core    project schema, node graph, render-job/queue persistence
crates/morphogen-render  deterministic CPU effect renderers, caches, samplers
crates/morphogen-audio   WAV I/O, RMS/STFT/onset/spectral-centroid, complex STFT
crates/morphogen-media   optional external FFmpeg/FFprobe wrappers
crates/morphogen-metal   Metal device/pipeline/texture + compute kernels
crates/morphogen-cli     the engine validation + render/queue driver
apps/macos               native SwiftUI shell
```

## Getting started

**Prerequisites**: macOS (Apple Silicon preferred), a stable Rust toolchain
with `cargo`, Swift 5.9+ (Xcode command line tools), and optionally
`ffmpeg`/`ffprobe` on `PATH` for media probing and proxy extraction (media
commands return a clear error without them; nothing is vendored).

```sh
# Rust engine
cargo test --workspace                  # run the full test suite
cargo run -p morphogen-cli -- <command> # see docs/REFERENCE.md for the full catalog

# SwiftUI app shell
swift build && swift test
swift run MorphogenMacApp
```

A quick taste of the CLI:

```sh
# Extract two sources to analysis-ready proxies
cargo run -p morphogen-cli -- extract-frames source-a.mov /tmp/a-frames --fps 24
cargo run -p morphogen-cli -- extract-frames source-b.mov /tmp/b-frames --fps 24

# Feedback with an LFO on feedback_mix — no modulator file needed
cargo run -p morphogen-cli -- render-feedback-sequence /tmp/a-frames /tmp/b-frames /tmp/out \
  --feedback-amount 24 --modulate "feedback_mix=lfo(sine,0.25):0.4,0.3"

# Chain rutt-etra scanlines into a posterize pass from one spec
cargo run -p morphogen-cli -- render-chain chain.json /tmp/b-frames /tmp/chain-out

# A fast quarter-res look before committing to the full render
cargo run -p morphogen-cli -- downscale-frames /tmp/b-frames /tmp/b-quarter --scale 4 --max-frames 48
```

## Documentation map

- **[`docs/REFERENCE.md`](docs/REFERENCE.md)** — the exhaustive CLI command
  catalog and source-file key-path map.
- **[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)** — system shape (core /
  Metal / cache / queue / UI).
- **[`docs/EFFECTS_ROADMAP.md`](docs/EFFECTS_ROADMAP.md)** — the full,
  detailed per-effect design notes and landed-vs-deferred tiers.
- **`docs/*_MILESTONE.md`** — the acceptance contract for each effect/feature,
  written before it was built.
- **[`docs/BACKLOG.md`](docs/BACKLOG.md)** — completed work + what's next.
- **[`docs/RECOMMENDATIONS.md`](docs/RECOMMENDATIONS.md)** — strategic
  "what's underdeveloped, what's next-level" notes.
- **[`STATUS.md`](STATUS.md)** — the current session-resume checkpoint:
  verified test baselines and the most recent landed work.

## Current status

`cargo test --workspace`: **532 passing, 0 failing** across 7 crates.
`swift test`: **113 passing, 0 failing**. Every effect above has a landed CPU
reference; Metal parity, queue exposure, and SwiftUI panels are landed for
the large majority (noted per-effect in `docs/EFFECTS_ROADMAP.md` where a
tier is still CPU-only or direct-CLI-only). See [`STATUS.md`](STATUS.md) for
the exact, currently-verified baseline and the most recent work.
