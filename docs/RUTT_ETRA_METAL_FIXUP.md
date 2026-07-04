# Rutt-Etra Metal Port — Code-Review Fixup

Four confirmed findings from a medium-effort review of commit `9c622de`, resolved
as **three implementation fixes** (findings 1 and 3 are one change: the parity
gate is both wrong and in the wrong place — moving the correct gate into the
frame function fixes both at once). No design decisions are left open.

Root cause worth knowing before you start: the milestone contract itself
(`docs/RUTT_ETRA_METAL_MILESTONE.md` lines 133–140) sketched the `!=` gate and
claimed it matched "granular-mosaic-pool and flow-feedback Metal paths" — it
doesn't; every other gate in the codebase uses `max_channel_difference` +
`METAL_CPU_PARITY_EPSILON`. The build faithfully implemented a wrong contract,
so Fix 1 amends the contract text as well as the code.

Project root: `/Users/marcscully/Projects/morphogen-av`

---

## Baseline (before touching anything)

```sh
cargo test --workspace   # expect 534 passing, 0 failing
swift build && swift test  # expect 115 passing, 0 failing (unchanged by this work)
cargo clippy --workspace --all-targets -- -D warnings  # clean
```

Capture the exact numbers; report the delta at the end.

---

## Fix 1 — Parity gate: wrong semantics AND wrong location
*(resolves review findings 1 and 3)*

**Files:** `crates/morphogen-cli/src/render.rs` (both changes),
`docs/RUTT_ETRA_METAL_MILESTONE.md` (contract amendment)

**Problem, two halves:**

- *Semantics* (`render.rs:6237`): the gate is `if metal != cpu` — exact
  `PartialEq` over `Vec<f32>`. Every other Metal gate in this file (12+ sites:
  456, 1521, 2199, 2801, 5991, …) computes `gpu.max_channel_difference(&cpu)`
  and compares against `METAL_CPU_PARITY_EPSILON`. Exact equality means a 1-ULP
  hardware rounding difference permanently disables the Metal backend with no
  divergence magnitude in the error, and a NaN pixel fails spuriously
  (`NaN != NaN`).
- *Location* (`render.rs:6162`): the gate lives inline in
  `render_rutt_etra_sequence`, while `render_rutt_etra_frame_metal` is a raw
  GPU passthrough. Every other `render_*_frame_metal` in this file (e.g.
  `render_retro_static_frame_metal` at 5985–6004,
  `render_palette_quantize_frame_metal` at 6130–6148) embeds the gate inside
  the function so any future call site is automatically gated.

**Change A** — replace the body of the macOS `render_rutt_etra_frame_metal`
(currently lines 6161–6169) with the embedded gate, copying the retro-static
shape exactly:

```rust
#[cfg(target_os = "macos")]
pub(crate) fn render_rutt_etra_frame_metal(
    source_b: &ImageBufferF32,
    settings: &RuttEtraSettings,
) -> Result<ImageBufferF32, CliError> {
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
}
```

The non-macOS variant (lines 6171–6179) is untouched.

**Change B** — collapse the Metal branch in `render_rutt_etra_sequence`
(currently lines 6234–6243) to a plain call, matching the retro-static call
site at line 5961:

```rust
let rendered = match request.backend {
    RenderBackend::Cpu => render_rutt_etra_frame(&source_b, &frame_settings)?,
    RenderBackend::Metal => render_rutt_etra_frame_metal(&source_b, &frame_settings)?,
};
```

The old error message carried the frame index; the embedded gate loses it. That
matches every other effect (none report frame index) — accept it.

**Change C** — amend `docs/RUTT_ETRA_METAL_MILESTONE.md`: in the dispatch
sketch (lines ~133–140), replace the `if metal != cpu` snippet and its
"(same as granular-mosaic-pool and flow-feedback Metal paths)" comment with the
epsilon-gate pattern above, embedded in the frame function. Leave acceptance
criterion 2 (`cpu_result == metal_result` in the runtime test) as-is — an
exact-equality *test* on a fixed 32×16 fixture is fine and currently passes;
only the production *gate* needed tolerance.

