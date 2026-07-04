# Rutt-Etra Metal Port — Code-Review Fixup

Four confirmed findings from a medium-effort review of commit `9c622de`.
All four require code changes; none require design decisions. Implement them in
order (1→4) — they are independent but easiest to verify that way.

Project root: `/Users/marcscully/Projects/morphogen-av`

---

## Baseline (before touching anything)

```sh
cargo test --workspace   # 534 passing, 0 failing
swift test               # 115 passing, 0 failing
cargo clippy --workspace --all-targets -- -D warnings  # clean
```

Capture the exact numbers. Report the delta at the end.

---

## Fix 1 — Wrong parity gate semantics (HIGHEST PRIORITY)

**File:** `crates/morphogen-cli/src/render.rs`  
**Lines:** 6235–6243 (the Metal branch inside `render_rutt_etra_sequence`)

**Problem:** the gate uses `if metal != cpu` — exact `PartialEq` on `Vec<f32>`.
Every other Metal gate in this file (12+ sites, e.g. lines 456–466, 5991–6003,
6130–6148) uses `gpu.max_channel_difference(&cpu)` compared against
`METAL_CPU_PARITY_EPSILON`. A 1-ULP hardware rounding difference causes a
permanent hard-failure with no diagnostic. NaN pixels cause a spurious failure
via `NaN != NaN`.

**The established pattern (copy from lines 456–466):**
```rust
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
```

**Required change:** replace the current Metal branch body:
```rust
// BEFORE (lines 6235–6242):
RenderBackend::Metal => {
    let metal = render_rutt_etra_frame_metal(&source_b, &frame_settings)?;
    let cpu = render_rutt_etra_frame(&source_b, &frame_settings)?;
    if metal != cpu {
        return Err(CliError::Message(format!(
            "Metal/CPU parity failure on frame {index}"
        )));
    }
    metal
}

// AFTER:
RenderBackend::Metal => {
    let metal = render_rutt_etra_frame_metal(&source_b, &frame_settings)?;
    let cpu = render_rutt_etra_frame(&source_b, &frame_settings)?;
    let difference = metal.max_channel_difference(&cpu).ok_or_else(|| {
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
    metal
}
```

**Verification:** `cargo test -p morphogen-cli` passes; `cargo clippy` clean.

---

## Fix 2 — `line_pitch = 0` not validated before Metal dispatch

**File:** `crates/morphogen-metal/src/flow_displace_dispatch.rs`  
**Lines:** 589–610 (`RuttEtraDispatchPlan::new`)

**Problem:** `new()` guards `width/height > 0` and `displacement_depth.is_finite()`
but not `settings.line_pitch >= 1`. The shader at `shaders/rutt_etra_scanline.metal`
line 41 computes `(params.height + params.line_pitch - 1) / params.line_pitch` —
integer division by zero if `line_pitch == 0`. The CPU renderer's own guard in
`rutt_etra.rs:validate()` is never reached because `rutt_etra_scanline_metal` in
`runtime.rs` calls `RuttEtraDispatchPlan::new` first.

**Required change:** add a `line_pitch` guard immediately after the
`displacement_depth` check (line 598):
```rust
// After line 598 (the is_finite check), add:
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

Note: `MetalDispatchError::InvalidRuttEtraSettings` already exists (added in
`9c622de`). The `line_thickness` guard is added here too for completeness —
`int(line_thickness) - 1` in the shader would underflow if `line_thickness == 0`
(currently not reachable through normal paths but the dispatch plan is `pub`).

**Also add a test** in `flow_displace_dispatch.rs`'s existing `#[cfg(test)] mod tests`
block (beside `rutt_etra_shader_has_expected_bindings`):
```rust
#[test]
fn rutt_etra_dispatch_plan_rejects_zero_line_pitch() {
    use morphogen_render::RuttEtraSettings;
    let s = RuttEtraSettings { line_pitch: 0, ..RuttEtraSettings::default() };
    assert!(RuttEtraDispatchPlan::new(&s, 16, 16).is_err());
}

#[test]
fn rutt_etra_dispatch_plan_rejects_zero_line_thickness() {
    use morphogen_render::RuttEtraSettings;
    let s = RuttEtraSettings { line_thickness: 0, ..RuttEtraSettings::default() };
    assert!(RuttEtraDispatchPlan::new(&s, 16, 16).is_err());
}
```

