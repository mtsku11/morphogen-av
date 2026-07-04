# Realtime-ish Preview Loop Milestone — play the instrument

**Status: Slices 1–3 landed and verified — MILESTONE COMPLETE** (`4908712`
box-downscale CLI, `bdd1a5f` playback, `7448c74` quarter-res fast path;
end-to-end on a 720p fixture: flow feedback **13.4×** faster at quarter res,
6.0 s → 0.4 s for 12 frames). Only the Deferred section remains open. This
doc is the acceptance contract
(per the CLAUDE.md "contract first" workflow). Origin: `docs/RECOMMENDATIONS.md`
Part 2 §C — "render N seconds at quarter-res straight to the preview surface
using the same engine." The invariant that keeps this tractable is
non-negotiable: **the preview is a downsampled view of the same project
graph, never a second engine and never a fork of any render logic.**

## Origin & Goal

What exists today: the Workflow panel's "Quick Preview" band
(`beginEffectPreview` / `EffectPreviewSession` in `AppState.swift`,
`previewBand` in `WorkflowPanelView.swift`) renders the first **8 frames at
full resolution** through the selected effect into a temp directory and shows
them as a **static thumbnail filmstrip**. Two things make it not feel like
playing an instrument: full-res renders are slow (so the cap stays tiny), and
nothing moves.

This milestone adds the two missing halves:

1. **A quarter-res fast path** — the preview session downscales the source
   proxy frames once (a new deterministic CLI command; the engine renders
   whatever frames it is given, so downscaled inputs ⇒ downscaled output with
   zero changes to any effect), letting the preview cover seconds of motion
   instead of 8 frames.
2. **Playback** — the preview band plays its frames at the render frame rate
   (loop + play/pause), replacing the static filmstrip.

Explicitly **not** realtime interaction: no live knob scrubbing, no
incremental re-render, no preview-while-rendering. Those need engine work
(streaming render) that must not be improvised here.

## Slice 1 — deterministic downscale command (CLI)

`downscale-frames <input-dir> <output-dir> --scale <n> [--max-frames <m>]`:

- **Mechanic:** per frame, exact **box average**: output pixel `(x, y)` is the
  unweighted mean of the `n×n` input block at `(n·x, n·y)`, computed in f32
  over all 4 channels; edge blocks clip to the image bounds (mean over the
  in-bounds subset, so non-divisible dimensions are well-defined:
  `out_dim = ceil(in_dim / n)`). Deterministic by construction; CPU-only
  (a preview utility does not get a Metal tier).
- `--scale` ≥ 1 (integer); `--scale 1` is the identity anchor — output pixels
  equal input pixels exactly (PNG re-encode may differ byte-wise; assert
  pixel equality, not file equality). `--max-frames` caps how many frames are
  processed (sorted order, the house frame-collection convention).
- Output frame names mirror the input basenames (the downstream render
  commands only care about sorted PNGs in a directory).
- Algorithm id `box_downscale_cpu_v1` printed in the summary line (no
  manifest — this is a preview utility, the palette-quantize stdout
  precedent applies and IS the contract here).

Acceptance:

1. Unit tests: dimension math (divisible + non-divisible), scale-1 pixel
   identity, box-average values pinned on a tiny synthetic image, max-frames
   cap, scale-0 rejection.
2. Smoke: two runs byte-identical; a downscaled directory feeds an existing
   render command (rutt-etra) and renders at the reduced dimensions.

## Slice 2 — preview playback (SwiftUI)

- The preview band gains a **player**: the loaded `previewFrames` animate at
  a preview frame rate (the proxy-extraction fps the frames were rendered
  at), looping, with a play/pause control and a frame-position indicator.
  Playback state lives in a small observable helper whose stepping logic
  (`index(at:)`-style pure function of elapsed time, frame count, fps) is
  **unit-testable without UI** — pin wraparound and pause semantics in tests.
- The static filmstrip remains as the paused state (or a thumbnail strip
  under the player — keep it simple; the player is the point).
- No Rust changes in this slice.

## Slice 3 — the quarter-res fast path (wire-up)

- `EffectPreviewSession` gains the input side: on `beginEffectPreview`, the
  session (a) downscales the selected proxy directories via the Slice-1
  command into the preview temp root (scale default **4**), (b) reroutes the
  effect render's *input* directories to the downscaled copies (the output
  reroute already exists), and (c) raises the preview frame cap from 8 to
  **N seconds × proxy fps** (default ~4 s — pick the exact default and pin it
  in the bridge tests).
- The preview then plays back through the Slice-2 player at that fps.
- Preview knobs surface minimally: a scale picker (1/2/4/8) and a seconds
  stepper in the preview band; both feed the session, nothing else.
- Bridge tests pin the downscale token sequence; the effect render's argument
  assembly must be **unchanged** apart from receiving different input paths
  (the same-engine invariant made visible in the tests).
- End-to-end proof: preview a rutt-etra render — report the downscaled
  dimensions, the frame count, and wall-clock vs a full-res preview of the
  same frame count (the number that justifies the milestone).

## Build plan (handoff notes)

- CLI: `downscale_frames` beside the other frame utilities (grep
  `collect_image_frames` in `imaging.rs` for collection; reuse the PNG
  load/save the render commands use). `args.rs` + `main.rs` wiring.
- App: `EffectPreviewSession` (currently output-root + maxFrames) grows
  input-override fields; `runSelectedEffectPreview` in
  `WorkflowPanelView.swift` is the choke point where every effect's preview
  starts; the per-effect render methods must NOT change (they already read
  `frameSequenceModulatorURL`/`frameSequenceCarrierURL` — the session
  swaps what those resolve to, or passes overrides through
  `effectiveOutputRoot`-style helpers).
- Playback: a `PreviewPlayerModel` (ObservableObject, Timer-driven) with the
  pure stepping function separated for tests.

Working agreements (standing, non-negotiable):

- Baseline before touching anything: `cargo test --workspace` (**522** green
  at contract time) and `swift test` (**100** green); report deltas, not
  adjectives.
- `/checkpoint` after each verified slice (local commit, source only, never
  push). `/verify` before calling any slice done.
- Never commit the untracked `scripts/solitaire-cascade-prototype.py` or
  `shader-port-ideas/`.
- Record non-obvious findings in `/memory/`, not in prose docs.

## Deferred (explicitly out of scope)

- Live knob scrubbing / incremental re-render / preview-while-rendering
  (needs a streaming engine path — its own milestone, engine-first).
- Metal-accelerated downscale (preview utility stays CPU).
- Preview for the audio commands and `render-chain` (the chain previews
  naturally once its builder panel exists — that design is still open).
- Scrub bar / frame stepping in the player (loop + play/pause is the MVP).
