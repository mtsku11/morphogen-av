# Rutt-Etra Two-Source A→B Milestone

**Status: COMPLETE (slices 1–3, 2026-07-04).** CPU reference + CLI (`eda63f1`),
parity-gated Metal gather kernel (`611b859`), queue task + SwiftUI panel
(`ec53aa1`). Look confirmed on real footage (cello A × harp B → cellist
silhouette carrying harp's colour, cross-delta ~38/255). Follows the
single-source CPU MVP (`docs/RUTT_ETRA_MILESTONE.md`) and Metal port
(`docs/RUTT_ETRA_METAL_MILESTONE.md`). This is the first *cross-synthesis*
Rutt-Etra — dead-center on the project's A-modulates-B thesis.

## Origin & Goal

The single-source renderer displaces a carrier's scanlines by the carrier's
**own** luma — Source B reshaping itself. The natural next slice, deferred in
`RUTT_ETRA_METAL_MILESTONE.md`, is **two-source**: Source A (modulator /
analysis) supplies the luma that drives the vertical displacement, while Source
B (carrier / material) supplies the **colour** drawn on the scanline. The output
is B's material reorganised by A's structure — the Rutt-Etra wireframe terrain
of A, painted in B's pixels.

## Mechanic (the design invariant)

For scanline `y0` (`0, pitch, 2·pitch, …`), at column `x`:

1. `luma_a = clamp(Rec.709 luma of source_a(x, y0), 0, 1)` — **from A**.
2. `shift = round(displacement_depth · luma_a)`; `y = y0 − shift` (positive depth
   pushes up), computed for column `x` and its neighbour `x+1` (last column
   clamps to itself), exactly as single-source.
3. `span_lo = min(y_a, y_b)`, `span_hi = max(y_a, y_b) + line_thickness − 1`,
   clipped to `[0, height−1]`.
4. Fill drawn colour = `source_b(x, y0)` (or `[1,1,1,1]` under `mono`) — **from B**.
5. Scanlines drawn top→bottom, **last-writer-wins** (identical draw order to
   single-source).

**Continuity identity (the parity proof):** with `source_a == source_b`, this is
byte-identical to `render_rutt_etra_frame`. The single-source renderer *is* the
A==B special case; the two-source function must reduce to it exactly. This is the
anchor test — a single equality proves the fold is correct.

Everything else — `line_pitch`, `line_thickness`, `mono`, the modulation targets
(`displacement_depth` / `line_pitch` / `line_thickness`), the `round()` /
Rec.709 conventions, the clip-don't-wrap edge behaviour — is unchanged from
single-source. Only the **luma source** moves from B to A.

## Dimensions

A and B must have identical dimensions. A mismatch returns
`RenderError::IncompatibleInputs` (the `fluid_advect_two_source_frame_cpu`
precedent), never a panic or a silent clamp. Normalized-coordinate sampling of a
differently-sized A is explicitly **out of scope** (a later slice if a use case
needs it) — proxy extraction already gives A and B matching dimensions in
practice.

## New algorithm ids

- `rutt_etra_two_source_cpu_v1`
- `rutt_etra_two_source_metal_v1`

Written to the manifest **only** when a Source A is supplied. The single-source
ids (`rutt_etra_scanline_cpu_v1` / `_metal_v1`) stay for the A-absent path,
byte-identical to today.

## CLI surface (the fold decision)

Add an **optional** `--source-a-dir <dir>` to the existing
`render-rutt-etra-sequence` command (the channel-shift `--source-a-dir`
flow-driven precedent — one command, mode selected by A's presence), **not** a
separate `render-rutt-etra-two-source-sequence` command:

- **A absent** ⇒ current single-source behaviour, byte-identical, algorithm id
  and manifest unchanged.
- **A present** ⇒ two-source; A frames paired with B frames by index,
  `frame_count = min(frames, len_a, len_b)`; algorithm id switches to the
  two-source id; manifest gains a `source_a` provenance field.

All existing knobs, `--backend`, and the full `--modulate` flag set carry over
unchanged (modulation still drives `displacement_depth` etc. — the settings
struct is identical).

## Slices

### Slice 1 — CPU reference + CLI

**`crates/morphogen-render/src/rutt_etra.rs`**
- `pub const RUTT_ETRA_TWO_SOURCE_ALGORITHM: &str = "rutt_etra_two_source_cpu_v1";`
- `render_rutt_etra_two_source_frame(source_a, source_b, settings) -> Result<ImageBufferF32, RenderError>`:
  luma from A, colour from B, per the mechanic. Dimension-mismatch →
  `IncompatibleInputs`. Refactor: single-source `render_rutt_etra_frame` should
  delegate to the two-source fn with `source_a = source_b` (or share a private
  core taking a luma source + a colour source) so the two paths cannot drift.
  Keep `render_rutt_etra_frame`'s public signature and behaviour byte-identical.
- Tests: **continuity identity** (A==B ⇒ byte-identical to `render_rutt_etra_frame`
  on a gradient, the keystone); a case where A≠B proves the split (A a
  vertical-luma ramp, B a flat colour → displacement follows A, colour is B's);
  dimension-mismatch error; the existing off/identity/mono/thickness anchors
  re-asserted through the two-source fn.

**`crates/morphogen-cli`** (`args.rs`, `render.rs`)
- `--source-a-dir: Option<PathBuf>` on `RenderRuttEtraSequence`.
- `RuttEtraSequenceRequest` gains `source_a_dir: Option<&Path>`. When `Some`,
  `collect_image_frames` on A, pair by index, dispatch the two-source fn; switch
  the manifest `algorithm` + add `source_a`.
- Acceptance readout (per CLAUDE.md workflow §3): render the **same B** twice —
  once single-source (B drives its own scanlines) and once two-source with a
  **distinct A** (A drives displacement) — Read frames from both and report the
  `scripts/frame-delta.py` cross-delta. A number without the pixels proves
  nothing; the pixels must show A's structure in the scanline shape with B's
  colour in the fill. Also prove the A==B CLI render is byte-identical to the
  single-source render (`diff -r`).

### Slice 2 — Metal gather kernel

**`crates/morphogen-metal/shaders/rutt_etra_two_source.metal`** (new)
- The single-source gather kernel with a **second texture**: `source_a`
  (`::read`, texture 0) for luma, `source_b` (`::read`, texture 1) for colour;
  output at texture 2. Same reverse-scan / first-covering-span / last-writer-wins
  gather proof as `rutt_etra_scanline.metal` — only the luma read moves to A.
  Same `RuttEtraParams` buffer.

**`crates/morphogen-metal/src/flow_displace_dispatch.rs`**
- `RUTT_ETRA_TWO_SOURCE_KERNEL_NAME` + `_SHADER_SOURCE`; a
  `validate_rutt_etra_two_source_shader_source()` checking the kernel name and
  all four bindings (two read textures, one write, one buffer). Reuse
  `RuttEtraDispatchPlan` (dims + params are identical; the guards added in the
  fixup — `line_pitch >= 1`, `line_thickness >= 1` — apply unchanged).

**`crates/morphogen-metal/src/runtime.rs`**
- `render_rutt_etra_two_source_frame_metal(source_a, source_b, settings)` on the
  single-source `render_rutt_etra_frame_metal` shape (upload two read textures).

**`crates/morphogen-cli/src/render.rs`**
- `render_rutt_etra_two_source_frame_metal` wrapper embedding the epsilon parity
  gate (`max_channel_difference` vs `METAL_CPU_PARITY_EPSILON`, the fixup
  convention — **not** exact `!=`). Sequence dispatch picks the two-source Metal
  fn when A present + `--backend metal`.

- Acceptance: runtime parity test (small synthetic A + B, CPU vs Metal exact
  `ImageBufferF32` equality — the definitive gate); CLI smoke byte-comparing
  `--backend cpu` vs `--backend metal` two-source frames; the manifest records
  `rutt_etra_two_source_metal_v1`.

### Slice 3 — Queue + SwiftUI

**Queue** (`args.rs`, `queue.rs`, `render_job.rs`)
- `FrameSequenceRuttEtra` task gains `source_a: Option<...>` provenance
  (`#[serde(default)]`, skip-when-none, so pre-slice queue JSON stays
  byte-identical). `queue-add-rutt-etra-sequence` passes `--source-a-dir`
  through; `queue-run` dispatches two-source when present. add→run
  byte-identical to the direct two-source render (smoke-pinned, both CPU and
  Metal backends).

**SwiftUI** (`RenderPanelView.swift`, `AppState.swift`, `RustBridgePlaceholder.swift`)
- The Rutt-Etra panel gains an **optional Source A** picker (the shared
  frame-sequence modulator picker — the channel-shift panel's Source-A pattern).
  Empty ⇒ single-source (arg array byte-identical to today, pinned). Set ⇒ the
  bridge emits `--source-a-dir` before the positional args. Bridge tests pin the
  A-absent and A-present token sequences.

## Acceptance criteria (roll-up)

1. **Continuity identity**: `render_rutt_etra_two_source_frame(b, b, s)` is
   byte-identical to `render_rutt_etra_frame(b, s)` (render-crate test).
2. **Split proof**: with A≠B, displacement tracks A's luma and fill is B's
   colour (render-crate test + a CLI off-vs-on readout with a `frame-delta.py`
   number and Read frames).
3. **Dimension mismatch** → `IncompatibleInputs`, no panic.
4. **CLI A==B byte-identity**: `--source-a-dir` pointing at the same dir as B is
   `diff -r`-clean vs the single-source render.
5. **Metal parity**: two-source Metal byte-identical to CPU (runtime test +
   per-frame gate in the render path); manifest records the two-source Metal id.
6. **Queue add→run** byte-identical to direct (both backends).
7. **Bridge**: A-absent and A-present `--source-a-dir` token sequences pinned.
8. **Visual proof**: Read two-source frames on real footage (cello A ×
   harp/other B, 640×360 proxies) — confirm A's wireframe carrying B's colour.

## Working agreements (standing, non-negotiable)

- Baseline before touching anything: `cargo test --workspace` (**538 passing,
  0 failing** as of `e0c27b5`) and `swift test` (**115 passing, 0 failing**);
  report deltas, not adjectives.
- CPU reference first, then the parity-gated Metal kernel. Don't expose the
  feature in queue/SwiftUI before its CPU path is proven.
- `/checkpoint` after each verified slice (local commit, source only, never
  push).
- Never commit the untracked `scripts/solitaire-cascade-prototype.py` or
  `shader-port-ideas/`.
- Record non-obvious findings (parity traps, luma-source edge cases) in
  `/memory/`, not in prose docs.

## Deferred (out of scope)

- Normalized-coordinate sampling of a differently-sized A.
- A driving a *second* knob (e.g. A's luma → colour intensity as well as
  displacement) — the MVP is one cross-synth channel: A→displacement, B→colour.
- Depth-descriptor displacement (blocked on the depth-model carve-out).
- HQ anti-aliased lines (roadmap long-term tier).
