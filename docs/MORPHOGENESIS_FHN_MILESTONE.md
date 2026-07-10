# Morphogenesis Track A1 — FitzHugh–Nagumo excitable media (`--model`)

Status: **IN PROGRESS.** Builds on
[MORPHOGENESIS_RELIEF_SHADING_MILESTONE.md](MORPHOGENESIS_RELIEF_SHADING_MILESTONE.md)
(`300fb73`) and the plan in
[MORPHOGENESIS_EXPANSION_HANDOFF.md](MORPHOGENESIS_EXPANSION_HANDOFF.md)
(Track A, A1). Baseline at slice start: cargo 737/0, swift 155/0, clippy
clean, `cargo fmt --check` dirty on 8 pre-existing files (zero new diffs
allowed). Ground rules as ever (real-footage readouts, presets not raw
numbers, session-limit finish-inline, 64MiB CLI thread, CPU-only).

## Architecture decision (pinned)

Extend the EXISTING `render-morphogenesis-sequence` /
`queue-add-morphogenesis-sequence` commands with `--model
<gray-scott|fitzhugh-nagumo>` (default `gray-scott` — continuity anchor:
model absent/gray-scott is byte-identical to every pre-A1 render). No new
command family, no `FieldModel` trait (rule of three — Gray-Scott is the
only precedent so far).

**State container:** FHN reuses [`MorphogenesisField`] verbatim as its raw
`(u, v)` grid — no new struct. Gray-Scott's `U,V ∈ [0,1]`; FHN's `u,v` are
signed. Nothing in `MorphogenesisField` (storage, `v_variance`, the RGBA32F
pack/unpack) assumes a `[0,1]` range, so the checkpoint codec is reused
completely unchanged.

**Settings:** a new, separate `FhnSettings` struct (own `validate`, own
presets) — NOT folded into `MorphogenesisSettings` (different parameters,
different physical meaning; folding them would make every Gray-Scott-only
field awkwardly optional). The sequence checkpoint contract gets two
additive fields: `model: MorphogenesisModel` (`#[serde(default)]` →
`GrayScott`, so every pre-A1 checkpoint deserializes unchanged and stays
resumable) and `fhn_settings: FhnSettings` (`#[serde(default)]`, always
present but only authoritative when `model == FitzhughNagumo` — the same
"always-present, model-selects-meaning" shape `render_job.rs` already uses
for its flattened queue-task fields, chosen over a tagged-enum restructure
of `settings` because that would break the wire shape of every existing
checkpoint/queue JSON rather than just adding keys to it).

**Composite/field-view reuse (the "everything downstream is model-agnostic"
claim, made concrete):** `composite_morphogenesis_frame`,
`render_v_field_grayscale_upsampled_with_shading`, and
`morphogenesis_v_gradient` only ever read a field's `.v`/`.width`/`.height`
— never `.u`. A new adapter, `fhn_display_field(field) -> MorphogenesisField`,
returns a throwaway field with `.v` set to a display-normalized `u`
(`((u.clamp(-2,2) + 2) / 4).clamp(0,1)`, so resting `u≈-1.2` reads near-black
and a firing pulse near-white) and `.u` unused (set to `1.0`, dummy). Piping
that adapter into the SAME two functions gets both output views for FHN with
**zero changes to either function** — proving the reuse claim rather than
asserting it.

## The model

```
du/dt = Du·∇²u + u − u³/3 − v + I(x,y)     // fast activator
dv/dt = ε·(u + a − b·v)                      // slow recovery, no diffusion
```

- 5-point Laplacian on `u` only (`v` has no spatial coupling term), same
  clamped-edge stencil convention as Gray-Scott's `morphogenesis_substep`.
- Both `u` and `v` are clamped to a declared **safety box `[-3, 3]`** after
  every substep (NOT `[0,1]` — that would destroy the excitable dynamics;
  the box only guards against float blow-up, it is not part of the physics).
- **Frame-zero seed:** the WHOLE field starts at the model's analytic resting
  state `(u_rest, v_rest)` — solved once per render via Newton–Raphson on the
  fixed-point cubic `b/3·u³ − (b−1)·u + a = 0` (monotonic for the presets
  below, so Newton converges from `u₀ = 0` in a handful of iterations,
  deterministically) — rather than an arbitrary `(0,0)`, which for these
  parameters is already past the firing threshold and would make the WHOLE
  frame fire simultaneously at frame zero instead of staying quiescent.
  Carrier-luma-thresholded cells (plus the standard speckle) get `u = u_rest
  + stimulus` (`v` untouched) — "fires u, not v," per the handoff.
