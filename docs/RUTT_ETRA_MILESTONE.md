# Rutt-Etra Scanline Milestone — luma-displaced scanline rendering

**Status: contract written, no code landed.** This doc is both the acceptance
contract (per the CLAUDE.md "contract first" workflow) and the build handoff:
a fresh session should be able to implement Slices 1–3 from this file plus the
cited precedents alone. Origin: `docs/EFFECTS_ROADMAP.md` "Scanline /
Rutt-Etra Style Carrier Modulation" (the only roadmap effect with no code) and
`docs/RECOMMENDATIONS.md` Part 1 §1 (highest payoff ÷ effort).

## Origin & Goal

The Rutt-Etra video synthesizer look: the frame is re-rendered as a sparse set
of **horizontal scanlines on black**, and each scanline is **displaced
vertically by the local luminance** — bright regions push the line up, so the
image reads as a wireframe terrain of its own brightness. It is the most
iconic analog-video-synth aesthetic and an instant modulation-matrix showcase:
`displacement_depth=audio-rms` is *the* classic demo (lines surge with the
music).

MVP is **single-source** (the carrier's own luma displaces its own scanlines),
**stateless** (no prior-frame state, no checkpoint contract), and **CPU-only**
(Metal is a later slice; see Deferred). This is deliberately the smallest
vertical that produces the recognizable look.

## Mechanic (deterministic CPU reference)

Output canvas = source dimensions, prefilled **black** (`[0,0,0,1]`).
Scanlines are source rows `y0 = 0, pitch, 2*pitch, …` (top row always
included), drawn **top→bottom, last-writer-wins** — a displaced line from a
lower scanline overwrites earlier ones, which gives the classic occlusion
feel deterministically.

For each scanline `y0`, for each column `x in 0..w`:

- `luma(x, y0) = 0.2126*R + 0.7152*G + 0.0722*B` (linear, the house Rec.709
  convention — same coefficients as `conv_blend.rs:71` / `datamosh.rs:750`).
  Clamp to `[0,1]` before use so >1.0 float pixels can't overshoot.
- Displaced row `y = y0 - round_half_away_from_zero(displacement_depth * luma)`
  (positive depth pushes **up**; negative depth is allowed and pushes down).
  Integer rasterization, no anti-aliasing at MVP — hard pixels are the
  deterministic baseline; smoothing is the deferred HQ tier's job.
- **Connect adjacent columns**: with `y_a` = displaced row at `x` and `y_b` at
  `x+1`, fill the vertical span `min(y_a,y_b)..=max(y_a,y_b)` in column `x`
  (for the final column just `y_a`). This vertical join is what makes it read
  as a continuous wireframe rather than scattered dots.
- Each filled cell extends downward by `line_thickness` px (thickness 1 = the
  single cell). Rows falling outside the canvas are clipped, never wrapped.
- Colour: the **source pixel at `(x, y0)`** (the line carries the image's
  colour along its length). `--mono` renders all lines as white `[1,1,1,1]`
  instead — the classic monochrome CRT look.

## Knobs (CLI defaults)

| knob | flag | default | valid | notes |
|---|---|---|---|---|
| line pitch | `--line-pitch` | 8 | int ≥ 1 | rows between scanlines |
| displacement depth | `--displacement-depth` | 48.0 | finite | px at luma 1.0; sign = direction |
| line thickness | `--line-thickness` | 1 | int ≥ 1 | px, extends downward |
| mono | `--mono` | off | flag | white lines instead of source colour |

Validation at the CLI boundary: reject non-finite depth, pitch/thickness < 1
(clear `CliError::Message`, no `unwrap()` anywhere in library code — errors via
`thiserror` per the invariants).

Algorithm id: **`rutt_etra_scanline_cpu_v1`** in the render manifest.

## Off / identity anchors (the falsifiable base cases)

- `--displacement-depth 0` → **flat scanlines**: output row `y0..y0+thickness`
  equals the source row `y0` verbatim (or white when `--mono`), all other rows
  exactly black. This is the byte-stable "off" baseline every off-vs-on
  comparison renders against.
- Uniform-luma frame (e.g. solid white) at depth `d` → every line shifted by
  exactly `round(d)`; solid black source → identical to depth 0. Unit-test
  both.
- Two identical invocations → byte-identical frames (determinism smoke, same
  shape as the assertions in `crates/morphogen-cli/tests/smoke.rs`).

## Modulation targets (Slice 2)

Register in `crates/morphogen-render/src/modulation.rs` following
`apply_palette_quantize_modulation` (`modulation.rs:517`) exactly:

- `displacement_depth` — float, **clamp `[-512, 512]`**, clamp-never-error.
- `line_pitch` — integer rule: clamp `[1, 256]` **then** round nearest,
  ties away from zero (the established palette-quantize `levels` convention).
- `line_thickness` — integer rule, clamp `[1, 64]`.

Zero-route path must stay byte-identical to the unrouted render (the
per-frame settings-copy pattern used by every other apply fn). Because the
effect is stateless there is **no checkpoint contract** — routed velocity
never touches prior-frame state. Named modulators, `@hold`/`@smooth`
per-route sampling, and `--modulation-cache-dir` all come free through the
shared `ModulationCliArgs` plumbing — do not re-implement any of it.

## Acceptance criteria

Slice 1 — CPU reference + CLI (`render-rutt-etra-sequence`):
1. Unit tests in `rutt_etra.rs`: depth-0 identity anchor; solid-white shift
   == `round(depth)`; clipping at the top edge (huge depth) never panics and
   never wraps; `--mono` colour override; thickness fill.
2. CLI renders a PNG sequence with a manifest carrying the algorithm id +
   knobs; invalid knob values error clearly.
3. **Visual proof (the backbone):** render a horizontal black→white gradient
   fixture off (`depth 0`) vs on (default depth) — Read frames from both, the
   on-render's lines must ramp upward left→right; report the
   `scripts/frame-delta.py` cross-delta number. A look without a number is
   unfalsifiable; a number without the pixels proves nothing.

Slice 2 — modulation targets:
4. Clamp/integer-rule unit tests per target (use 0.5/0.25-style weights in any
   serde assertions — the f32 JSON round-trip trap).
5. `displacement_depth=audio-rms` on a chirp WAV against a **static** carrier:
   early-vs-late frames visibly differ in line height; report off-vs-on
   frame-delta numbers + Read the frames. (Static carrier so the envelope is
   the only source of change — the audio→video route precedent.)

Slice 3 — queue + SwiftUI:
6. `queue-add-rutt-etra-sequence` / `queue-run-rutt-etra-sequence` with
   add-time route validation via the shared `parse_queue_modulation_routes`;
   named-modulator vectors on the task (serde-defaulted, skip-when-empty so
   pre-slice queue JSON is untouched); **add→run byte-identical to the direct
   CLI render** (smoke-pinned, the `parity-check` philosophy).
7. SwiftUI panel mirroring the palette-quantize panel: knob controls, sticky
   backend picker omitted (CPU-only — no picker until Metal lands), mod slots
   for all three targets (`ModulationSlotRow`, ±ranges sized to the clamp
   windows: depth slot needs a large range like ±256/step 8 — the invisible
   ±8 trap for pixel-unit knobs), per-slot sampling override, per-slot
   Modulator picker + Named Modulators section (the swept-panel pattern),
   bridge arg tests pinning the emitted token sequence.

## Build plan (handoff notes)

**Mirror the palette-quantize vertical end to end** — it is the closest
precedent: stateless, single-source, modulated, queue task, SwiftUI panel.
Copy its shape, not its code:

- Renderer: new `crates/morphogen-render/src/rutt_etra.rs`, exported from
  `lib.rs` beside `palette_quantize`.
- CLI: `RenderRuttEtraSequence` in `args.rs` + `render.rs` handler (grep
  `RenderPaletteQuantizeSequence`, `args.rs:1096`, and follow every site it
  appears).
- Apply fn: `modulation.rs` beside `apply_palette_quantize_modulation`.
- Queue: follow `queue_add_palette_quantize_sequence` /
  `queue_run_palette_quantize_sequence` in `queue.rs`, including the
  named-modulator fields and `parse_queue_modulation_routes`.
- SwiftUI: `AppState` + `RustBridgePlaceholder` + `RenderPanelView` sections
  for palette quantize, including `modulationRoutes(slots:…slotModulators:…)`
  and `NamedModulatorsSection`.

Working agreements (standing, non-negotiable):
- Baseline before touching anything: `cargo test --workspace` (**469** green
  at contract time) and `swift test` (**90** green); report deltas, not
  adjectives.
- `/checkpoint` after each verified slice (local commit, source only, never
  push). `/verify` before calling any slice done.
- Fixture rendering + frame Reads are the proof for every look claim;
  `frame-delta.py` needs matching PNG pixel formats (RGB-vs-RGBA mis-decode
  trap).
- Never commit the untracked `scripts/solitaire-cascade-prototype.py` or
  `shader-port-ideas/`.
- Record non-obvious findings (tuning dead-ends, "looks right but isn't"
  traps) in `/memory/`, not in prose docs.

## Deferred (explicitly out of MVP scope)

- **Metal port.** The renderer is a scatter (each scanline writes a variable
  span), which is parity-hostile the same way the mosaic sim is; the
  field-particles splat precedent (scatter→gather inversion) is the likely
  shape when it happens. CPU must be proven and the look user-confirmed first.
- **Two-source A→B** (A's luma displaces B's scanline colours) — natural
  slice 4, after the single-source look is confirmed.
- **Depth-descriptor displacement** — blocked on the depth-modulator carve-out
  (`RECOMMENDATIONS.md` Part 2 §D: Apple depth models are not bit-reproducible
  across OS versions; needs the sidecar-fingerprint treatment).
- **HQ line rendering** (anti-aliasing, temporal supersampling, mesh-style
  continuous lines) — the roadmap's "future high-quality version".