**Verification:** `cargo test -p morphogen-cli` and
`cargo test -p morphogen-metal` pass; clippy clean.

---

## Fix 2 — `line_pitch = 0` reaches the GPU as integer division by zero
*(resolves review finding 2)*

**File:** `crates/morphogen-metal/src/flow_displace_dispatch.rs`

**Problem:** `RuttEtraDispatchPlan::new` (lines 589–599) guards
`width/height > 0` and `displacement_depth.is_finite()` but not
`line_pitch >= 1`. The shader (`shaders/rutt_etra_scanline.metal:41`) computes
`(params.height + params.line_pitch - 1) / params.line_pitch` — uint division
by zero when `line_pitch == 0`. The CPU renderer's own `validate()` guard
(`crates/morphogen-render/src/rutt_etra.rs:50`) is never consulted on this
path: `rutt_etra_scanline_metal` in `runtime.rs` builds the dispatch plan
straight from the settings struct, so crafted queue JSON with `line_pitch: 0`
reaches the GPU.

**Note a correction to the previous version of this doc:** there is **no**
existing `MetalDispatchError::InvalidRuttEtraSettings` variant — `9c622de`
added only `MissingRuttEtraKernelEntryPoint` and `MissingRuttEtraBindingLayout`.
You must add the variant, following the existing pixel-sort precedent at
lines 171–172:

```rust
#[error("invalid rutt-etra settings: {0}")]
InvalidRuttEtraSettings(String),
```

Add it to the `MetalDispatchError` enum beside the two `MissingRuttEtra*`
variants (lines 181–184).

**Then add the guards** in `RuttEtraDispatchPlan::new`, after the
`displacement_depth.is_finite()` check at line 597–599:

```rust
if settings.line_pitch == 0 {
    return Err(MetalDispatchError::InvalidRuttEtraSettings(
        "line_pitch must be >= 1".to_string(),
    ));
}
if settings.line_thickness == 0 {
    return Err(MetalDispatchError::InvalidRuttEtraSettings(
        "line_thickness must be >= 1".to_string(),
    ));
}
```

(`line_thickness == 0` is not UB in the shader — the span just becomes empty —
but it silently diverges from the CPU path's `validate()` contract, and the
plan is `pub`; guard both for the same reason.)

**Add two unit tests** in the existing `#[cfg(test)] mod tests` of
`flow_displace_dispatch.rs`, beside `rutt_etra_shader_has_expected_bindings`.
Check how that test module constructs `RuttEtraSettings` (whether a `Default`
impl exists or fields are spelled out) and match it:

```rust
#[test]
fn rutt_etra_dispatch_plan_rejects_zero_line_pitch() {
    // line_pitch: 0, other fields valid (pitch 4 → 0, depth 6.0, thickness 1, mono false)
    assert!(RuttEtraDispatchPlan::new(&settings_with_zero_pitch, 16, 16).is_err());
}

#[test]
fn rutt_etra_dispatch_plan_rejects_zero_line_thickness() {
    assert!(RuttEtraDispatchPlan::new(&settings_with_zero_thickness, 16, 16).is_err());
}
```

**Verification:** `cargo test -p morphogen-metal` — both new tests green.

---

## Fix 3 — Missing CLI smoke tests for `--backend metal` (contract AC 3 & 4)
*(resolves review finding 4)*

**File:** `crates/morphogen-cli/tests/smoke.rs`

**Problem:** the milestone acceptance criteria 3 and 4 require two smoke tests
in `tests/smoke.rs`; neither exists. The runtime parity test in `runtime.rs`
is unit-level on a 32×16 fixture — it never exercises the CLI dispatcher, the
manifest algorithm-id selection, or the queue backend round-trip.

