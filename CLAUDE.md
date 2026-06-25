# Morphogen AV — Project Guide

Mac-first experimental audiovisual cross-synthesis app. Two loaded sources:
**Source A** (modulator / analysis) drives the transformation of **Source B**
(carrier / material). Output is B reshaped by motion, audio, spectral, temporal,
or structural analysis derived from A. The long-term target is an audiovisual
modular synthesizer where typed analysis signals modulate visual/audio params.

This file is the canonical entry point for agents (auto-loaded each session).
`AGENTS.md` points here. Keep this file lean and always-true; depth lives in `docs/`.

## Non-Negotiable Invariants

- **Determinism first.** Offline deterministic rendering leads; realtime preview
  is a lower-fidelity view of the same project graph, never a separate engine.
  Identical inputs + settings ⇒ bit-reproducible output.
- **CPU reference is ground truth.** Every Metal kernel must match the CPU
  reference within tolerance (`METAL_CPU_PARITY_EPSILON`) and is gated frame-by-frame
  against it before export. Never ship a GPU path that hasn't passed parity.
- **Metal is the only GPU target.** No Vulkan, CUDA, WebGPU, or WGSL.
- **Stateful temporal nodes** declare frame-zero behavior, the exact prior-frame
  state consumed, and a checkpoint representation. Resume from an **unquantized**
  internal state buffer (RGBA32F), never from a display PNG. Changing the
  algorithm identifier, inputs, or settings must invalidate stale caches/checkpoints.
- **Analysis is reusable sidecar data**, regenerable from source + settings.
  Sidecars carry algorithm id, dimensions, sampling convention, and source
  fingerprint; reuse only a matching sidecar.
- **FFmpeg stays external and optional** — never vendor it; missing tools return a
  clear error. Avoid GPL-only application dependencies.
- **No `unwrap()` in library code** (tests excepted). Errors via `thiserror`.
- Prefer small, concrete vertical slices over broad abstractions.

## Everyday Commands

```sh
cargo test --workspace          # Rust tests (baseline: 343 passing across 7 crates)
cargo build --workspace         # build all crates
cargo run -p morphogen-cli -- <subcommand>   # engine validation path
swift build && swift test       # macOS SwiftUI shell + its service tests
swift run MorphogenMacApp        # run the app shell
```

The full CLI catalog and key-path map are in **[docs/REFERENCE.md](docs/REFERENCE.md)**.

### Project tooling

- **`/verify`** (project-local skill) — clippy + targeted tests + offline shader
  compile + a visual PNG check. Overrides the generic verify.
- **`/preview`** (project-local skill) — render an effect on a small fixture and
  look at the frames; the inner loop for tuning an effect's look.
- **`/fixture`** (project-local skill, `scripts/make-fixture.sh`) — scaffold a
  synthetic readout fixture for the granular-pool path. `--readout frame`
  (solid-colour frames whose output colour reveals the selected source *frame*) or
  `--readout origin` (coordinate-gradient carrier whose output colour reveals the
  selected source *location*, for spatial/selection knobs); optional chirp WAVs +
  RMS/STFT caches for audio/centroid runs. **Render readouts with `--variation 0`**
  (the pool render's 0.25 default scatters them).
- **`scripts/frame-delta.py`** — mean frame-to-frame change of a PNG sequence; the
  quantitative half of the off-vs-on knob check (pair it with Reading the frames).
- **`/parity`** (project-local skill, `scripts/parity-check.sh`) — prove a
  granular-pool render is path-independent: render the same job via the direct CLI
  and the queue add→run path, byte-compare every frame, show the persisted
  manifest knobs. The exploratory complement to the determinism assertions in
  `crates/morphogen-cli/tests/smoke.rs`.
- **`scripts/check-shaders.sh`** — offline-compiles the `.metal` shaders. Skips
  cleanly unless the Xcode Metal Toolchain component is installed
  (`xcodebuild -downloadComponent MetalToolchain`); the runtime tests in
  `morphogen-metal` already validate wired shaders + CPU parity during `cargo test`.
- **Visual verification is the backbone**: render to PNG, then read it as an image.
  Use `ffmpeg -i out.mov -frames:v 1 f.png` to inspect video/ProRes output.
- Use **context7** for current Metal Shading Language / AVFoundation / VideoToolbox
  and crate docs rather than relying on memory.

## Workflow (how I build here)

1. **Contract first.** Before implementing an effect, read/extend its
   `docs/*_MILESTONE.md` contract. Acceptance criteria are defined there.
2. **CPU reference, then Metal.** Land the deterministic CPU path with focused
   tests, then add the Metal kernel gated against it. Don't expose a feature in
   the queue/SwiftUI before its CPU path is proven.
3. **Verify before "done".** Run `/verify` (typecheck/tests, build if config
   changed). Capture a baseline pass/fail count *before* changing anything and
   report the delta — "no regressions" needs a number. Show evidence, don't assert.
   For any feature that affects output (an effect, parameter, or selection/
   scheduling knob), tests + parity prove determinism but **not** that the knob
   does what it claims — also render it **off vs on** on a readout fixture with
   `--variation 0`, Read frames from both, and report the `frame-delta.py` number.
   A look without a number is unfalsifiable; a number without the pixels proves
   nothing.
4. **Checkpoint each verified increment** with `/checkpoint` (local commit,
   source files only, no push). Commit/push only when asked; branch off `main`
   first if pushing. Long uncommitted stretches lose work to session cutoffs.
5. **Record non-obvious findings** in `/memory/` (auto-recalled), not in prose
   docs. Empirical lever sweeps, "looks right but isn't" traps, tuning dead-ends.

## Context-Loading Order

1. This file (`CLAUDE.md`) — invariants + workflow.
2. `STATUS.md` — current phase, baseline, next action.
3. `docs/ARCHITECTURE.md` — system shape (core / Metal / cache / UI / queue).
4. `docs/BACKLOG.md` — completed + next tasks.
5. `docs/EFFECTS_ROADMAP.md` — long-term effect plan.
6. The relevant `docs/*_MILESTONE.md` contract for the effect in flight.
7. `docs/REFERENCE.md` + the crate/app files for the task at hand.

## Layout

- `crates/morphogen-core` — project schema, graph, timeline, render-job + queue persistence.
- `crates/morphogen-render` — deterministic CPU renderers, flow/grain/feedback caches, samplers.
- `crates/morphogen-audio` — WAV I/O, RMS, STFT, onset, spectral centroid.
- `crates/morphogen-media` — optional external FFmpeg/FFprobe wrappers.
- `crates/morphogen-metal` — Metal device/pipeline/texture + compute kernels (parity-gated).
- `crates/morphogen-cli` — the engine validation + render driver.
- `apps/macos` — native SwiftUI shell (calls the CLI via a dev bridge; no direct Rust link yet).
