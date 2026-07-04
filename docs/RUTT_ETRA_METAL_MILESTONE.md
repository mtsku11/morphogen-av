# Rutt-Etra Metal Port Milestone

**Status: Not started.** Contract written 2026-07-04. CPU look confirmed on real
footage (cello player, 640×360 proxies); gate cleared. This is a single slice.

## Origin & Goal

The CPU Rutt-Etra renderer (`render_rutt_etra_frame`, `rutt_etra_scanline_cpu_v1`)
is a **scatter**: it iterates scanlines top→bottom and for each column writes a
vertical span to the output canvas (last-writer-wins). Scatter is parity-hostile
on the GPU for the same reason the granular mosaic sim was — concurrent writes
need atomics and ordering guarantees. The field-particles splat precedent solved
this with a **scatter→gather inversion**: each output pixel becomes a thread that
*gathers* its colour by asking "which scanlines cover me?" instead of "which
pixels do I write to?". This milestone applies the same inversion to Rutt-Etra.

## Gather mechanic (the parity proof)

For output pixel `(px, py)`:

1. Scan scanlines `y0 = 0, pitch, 2·pitch, …` in **reverse order** (bottom →
   top, i.e. from the largest `y0 < height` downward).
2. For each `y0`, compute the displaced rows at `px` and `px+1` (using the
   **same** Rec.709 luma and `round()` as the CPU):
   - `luma_a = clamp(0.2126·R + 0.7152·G + 0.0722·B, 0, 1)` at `source_b(px, y0)`
   - `y_a = y0 − round(displacement_depth · luma_a)`
   - `y_b = y0 − round(displacement_depth · luma_b)` where `luma_b` uses
     `source_b(min(px+1, width−1), y0)` (clamped for the last column)
   - `span_lo = min(y_a, y_b)`, `span_hi = max(y_a, y_b) + line_thickness − 1`
3. If `span_lo ≤ py ≤ span_hi`: **stop** and return
   - `source_b(px, y0)` colour (or `[1,1,1,1]` when `mono`).
4. If no scanline covers `(px, py)`: return `[0,0,0,1]` (black).

Stopping at the first match from the bottom is equivalent to last-writer-wins
from the top (the CPU draw order), so the result is **byte-identical** to the
CPU path without any atomics.

Metal's `round()` is round-half-away-from-zero (MSL spec §2.12), matching Rust's
`f32::round()` for all finite inputs — the luma product is always non-negative
(luma ∈ [0,1], depth can be negative but its product with luma can be any finite
float), and round-half-away-from-zero is symmetric, so no special-casing needed.

## New algorithm id

`rutt_etra_scanline_metal_v1` — written to the manifest only when
`--backend metal` is used. The CPU path keeps `rutt_etra_scanline_cpu_v1`.

## Shader: `rutt_etra_scanline.metal`

```c
struct RuttEtraParams {
  uint width;
  uint height;
  uint line_pitch;
  float displacement_depth;
  uint line_thickness;
  uint mono; // 0 or 1
};

kernel void rutt_etra_scanline(
  texture2d<float, access::read>  source_b [[texture(0)]],
  texture2d<float, access::write> output   [[texture(1)]],
  constant RuttEtraParams&        params   [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
)
```

Binding layout is the source of truth — the Rust-side `validate_rutt_etra_shader_source()` must check the kernel name string and all three binding declarations exactly.

## Changes by crate

### `crates/morphogen-metal/shaders/rutt_etra_scanline.metal` (new)

The gather kernel per the mechanic above. Texture reads use `::read` access (no
sampler — the CPU uses integer pixel positions with no interpolation).

### `crates/morphogen-metal/src/flow_displace_dispatch.rs`

Add beside the other shader constants + validate functions:

```rust
pub const RUTT_ETRA_KERNEL_NAME: &str = "rutt_etra_scanline";
pub const RUTT_ETRA_SHADER_SOURCE: &str = include_str!("../shaders/rutt_etra_scanline.metal");
```