**Conventions in this file (use these, don't invent helpers):**

- CLI invocation: `Command::cargo_bin("morphogen").expect("morphogen binary").args([...])`
  (`assert_cmd`); **not** any `run_cli` helper.
- Fixtures: built inline with `ImageBuffer::from_fn` — copy the gradient loop
  from `render_rutt_etra_sequence_writes_frames_and_manifest_with_knobs`
  (lines 73–81).
- Manifest reads: the existing `read_json` helper (see line 112).
- Queue run command: **`queue-run-rutt-etra-sequence`** (task-specific), not a
  generic `queue-run`.
- Queued output lands at `<output_root>/job-0001/frames/` (see the byte-compare
  loop at lines 7553–7559 in `queue_rutt_etra_modulated_matches_direct_and_records_routes`
  — that test is the AC 4 pattern to mirror).
- The render manifest has **no** `backend` field (see `render.rs:6255–6262`) —
  do not assert one. The Metal discriminator in the manifest is
  `"algorithm": "rutt_etra_scanline_metal_v1"`. The **queue task JSON** does
  have a `backend` field (`#[serde(default)]`, snake_case → `"metal"`), and the
  review noted it's unasserted — pin it in the AC 4 test.

**Test 1 — AC 3** (`#[cfg(target_os = "macos")]`):
`render_rutt_etra_sequence_metal_matches_cpu_byte_identical`

1. Build a 16×16 two-frame gradient fixture (copy lines 73–81).
2. Render once with `--backend cpu` and once with `--backend metal` into
   separate dirs — same knobs (`--frames 2 --line-pitch 4
   --displacement-depth 12.5 --line-thickness 2`).
3. `fs::read` and `assert_eq!` each frame pair byte-for-byte. (The kernel is
   stateless and currently byte-identical on Apple silicon; if this ever fails
   while the epsilon gate passes, that is real hardware drift — loosen the test
   to the epsilon comparison then, not now.)
4. `read_json` the Metal manifest: assert
   `manifest["algorithm"] == "rutt_etra_scanline_metal_v1"` and that the knob
   fields match (mirror the assertions at lines 113–118).

**Test 2 — AC 4** (`#[cfg(target_os = "macos")]`):
`queue_rutt_etra_metal_matches_direct_render`

1. Same fixture. Direct render with `--backend metal`.
2. `queue-add-rutt-etra-sequence <queue.json> <source> <output_root>` with the
   same knobs plus `--frame-rate 4 --backend metal`.
3. Parse the persisted queue JSON and assert
   `queue_json["jobs"][0]["task"]["backend"] == "metal"` (this is the
   previously-unasserted field).
4. `queue-run-rutt-etra-sequence <queue.json>`.
5. Byte-compare `frame_000000.png`, `frame_000001.png`, **and** `manifest.json`
   between `<output_root>/job-0001/frames/` and the direct dir — mirror the
   loop at lines 7553–7559.

**Verification:** `cargo test -p morphogen-cli --test smoke -- rutt_etra`
runs all rutt-etra smoke tests green on macOS.

---

## Order, commit, report

Apply Fix 1 → Fix 2 → Fix 3 (the smoke tests exercise the gate Fix 1 rewrites,
so they must land after it).

One commit, all three fixes:

```
fix: Rutt-Etra Metal — epsilon parity gate in frame fn, dispatch guards, AC 3/4 smoke tests
```

Working agreements:

- `cargo test --workspace` ≥ 534 passing, 0 failing (expect +4: 2 dispatch
  guards + 2 smoke). `swift test` stays 115 — no Swift changes here.
- `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --check`: clean.
- Never stage `scripts/solitaire-cascade-prototype.py` or `shader-port-ideas/`
  (intentionally untracked).
- **No push** — local commit only; report the hash.

Report format:

```
Baseline: cargo 534/0, swift 115/0
After:    cargo NNN/0 (+N), swift 115/0
Clippy + fmt: clean
Fixes landed: 1 (gate semantics+location+contract), 2 (dispatch guards), 3 (AC 3/4 smoke)
Commit: <hash>
Deviations from this doc: <none, or list with reasons>
```
