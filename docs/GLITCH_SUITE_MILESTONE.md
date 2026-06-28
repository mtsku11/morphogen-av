# Glitch Suite Milestone — Pixel Sort, Channel Shift, Palette Quantize

## Origin & Goal

Reference look (databend / glitch art, Reddit r/AbstractArt): long axis-locked
**pixel-sorted streaks** bounded by brightness/edge thresholds, **stepped row
shear**, **chromatic channel split**, and a **limited high-saturation palette**
(neon magenta / orange / teal flats). None of the current effects produce this —
`fluid-colour-sort-mosaic` sorts *2D tiles* by colour bin, not *1D scanlines*;
datamosh smears along curved optical flow, not clean axis-aligned spans.

This milestone adds three new effects that together reproduce the look and open a
whole glitch-art genre the app can't currently touch:

1. **Pixel Sort** (headline) — per-row/column threshold-bounded pixel sorting.
2. **Channel Shift** — per-channel spatial offset (RGB split / chromatic edges).
3. **Palette Quantize** — posterize into a limited palette (the "designed" feel).

All three are **stateless, per-frame, deterministic** effects (no cross-frame
state, no checkpoint representation needed — unlike datamosh/feedback). Each
lands as a CPU reference first, then a parity-gated Metal kernel, per the
project's non-negotiable workflow.

**Recommended build order:** ① Pixel Sort (≈80% of the look) → ② Channel Shift →
③ Palette Quantize. ② and ③ are small finishers that stack on ① (and on any
existing effect). Ship and review each before starting the next.

The target reproduction pipeline once all three land:
`pixel-sort (A-edge mask, luma key) → channel-shift → palette-quantize`.

---

## Cross-Synth Framing (why this fits Morphogen, not just a stock glitch tool)

The app's identity is **A modulates B**. A stock pixel sorter sorts one image by
its own threshold. Our differentiator: **Source A's analysis defines the sort
mask; Source B is the material being sorted.** A's luma / edges / optical-flow
decide *where spans start and stop and how far they smear* — A "plays" the sort
on B. Pixel Sort therefore ships in **two modes**:

- **`--mask-source self`** — single-source classic pixel sort (B masks itself).
  This is the simplest slice and the off-vs-on baseline.
- **`--mask-source a-luma | a-edge | a-flow`** — two-source cross-synth. A's
  descriptor drives B's sort. `a-flow` reuses the existing optical-flow path
  (`compute_optical_flow_backend`); `a-edge` is a Sobel magnitude on A's luma.

Channel Shift and Palette Quantize are primarily single-source finishers but each
takes an optional A-driven modulation (A-flow → per-row shift; A-palette → k-means
palette extracted from A). Keep those A-driven variants as **later slices** — the
single-source core is the MVP for all three.

---

## Effect ① — Pixel Sort

### Algorithm (CPU reference — ground truth)

For each output frame, operating on Source B at B's dimensions:

1. **Build the mask** (a per-pixel boolean "sortable"):
   - `self`: pixel is sortable iff its sort-key `k(px) ∈ [threshold_low, threshold_high]`.
   - `a-luma`: resample A's luma to B's grid; sortable iff `luma_A ∈ [low, high]`.
   - `a-edge`: Sobel magnitude of A's luma (resampled to B); sortable iff
     `edge_mag ∈ [low, high]` — sorts smear *between* edges, leaving edges crisp
     (this is what produces the "coherent block next to smeared zone" look).
   - `a-flow`: optical-flow magnitude A→A(prev) (resampled to B); sortable iff
     `flow_mag ∈ [low, high]` — moving regions sort, static regions hold.
2. **Per line** (row if `--axis row`, column if `--axis col`): walk the line and
   split it into maximal contiguous **spans** of sortable pixels (mask = true).
   Each span is sorted **independently**; non-sortable pixels stay in place.
3. **Sort each span** by the sort key `--key {luma,hue,sat,red,green,blue}`,
   ascending or descending (`--direction`). Stable sort for determinism.
   Optional `--max-span N` (0 = unbounded): spans longer than `N` are sorted in
   `N`-pixel chunks (bounds the streak length; the reference has finite streaks).
4. Write sorted pixels back into their span positions.

- Sort key on a pixel: `luma` = Rec.709 (`0.2126R+0.7152G+0.0722B`, the project's
  standard, see `granular_mosaic`); `hue`/`sat` from RGB→HSV; `red/green/blue` =
  raw channel. Keep the conversion in one helper, mirror it exactly in MSL.
- **Off case (byte-identical passthrough):** `threshold_low > threshold_high`
  (empty mask → nothing sortable → B verbatim). Document and unit-test this. A
  second identity: `--axis row --key luma` with `low=0,high=0` on a non-black
  image sorts nothing (no pixel has luma exactly 0 in typical footage) — prefer
  the explicit empty-range identity for the test.