Add `MetalDispatchError` variants:
```rust
#[error("rutt_etra_scanline.metal does not contain the expected kernel entry point")]
MissingRuttEtraKernelEntryPoint,
#[error("rutt_etra_scanline.metal does not contain the expected texture and buffer bindings")]
MissingRuttEtraBindingLayout,
```

Add `validate_rutt_etra_shader_source()` checking the kernel name and all three
binding declarations (mirror `validate_palette_quantize_shader_source` shape).

Add a `RuttEtraDispatchPlan` (analogous to the `pixel_sort`-style plans that have
only dims + params, not flow):
- Fields: `width`, `height`, `line_pitch`, `displacement_depth`, `line_thickness`,
  `mono`, `threads_per_threadgroup`, `threadgroups_per_grid`.
- `new(settings: &RuttEtraSettings, width: u32, height: u32)` — validate and
  compute 16×16 threadgroups.
- `kernel_name() -> &'static str`.

### `crates/morphogen-metal/src/runtime.rs`

Add `render_rutt_etra_frame_metal(source_b: &ImageBufferF32, settings: &RuttEtraSettings) -> Result<ImageBufferF32, CliError>` following the `render_palette_quantize_frame_metal` shape:
- Upload `source_b` as a read texture.
- Create the output texture (write).
- Build `RuttEtraParams` from settings.
- Compile `RUTT_ETRA_SHADER_SOURCE`, look up `RUTT_ETRA_KERNEL_NAME`, build pipeline.
- Dispatch with the plan's threadgroup counts.
- Readback → `ImageBufferF32`.

### `crates/morphogen-render/src/rutt_etra.rs`

No changes to the CPU renderer or settings struct. The algorithm id constant stays
`rutt_etra_scanline_cpu_v1`; add a separate one for Metal.

Add to `lib.rs` export: `pub const RUTT_ETRA_METAL_ALGORITHM: &str = "rutt_etra_scanline_metal_v1";`
(or define it in `rutt_etra.rs` and re-export — follow the existing pattern).

### `crates/morphogen-cli/src/render.rs`

`RuttEtraSequenceRequest` gains `pub backend: RenderBackend` (CPU default).

Embed the parity gate **inside** `render_rutt_etra_frame_metal` (as every other
`render_*_frame_metal` in `render.rs` does), using `max_channel_difference` +
`METAL_CPU_PARITY_EPSILON` — **not** exact `!=` equality (a 1-ULP hardware
rounding difference or a NaN pixel would spuriously disable the backend):

```rust
let gpu = morphogen_metal::rutt_etra_scanline_metal(source_b, settings)?;
let cpu = render_rutt_etra_frame(source_b, settings)?;
let difference = gpu.max_channel_difference(&cpu).ok_or_else(|| {
    CliError::Message(
        "Metal and CPU rutt-etra outputs have mismatched dimensions; cannot verify parity"
            .to_string(),
    )
})?;
if difference > METAL_CPU_PARITY_EPSILON {
    return Err(CliError::Message(format!(
        "Metal rutt-etra render diverged from CPU reference by {difference} \
         (tolerance {METAL_CPU_PARITY_EPSILON})"
    )));
}
Ok(gpu)
```

`render_rutt_etra_sequence` then just dispatches by backend with a plain call:

```rust
let rendered = match request.backend {
    RenderBackend::Cpu => render_rutt_etra_frame(&source_b, &frame_settings)?,
    RenderBackend::Metal => render_rutt_etra_frame_metal(&source_b, &frame_settings)?,
};
```

The manifest `"algorithm"` field emits `RUTT_ETRA_ALGORITHM` for CPU and
`RUTT_ETRA_METAL_ALGORITHM` for Metal (so the manifest faithfully records which
path ran).

### `crates/morphogen-cli/src/args.rs`

`RenderRuttEtraSequence` gains `--backend cpu|metal` (same `RenderBackend` enum,
`clap::ValueEnum`, already defined — grep `--backend` in the channel-shift command
for the pattern). Default: `cpu`.

### `crates/morphogen-cli/src/queue.rs`

