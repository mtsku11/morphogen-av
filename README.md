# Morphogen AV

Morphogen AV is a Mac-first, deterministic audiovisual cross-synthesis
instrument. You load two sources — **Source A** (modulator / analysis) and
**Source B** (carrier / material) — and Source A's motion, audio, spectral, or
structural character reshapes Source B. The long-term target is an audiovisual
modular synthesizer: typed analysis signals (optical flow, audio descriptors,
LFOs, MIDI, recorded gestures) patch onto any effect's knobs, effects chain
together into instruments, and everything renders bit-reproducibly offline
first, with a fast, real preview loop on top of the same engine.

It is not a toy demo. Every effect below is a deterministic CPU reference —
many with a Metal kernel gated frame-by-frame against that CPU reference —
driven by a real render queue with resumable checkpoints, wired end to end
through a native SwiftUI shell.

> **Using this as an agent/model?** Read **[`INSTRUCTIONS.md`](INSTRUCTIONS.md)**
> — the complete operational manual for driving every feature (CLI + app).
> This README is the orientation; `INSTRUCTIONS.md` is the how-to.

## Why it's built the way it is

- **Determinism first.** Offline rendering leads; the realtime preview is a
  lower-fidelity *view* of the same project graph, never a second engine.
  Identical inputs + settings ⇒ bit-reproducible output, every time.
- **CPU is ground truth.** Every Metal kernel is validated frame-by-frame
  against its CPU reference within a tight tolerance before it ships.
- **Stateful effects are resumable.** Temporal nodes (feedback, datamosh,
  morphogenesis, chain stages) declare frame-zero behavior and checkpoint from
  an unquantized float state buffer — never a display PNG — so a render can
  stop and resume without drifting.
- **Analysis is reusable sidecar data.** Optical flow, RMS/onset/STFT, grain
  descriptors — all regenerable from source + settings, fingerprinted so a
  stale sidecar is never silently reused.

## The effect catalog

Two-source (A shapes B), single-source, and audio effects, each with a CPU
reference and — where marked **⚡** — a parity-gated Metal kernel. In the app,
these are grouped into the sidebar categories shown in **bold** below.

**Displacement**
- **Flow displace** — Source A's optical flow (or luminance gradient)
  displaces Source B's pixels. ⚡
- **Flow feedback** — A's motion repeatedly smears B into its own previous
  output frame; float-state checkpoint, `--structure-mix` re-injects carrier
  detail so high feedback regenerates structure instead of flattening to fog. ⚡
- **Rutt-Etra scanlines** — the classic analog-video-synth look: the frame
  redraws as sparse horizontal scanlines vertically displaced by luminance
  (or any modulator, including LFOs, MIDI, or a recorded gesture). Two-source
  mode: A's luma displaces B's scanlines while B supplies colour. ⚡

**Fluid / Advection**
- **Fluid dye advection** — a source treated as continuous dye, advected
  through a divergence-free curl-noise vortex field, through A's real optical
  flow onto B, or through its own motion. ⚡
- **Field particles** — a grid of coloured particles rides the same steady
  vortex field, with optional live colour re-sampling from the playing
  source. ⚡

**Blend / Mosaic (mutual A×B)**
- **Convolutional blend** — Source A's frame becomes a normalized image kernel
  convolved with B (colour or luma kernels; ⚡), or A's audio becomes an
  L1-normalized impulse response reverbed onto B (mono or true-stereo, direct
  or FFT convolution).
- **Descriptor-coagulated flow blend** — A and B *mutually* mangled: patches
  of the screen group by colour/texture similarity into an ownership field
  that advects, smears, and collides over time (the first true two-source
  mutual effect). ⚡
- **Colour-group dispersion blend** — the content-advecting sibling: image
  content itself (not just an ownership mask) flows, shatters, and disperses
  along A's optical-flow current.
- **Fluid colour-sort mosaic** — both sources' tiles phase-separate into
  colour domains via cohesion/repulsion forces, then advect through a fluid
  field (adaptive tile sizes, live-refresh content, cluster-blob layout,
  dispersion band, turbulence, and steady vortex flow variants).

**Feedback / Datamosh**
- **Controlled datamosh** — recursive flow-reuse "bloom" plus a full
  codec-*simulated* tier: block-quantized motion, residual re-injection,
  per-block keep/drop refresh, and curated presets (`structured-melt`,
  `macroblock-rot`, `scanline-smear`, `codec-engrave`, …). ⚡
- **Real bitstream datamosh** — an explicit, intentionally *non-deterministic*
  carve-out outside the render graph: pure-Rust RIFF surgery duplicates
  P-frame chunks or strips the keyframe for authentic codec-artifact
  bloom/void-mosh, plus motion transfer, decoded back through ffmpeg.