**Verification:** `cargo test -p morphogen-metal` passes; both new tests green.

---

## Fix 3 — `render_rutt_etra_frame_metal` is a raw passthrough (no embedded parity gate)

**File:** `crates/morphogen-cli/src/render.rs`  
**Lines:** 6161–6179

**Problem:** every other `render_*_frame_metal` in this file (e.g.
`render_retro_static_frame_metal` at lines 5985–6004,
`render_palette_quantize_frame_metal` at lines 6130–6148) embeds the CPU
comparison and epsilon gate so any call site is automatically safe.
`render_rutt_etra_frame_metal` just calls `rutt_etra_scanline_metal` and returns
raw GPU output — the parity gate lives only in `render_rutt_etra_sequence`.
Future call sites bypass the invariant silently.

**Required change:** move the parity logic into `render_rutt_etra_frame_metal`
itself (macOS variant only), and simplify the sequence's Metal branch to a plain
call:

```rust
// BEFORE — render_rutt_etra_frame_metal (macOS, lines 6162–6169):
#[cfg(target_os = "macos")]
pub(crate) fn render_rutt_etra_frame_metal(
    source_b: &ImageBufferF32,
    settings: &RuttEtraSettings,
) -> Result<ImageBufferF32, CliError> {
    Ok(morphogen_metal::rutt_etra_scanline_metal(source_b, settings)?)
}

// AFTER:
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

Once the parity gate is embedded in the function, simplify the Metal branch in
`render_rutt_etra_sequence` (which was already fixed by Fix 1) to:
```rust
RenderBackend::Metal => render_rutt_etra_frame_metal(&source_b, &frame_settings)?,
```

This is now identical in shape to the `render_retro_static_frame_metal` call site
at line 5961: `render_retro_static_frame_metal(&source, &frame_settings)?`.

**Important:** Fix 1 and Fix 3 touch overlapping lines. Apply Fix 1 first as
written, then apply Fix 3's two changes (the function body, then simplify the
call site). The end state after both fixes is:
- `render_rutt_etra_frame_metal` embeds the gate (using `max_channel_difference`)
- The Metal branch in `render_rutt_etra_sequence` is a single call with no
  inlined parity logic

**Verification:** `cargo test -p morphogen-cli` passes; `cargo clippy` clean.

---

## Fix 4 — Missing CLI-level smoke tests for `--backend metal` (AC 3 & 4)

**File:** `crates/morphogen-cli/tests/smoke.rs`

**Problem:** the milestone contract (`docs/RUTT_ETRA_METAL_MILESTONE.md`
acceptance criteria 3 and 4) requires two smoke tests in `tests/smoke.rs`:
- AC 3: `render-rutt-etra-sequence --backend metal` byte-compares with CPU
- AC 4: `queue-add-rutt-etra-sequence --backend metal` → `queue-run` is
  byte-identical to the direct Metal render

The runtime parity test in `runtime.rs` is unit-level on a 32×16 fixture; it
does not cover the CLI dispatcher, manifest algorithm-id, or queue serialization.

**Pattern to follow:** the existing CPU smoke test at line 67
(`render_rutt_etra_sequence_writes_frames_and_manifest_with_knobs`) and the
queue smoke test nearby. Copy their fixture setup; add `--backend metal`.

**Add two tests:**

```rust
#[test]
#[cfg(target_os = "macos")]
fn render_rutt_etra_sequence_metal_is_byte_identical_to_cpu() {
    // AC 3: --backend metal produces the same frames as --backend cpu and
    // records the Metal algorithm id in the manifest.
    let fixture_dir = fixtures::gradient_frame_dir();  // use the same helper the CPU test uses
    let cpu_out = tempdir().unwrap();
    let metal_out = tempdir().unwrap();

    run_cli(&[
        "render-rutt-etra-sequence",
        fixture_dir.path().to_str().unwrap(),
        cpu_out.path().to_str().unwrap(),
        "--frames", "3",
        "--line-pitch", "4",
        "--displacement-depth", "20",
        "--line-thickness", "1",
        "--backend", "cpu",
    ])
    .assert()
    .success();

    run_cli(&[
        "render-rutt-etra-sequence",
        fixture_dir.path().to_str().unwrap(),
        metal_out.path().to_str().unwrap(),
        "--frames", "3",
        "--line-pitch", "4",
        "--displacement-depth", "20",
        "--line-thickness", "1",
        "--backend", "metal",
    ])
    .assert()
    .success();

    // Frames must be byte-identical.
    for i in 0..3 {
        let cpu_frame = std::fs::read(cpu_out.path().join(format!("frame_{i:06}.png"))).unwrap();
        let metal_frame = std::fs::read(metal_out.path().join(format!("frame_{i:06}.png"))).unwrap();
        assert_eq!(cpu_frame, metal_frame, "frame {i} differs between cpu and metal");
    }

    // Metal manifest records the Metal algorithm id.
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(metal_out.path().join("manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["algorithm"], "rutt_etra_scanline_metal_v1");
}

#[test]
#[cfg(target_os = "macos")]
fn queue_rutt_etra_metal_add_run_byte_identical_to_direct() {
    // AC 4: queued --backend metal is byte-identical to direct --backend metal.
    // Follow the pattern of the existing CPU queue smoke test nearby.
    let fixture_dir = fixtures::gradient_frame_dir();
    let queue_dir = tempdir().unwrap();
    let direct_out = tempdir().unwrap();
    let queue_out = tempdir().unwrap();

    // Direct Metal render.
    run_cli(&[
        "render-rutt-etra-sequence",
        fixture_dir.path().to_str().unwrap(),
        direct_out.path().to_str().unwrap(),
        "--frames", "3",
        "--line-pitch", "4",
        "--displacement-depth", "20",
        "--line-thickness", "1",
        "--backend", "metal",
    ])
    .assert()
    .success();

    // Queue add.
    run_cli(&[
        "queue-add-rutt-etra-sequence",
        queue_dir.path().to_str().unwrap(),
        fixture_dir.path().to_str().unwrap(),
        queue_out.path().to_str().unwrap(),
        "--frames", "3",
        "--frame-rate", "12",
        "--line-pitch", "4",
        "--displacement-depth=20",
        "--line-thickness", "1",
        "--backend", "metal",
    ])
    .assert()
    .success();

    // Queue run.
    run_cli(&["queue-run", queue_dir.path().to_str().unwrap()])
        .assert()
        .success();

    // Frames byte-identical to the direct Metal render.
    let bundle = find_only_bundle(queue_out.path());
    for i in 0..3 {
        let direct = std::fs::read(direct_out.path().join(format!("frame_{i:06}.png"))).unwrap();
        let queued = std::fs::read(bundle.join("frames").join(format!("frame_{i:06}.png"))).unwrap();
        assert_eq!(direct, queued, "frame {i} differs between direct and queued metal");
    }

    // Manifest records Metal algorithm id and backend field.
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(bundle.join("manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["algorithm"], "rutt_etra_scanline_metal_v1");
    assert_eq!(manifest["backend"], "metal");
}
```

**Pattern notes:**
- Look at the existing `render_rutt_etra_sequence_writes_frames_and_manifest_with_knobs`
  test (line 67) for the exact `run_cli`, `tempdir`, fixture, and `find_only_bundle`
  helpers used in this file. Use the same helpers — do not introduce new ones.
- `#[cfg(target_os = "macos")]` gates both tests so they are skipped on
  non-Mac CI that lacks Metal.
- The `gradient_frame_dir()` fixture name is illustrative; use whatever the
  actual helper in the file is called (check the imports and fixture module at
  the top of `smoke.rs`).

**Verification:** `cargo test -p morphogen-cli -- rutt_etra` runs all rutt-etra
smoke tests green; on macOS the two new Metal tests execute and pass.

---

## Commit instructions

One commit covering all four fixes:
```
fix: Rutt-Etra Metal port — epsilon gate, line_pitch guard, embedded parity, smoke tests
```

Working agreements:
- `cargo test --workspace` must finish with the same or higher pass count (≥534)
- `swift test` must stay ≥115
- `cargo clippy --workspace --all-targets -- -D warnings`: clean
- `cargo fmt --check`: clean
- Never commit `scripts/solitaire-cascade-prototype.py` or `shader-port-ideas/`
- No push — local commit only; report the commit hash

## Report format

```
Baseline: cargo 534, swift 115
After:    cargo NNN (+NN), swift NNN (+NN)
Clippy: clean
Findings fixed: 1, 2, 3, 4
Commit: <hash>
Deviations from this doc (if any): …
```