`FrameSequenceRuttEtraTask` gains `backend: RenderBackend` with
`#[serde(default)]` so pre-Metal queue JSON keeps loading. `queue-add` passes
`--backend` through; `queue-run-rutt-etra-sequence` dispatches by backend.

### `apps/macos/Sources/MorphogenMacApp/Services/RustBridgePlaceholder.swift`

`RuttEtraSequenceRenderQueueCommandRequest` gains `backend: String` (default
`"cpu"`). The argument builder emits `--backend` before the positional args (same
position as in every other bridged command). Add bridge tests pinning the
`--backend cpu` and `--backend metal` token sequences.

### `apps/macos/Sources/MorphogenMacApp/Views/RenderPanelView.swift`

The Rutt-Etra panel section gains a `CPU / Metal` backend picker (same two-option
picker as the palette-quantize and granular-mosaic panels — grep
`backendPickerRow` for the exact SwiftUI helper).

### `apps/macos/Sources/MorphogenMacApp/Models/AppState.swift`

`ruttEtraBackend` stored property (default `"cpu"`), wired into
`modulationRoutes` call (or wherever the backend string is read) and persisted via
`UserDefaults` (same pattern as `flowFeedbackBackend`).

## Acceptance criteria

1. **Shader preflight test**: `validate_rutt_etra_shader_source()` passes in
   `tests::rutt_etra_shader_has_expected_bindings()` — added beside the palette-
   quantize test.

2. **Runtime parity test** in `crates/morphogen-metal/src/runtime.rs` (or a
   nearby test module): render a small synthetic gradient frame (e.g. 32×16,
   pitch=4, depth=6, thickness=1, mono=false) with both CPU and Metal; assert
   `cpu_result == metal_result` (identical `ImageBufferF32`). This is the
   definitive parity gate — a single compile-and-run proves the gather inversion
   is correct.

3. **CLI smoke**: `render-rutt-etra-sequence --backend metal` on the synthetic
   gradient fixture; byte-compare with `--backend cpu` output; assert zero
   divergent frames. (The parity gate inside the render path already enforces
   this, but an explicit smoke test in `tests/smoke.rs` pin it permanently.)

4. **Queue add→run**: `queue-add-rutt-etra-sequence --backend metal` →
   `queue-run-rutt-etra-sequence`; frames byte-identical to the direct
   `--backend metal` CLI render. Add to `tests/smoke.rs` beside the existing
   rutt-etra queue smoke.

5. **Bridge tests**: the two-arg `--backend cpu/metal` token sequences are pinned
   in `RustBridgePlaceholderTests.swift`.

6. **Visual proof**: Read 3 Metal output frames from a real-footage sequence
   (the 640×360 cello proxy already used in the look confirmation); confirm the
   look is unchanged vs the CPU renders already viewed.

## Build plan

Follow the `palette_quantize` Metal port shape exactly — it is the closest
precedent (stateless, single-source, same `source_b` texture input, same
`source_b` colour readout).

1. Write `rutt_etra_scanline.metal`.
2. Add constants + `validate_*` + `RuttEtraDispatchPlan` to `flow_displace_dispatch.rs`.
3. Add `render_rutt_etra_frame_metal` to `runtime.rs`.
4. Wire backend dispatch + parity gate into `render.rs`.
5. Add `--backend` to `args.rs` + `queue.rs`.
6. SwiftUI backend picker + bridge test.

Working agreements (standing, non-negotiable):

- Baseline before touching anything: `cargo test --workspace` (**532 passing,
  0 failing**) and `swift test` (**113 passing, 0 failing**); report deltas, not
  adjectives.
- `/checkpoint` after this single verified slice (local commit, source only,
  never push).
- Never commit the untracked `scripts/solitaire-cascade-prototype.py` or
  `shader-port-ideas/`.
- Record non-obvious findings (parity traps, `round()` edge cases, unexpected
  shader behaviour) in `/memory/`, not in prose docs.

## Deferred (still out of scope)

- Two-source A→B (A's luma displaces B's scanlines) — next natural slice.
- Depth-descriptor displacement — blocked on depth-model carve-out.
- HQ anti-aliased lines — roadmap's long-term tier.