- **Cascade collage** — a scribbled, morphing-edge tile cascade generator with
  per-step hue drift (source-less collage of rect/L tiles).
- **Trail cascade** — a grid of source tiles advected along a steady vector
  field and stamped onto a never-cleared canvas, smearing the image into
  streamline ribbons (vortex / river / oscillate / square-pop fields).

**Generative**
- **Morphogenesis** — a reaction-diffusion field simulation seeded from Source
  B's luma, reshaping B either by colourizing the growth into the frame
  (`pattern-mix`) or pushing B's pixels along the pattern gradient (∇V
  chemotaxis `displace`). Three models: **Gray-Scott** (coral/worms/mitosis),
  **FitzHugh-Nagumo** (excitable-media pulses), and **Lenia** (continuous
  breathing membranes). Source A's motion can live-inject into the field.
- **Granular mosaic** — Source B recomposed as luma- or colour-matched visual
  grains selected by Source A, with a temporal grain-pool mode that matches
  grains across the whole clip by colour + texture + audio RMS. ⚡

**Post / Look**
- **Retro static** — simulated bit-depth banding/dither with a selectable
  error-diffusion filter and progressive per-row shear.
- **Channel shift** — independent per-channel RGB pixel offsets, constant or
  driven per-row by Source A's optical flow.
- **Palette quantize** — posterize or a fixed neon palette; integer/enum
  modulation targets included.
- **Pixel sort** — threshold-bounded sort of contiguous runs, by row or column.

**Audio / Cross-Synth**
- **Video vocoder** — B's tonal envelope reshaped to match A's (histogram
  specification, or per-band gain). ⚡
- **Spectral cross-synthesis** — A's RMS drives B's amplitude, A's spectral
  centroid sweeps a filter on B, or — the headline mode — a real phase-vocoder
  imposes A's log-band spectral envelope onto B's spectrum through a complex
  inverse STFT, keeping B's own phase.
- **Audio impulse convolution** — A's audio as an impulse response reverbed
  onto B's audio (direct or FFT; mono, per-channel, or true-stereo IR).
- **Audio-to-video routing** — A's RMS/onset/centroid drives B's visual
  displacement amount.
- **Video-to-audio routing** — A's luminance or optical-flow magnitude drives
  B's audio gain, stereo pan, or filter cutoff, with hold or smoothed envelope
  sampling.

**Composition & tooling**
- **Effect chains** (`render-chain`) — compose single-source effects into an
  ordered pipeline from one JSON spec: each stage's output feeds the next, one
  reproducible manifest, stateful stages checkpoint per-stage, every stage can
  carry its own modulation routes.
- **Composition timeline** (`render-composition`) — arrange finished
  effect-chain scenes on a global timeline: per-scene sources, hard cuts,
  crossfades, a scene-fingerprint cache, and a reserved `master.` clock
  modulator.
- **Video oscillators** (`generate-frames`) — a source-less deterministic
  pattern generator that writes an ordinary PNG frame dir, so any effect,
  route, queue, or chain can consume it as a synthetic source.
- **Block collage** — hard-cut NxN tile collage between A and B driven by a
  spatially-coherent noise field.

## The modulation matrix — the modular-synth core

Nearly every knob above accepts a modulation route
(`--modulate "<target>=<source>[:<scale>[,<offset>]][@hold|@smooth]"`,
repeatable) instead of a static value:

- **Analysis sources**: `audio-rms`, `audio-onset`, `audio-centroid` (from a
  modulator WAV), `luma`, `flow` (from a modulator frame sequence),
  `edge-density`.
- **LFOs**: `lfo(sine|triangle|square|saw[,rate_hz[,phase]])` — a pure,
  media-free function of frame time. No modulator file needed at all.
- **Breakpoints**: `breakpoints(t0:v0;t1:v1;…)` — a piecewise-linear envelope,
  also the target format for a gesture recorded live in the app's preview.
- **MIDI**: a CC lane from a `.mid` file drives a knob (per-slot CC number).
- **Signal algebra**: combinators compose sources on the mod bus (Tier 5.1).
- **Named modulators**: `<name>.<source>` lets different routes on the same
  render read different WAVs or frame sequences.
- **Per-route sampling**: a trailing `@hold`/`@smooth` overrides the render's
  default envelope evaluation.
- **Mattes**: spatial matte gating restricts an effect (or route) to a masked
  region, defaulting to Source A in two-source effects.

Values always **clamp, never error** — an envelope can never abort a render
mid-sequence — and every stateful effect's checkpoint contract includes the
active routes, so changing a route on resume is refused rather than silently
drifting. This whole surface is symmetric across the direct CLI, the persisted
render queue (`queue-add-*` validates and rejects before persisting;
`queue-run-*` is byte-identical to the direct render), and the SwiftUI panels.