- **Determinism:** stable sort + fixed key + integer span boundaries ⇒ identical
  inputs/settings ⇒ identical output. No RNG.

### Metal port

Embarrassingly parallel **by line**: one threadgroup per row (or column).
- Each thread-group loads its line into threadgroup memory, builds the mask,
  finds spans, sorts each span, writes back. A bitonic sort within threadgroup
  memory is the clean GPU approach (power-of-two pad per span, or sort the whole
  line with sentinel keys that pin non-sortable pixels to their index).
- Parity surface = one kernel (`pixel_sort.metal`). Gate against the CPU
  reference frame-by-frame within `METAL_CPU_PARITY_EPSILON` like every other
  kernel. **Watch:** sort stability — a comparison sort that isn't stable can
  reorder equal keys differently than the CPU's stable sort and break parity.
  Make the GPU sort key a composite `(key, original_index)` so ties resolve by
  original position exactly as the CPU stable sort does. This is the single
  biggest parity risk in this milestone — call it out in the kernel.
- The `a-flow` mask's flow computation stays CPU (reuses
  `compute_optical_flow_backend`); only the sort kernel is GPU. Same one-parity-
  surface discipline as the LK port ([[metal-optical-flow-port]]).

### Parameters (`render-pixel-sort-sequence`, two-source A + B)

| Flag | Default | Meaning |
|---|---|---|
| `--axis row\|col` | `row` | Sort direction |
| `--key luma\|hue\|sat\|red\|green\|blue` | `luma` | Sort key |
| `--mask-source self\|a-luma\|a-edge\|a-flow` | `self` | What defines sortable spans |
| `--threshold-low` | `0.25` | Lower bound of sortable mask range [0,1] |
| `--threshold-high` | `0.80` | Upper bound |
| `--direction asc\|desc` | `asc` | Sort order |
| `--max-span` | `0` | Max streak length px (0 = unbounded) |
| `--frames` | `120` | Output frame count |
| `--backend cpu\|metal` | `cpu` | CPU is ground truth |
| `--seed` | `0` | (Reserved; no RNG in MVP — keep for parity with siblings) |

`self` mode ignores Source A (single-source); still accept an A dir for CLI
uniformity OR add a `render-pixel-sort-single-sequence`—prefer the former (pass A
== B is fine; `self` never reads A).

### Slices

- **Slice 1 (MVP):** `self` mode, row axis, luma key, threshold mask, CPU only.
  CLI `render-pixel-sort-sequence`. Off-vs-on readout + tests. **Review here.**
- **Slice 2:** Metal kernel for Slice 1 (parity-gated). The hard part — bitonic
  sort + tie-stability. Verify max diff < 1/255 on a textured fixture.
- **Slice 3:** `--axis col`, `--direction`, `--key {hue,sat,rgb}`, `--max-span`.
  All CPU-side knobs over the same kernel; extend Metal where trivial.
- **Slice 4 (cross-synth):** `--mask-source a-luma | a-edge | a-flow`. A drives
  the mask. `a-flow` reuses optical flow. This is the headline differentiator.
- **Slice 5 (optional):** queue task + SwiftUI wiring (see "Queue/UI" below).

---

## Effect ② — Channel Shift (RGB split)

### Algorithm (CPU reference)

Offset each colour channel spatially by an independent vector, sampling B with
clamped addressing (reuse `sampler.rs` `sample_*_clamped`):
`out.R = B.R(x - shift_r, y)`, `out.G = B.G(x - shift_g, y)`, `out.B = B.B(...)`.
- MVP: constant per-channel `(dx,dy)` offsets. Alpha (if any) untouched.
- **Off case:** all offsets `0` ⇒ B verbatim (byte-identical). Unit-test.
- A-driven variant (later slice): per-row shift = A's optical-flow X-magnitude at
  that row × gain → the shift *animates with A's motion*. Reuses
  `compute_optical_flow_backend`.

### Metal port

Trivial: per-pixel gather, three clamped bilinear samples. New kernel
`channel_shift.metal`. Parity is exact (same bilinear math as `flow_displace`).

### Parameters (`render-channel-shift-sequence`)

`--shift-r-x/-y`, `--shift-g-x/-y`, `--shift-b-x/-y` (px, default 0),
`--frames`, `--backend`. Keep it dead simple. Reference look uses a small
horizontal R/B split (±2–6 px) — note that as a preset in the doc.

### Slices

- **Slice 1:** constant offsets, CPU. Off-vs-on readout (offset 0 vs ±6 px on a
  hard-edged fixture so the chromatic fringe is visible). **Review.**