- **Inject (live coupling):** reuses `InjectSource`/`injection_weight_luma`/
  `injection_weight_motion` verbatim. `I(x,y) = settings.inject * w(x,y)` is
  the forcing CURRENT in the `du` equation itself (not an additive V bump
  before substepping, unlike Gray-Scott's inject) — sampled once per output
  frame from the CURRENT carrier frame and held constant across that frame's
  substeps (the same "reads the current B frame once per output frame"
  convention as Gray-Scott's param map). No erode/coverage_target/param-map
  for FHN in A1 — out of scope per the handoff (only `I` via inject is
  specified).
- **Presets** (own value-enum, `--fhn-preset`): `pulse` (excitable, a single
  stimulus fires and dies out — pure music-reactive one-shot),
  `spiral` (self-sustaining rotors after a broken-symmetry stimulus),
  `labyrinth` (a Turing-ish FHN regime, dense standing structure). Tuned
  empirically on the real cello fixture per the aliveness test below — the
  atlas values in the handoff (`ε≈0.08, a≈0.7, b≈0.8, Du≈1.0`) are the
  starting point, not the pinned numbers.

## Aliveness (falsifiable, not variance — waves MOVE)

A single interior point stimulus on an otherwise-resting field must
propagate a front outward: track, per frame, the maximum sim-lattice radius
(from the stimulus centre) at which `u` has crossed a fixed threshold above
rest, and assert it grows monotonically over N frames while a matched
control (same field, `inject`/stimulus withheld) stays at rest (variance ≈
0) for the same N frames — the "does it actually move" test the handoff
calls out, paired with a quiescence control so a preset that's alive only
because IT NEVER SETTLES (numerical drift) can't pass by accident.

## Anchors

- **FHN0 (continuity):** `--model` absent, or `--model gray-scott`, is
  byte-identical to every pre-A1 render (existing Gray-Scott tests untouched
  and still green; the contract's new fields default away).
- **FHN1 (resting-state correctness):** the Newton solver's `(u_rest,
  v_rest)` satisfies both nullcline equations to within `1e-4`, for every
  shipped preset's `(a, b)`.
- **FHN2 (quiescence):** an unstimulated field (frame-zero seed with no
  carrier-luma crossing and no speckle — a flat dark carrier below
  threshold) stays at rest (`v_variance` ≈ 0) for 60 frames — proves the
  resting state is actually a fixed point of the discretized system, not
  just the continuous one.
- **FHN3 (wave propagation):** the aliveness test above, per preset.
- **FHN4 (checkpoint):** interrupt+resume byte-identical (round-trip through
  the unchanged RGBA32F codec); a model change on an existing output
  directory refuses to resume (contract equality already covers this once
  `model` is a field); legacy (pre-A1) checkpoints resume fine defaulted to
  `gray-scott`.
- **FHN5 (composite/field-view reuse):** an FHN field-view and composite-view
  render both produce non-trivial (non-flat) output on the real cello
  fixture — Read the frames — using the two existing functions unchanged
  via `fhn_display_field`.

## Slices

- **A1-S1** (this commit): core FHN engine (`FhnSettings`, presets, resting-
  state solver, seed, substep/advance with inject-as-current, checkpoint
  `model`/`fhn_settings` fields, `fhn_display_field` adapter) + `--model`/
  `--fhn-preset`/`--epsilon`/`--fhn-a`/`--fhn-b`/`--fhn-du`/`--fhn-dt`/
  `--fhn-substeps`/`--fhn-stimulus` CLI flags on `render-morphogenesis-
  sequence` only + FHN0–FHN5 tests + a real-footage wave-speed readout
  (numbers + frames Read) rendered via the direct CLI.
- **A1-S2** (follow-up): `inject` joins the modulation registry for FHN
  (`apply_morphogenesis_modulation` gains a third `&mut FhnSettings` param,
  writes `inject` to whichever struct — harmless since only one is
  authoritative per `model`); `queue-add-morphogenesis-sequence` +
  `render_job.rs` + SwiftUI panel ride-along (model picker, FHN preset
  picker, the 8 numeric knobs, matching the established per-knob-row
  template); the flagship `--modulate "inject=audio-rms"` clip on the cello
  fixture (field view + shade).

## Acceptance criteria

FHN0–FHN5 as tests; clippy clean; zero new fmt diffs (8 pre-existing dirty
files, count unchanged); baseline cargo 737 → delta reported; no
`unwrap()`. A1-S1's deliverable: real-footage frames Read + wave-speed
numbers, no clip yet (the modulated audio-reactive flagship is A1-S2's
deliverable, since the flagship explicitly wants `inject=audio-rms`).