## Deterministic rendering, the render queue, and analysis caches

Renders happen two ways: a **direct CLI command** for one-off/scripted runs,
or a **persisted offline queue** (`queue-init` / `queue-add-*` / `queue-run-*`)
that survives interruption, records source/cache provenance, and produces a
ProRes-ready output bundle (`frames/`, audio stems, `manifest.json`,
`checkpoint.json`). Stateful effects resume exactly: `--stop-after-frame`
writes a checksummed unquantized RGBA32F state buffer, and re-running the same
command continues from it — a changed input, setting, or modulation route is
detected and refused rather than silently producing wrong output.

Analysis (optical flow, RMS/STFT/onset envelopes, grain descriptors) is
reusable sidecar data: each cache records its algorithm id, dimensions,
sampling convention, and a content fingerprint of its source, so a renderer
only reuses a sidecar that still matches and regenerates deterministically
otherwise.

## The SwiftUI macOS app

A native shell (not a wrapper), organized as a **sidebar + detail**
`NavigationSplitView`:

- A **persistent header** pins Source A / Source B (native file pickers,
  auto-extracted to PNG/WAV proxies with RMS/STFT sidecars) plus the global
  render-quality, export-format, and ProRes settings — always visible
  regardless of which effect you're on.
- A **categorized sidebar** lists every effect under the same groups shown
  above (Displacement, Fluid/Advection, Blend/Mosaic, Feedback/Datamosh,
  Generative, Post/Look, Audio/Cross-Synth, Composition, Tools).
- A **detail pane** shows only the selected effect's controls: its knobs, a
  "More knobs" disclosure, per-slot modulation routing (source, scale/offset,
  named modulators, LFO/MIDI/capture options, mattes), a Run button (submits a
  real persisted queue job), and — on effects that use the shared proxy
  pipeline — a **Quick Preview** band.

**Quick Preview** downscales the source proxies once (exact deterministic
box-average, default quarter resolution), renders a few seconds of the
selected effect through the *same engine* (only the input paths change), and
loops it in a real player (play/pause, frame counter, scale/seconds controls).
On an expensive effect like flow feedback this is over **13× faster** than a
full-res render. A **performance-capture** strip layers on top: arm a Rutt-Etra
modulation slot, scrub a slider against the loop, and the gesture is recorded
as a `breakpoints(…)` route — bit-exact, forever.

Completed queue bundles export directly to configurable ProRes `.mov` via
VideoToolbox, with any audio stems muxed in.

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
cargo run -p morphogen-cli -- <command> # see INSTRUCTIONS.md / docs/REFERENCE.md

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

# Rutt-Etra scanlines with an LFO on displacement depth (the canonical demo)
cargo run -p morphogen-cli -- render-rutt-etra-sequence /tmp/b-frames /tmp/re-out \
  --modulate "displacement_depth=lfo(sine,0.5):128,128"

# A fast quarter-res look before committing to the full render
cargo run -p morphogen-cli -- downscale-frames /tmp/b-frames /tmp/b-quarter --scale 4 --max-frames 48
```

## Documentation map

- **[`INSTRUCTIONS.md`](INSTRUCTIONS.md)** — the complete operational manual:
  how to use every feature from the CLI and the app (start here to *do*
  anything).
- **[`docs/REFERENCE.md`](docs/REFERENCE.md)** — the exhaustive CLI command
  catalog and source-file key-path map.
- **[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)** — system shape (core /
  Metal / cache / queue / UI).
- **[`docs/EFFECTS_ROADMAP.md`](docs/EFFECTS_ROADMAP.md)** — full per-effect
  design notes and landed-vs-deferred tiers.
- **`docs/*_MILESTONE.md`** — the acceptance contract for each effect/feature,
  written before it was built.
- **[`docs/BACKLOG.md`](docs/BACKLOG.md)** — completed work + what's next.
- **[`CLAUDE.md`](CLAUDE.md)** — the agent guide: invariants, workflow, and
  context-loading order.
- **[`STATUS.md`](STATUS.md)** — the current session-resume checkpoint.

## Current status

`cargo test --workspace`: **753 passing, 0 failing** across 7 crates.
`swift test`: **158 passing, 0 failing**. Every effect above has a landed CPU
reference; Metal parity, queue exposure, and SwiftUI panels are landed for the
large majority (noted per-effect in `docs/EFFECTS_ROADMAP.md` where a tier is
still CPU-only or direct-CLI-only). See [`STATUS.md`](STATUS.md) for the exact,
currently-verified baseline and the most recent work.
</content>
</invoke>