- **Slice 2:** Metal kernel (parity-gated).
- **Slice 3 (optional):** A-flow-driven per-row shift.

---

## Effect ③ — Palette Quantize (posterize)

### Algorithm (CPU reference)

Map each B pixel to the nearest entry of a palette of `K` colours:
- MVP: **uniform posterize** — quantize each channel to `--levels L` steps
  (`round(c * (L-1)) / (L-1)`). `L = 256` ⇒ identity (off case, byte-identical).
- Mode 2 (`--mode palette`): a **fixed named palette** (ship a small neon LUT
  matching the reference: magenta/orange/teal/black) — nearest-colour in RGB
  (or better, in a perceptual space; RGB L2 is fine for MVP). Nearest match by
  L2 distance, deterministic tie-break by lowest palette index.
- Mode 3 (later, A-driven): **k-means palette extracted from Source A** (K
  centroids, fixed iteration count + fixed init = deterministic). A literally
  donates its colour palette to B. k-means must be seeded deterministically
  (k-means++ with a fixed seed, fixed iteration count) — no wall-clock, no RNG
  drift, per the determinism invariant.

- **Off case:** `--mode posterize --levels 256` ⇒ B verbatim. Unit-test.
- **Determinism:** all modes are pure lookups or fixed-iteration k-means; no RNG
  beyond a seeded init.

### Metal port

Posterize and fixed-palette nearest are per-pixel → trivial kernel
`palette_quantize.metal`, parity-exact. k-means centroid *computation* (Mode 3)
stays CPU (reduction order); only the per-pixel nearest-assignment is GPU — same
compute-on-CPU / apply-on-GPU split as the vocoder LUT ([[video-vocoder-look]]).

### Parameters (`render-palette-quantize-sequence`)

`--mode posterize\|palette\|kmeans`, `--levels` (posterize, default 256),
`--palette <name>` (built-in neon set; default `neon`), `--colors K` (kmeans,
default 6), `--frames`, `--backend`.

### Slices

- **Slice 1:** posterize, CPU. Off-vs-on (levels 256 vs 4). **Review.**
- **Slice 2:** fixed neon palette mode + Metal kernel for both.
- **Slice 3 (later):** k-means-from-A palette (deterministic seeded).

---

## Architecture — where everything lands (mirror the block-collage wiring)

Each effect follows the exact pattern of `block_collage` / `fluid_mosaic`. File
touch-list **per effect** (use `block_collage` as the copy-from template):

1. **`crates/morphogen-render/src/<effect>.rs`** (new) — settings struct
   (`#[derive(Serialize,Deserialize,Copy,Clone,PartialEq)]` with `Default`),
   algorithm id const (`PIXEL_SORT_ALGORITHM = "pixel_sort_threshold_span_v1"`,
   etc.), `render_<effect>_frame(...) -> Result<ImageBufferF32, RenderError>`,
   and focused `#[cfg(test)]` unit tests (off-case identity + a known small-array
   sort result). **No `unwrap()`** in non-test code; errors via `RenderError`.
2. **`crates/morphogen-render/src/lib.rs`** — `pub mod <effect>;` + re-export the
   frame fn, settings, and algorithm const (copy the `block_collage` block at
   lib.rs:30).
3. **`crates/morphogen-cli/src/args.rs`** — add the `Render<Effect>Sequence`
   command variant (copy `RenderBlockCollageSequence` at args.rs:663) with the
   flags above.
4. **`crates/morphogen-cli/src/render.rs`** — `<Effect>SequenceRequest<'a>`
   struct + `render_<effect>_sequence(...)` driver (copy
   `render_block_collage_sequence` at render.rs:2349). Load A/B PNG dirs, loop
   frames, write `frame_%06d.png`, print the summary line with settings + backend.
5. **`crates/morphogen-cli/src/main.rs`** — dispatch the new command(s).
6. **Metal (Slice 2 of each):**
   - `crates/morphogen-metal/shaders/<effect>.metal` (new kernel).
   - `crates/morphogen-metal/src/flow_displace_dispatch.rs` — kernel-name const,
     `<EFFECT>_SHADER_SOURCE` include, error variants, and a
     `validate_<effect>_shader_source()` + binding test (copy the LK refine block).
   - `crates/morphogen-metal/src/runtime.rs` — `<effect>_metal(...)` dispatch +
     a `metal_<effect>_matches_cpu_reference` parity test on a synthetic fixture.
   - `crates/morphogen-metal/src/lib.rs` — exports + `#[cfg(target_os="macos")]`
     re-export of the dispatch fn.
   - Wire `--backend metal` in the CLI driver to call it, **parity-gated
     per-frame** (these are stateless, so use the standard every-frame gate —
     NOT the validate-then-trust model, which is only for the expensive recursive
     flow path).
7. **Queue/UI (final optional slice per effect):**
   - `crates/morphogen-core/src/render_job.rs` — `FrameSequence<Effect>` task
     variant + `default_*` serde fns (copy `FrameSequenceBlockCollage` at
     render_job.rs:202).
   - `args.rs` — `QueueAdd<Effect>Sequence` / `QueueRun<Effect>Sequence`.
   - `apps/macos` — SwiftUI controls + a sticky backend picker
     (`backend.<effect>`, see `AppState.stickyBackend`).

---

## Verification (per the project workflow — required, not optional)

For **every** slice, before "done":

1. **Baseline first.** `cargo test --workspace` (baseline 355+; record the number
   *before* touching anything) → report the delta, not "no regressions".
2. **Unit tests** in the render module: off-case byte-identity + a hand-checked
   small-input result (e.g. sort a known 8-px row, assert the exact reordering).
3. **Metal slices:** `cargo test -p morphogen-metal` runs the parity gate; assert
   max diff < `METAL_CPU_PARITY_EPSILON` (1/255) on a textured fixture. Report the
   measured number (the LK port came in ~56× under tolerance — aim similar).
4. **Off-vs-on readout (the look proof — tests prove determinism, NOT that the
   knob does what it claims):** render the effect **off vs on** on a fixture with
   `--variation 0`-equivalent determinism, Read frames from both, and report the
   `scripts/frame-delta.py` (within-seq) or cross-sequence delta number.
   - Pixel sort: use a **textured, high-contrast** fixture (a flat colour can't
     show sorting — needs luma variation along the sort axis). Cross-seq delta
     off (empty mask) vs on.
   - Channel shift: a **hard-edged** fixture (chromatic fringe only appears at
     edges); off (0 px) vs on (±6 px), cross-seq.
   - Palette: any real frame; off (levels 256) vs on (levels 4), cross-seq.
   - **A number without the pixels proves nothing; pixels without a number is
     unfalsifiable. Report both.**
5. **`/checkpoint`** after each verified slice (local commit, source only, no push).

### Known traps to write into `/memory/` as discovered

- Metal sort **tie-stability** vs CPU stable sort (composite key — see ① Metal).
- Pixel sort on a **flat fixture is a no-op** — the off-vs-on readout will read 0
  and look like a broken effect; the fixture must have luma variation along the
  axis. (Sibling of the [[granular-texture-dims]] / "fixture must MOVE" traps.)
- HSV hue is **circular** — sorting by hue near the red wrap (0°≈360°) can look
  discontinuous; document the convention, don't "fix" it silently.
- Palette k-means must be **seeded + fixed-iteration** or it breaks determinism
  (no wall-clock, no unseeded RNG — the script env forbids `Math.random`-style
  nondeterminism for exactly this reason).

---

## Acceptance Criteria (definition of done for the milestone)

- [ ] `render-pixel-sort-sequence` (`self`, row, luma, threshold) CPU + Metal,
      parity < 1/255, off-case byte-identical, off-vs-on readout reported.
- [ ] Pixel sort cross-synth modes (`a-luma`/`a-edge`/`a-flow`) land with A
      genuinely driving the mask (off-vs-on shows A's structure in B's sort).
- [ ] `render-channel-shift-sequence` CPU + Metal, off-case byte-identical.
- [ ] `render-palette-quantize-sequence` (posterize + neon palette) CPU + Metal,
      off-case byte-identical.
- [ ] The three-effect reproduction pipeline renders the reference look on
      `cello2-frames` (pixel-sort → channel-shift → palette-quantize) — ship a
      contact sheet as the visual proof.
- [ ] `docs/EFFECTS_ROADMAP.md` and `docs/BACKLOG.md` updated; non-obvious
      findings recorded in `/memory/`.

---

## Scope discipline (read before starting)

- **Simplest thing first.** Slice 1 of each effect is single-source, one axis,
  one key, CPU-only, constant params. Resist building the cross-synth/k-means/UI
  before the MVP is proven and reviewed.
- **CPU reference is ground truth.** Never wire `--backend metal` into the CLI
  before its CPU path has focused tests passing. No GPU path ships without the
  parity gate.
- **Stateless = no checkpoint machinery.** These are per-frame effects; do NOT
  add feedback-state / checkpoint plumbing (that's only for datamosh/feedback).
- **Surgical changes.** Copy the block-collage wiring; don't refactor adjacent
  effects. Every changed line traces to this milestone.
- **Pause for review after Slice 1 of each effect** before the Metal port.
