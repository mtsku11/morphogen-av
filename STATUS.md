# Status

Session-resume checkpoint. Update at the end of any working session so a fresh
session (or a fresh agent) can pick up in seconds. Keep it short; durable detail
lives in `docs/`, cross-session findings live in `/memory/`.

_Last updated: 2026-07-03_

## Baseline (verified)

- `cargo test --workspace`: **496 passing across 7 crates, 0 failing.**
  One benign warning (`block v0.1.6` transitive dep, future-Rust deprecation);
  one pre-existing `items_after_test_module` clippy warning in
  `morphogen-cli/src/render.rs` (Rutt-Etra slice, test targets only).
- `swift test`: **98 passing, 0 failing.**
- `cargo clippy --workspace --all-targets -- -D warnings`: **clean**.
- Toolchain: Homebrew rustc **1.96.0** (`rust-toolchain.toml` pins `channel =
  "stable"`, which Homebrew installs ignore — a rustc upgrade can shift
  fmt/clippy behaviour; the 1.96 drift was reconciled 2026-07-02).
- Manual-testing clips (`cello.mp4`, `cello2.mp4`, `harp.mp4`) are gitignored, not tracked.

## What just landed

- **LFO modulation sources — Slice 3 of 3, MILESTONE COMPLETE** (`b754a16`;
  built inline after the Sonnet agent hit its session limit before writing
  anything): `ModulationSlotRow` gains an opt-in LFO capability (defaulted
  bindings, the named-modulator precedent) — Rutt-Etra's three slots opt in,
  showing shape/rate/phase controls; `EnumModulationSlotRow` and non-opted
  rows filter the LFO source out so the other 6 panels are unchanged.
  `modulationRoutes` gains a parallel `slotLfos` param; `lfoSourceSpec` free
  fn validates (rate finite > 0, phase finite) and spells the source clause.
  KEY: the bridge needed ZERO changes — its media-flag guards key off the
  source string, which `lfo(...)` never matches. swift 95 → **98** (LFO
  token sequence with no media flags, LFO+luma coexistence, spec
  format/validation). End-to-end: bridge-shaped queue-add→run on the gradient
  fixture — manifest records the lfo route object, within-on delta 20.340,
  raked frames Read-confirmed. **Next action:** effect chaining
  (RECOMMENDATIONS Part 2 §A) — write its milestone contract first.

- **LFO modulation sources — Slice 2 of 3** (`4892377`): core
  `ModulationSource` mirror gains the Lfo variant (unit variants still
  bare-string serde → pre-slice queue JSON byte-identical, test-pinned);
  `core_modulation_source` back to infallible (Slice 1's interim rejection
  replaced by the real mapping); `modulation_specs_from_routes` round-trips
  `lfo(shape,rate,phase)` via `spec_text` (f32 Display). Smoke-pinned:
  queue add→run byte-identical to direct (frames + manifest, rutt-etra,
  zero `--modulator-*` flags; add-time unknown-target rejection persists
  nothing) and flow feedback with an LFO route — checkpoint carries the
  params on the route in the EXISTING modulation block (no new contract
  fields), resume byte-identical, changed rate_hz/shape both refuse, legacy
  checkpoint resumes. cargo 492 → **496**, swift 95 untouched (re-run:
  95/0). **Next action:** Slice 3 — SwiftUI one-panel vertical (LFO slot
  option on the Rutt-Etra panel).

- **LFO modulation sources — Slice 1 of 3** (`118c697`, contract
  `docs/LFO_MODULATION_MILESTONE.md` committed as `84e427b`; Sonnet subagent
  build, orchestrator-verified). `ModulationSource::Lfo{shape,rate_hz,phase}`
  (sine/triangle/square/saw, pinned formulas, all emit [0,1], f64 math →
  f32) parsed as `lfo(<shape>[,<rate_hz>[,<phase>]])` in the existing route
  grammar — no media, no sidecar, exact per-frame evaluation in
  `modulated_value` (`@hold`/`@smooth` are test-pinned no-ops). Works on all
  modulatable commands via the shared plan. Proof:
  `displacement_depth=lfo(sine,0.5):64` on a static gradient, within-off
  **0.000**, within-on **20.546**, frames Read-confirmed flat → raked →
  steepest at half-cycle → flat, and the on-render's LFO zero-crossing frames
  (0, 24) are **byte-identical** to the off render. cargo 481 → **492**.
  Compiler-forced deviation (accepted): `core_modulation_source` in queue.rs
  now returns `Result` and rejects LFO at queue-add time with a clear
  "not yet supported" error — full queue wiring is Slice 2. **Next action:**
  Slice 2 (queue + stateful checkpoint verification), then Slice 3 (SwiftUI
  one-panel vertical on the Rutt-Etra panel).

- **Rutt-Etra scanline MVP — all 3 slices** (`d736048`, `6efc990`, `e656d2c`;
  built by a Sonnet subagent against `docs/RUTT_ETRA_MILESTONE.md`, each slice
  independently verified before acceptance). Slice 1: deterministic CPU
  reference `rutt_etra_scanline_cpu_v1` (Rec.709 luma → vertical scanline
  displacement, adjacent-column spans, last-writer-wins) +
  `render-rutt-etra-sequence` CLI with a knobs+algorithm manifest (the manifest
  was the one audit catch — the agent initially mirrored palette-quantize's
  stdout-only convention; the contract required a manifest.json). Gradient
  fixture off-vs-on cross-delta **19.382/255**, frames Read-confirmed (flat
  scanlines → left-to-right upward ramp). Slice 2: modulation targets
  `displacement_depth` (clamp ±512) / `line_pitch` [1,256] / `line_thickness`
  [1,64] via the standard apply fn; `displacement_depth=audio-rms:96` on a
  static carrier: within-off 0.000, within-on 18.528, quiet frames flat, loud
  frames steeply raked (Read-confirmed). Slice 3: queue add/run byte-identical
  to direct (frames AND manifest, smoke-pinned) + SwiftUI panel on the
  palette-quantize pattern (depth slot ±256/step 8, no backend picker —
  CPU-only, 5 bridge arg tests). cargo 469 → **481**, swift 90 → **95**, clippy
  + fmt clean. **Next action:** pick from `docs/RECOMMENDATIONS.md` — next in
  its ordering is LFO modulation sources, or the Rutt-Etra deferred slices
  (Metal port, two-source A→B) once the look is user-confirmed on real footage
  (`swift run MorphogenMacApp` → Rutt-Etra panel, or
  `cargo run -p morphogen-cli -- render-rutt-etra-sequence <frames> <out>`).

- **Post-`2ff7612` audit + follow-up fixes; Rutt-Etra contract written**
  (`b5d3ef0` + docs). Audited the 7 commits after `2ff7612` (sampling UI,
  named-modulator queue path, panel UI ×2, STATUS ×3): gates re-run fresh
  (cargo 469/0, swift 89/0, clippy clean), slot↔binding alignment cross-checked
  programmatically at all 9 call sites, all 29 view slot rows stem-consistent,
  all 7 remove fns reset their full slot set, queue-run reconstruction
  symmetric. Two findings, both fixed: the milestone doc's stale "SwiftUI still
  emits unnamed routes" claim (replaced with the landed panel-UI paragraph) and
  a duplicate-declared-name rename collision (bridge now refuses a route bound
  to a name declared more than once, before the CLI's blunter duplicate-flag
  error; unreferenced duplicates stay harmless). `swift test` 89 → **90**.
  **Next action:** build the Rutt-Etra scanline MVP — the contract + build
  handoff is `docs/RUTT_ETRA_MILESTONE.md`, slice plan in `docs/BACKLOG.md`
  "Next" (slice 1 = CPU reference + CLI + gradient-fixture visual proof).

- **Named-modulator panel UI — 6-panel sweep** (`c6f62af`). The named-modulator
  UI now covers **every** mod panel: feedback, fluid, retro-static, palette-
  quantize, datamosh, pixel-sort each gain a per-slot **Modulator** picker
  (Default or a declared name) + a **Named Modulators** list (name + WAV/Frames,
  add/remove). `EnumModulationSlotRow` got the same **optional** `modulator:
  Binding<String>?` picker so the enum slots (retro Filter, palette Mode, pixel-
  sort Direction/Axis) can bind too. Boilerplate collapse: add/choose delegate to
  three shared `AppState` helpers (`appendNamedModulator` / `pickNamedModulator{WAV,
  Frames}`); only the per-panel `remove*` stays inline (it resets that panel's own
  slot bindings). Fluid feeds one declared list into **all three** advect commands
  (procedural = 5 slots, two-source/self-flow = 2 slots, matched by slot order).
  Bridge threads each panel request's `namedModulators` into the shared
  `appendModulationArguments`. **No Rust changes** — the six queue tasks already
  expose `--named-modulator-audio/-frames` (confirmed in `args.rs`). `swift test`
  83 → **89** (+6, one threading test per panel: named route → prefixed spec +
  `name=path` token). **Verified:** `swift build` clean, swift 89/0, CLI flag
  presence confirmed per command. The modulation-matrix route surface is now
  COMPLETE across CLI, queue, and SwiftUI.

- **Named-modulator panel UI — channel-shift vertical slice** (`299281e`).
  The last modulation-matrix surface, started. A panel can now declare N named
  modulators (name + WAV/Frames, add/remove) and bind each mod slot to one,
  emitting `target=name.source` + `--named-modulator-audio/frames name=path`.
  Design forks confirmed via AskUserQuestion: **dynamic add/remove list** (not
  fixed-N) and **one panel first** (not all 7). `ModulationSlotRow` gained an
  **optional** `modulator: Binding<String>?` + `modulatorNames` — nil/empty
  hides the picker, so the other 28 slots stay byte-identical. Churn-avoider:
  the `modulationRoutes` helper takes a **parallel `slotModulators: [String]`**
  (default empty) instead of growing its tuple, so the 6 other render fns are
  untouched. Bridge scopes the default `--modulator-*` guards to unnamed routes
  and emits `--named-modulator-*` (two-token `name=path`) only for referenced
  names. `swift test` 81 → **83** (+2 bridge tests: named arg shape incl
  default+named coexistence & unreferenced-skip, and named-missing-media
  rejection). **No Rust changes** — the bridge emits the exact token sequence
  the passing `queue_channel_shift_named_modulators_matches_direct_and_records_routes`
  smoke test already validates end-to-end. **Verified:** `swift build` clean,
  swift 83/0, named-modulator smoke tests green. (The 6-panel sweep that this
  slice deferred landed next — see above.)

- **Direction recommendations doc + two exposure slices (prior session).**
  (0) **`docs/RECOMMENDATIONS.md`** (`2ff7612`) — strategic "where next / what
  would take this to the next level" doc (underdeveloped areas ranked by
  payoff÷effort: Rutt-Etra is the only empty roadmap slot, audio lags video,
  exposure debt, uneven mod-target coverage; next-level: effect chaining, LFO/
  drawn mod sources, realtime preview, edge/depth descriptors; explicit
  *don't*-list: multiscale-morph exposure, FFglitch dep). Cross-linked from
  `BACKLOG.md` + `EFFECTS_ROADMAP.md`.
  (1) **Per-route sampling in the SwiftUI panels** (`da945f6`): every mod slot
  (29 across all 7 panels) gains a Default/Hold/Smooth override picker;
  `ModulationRouteSpec.sampling: ModulationSamplingOption?` + the bridge appends
  `@hold`/`@smooth` to that route's `--modulate` spec, while the shared
  `--modulation-sampling` default stays. `.default` ⇒ nil ⇒ no suffix ⇒
  byte-identical arg array (regression-pinned). Queue/CLI already parsed the
  suffix — pure UI-gap close. `swift test` 78 → **81**.
  (2) **Named modulators on the queue path** (`a66a364`): reverses the
  "direct-CLI only" queue-add rejection. `RenderJobModulationRoute` gains
  skip-when-none `modulator`; the 9 modulatable tasks gain skip-when-empty
  `named_modulator_audio`/`named_modulator_frames: Vec<NamedModulatorMedia>`
  (pre-slice queue JSON byte-identical). `queue-add-*` gain
  `--named-modulator-audio/-frames` and validate named-route media through the
  **shared `resolve_modulator_media`** (error wording identical to direct);
  `queue-run` rebuilds the `<name>.<source>` specs + flags from the persisted
  vectors → queued named-modulator render **byte-identical** to direct
  (smoke-pinned). Stateful checkpoint contracts carry the named fingerprints
  through the queue for free. `cargo test` 466 → **469**. Contract:
  `docs/MODULATION_MATRIX_MILESTONE.md` ("Queue exposure (landed)").
  **Verified:** cargo 469 / swift 81, 0 failing; clippy `-D warnings` + fmt
  clean (all gates re-run independently of the two Sonnet subagents used for
  the build-out). **Remaining:** SwiftUI named-modulator panel UI (declare N
  modulators + bind each slot to one) — the last modulation-matrix surface;
  plus field-particles/cascade/dispersion modulation targets.

- **Modulation matrix — the three nice-to-haves, one commit each.**
  (1) **Per-route sampling** (`f51f5bd`): route grammar gains a terminal
  `@hold`/`@smooth` that overrides `--modulation-sampling` for that route;
  unset inherits the default (byte-identical, smoke-pinned); the suffix
  persists on queue routes, round-trips through queue-run byte-identically,
  and joins stateful checkpoint contracts. Serde skip-when-unset keeps all
  pre-slice JSON byte-identical. **Readout trap recorded:** hold==smooth
  when frame times land on the envelope sample grid (fps 4 × 62.5 ms RMS
  hop) — the smoke test proves `@smooth` at fps 3.
  (2) **Envelope sidecar cache** (`248f22b`): `--modulation-cache-dir` on
  all 9 modulatable commands persists extracted **luma/flow** envelopes
  (flow = per-pair Lucas-Kanade, the dominant cost) as sidecars keyed by
  algorithm id + fps + content fingerprint; reuse only on a full match,
  regenerate on any mismatch (content-change invalidation smoke-pinned);
  a hit is byte-identical to fresh extraction (serde_json round-trips
  floats exactly). Never part of a render's contract; queue jobs uncached.
  (3) **Named modulators** (`d70fac3`): `<target>=<name>.<source>` routes
  read media from repeatable `--named-modulator-audio/frames name=path`
  flags; bare sources keep the default modulator (aliased-vs-unnamed
  byte-identity smoke-pinned); envelopes extract once per (modulator,
  source); stateful contracts gain a skip-when-empty named-fingerprint
  list (rename/content change refuses resume, smoke-pinned); queue-add
  rejects named routes ("direct-CLI only") persisting nothing.
  **Verified:** workspace 460 → **466**, 0 failing (3 unit + 3 smoke);
  clippy `-D warnings` + fmt clean; swift build/test unaffected (78).
  Remaining ideas: queue/SwiftUI exposure of named modulators, per-route
  sampling UI, field-particles/cascade modulation targets.

- **Modulation matrix — queue/SwiftUI exposure of stateful-effect routes
  (the milestone's last deferred slice).** All five stateful queue tasks
  (flow feedback, datamosh, fluid advect ×3) persist modulation routes via
  the same serde-defaulted core fields as the stateless tasks (pre-slice
  jobs deserialize unmodulated, pinned by a core test); queue-add validates
  routes BEFORE persisting (probe through each effect's apply fn — the
  shared `parse_queue_modulation_routes` probe now returns `CliError` so
  datamosh's CLI-side apply fn plugs in directly); queue-run rebuilds spec
  strings so it shares the direct render's code path. Envelope time base =
  the job's `frame_rate`; datamosh (no per-job rate) uses its manifest's
  fixed 30 fps — a direct render matches with `--modulation-fps 30`.
  Manifests gain the `modulation` block only when routes exist; the
  feedback/datamosh **checkpoint contracts carry the routes through the
  queue path** (queued-job checkpoint pinned by smoke test). SwiftUI: mod
  slots on the Flow Feedback (5 targets), Datamosh (4), and Fluid/Advection
  panels (6 slots shared across the three runs — Procedural consumes all,
  A-to-B/Self-Flow only flow-advect+reinject since their commands have no
  turbulence targets), each with shared modulator pickers + sampling; the
  bridge appends the standard modulation flags on all five queue-add
  commands (no routes ⇒ byte-identical pre-slice command shape,
  test-pinned). **Verified:** workspace 456 → **460** (core serde-default
  test + 3 add→run byte-identity smokes: feedback incl. queued-checkpoint
  modulation block, datamosh incl. add-time rejection persisting nothing +
  manifest block, fluid advect + manifest block); `swift test` 75 → **78**
  (feedback/datamosh/fluid bridge arg tests incl. media-requirement
  rejection and no-routes flag omission); clippy `-D warnings` + fmt clean;
  swift build clean. Two local commits: queue slice `b1d8c38`, SwiftUI
  slice (this entry). The modulation-matrix milestone's route surface is
  now fully exposed across direct CLI, queue, and SwiftUI.

- **Modulation matrix — stateful targets slice 7: datamosh + fluid advect
  (direct CLI).** The two deferred stateful effects gain routes.
  **Datamosh** (`render-datamosh-sequence`): targets `amount` /
  `residual_gain` / `residual_decay` / `refresh_threshold`, all clamped
  `[0, 4096]` mirroring the command's own validation; the apply fn lives
  **CLI-side in `modulate.rs`** (the answer to the recorded placement
  question — `DatamoshSequenceSettings` is a CLI struct). The checkpoint
  contract gains the same serde-defaulted `modulation` block as feedback
  (reused `FeedbackModulationContract`), so changed/dropped routes refuse
  resume and legacy checkpoints stay resumable unmodulated. **Tier activation
  (residual/refresh/remix) is now re-evaluated per frame** from the routed
  knobs — an envelope can pulse the residual tier on a plain block base; a
  zero-gain frame holds the accumulator untouched; without routes the flags
  equal the base flags (exact off path). Excluded: `keyframe_interval`,
  `block_size`, remix/seed/preset/smear/engrave (structural). **Fluid advect**
  (all three commands): stateful per-frame application only — **no checkpoint
  path exists**, so routes are printed provenance. Single-source targets
  `advect`/`turbulence_scale`/`turbulence_speed`/`detail`/`reinject`;
  two-source + optical-flow share `advect` (may go negative = reversed flow) /
  `reinject`; `seed` excluded. All three commands sample envelopes against
  `--modulation-fps` (stateless default 12 — none has a `--frame-rate`).
  Queue/showcase call sites pass the default (unmodulated) bundle. Contract:
  `docs/MODULATION_MATRIX_MILESTONE.md` ("Stateful targets"). **Verified:**
  workspace 451 → **456**, 0 failing (+2 unit: fluid clamp rules incl.
  validate-legality, datamosh clamp + 7 structural exclusions; +3 smoke:
  datamosh full checkpoint-contract acceptance on a translating-texture
  fixture — ON≠OFF, modulation block pinned, changed/dropped routes refuse,
  resume byte-identical 3/3, legacy compat; fluid + optical-flow continuity
  identities `scale 0, offset K` byte-identical to the constant knob, route ≠
  default, unknown-target rejection); clippy `-D warnings` + fmt clean.
  **Readout** (12-frame 192×144 testsrc2 + RMS ramp, `dm-cross-delta.py`):
  datamosh `amount=audio-rms:3,0` OFF-vs-ON **0.000 → 21.5/255** monotone;
  optical-flow `advect=audio-rms:6,0` **0.000 → 21.9/255** monotone; fluid
  advect `advect=audio-rms:24,0` **non-monotone 0 → 30.5 → dip 9.1 (frame 7)
  → 30.9** — the dip is where the rising envelope drives advect *through* the
  OFF constant 12, itself proof the route tracks the envelope. Frames Read:
  datamosh ON = heavy accumulated melt vs OFF mild smear; fluid ON =
  deeper-wrapped spirals; optical-flow ON = bars dissolved along own motion.
  Remaining: queue/SwiftUI exposure of stateful-effect routes.

- **Modulation matrix — stateful targets (slice 6, flow feedback, direct
  CLI).** The last deferred target class opens: `render-feedback-sequence`
  gains the standard `--modulate` flag set (envelopes sample against
  `--frame-rate` — one timeline per stateful render, the queue-slice
  precedent, no separate `--modulation-fps`). Targets `carrier_amount` /
  `feedback_amount` (±4096 px, shift-range precedent), `feedback_mix` [0, 1],
  `decay` / `structure_mix` (one-sided ≥ 0, mirroring `validate`);
  `structure_mode` and `iterations` are deliberately not targets (an envelope
  must not drive a backend-invalid or contract-breaking configuration). On a
  stateful effect frame N depends on the whole knob history, so the
  **modulation config joins the sequence contract**: the checkpoint's
  serde-defaulted `modulation` block records routes (CLI order), sampling,
  envelope fps, and **fnv1a64 content fingerprints of exactly the modulator
  media the routed sources consume** — contract equality already gates
  resume, so any route/sampling/fps/modulator-content change refuses with the
  existing "settings changed" error, while pre-slice checkpoints deserialize
  to `None` and stay resumable unmodulated (both pinned by smoke test). The
  per-frame settings copy is applied at the top of each frame's state update
  and feeds **both** the render and the supersample. Contract:
  `docs/MODULATION_MATRIX_MILESTONE.md` ("Stateful targets"). **Verified:**
  workspace 449 → **451** (clamp-rule unit test; smoke: interrupted+resumed
  modulated render byte-identical to uninterrupted 3/3 frames, changed/dropped
  routes refuse resume, checkpoint modulation block pinned, ON≠OFF), clippy
  `-D warnings` + fmt clean; readout on moving testsrc2 self-feedback with an
  RMS ramp → `feedback_mix=audio-rms:0.75,0.2`: OFF-vs-ON cross-delta grows
  **0.000 → 42.1/255** monotonically over 12 frames (frame 0 identical —
  envelope starts silent), frames Read (OFF = mild displacement, ON = heavy
  accumulated feedback smear). Remaining: datamosh/fluid-advect stateful
  targets, queue/SwiftUI exposure of feedback routes.

- **SwiftUI enum mod slots (From→To variant pickers).** The deferred
  enum-slot design is resolved: `EnumModulationSlotRow` shows two variant
  pickers instead of scale/offset steppers — envelope 0 selects **From**,
  envelope 1 selects **To**; `enumModulationMapping` (AppState.swift) emits
  the equivalent affine route (`offset = fromIndex`,
  `scale = toIndex − fromIndex`) over the option enum's declared case order.
  From == To emits `scale 0` = a constant variant override (continuity
  identity); reversed and partial sweeps fall out of the same two pickers.
  Slots added: retro-static `filter` (None→Paeth default), pixel-sort
  `direction` (Asc→Desc) + `axis` (Row→Col), palette-quantize `mode`
  (Posterize→Palette); all four option enums' case order is **pinned by
  test** against the contract variant tables so a reorder can't silently
  remap envelopes. **Verified:** `swift build` clean, `swift test` 73 →
  **75** (mapping unit test: full/reversed/partial/constant sweeps on all
  four enums; case-order pin). End-to-end against the real CLI: the
  filter slot's full-sweep route `filter=luma:4,0` queued via
  `queue-add-retro-static-sequence` on the gray-ramp modulator renders
  8 frames that **byte-match** the direct constant-filter renders in the
  contracted order none, sub, sub, up, up, average, average, paeth.

- **Palette-quantize SwiftUI panel.** RenderPanelView gains a "Palette
  Quantize — Posterize / Neon Palette" section next to channel-shift: sticky
  backend picker (defaults **Metal**, retro-static precedent — both modes are
  parity-gated, no CPU-only mode), posterize/palette mode picker, a levels
  stepper shown only in posterize mode (2–256; app default **8** so the first
  Run is visible, the CLI's 256 passthrough stays reachable), and a slice-3
  mod slot on the integer `levels` target with ±254/±256 step-8 ranges (enum
  `mode` gets **no** slot — enum mod-slot presentation is still the deferred
  design). Bridge: `runQueuedPaletteQuantizeSequenceRender` +
  `queueAddPaletteQuantizeSequenceArguments` on the channel-shift template;
  app-side validation rejects out-of-range levels only in posterize mode
  (palette ignores levels). **Verified:** `swift build` clean, `swift test`
  70 → **73** (args include mode/levels/backend/no-`--modulate` when slots
  are off; routes + smooth sampling carried; levels 1 rejected in posterize,
  accepted in palette). Bridge arg shape proven end-to-end against the real
  CLI: queue-add with the bridge-emitted flags (`levels=luma:-254,256` route)
  → run → manifest records algorithm/settings/route; levels sweeps 256→2 on
  the gray-ramp modulator with frame 0 **pixel-exact passthrough** (0.000/255
  vs carrier via `dm-cross-delta.py` — file-level `cmp` is the known
  RGB↔RGBA false-negative) rising to 16.17/255 at levels 2, frames Read
  (final frame collapses to pure primaries).

- **Palette-quantize queue task.** `frame_sequence_palette_quantize` core
  render-job task + `queue-add`/`queue-run-palette-quantize-sequence` CLI,
  built on the channel-shift precedent: routes (integer `levels`, enum
  `mode`) validated at add time through the same
  `apply_palette_quantize_modulation` probe (rejection persists nothing),
  `mode` stored as a string label (`posterize`/`palette`) like retro-static's
  `filter`, queue-run rebuilds `--modulate` spec strings from persisted
  routes so it shares the direct render path. Manifest gets the
  `palette_quantize` effect block (algorithm id, static settings, backend,
  modulation block when routes exist). **Verified:** workspace 448 → **449**
  (new smoke test: add→run byte-identical to the direct render 2/2 frames
  with a `levels=luma:6,2` + `mode=luma:0,0` route pair, routed levels
  actually posterize the gradient, manifest task/algorithm/settings/routes
  pinned); clippy `-D warnings` + fmt clean. SwiftUI palette-quantize panel
  section is its own later slice.

- **Modulation matrix — enum targets (slice 5).** Pixel-sort
  `direction`/`axis`, retro-static `filter`, and palette-quantize `mode` join
  the registries under the **contracted variant-index rule** (table in
  `docs/MODULATION_MATRIX_MILESTONE.md`): variants get indices `0..N−1` in
  declared order and the mapped value selects by the same clamp-then-round,
  ties-away-from-zero rule (`enum_knob` over `integer_knob` in
  `modulation.rs`). **Unimplemented variants are excluded** — palette-quantize
  `kmeans` renders an error, so it is not in the variant list;
  clamp-never-error extends to enum selection. Range trap documented: a
  `[0, 1]` envelope at default scale only spans indices 0–1; sweeping the
  5-variant `filter` needs `scale ≈ 4`. No queue changes needed — the queue
  path validates/applies through the same per-effect apply functions, so enum
  routes persist on the existing pixel-sort/retro-static tasks. SwiftUI mod
  slots for enum targets deferred (steppers need an enum-aware presentation).
  **Verified:** workspace 447 → **448** (variant-indexing unit test: boundary
  clamp both ends, 0.5 tie flips to `desc`, all 5 filters reachable, `kmeans`
  unreachable at 9999); clippy/fmt clean; three continuity identities
  byte-identical (`direction=luma:0,1` ≡ `--direction desc`,
  `filter=luma:0,4` ≡ `--filter paeth`, `mode=luma:0,1` ≡ `--mode palette`,
  8/8 frames each); luma-ramp sweep on `direction=luma`: frames 0–3
  byte-match the pure asc render, frames 4–7 byte-match pure desc — the flip
  lands exactly at the 0.5 rounding boundary, flip frames Read (sorted-streak
  gradients invert). Remaining target class: stateful effects (must join the
  checkpoint-invalidation contract first).

- **Modulation matrix — integer targets (slice 4).** Palette-quantize `levels`
  is the first integer modulation target, under the newly **contracted
  rounding rule** (in `docs/MODULATION_MATRIX_MILESTONE.md`): clamp to the
  declared `[2, 256]` range, then round to nearest with ties away from zero
  (`f32::round`); clamp-then-round order and the tie rule are contract, and
  the knob's off case stays reachable (envelope → 256 = that frame's
  byte-identical passthrough). Engine: `apply_palette_quantize_modulation` +
  `PALETTE_QUANTIZE_MODULATION_TARGETS` in `modulation.rs`;
  `render-palette-quantize-sequence` gains the standard `--modulate` flag set
  (direct CLI only — palette-quantize has no queue task or SwiftUI section
  yet; those are their own later slices, channel-shift precedent).
  **Verified:** workspace 446 → **447** (rounding-rule unit test:
  clamp ends, 4.4/4.5 tie, 255.5→256, integer continuity identity), clippy/fmt
  clean; off case byte-identical pre-vs-post change (4/4 frames);
  luma-ramp readout on the gradient carrier: ON-vs-carrier cross-delta falls
  **61.6 → 0.755 → 0.380 → 0.000**/255 as levels sweeps 2→87→171→256 with the
  final frame byte-identical (passthrough reached), frames Read (levels-2
  frame collapses to pure primaries); `scale 0, offset 5` byte-identical to
  `--levels 5`. Remaining target classes: enum knobs, stateful effects.

- **Channel-shift SwiftUI panel.** RenderPanelView gains a "Channel Shift —
  RGB Split (+ A-Flow Rows)" section next to retro-static: sticky backend
  picker (defaults **CPU** so the out-of-box state keeps flow-driven mode
  valid; Metal is constant-offsets-only), six shift steppers (R/G/B × X/Y,
  ±64 px), flow-gain stepper with a radius stepper that appears when gain ≠ 0,
  and the slice-3 mod-slot pattern on all six `shift_*` targets.
  `ModulationSlotRow` gained defaulted range parameters (existing call sites
  unchanged) because its ±8 scale / ±1 offset defaults suit [0, 1] knobs but
  are invisible for pixel-unit targets — channel-shift slots use ±64.
  Bridge: `ChannelShiftSequenceRenderQueueCommandRequest` +
  `queueAddChannelShiftSequenceArguments` (shift/flow-gain flags emitted in
  `--flag=value` form so negative pixels survive clap) +
  `runQueuedChannelShiftSequenceRender`; app-side fail-fast for flow-without-
  Source-A and flow-on-Metal. Source A comes from the shared frame-sequence
  modulator picker; Source B from the shared carrier picker. **Verified:**
  `swift test` 67 → **70** (constant-mode args incl. `=`-form negatives,
  flow+modulation args, both invalid-flow rejections); the exact bridge
  argument shape run end-to-end against the real CLI (flow + `--modulate
  "shift_r_x=luma:12,0"`: manifest records `channel_shift_flow_driven_cpu_v1`,
  `shift_b_x: -6.0`, the route, smooth sampling). No interactive app launch
  this session (no screenshot harness for the shell — same caveat as slice 3).
  Remaining: integer/enum/stateful modulation targets.

- **Channel-shift queue task.** `frame_sequence_channel_shift` joins the render
  queue: core `RenderJobTask::FrameSequenceChannelShift` (all fields
  serde-default), `queue-add-channel-shift-sequence` /
  `queue-run-channel-shift-sequence` CLI commands covering all three existing
  modes — constant offsets (CPU or parity-gated Metal), A-flow-driven per-row
  shifts (CPU-only, `--flow-gain` + `--source-a-dir`), and modulation-matrix
  routes on the six `shift_*` targets (same `--modulate` flag set as slice 2,
  validated fail-fast before persisting). `queue-run` shares
  `render_channel_shift_sequence` with the direct command, so add→run is
  byte-identical to the direct render — pinned by smoke test along with the
  manifest's algorithm/settings/modulation block. **Verified:** workspace
  445 → **446**, clippy/fmt clean; manual queue runs of the flow-driven branch
  (manifest records `channel_shift_flow_driven_cpu_v1`, flow knobs) and the
  Metal constant branch (CPU algorithm id + `"backend": "Metal"`, matching the
  retro-static convention); both add-time errors (flow without
  `--source-a-dir`, flow + Metal) fire before any queue file is written.
  Remaining from the slice-3 note: SwiftUI channel-shift panel exposure,
  integer/enum/stateful modulation targets.

- **Modulation matrix — slice 3 (SwiftUI mod slots).** The route editor ships
  as per-knob **mod slots**, not a free-form route list: retro-static
  (strength) and pixel-sort (threshold low/high) panel sections each gain a
  source picker (Off/audio-rms/onset/centroid/luma/flow; Off = no route, so
  duplicate targets are impossible by construction), scale/offset steppers
  (shown only when a source is chosen), shared modulator WAV / frames pickers,
  and a hold/smooth sampling picker (`ModulationSlotRow` / `ModulationMediaRow`
  in RenderPanelView, option enums in AppState). The bridge appends the
  `--modulate` flag set to the queue-add commands via a shared
  `appendModulationArguments` (no routes ⇒ no flags = the exact unmodulated
  arg array, pinned by test) and rejects non-finite scale/offset or missing
  modulator media app-side; AppState guards give a status message before
  dispatch. Request structs take defaulted `var` fields so pre-slice call
  sites keep meaning (house pattern). **Verified:** `swift build` +
  `swift test` 64 → **67** (3 new bridge arg tests: route formatting incl.
  `cliNumber` output, no-route flag omission, flow-route-without-frames
  throws). Not visually exercised beyond compile + tests (no interactive
  launch in this session). Remaining: integer/enum targets, stateful-effect
  targets, channel-shift queue task + panel exposure.

- **Modulation matrix — slice 2 (queue persistence).** `--modulate` routes now
  persist on the `frame_sequence_retro_static` and `frame_sequence_pixel_sort`
  queue jobs: core gained `RenderJobModulationRoute` + `ModulationSource`/
  `ModulationSampling` mirrors (the flat route documents itself as the two-node
  degenerate case of graph.rs's `ModulationRoute`), all fields serde-default so
  pre-slice jobs deserialize unmodulated (core test pins it). `queue-add-…`
  takes the same `--modulate`/`--modulator-audio`/`--modulator-frames`/
  `--modulation-sampling` flags and **fails fast before persisting** (grammar,
  duplicate/unknown targets, missing modulator flags — smoke-tested, no queue
  file written on rejection); envelope times sample against the job's
  `frame_rate`. `queue-run` reconstructs the route specs from the persisted
  form (`f32` `Display` round-trips exactly) so it shares the direct render's
  exact code path — smoke test pins add→run **byte-identical** to the direct
  modulated render and the manifest's `modulation` block (routes, modulator
  paths, sampling, fps; the key is omitted for unmodulated jobs so old
  manifests keep their format). Workspace 442 → **445**, clippy/fmt clean.
  *Note:* channel-shift has no queue task at all — adding one is its own
  vertical slice. Remaining: SwiftUI route editor (slice 3), integer/enum/
  stateful targets.

- **Modulation matrix — slice 1 (CPU + CLI).** The first generic typed-signal
  routing layer toward the modular-synth goal: `--modulate
  "<target>=<source>[:<scale>[,<offset>]]"` (repeatable) binds a normalized
  analysis envelope — `audio-rms`/`audio-onset`/`audio-centroid` from
  `--modulator-audio`, `luma`/`flow` from `--modulator-frames` — to a float
  knob, per frame, on `render-retro-static-sequence` (`strength`),
  `render-pixel-sort-sequence` (`threshold_low/high`), and
  `render-channel-shift-sequence` (six `shift_*`). Engine in
  `morphogen-render/src/modulation.rs` (route grammar, hold/smooth sampling,
  per-effect clamped apply registries, 11 unit tests); CLI envelope extraction
  in `morphogen-cli/src/modulate.rs` reuses the existing RMS/onset/centroid/
  luma/flow extractors. Values clamp (never error); zero routes = the exact
  unmodulated path; effect algorithm ids unchanged; the resolved routes print
  at render time. **Verified:** workspace 431 → **442**, clippy/fmt clean;
  readout on testsrc2 + a volume-ramp WAV — RMS→strength ON-vs-OFF shrinks
  96.4 → 2.3/255 while ON-vs-source grows 0.68 → 95.6/255 (the knob tracks the
  envelope; frames Read: clean early, fully glitched late); continuity
  identity `strength=audio-rms:0,0.6` byte-identical to `--strength 0.6`;
  flow→RGB-split frame 0 = 0.000 (flow's no-prior-frame convention) then
  ≈55–60/255. Contract: `docs/MODULATION_MATRIX_MILESTONE.md` (notes the
  reconciliation plan with core's schema-level graph `ModulationRoute`).
  Deferred: queue persistence (slice 2), SwiftUI route editor (slice 3),
  integer/enum/stateful targets.

- **Audit quick wins (bugfix batch).** From a full-repo audit: (1) the SwiftUI
  dev bridge now passes `--release` on every `cargo run` invocation, so
  GUI-initiated renders use release binaries (the first render after a clean
  build pays a one-time release compile); (2) `RenderQueue::save_json` writes
  temp-then-rename so a crash can never truncate `render-queue.json`;
  (3) float sort comparators standardized on `total_cmp` (`pixel_sort`,
  `granular_mosaic`) — NaN-safe under Rust ≥1.81 sort semantics, byte-identical
  ordering for the finite values these paths produce; (4) `renders/` and
  `__pycache__/` gitignored. Audit follow-ups still open: no in-flight render
  guard/cancel in the app (double-dispatch races on the shared queue JSON),
  fixed `job-0001` output overwrite, `%.6g` knob truncation in the bridge.

- **Datamosh Codec Engrave preset.** Added `--preset codec-engrave` for the
  denser subject-detail version of the glitch-art reference: block/vector
  datamosh plus gentler scanline tearing, carrier-edge hatching, block stepping,
  RGB edge offsets, and micro-contrast. It is available in CLI, queued datamosh
  manifests, and the SwiftUI datamosh picker. Verified on `harp.mp4`
  self-modulation; representative frame:
  `/tmp/morphogen-harp-reference/codec-engrave-v3/frame_000035.png`, subject
  crop: `/tmp/morphogen-harp-reference/codec-engrave-v3-frame35-subject-crop.png`,
  contact strip: `/tmp/morphogen-harp-reference/codec-engrave-v3-strip.png`.
  **Verified:** `cargo test --workspace` (365 passing), `cargo clippy
  --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `git diff
  --check`, and direct harp render `render-datamosh-sequence --preset
  codec-engrave`. `swift test` is pending for this exact change because the
  required escalated module-cache write was rejected by the approval system.

- **Datamosh Scanline Smear preset.** Added `--preset scanline-smear` for the
  glitch-art postcard look: block/vector datamosh followed by deterministic
  flow-driven horizontal tearing, edge-protected subject retention, and sparse
  chroma/white/black codec debris. The preset is available in CLI, queued
  datamosh manifests, and the SwiftUI datamosh preset picker. Verified on
  `harp.mp4` self-modulation; inspection strip:
  `/tmp/morphogen-harp-reference/scanline-smear-strip.png`. **Verified:**
  `cargo test --workspace` (362 passing), `cargo clippy --workspace --all-targets
  -- -D warnings`, `swift test` (54 passing), and direct harp render
  `render-datamosh-sequence --preset scanline-smear`.

- **Curated showcase preview path.** The CLI now has `render-showcase`, a
  product-facing short preview renderer for extracted Source A/B frame folders. It
  renders four A-modulates-B segments (flow displacement, flow feedback, temporal
  granular mosaic, vector datamosh), writes named segment folders, a combined PNG
  sequence, representative stills, `contact_sheet.png`, `showcase.json`, and an
  optional H.264 `showcase.mp4` via external ffmpeg. The SwiftUI workflow exposes
  the same path as a **Showcase Preview** action with Balanced/Destructive
  intensity. The flow-feedback `--iterations` flag now rejects unsupported values
  at CLI parse time, the advanced Swift panel shows the current fixed one-pass
  contract instead of a fake menu, and datamosh presets print their resolved knob
  set when they override manual values. **Verified:** `cargo test --workspace`
  (358 passing), `cargo clippy --workspace --all-targets -- -D warnings`, a real
  cello/harp `render-showcase` MP4/contact-sheet smoke, and `git diff --check`.
  Swift build/test verification is pending because the approval system rejected
  the required escalated SwiftPM module-cache write.

- **SwiftUI workflow shell — source, route, effect, render.** The macOS app now
  opens on a workflow-first surface instead of the dense render-parameter panel:
  Source A/B cards sit at the top, then a guided flow handles proxy extraction,
  modulation routing, effect-card selection, focused primary controls, output
  selection, and render/export actions. The existing diagnostic render panel is
  still available under an Advanced disclosure. Datamosh rendering now falls back
  to the common extracted Source A/B frame directories and sequence output root,
  so it works from the same workflow path as the other visual effects. **Verified:**
  `swift build`, `swift test` (52 passing), and a short `swift run MorphogenMacApp`
  launch check.

- **Controlled Datamosh — reusable flow sidecars, disk resume, and curated presets.**
  Direct `render-datamosh-sequence` now accepts `--flow-cache-dir`, writes/reuses
  per-P-frame Source A temporal-flow sidecars, and records cache provenance. It also
  writes `checkpoint.json` plus RGBA32F `state/datamosh_output_frame_*.rgba32f`
  after every frame; `--stop-after-frame` proves a subsequent identical command can
  resume byte-identically to an uninterrupted render. Residual-mode state persists
  as flow-cache sidecars under `state/datamosh_residual_frame_*`. Core gained
  `DatamoshPreset` (`custom`, `codec_bloom`, `structured_melt`, `macroblock_rot`,
  `vector_shuffle`); CLI/queue/SwiftUI expose `--preset`, queue jobs default their
  flow cache to `job-0001/cache/datamosh-flow`, and manifests record the resolved
  destructive recipe. **Verified:** new smoke coverage for stop/resume equivalence,
  flow-cache reuse/provenance, and preset resolution through queued vector-shuffle.
  `docs/DATAMOSH_MILESTONE.md` and `docs/REFERENCE.md` updated.

- **Controlled Datamosh — vector-remix tier: queue + SwiftUI exposure (full vertical slice).**
  The slice-1 CPU+CLI vector-remix now threads end-to-end. The schema mirror
  `VectorRemixMode` was added to **core** (beside `RenderBackend`/`KernelMode`); the
  persisted `frame_sequence_datamosh` job carries `vector_remix` (serde-default
  `None`) + `remix_seed` (serde-default `0`), so pre-slice jobs keep their id.
  `queue-add-datamosh-sequence` gained `--vector-remix`/`--remix-seed`; `queue-run`
  maps core→render (free fn, orphan rule) and records both in the manifest. macOS
  Render panel adds a Vector Remix picker + Remix Seed stepper (shown for Shuffle);
  Swift bridge passes the flags. **Verified:** queue add→run with `--vector-remix
  shuffle --remix-seed 42` byte-identical to the direct render, manifest carries the
  `…vector_remix…` algorithm id + `vector_remix: "shuffle"` + `remix_seed: 42` (new
  smoke test). Workspace 354 → **355** (+1 smoke), Swift **52** (bridge test
  extended), clippy clean. `docs/DATAMOSH_MILESTONE.md` updated.

- **Controlled Datamosh — vector-remix tier (FFglitch MV sort/shuffle, deterministic; slice 1 CPU + CLI).**
  The deterministic "family look" of FFglitch's motion-vector sort/shuffle, on the
  optical-flow field rather than the codec bitstream (user chose this over an
  FFglitch external dep or a pure-Rust MPEG-4 MV codec). The block-quantized flow
  *is* a per-block MV grid (FFglitch's "vector" unit), so a remix is a **permutation
  of that grid** before the parity-gated displace — pure flow→flow, **Metal free**.
  `remix_block_vectors(flow, block_size, mode, seed)` (shares a factored-out
  `block_mean_grid` with `quantize_flow_to_blocks`): `sort` reassigns block MVs by
  descending magnitude (motion pools), `shuffle` is a seeded Fisher–Yates permutation
  (motion scrambles); both preserve the motion-energy multiset. New id
  `flow_reuse_datamosh_vector_remix_cpu_v1` via a 4th `datamosh_algorithm` arg
  (`remix != None` + blocks ≥ 2 ⇒ most-specific). CLI `--vector-remix none|sort|shuffle
  --remix-seed N` on `render-datamosh-sequence`. **Continuity:** `none` ⇒ byte-identical
  to the block path; `block_size ≤ 1` ⇒ bloom. **Verified** (fixture, block 16, melt):
  none-vs-sort cross-delta 0 → 70.9/255, none-vs-shuffle 0 → ~37 (non-monotonic
  scramble), frame 0 identical (both B[0]), re-rendered sort byte-identical
  (deterministic); frames Read — sort redistributes the displacement, shuffle scatters
  it. +5 tests (render crate), workspace 349 → **354**, clippy clean. **Follow-up:
  queue/SwiftUI exposure** (queue caller passes `VectorRemixMode::None` for now).
  `docs/DATAMOSH_MILESTONE.md` updated.

- **Controlled Datamosh — real bitstream motion transfer (experimental, non-deterministic).**
  "Swap Source A's motion onto Source B's content" — and, contrary to the original
  "likely FFglitch" guess, done with the **same pure-Rust AVI chunk surgery**.
  `avi.rs::transfer_motion` keeps the carrier's (B) leading I-frame and replays the
  modulator's (A) P-frames, so B's pixels are pushed by motion that never belonged
  to them. The carrier supplies the rebuilt headers, so the modulator is encoded
  **scaled to the carrier's dimensions** (`encode_datamosh_avi_scaled`); a new
  `avi_dimensions` equality guard rejects mismatched macroblock grids. CLI:
  `datamosh-bitstream <MODULATOR> <OUT> --operation motion-transfer --carrier <B>
  [--carrier-keyframes N]` (default 1 = pure transfer = just the I-frame). Algorithm
  id `datamosh_bitstream_motion_transfer_experimental_v1`; sidecar records both
  inputs + `carrier_keyframes` + `deterministic: false`. Same carve-out as the other
  bitstream ops (surgery deterministic + unit-tested, decoded look codec-dependent,
  outside the render graph). **Verified** (testsrc2 motion → mandelbrot carrier,
  160×120): output frame 1 byte-identical to the carrier (I-frame seed, cross-delta
  0.000), then the fractal smears under testsrc2's macroblock motion (its moving
  structures bleed in); frame-delta 8.83/255 vs the plain carrier's 3.94 — Read-
  confirmed B's appearance + A's motion. +6 tests (5 avi splice/guard, 1 ffmpeg
  scaled-encode), workspace 343 → **349**, clippy clean. `docs/DATAMOSH_MILESTONE.md`
  updated (Deferred → Landed). **Remaining datamosh deferrals:** richer FFglitch vector
  remix on true codec motion vectors and an optional stateless motion-transfer mode.

- **Datamosh visual-regression contact sheet (tooling).**
  `scripts/datamosh-contact-sheet.py` renders every named destructive datamosh
  mode and tiles sampled frames into one labeled review PNG so each mode has
  pixels to inspect — the standing tool for the milestone's "post a contact sheet"
  verification gate. Deterministic tiers (PASSTHROUGH baseline, Codec Bloom,
  Macroblock Slide, Structured Melt, Macroblock Rot) run on the synthetic
  `make-datamosh-fixture.py` fixture and are byte-reproducible; the bitstream tiers
  (P-Frame Bloom, Void Mosh) are opt-in via `--video CLIP` (needs ffmpeg) and
  flagged NON-DETERMINISTIC on the sheet. Pure-stdlib PNG decode/encode + a
  built-in 5×7 font (no deps, like `frame-delta.py`); also prints each
  deterministic mode's mean RGB cross-delta vs PASSTHROUGH. Verified: 5-mode sheet
  (deterministic) + 7-mode sheet (with a testsrc2 clip) both Read — each mode's
  look matches its documented behavior (bloom speckles, coherent macroblock slide,
  streaky residual melt, self-erasing rot trail; bitstream codec decay / keyframe
  voids). Cross-deltas over the 8-frame fixture: Codec Bloom 9.8, Macroblock Slide
  23.5, Structured Melt 22.4, Macroblock Rot 12.5 /255. No render-graph change, so
  workspace stays 343. Documented in `docs/DATAMOSH_MILESTONE.md`. **Next: option 1
  — motion-transfer bitstream mosh (swap A's vectors into B; likely FFglitch-class).**

- **Controlled Datamosh — real bitstream keyframe removal.**
  `datamosh-bitstream --operation remove-keyframe` removes the controlled MPEG-4
  AVI substrate's leading keyframe so ffmpeg decodes from prediction data rather
  than a clean I-frame. The pure-Rust surgery path is unit-tested on synthetic AVI
  buffers and records `operation: remove_keyframe`, `deterministic: false`, ffmpeg
  version, and algorithm id `datamosh_bitstream_remove_keyframe_experimental_v1`
  in `datamosh_bitstream.json`. Verified on `av-synth/harp.mp4` at 12 fps: clean
  transcode vs keyframe removal differs by about **54/255 mean RGB delta** across
  the sampled early frames; representative comparison frame:
  `/tmp/morphogen-datamosh-keyframe-20260625/keyframe-removal-frame120-comparison.png`.
  Workspace 340 → 343 (+3 Rust). This stays outside queue/SwiftUI/parity under the
  same non-deterministic bitstream carve-out as P-frame bloom.

- **Fluid/advection family queue jobs.** The compact fluid effects now have
  persisted render-queue contracts and CLI add/run paths:
  `frame_sequence_fluid_advect`, `frame_sequence_fluid_advect_two_source`,
  `frame_sequence_optical_flow_advect`, and
  `frame_sequence_field_particles`. Each writes the standard ProRes-ready
  `frames/`, `manifest.json`, and `checkpoint.json` bundle, records timing,
  backend, source provenance, and algorithm id, and persists failures back to the
  queue. New smoke tests prove queued output is byte-identical to the direct CPU
  render for all four paths. SwiftUI render-panel exposure is now wired locally:
  compact controls dispatch all four queue jobs through the dev CLI bridge, update
  the ProRes-ready bundle target, and have Swift bridge argument tests. Workspace
  336 → 340 (+4 Rust), Swift tests 47 → 52 (+5). Decide separately whether the
  larger CPU-only `fluid_mosaic` surface belongs in the queue now or after presets.

- **Field-particles splat — parity-gated Metal port.** A `field_particles_splat` kernel
  rasterizes the CPU-computed particle carrier: each output pixel **gathers** the last
  (highest-index) particle whose `particle_size` square covers it, matching the CPU
  last-writer-wins **scatter** byte-for-byte (positions are the CPU floats uploaded verbatim,
  so `round()` lands on the same cells). `ParticleField` gained `dimensions()`/`splat_buffer()`
  accessors. Exposed via `--backend metal` on `render-field-particles-sequence` with a per-frame
  CPU parity gate. Parity test at **1e-6** on device (byte-identical); end-to-end Metal-vs-CPU
  **0.000/255**. **Caveat:** the gather is O(w·h·particles) — for a dense grid that's *more* work
  than the CPU scatter, so it's correctness-first; a tiled/binned scatter is the perf follow-up
  (like the large-K convolution kernel). Workspace 334 → 336 (+2). **This completes all
  fluid-advect-family Metal ports — nothing deferred remains.** See [[faux-fluid-advect]].

- **Flow-driven advect — parity-gated Metal port (two-source + single-source).** A
  `fluid_advect_two_source` Metal kernel does the parity-gated displace (reading A's flow from
  an RG32F texture) + the reinject composite in one pass, matching
  `fluid_advect_two_source_frame_cpu`. Exposed via `--backend metal` on both
  `render-fluid-advect-two-source-sequence` and `render-optical-flow-advect-sequence` (the
  single-source case reuses the same per-frame core). The CLI runs kernel + CPU per frame and
  errors past `METAL_CPU_PARITY_EPSILON`. Parity test holds at 1/255 on device; end-to-end on
  testsrc2/mandelbrot, both effects render with Metal-vs-CPU output **0.000/255** (byte-
  identical after PNG quantization). Workspace 332 → 334 (+2). See [[faux-fluid-advect]].

- **Field particles — opt-in live colour (`--live-colour`).** Particle colours were frozen at
  seed time, so video didn't play through. Now each particle can re-sample its **origin cell**
  from the current source frame every frame (the `fluid_mosaic` live-refresh semantics): the
  video's colour at the particle's birthpoint is carried to wherever it flowed — positions (the
  flow) untouched, only the carried colour updates. New `refresh_field_particle_colors` +
  `live_color` setting (serde-default false); each `Particle` now stores its home cell; algo id
  v1 → v2 (byte-identical to v1 when colours aren't refreshed). **Continuity** (unit-tested):
  refresh against the seed frame is a no-op; a changed frame updates the colour. **Off-vs-on:**
  static source ON==OFF byte-identical (0.000 — the no-op identity), moving testsrc2 frame 0
  ON==OFF 0.000, ON-vs-OFF grows 15 → 24.6 → 17/255 tracking the playing video; Read-confirmed
  identical flow with the video's colours played through (positions unchanged, palette updated).
  Workspace 330 → 332 (+2). See [[faux-fluid-advect]].

- **Single-source optical-flow-driven advection — NEW effect (CLI).** The video advected by
  its OWN motion: each frame the source's Lucas-Kanade flow (between consecutive frames)
  advects the held dye, then a fraction of the current frame is reinjected. The **self-driven
  case of the two-source advection** (source is both modulator and carrier), so it reuses
  `fluid_advect_two_source_frame_cpu` — no new render-crate logic, just an ergonomic
  single-source CLI command `render-optical-flow-advect-sequence` (CPU). Distinct from the
  procedural-vortex `render-fluid-advect-sequence`: the field is the source's *real* motion.
  **Off-vs-on** (moving testsrc2): OFF (reinject 1) == source verbatim (0.000), frame 0
  ON==OFF 0.000, ON-vs-OFF *tracks the source's actual motion* (23.9 f3 → 30.7 f7 → 21.2
  f12 — non-monotonic, ebbing with real motion, unlike the procedural field's steady
  accumulation); Read-confirmed the picture smears where content moves while static regions
  stay intact. No new tests (the per-frame core is the unit-tested two-source path); workspace
  stays 330. Metal deferred (would reuse `flow_displace_metal` + reinject, like two-source).
  See [[faux-fluid-advect]].

- **Discrete-carrier particle advection — NEW single-source effect (`field_particles.rs`,
  CPU + CLI).** The third "what rides the field?" option (after the continuous dye and the
  force-driven tile mosaic): a grid of coloured particles seeded from the source rides the
  shared steady-vortex field — **no cohesion/repulsion, just flow**. Each frame the particle
  positions integrate the field (forward Euler, clamped to canvas) and splat as
  `particle_size` squares onto black, in fixed index order (last writer wins = deterministic).
  New `initialize_field_particles`/`advance_field_particles`/`render_field_particles` +
  `ParticleField` state, id `field_particles_vortex_cpu_v1`, CLI
  `render-field-particles-sequence` (CPU). Stateful: frame 0 = initial grid (the checkpoint);
  colours fixed at seed time (live-refresh deferred). **Continuity identities** (unit-tested):
  `advect 0` holds the grid byte-identical; frame 0 is independent of advect; deterministic.
  **Off-vs-on** (testsrc2): frame 0 ON==OFF 0.000, OFF temporal delta 0.000 (static grid), ON
  temporal 20.5/255, ON-vs-OFF grows 77 → 98/255. **Tuning:** vortex scale must match canvas
  size — scale 0.008 (~125px vortices) is for real footage; a 128px fixture needs ~0.03 so
  several vortices fit and particles swirl rather than sweep to the edges (the steady-field
  void trap). Read-confirmed the swirl look at scale 0.03. Workspace 325 → 330 (+5). Metal
  deferred (a scatter splat — less natural than the gather kernels). See [[faux-fluid-advect]].

- **Two-source A→B faux-fluid advection — NEW mutual effect (CPU + CLI).** The cross-synth
  model (A reshapes B): Source A's optical-flow motion advects Source B's colour as a
  continuous dye. The first *mutual* two-source version of the fluid look. Each frame, A's
  Lucas-Kanade flow (between consecutive A frames, sized to B) advects the held dye via the
  already parity-gated `flow_displace_cpu`, then a fraction of the current B frame is
  reinjected (the "frame refresh"). New `fluid_advect_two_source_frame_cpu` +
  `FluidAdvectTwoSourceSettings { advect, reinject }`, id `fluid_advect_two_source_cpu_v1`,
  CLI `render-fluid-advect-two-source-sequence` (CPU; bounded by the shorter clip — no
  cyclic wrap, so the flow never jumps a clip boundary). **Continuity identities** (unit-
  tested): frame 0 = B verbatim; reinject 1 = B verbatim; advect 0 + reinject 0 = hold
  previous. **Off-vs-on** (testsrc2 A / mandelbrot B): OFF (reinject 1) == B verbatim
  (0.000), frame 0 == 0.000, ON-vs-OFF grows 0 → 14.5/255 (f6) → 18.1/255 (f11) as A's
  motion accumulates into B's dye; Read-confirmed the fractal smears into directional
  streaks along A's motion. Workspace 320 → 325 (+5). **Metal deferred** (would reuse
  `flow_displace_metal` + a CPU reinject composite, the datamosh pattern). See
  [[faux-fluid-advect]].

- **Faux-fluid dye advection — parity-gated Metal port (`fluid_advect.metal` + CLI
  `--backend metal`).** The first deferred Metal step. A new `fluid_advect` compute kernel
  reproduces the steady curl-noise vortex field in MSL — splitmix64 `ulong` hashing + 3D
  gradient (Perlin) noise, on the proven `coagulated_composite` precedent — plus the
  semi-Lagrangian gather with manual bilinear from `advect_feedback`. `time =
  frame_index · turbulence_speed` is computed CPU-side and the seed split lo/hi so the GPU
  matches bit-for-bit. **Parity holds to ~2e-6** vs `fluid_advect_frame_cpu` (integer
  hashing exact, float math with fast-math disabled), gated at **1e-5** (the flow-feedback
  manual-bilinear precedent, well under 1/255). `render-fluid-advect-sequence --backend
  metal` runs the GPU kernel + CPU reference per frame and errors past
  `METAL_CPU_PARITY_EPSILON`. End-to-end on a 96×72 testsrc2 clip: CPU-vs-Metal output
  0.000–0.001/255 (byte-identical after PNG quantization), temporal delta 27.1/255 (the dye
  flows). **The fluid colour-sort mosaic stays CPU** — a 1923-line sequential particle sim
  whose parallel reduction order would not hold byte-parity (decided with the user).
  Workspace 317 → 320 (+3). See [[faux-fluid-advect]].

- **Fluid Colour-Sort Mosaic — steady-vortex flow mode (Slice 9, CPU + CLI).** The
  perfected faux-fluid vortex field, now driving the mosaic *tiles* as a new opt-in flow
  (the user asked to "add as a new mode" — analytic fluid + value-noise turbulence stay).
  Extracted the steady-vortex field into a shared `vortex_field.rs`
  (`steady_vortex_velocity`); `fluid_advect` refactored to call it (verified
  byte-identical). `--vortex-flow > 0` adds that velocity to each tile so colour domains
  flow and swirl along persistent vortices; four serde-defaulted knobs; algo id v8 → v9;
  `vortex_flow 0` skips the call ⇒ byte-identical to v8. **Tuning:** a *steady, coherent*
  field advects all tiles the same way, so past ~0.4 it sweeps tiles out of their domains
  faster than cohesion refills → black voids; sweet spot ≈0.2–0.3 (domains swirl while
  staying space-filling). The mosaic is discrete tiles held by cohesion, so it can't be
  pushed as hard as the continuous dye. **Off-vs-on** (harp/cello, vortex 0.25 scale
  0.006): frame 0 byte-identical, cross-delta ≈42/255 f30 → ≈46/255 f59. New unit test.
  See [[fluid-colour-sort-mosaic]].

- **Faux-fluid dye advection — NEW effect (`fluid_advect.rs`, CPU + CLI).** A separate,
  single-source effect that ports the *Faux Fluid Sim* shadertoy **pixel** behaviour —
  built after the mosaic's turbulence knob (Slice 8) read as "≈off" because the mosaic is
  a tile/particle system and the blocky sorted look dominates. This is a **continuous
  per-pixel feedback advection**: a dye buffer is advected semi-Lagrangian-style (sample
  the previous frame at `p − v·advect` via `sample_bilinear_clamped`) along the same
  divergence-free curl-of-value-noise velocity field, and a little of the current source
  frame is bled back in each frame (`--reinject` = the "frame refresh"). The video becomes
  liquid and marbles — no tiles, no particles (Read-confirmed on harp: the figure
  dissolves into swirling dye trails). **Velocity field reworked to match the shader
  (id v1 → v2)** after feedback that it was "wobbly" vs the shader's flowing swirls:
  switched value noise → **3D gradient (Perlin) noise** (round vortices) and — the key
  fix — made the big-vortex octave **steady** (a fixed noise z-slice) so the dye flows
  along its streamlines and **spirals into the vortex centres** over frames (that
  accumulation *is* the swirl; an evolving field only wobbles in place). Only a 0.1
  `--detail` octave drifts. Defaults retuned so material wraps several times: advect 12,
  reinject 0.05 (lower = dye persists/spirals more), turbulence-speed 0.06. Frame-delta
  3.46 → 2.19 (calmer); Read-confirmed the same vortices persist across frames while dye
  spirals through them. Stateful temporal node: frame 0 = source verbatim,
  prior state = RGBA32F dye buffer (the checkpoint). CLI `render-fluid-advect-sequence`
  (single source). **Levers:** `--advect` (flow strength), `--reinject` in [0,1] (0 = pure
  smear, 1 = source verbatim, ~0.08 marble). Continuity identities unit-tested (reinject 1
  ⇒ source; advect 0 + reinject 0 ⇒ hold previous). **Readout** (harp, advect 6 reinject
  0.08): output frame 0 = source (cross-delta 0.000), source-vs-fluid ~14.5/255 steady,
  within-sequence frame-delta 3.45/255 (continuously flowing). Workspace 282 → 287 (+5
  tests). **Metal port now landed** (see top entry). Deferred variants: optical-flow-driven,
  two-source A→B, a discrete carrier (tiles/particles on this field). See
  [[faux-fluid-advect]].

- **Fluid Colour-Sort Mosaic — faux-fluid turbulence (Slice 8, CPU + CLI).** Ported the
  *faux-fluid* shadertoy look. The analytic fluid field is a regular swirl lattice; with
  `--turbulence > 0` a curl-of-value-noise streamfunction is **added** to it, giving the
  flow organic, evolving, multi-scale currents. Two octaves of value noise built on the
  existing splitmix `hash01` (GPU-safe — the reference shaders' own `sin()`-hashing is
  flagged accuracy-dependent), lattices drifting in different directions so the field
  *evolves* not just translates; the velocity is the analytic curl `(∂/∂y, -∂/∂x)` by
  central finite difference, **divergence-free by construction** (so the tuned force
  balance is preserved), normalized by `--turbulence-scale` so amplitude reads in pixels.
  Shares the dispersion band's `fluid_gain`. Three serde-defaulted settings, algo id
  bumped `…_v7` → `…_v8`; `turbulence 0` early-returns a zero contribution ⇒ off path
  **byte-identical** to v7. **Tuning finding:** amplitude is in the same pixel units as
  `fluid_strength` (≈0.5) — sweet spot ≈**0.6** (coherent domains, organic currents);
  overdriving (≈6) reproduces the boil-to-confetti failure mode *globally* (what the
  dispersion band does locally). **Off-vs-on readout** (harp→cello, `--turbulence 0.6`):
  frame 0 byte-identical (turbulence is advance-time only), cross-delta ≈23/255 by frame 5
  → ≈41/255 by frame 59; Read confirms coherent marbling along irregular currents, not
  boil. Workspace 281 → 282. New unit test (turbulence perturbs both tiles, off is
  byte-identical regardless of scale/speed, deterministic). See
  [[fluid-colour-sort-mosaic]]. Metal port deferred.

- **Fluid Colour-Sort Mosaic — spatially-varying dispersion band (Slice 7, CPU + CLI).**
  The destabilizing forces made spatially *local*: `--dispersion-band > 0` adds a
  soft-edged vertical band whose centre sweeps across the canvas
  (`--band-width`/`--band-speed`/`--band-start`) and amplifies each in-band tile's
  jitter + fluid advection during the per-frame **advance** — so colour domains boil
  apart into scattered confetti where the wipe sits while the rest stays coherent. This
  is the effect's documented failure-mode #3 ("high fluid + jitter → boil to gas")
  turned into a spatially-confined, opt-in glitch-wipe. `dispersion_band_weight` is a
  pure smoothstep-falloff function of `(x, frame)` with a toroidal wrap so the sweep
  loops; the modulation lives entirely in `advance_fluid_mosaic` (the warmup **settle is
  untouched**, so behind the sweep cohesion re-gathers the scattered tiles —
  disperse-then-re-form). Four serde-defaulted settings, algo id bumped `…_v6` → `…_v7`;
  `dispersion_band 0` multiplies the fluid gain by exactly 1.0 and adds 0 jitter ⇒ the
  off path is byte-identical. **Off-vs-on readout** (harp→cello, band 6, width 0.3,
  sweep 0.02/frame): **frame 0 byte-identical** (band is advance-time only), cross-delta
  growing monotonically from 0 (≈16/255 by frame 11, ≈50/255 by frame 47) as the wipe
  scatters tiles; Read confirms shatter localized to the band + its trailing wake,
  coherent domains ahead of it. Workspace 280 → 281. New unit test (2-tile state: the
  in-band tile moves ≥1.5× farther with the band on, the out-of-band tile byte-identical,
  off-path geometry-independent, deterministic). See [[fluid-colour-sort-mosaic]]. Metal
  port deferred.

- **Fluid Colour-Sort Mosaic — cluster-blob layout (Slice 6, CPU + CLI).** The
  deferred alternate layout. `--cluster-blob` swaps the cohesion *target*: each tile is
  pulled toward its colour bin's **global** centroid (precomputed once per force pass by
  `global_bin_centroids`) instead of the local same-colour mean, so each colour gathers
  into one compact blob rather than phase-separating into screen-filling domains. This is
  the effect's documented failure-mode #1 ("global-centroid → collapse to points") turned
  into an opt-in feature; stiff repulsion still keeps a blob a disc, not a point. Setting
  `cluster_blob: bool` serde-default false, algo id bumped `…_v5` → `…_v6`; the
  local-cohesion branch is untouched so the off path is byte-identical. **Centralization
  caveat** (why it isn't the default): spatially-uniform colours share a near-identical
  centroid (the canvas centre), so blobs only separate when each colour is spatially
  concentrated. **Off-vs-on readout** (fixture: red split into two discs + a blue disc,
  cohesion amplified, fluid/jitter off): cluster-blob merges the two red discs into one
  blob at red's global centroid while local cohesion keeps them as two domains (Read
  confirmed); cross-delta **57.8/255 at frame 0** (the *settle pass* already runs the
  cluster force, so frame 0 diverges — unlike refresh/resort which are frame-0-identical)
  settling to **49.5/255**. Workspace 279 → 280. New unit test builds a 2-tile state
  directly: local force ~0 across the gap, cluster pulls both to the midpoint by exactly
  dist·cohesion. See [[fluid-colour-sort-mosaic]]. Metal port deferred.

- **Controlled Datamosh — REAL bitstream mosh, P-frame "bloom" (experimental,
  non-deterministic CLI).** The authentic codec-artifact tier — mangles the
  *compressed stream* (not decoded float frames) so the decoder itself produces the
  glitch. New standalone `datamosh-bitstream` CLI subcommand: ffmpeg encodes the
  input to a P-frame-only AVI/MPEG-4 (LGPL `mpeg4` encoder, **no GPL dep**), pure-Rust
  RIFF surgery (`crates/morphogen-media/src/avi.rs`) duplicates a chosen P-frame's
  compressed chunk `--duplicate-count` times so its motion vectors re-bloom on
  redecode, ffmpeg decodes to PNGs. **Explicit invariant carve-out** (this tier was
  always gated on one): lives OUTSIDE the deterministic render graph — **no
  RenderJobTask / queue / SwiftUI, no parity gate**; output is **not bit-reproducible**
  by design (a `datamosh_bitstream.json` sidecar records params + ffmpeg version +
  `deterministic: false`). The AVI surgery itself *is* deterministic + unit-tested on
  synthetic byte buffers (`--duplicate-count 0` = exact identity / off case). Id
  `datamosh_bitstream_pframe_dup_experimental_v1`. **Off-vs-on (look check, not a
  determinism proof):** 2s `testsrc2`, P-frame 5, count 0 (48 frames) vs 30 (78
  frames) — the duplicated frames bloom/melt (rainbow diagonal dissolves, clock digits
  smear into macroblock glitches, blocky codec decay); frame-to-frame delta 5.982 →
  4.081 /255. Workspace 272 → 279. See [[datamosh-real-vs-simulated]],
  [[datamosh-bitstream-pframe-bloom]]. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh — REAL bitstream keyframe removal (experimental,
  non-deterministic CLI).** `datamosh-bitstream --operation remove-keyframe`
  removes the controlled MPEG-4 AVI substrate's leading keyframe so ffmpeg decodes
  from prediction data rather than a clean I-frame — the transition/void mosh
  follow-up. It reuses the pure-Rust RIFF rebuild path, patches `movi`, `idx1`,
  `avih.dwTotalFrames`, and video `strh.dwLength`, writes `operation:
  remove_keyframe` plus algorithm id
  `datamosh_bitstream_remove_keyframe_experimental_v1` in the sidecar, and stays
  outside queue/SwiftUI/parity for the same codec-version carve-out as P-frame
  bloom. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh — per-block keep/drop pseudo-keyframes (full vertical
  slice).** The patchy "some macroblocks refresh, some rot" half of the aesthetic,
  completing the codec-simulated block tier. After the recursive advect, each
  macroblock whose **mean-motion magnitude** is below `--block-refresh-threshold`
  "keeps" — it snaps back to the carrier `B[i]` (an intra/I-block refresh) — while
  busier blocks are denied refresh and keep rotting. **Content-driven** like a
  codec's intra-block map (not injected noise): calm blocks refresh, busy blocks
  smear, so the trail behind a moving subject **self-erases** (calm regions snap
  back to clean `B`) leaving the smear only at the subject's current position. A
  per-block composite over the *output* of the parity-gated displace, so **Metal
  came free again** (the Metal refresh path renders, per-frame gate passing); a
  refreshed block also **clears its residual accumulator** (intra-block reset).
  `--block-refresh-threshold` on `render-datamosh-sequence` + queue + a macOS Block
  Refresh stepper. Continuity: `threshold 0` ≡ the block/residual path
  (byte-identical); a threshold above the largest block motion ≡ a whole-frame
  keyframe (carrier verbatim, accumulator cleared); `block_size ≤ 1` ≡ bloom. New id
  `flow_reuse_datamosh_block_refresh_cpu_v1` via `datamosh_algorithm(block_size,
  residual_gain, refresh_threshold)` — **only** for blocks ≥ 2px **and** threshold >
  0 (a separate id, precedence refresh > residual > block > bloom, no id bump); job
  field `serde(default)` (=0 ≡ off). **Off-vs-on readout** (bouncing-square A over a
  static stripe+dot B, block 16, full melt): refresh off (`threshold 0`) vs on
  (`threshold 1.0`) cross-sequence delta grows **0 → 31.6/255** (frame 0 identical =
  both `B[0]`); frames Read — off = a cumulative smear everywhere the square has
  been, on = the diagonal stripes stay clean (trail self-erases) with the smear only
  at the square's current position. Workspace 265 → 272; Swift 46 → 47. See
  [[datamosh-codec-block-tier]]. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh — block-residual accumulation tier (full vertical slice).**
  The quantization-noise half of the macroblock aesthetic. Quantizing A's flow to a
  block mean discards the intra-block detail (`residual = flow − block_mean`); this
  tier accumulates it in a **per-pixel residual flow buffer** (`accum = accum·decay
  + residual`) and re-injects it (`effective = block_mean + accum·gain`) into the
  advecting flow, so macroblocks slide coherently **and** shed a trailing
  fine-motion haze. Still a **pure flow→flow transform** (`datamosh_residual_flow`),
  so the displace stays the existing parity-gated kernel and **Metal came free
  again** (no new kernel — the Metal render ran the residual path, per-frame gate
  passing). `--residual-gain` / `--residual-decay` on `render-datamosh-sequence` +
  queue + two macOS steppers. Continuity: `gain 0` short-circuits to the block path
  (byte-identical); `gain 1` first P-frame ≡ the smooth bloom (raw-flow) displace;
  `block_size ≤ 1` ≡ bloom (residual is a no-op without quantization). New id
  `flow_reuse_datamosh_block_residual_cpu_v1` via `datamosh_algorithm(block_size,
  residual_gain)` — **only** for blocks ≥ 2px **and** gain > 0 (a separate id, no
  block-id bump); job fields `serde(default)` (=0 ≡ off). **Off-vs-on readout**
  (high-motion bouncing-square A over a static stripe+dot B, block 16, full melt):
  residual off (`gain 0`) vs on (`gain 1, decay 0.9`) cross-sequence delta grows
  **0 → 33.8/255** (frame 0 identical = both `B[0]`); frames Read — the coherent
  macroblock slide gains a divergent streaky haze (stripes smear, the dot drags
  into a comet). Workspace 258 → 265; Swift 45 → 46. See
  [[datamosh-codec-block-tier]]. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh — codec-simulated ("block") tier (full vertical slice).**
  The first deferred datamosh tier: A's per-frame optical flow is **quantized to a
  coarse `block_size`×`block_size` grid** (one mean motion vector per block) before
  the recursive advection, so whole macroblocks slide coherently — the chunky
  "real datamosh" look vs the smooth per-pixel bloom. The only new pixel logic is
  `quantize_flow_to_blocks` (a pure flow→flow transform); the heavy displace is
  still the existing parity-gated kernel, so **Metal came free — no new kernel**.
  `--block-size` knob on `render-datamosh-sequence` + queue + a macOS Macroblock
  Size stepper; `block_size ≤ 1` ≡ the smooth bloom path (byte-identical), so the
  resolved algorithm id (`datamosh_algorithm`) is the new
  `flow_reuse_datamosh_block_cpu_v1` **only for blocks ≥ 2px**. Job field is
  `serde(default)` (=0 ≡ smooth) so legacy datamosh jobs keep their meaning.
  **Off-vs-on readout** (high-motion bouncing-square A over a static stripe+dot B):
  smooth (block 1) vs blocky (block 16) cross-sequence delta grows **0 → 35.9/255**
  (frame 0 identical = both `B[0]`); frames Read — block 16 melts into large
  coherent wavy warps (16px regions slide together) where block 1 shatters into
  per-pixel speckle. Workspace 250 → 258; Swift 44 → 45. See
  [[datamosh-codec-block-tier]]. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Controlled Datamosh / Motion-Vector Reuse — full vertical slice (CPU + CLI +
  Metal + queue + SwiftUI).** The roadmap's "flow-field reuse on decoded float
  frames" MVP, A→B, the deterministic flow-reuse tier (real melt/bloom, *in the
  datamosh family* but not the authentic macroblock/bitstream artifact — see
  [[datamosh-real-vs-simulated]]). A **stateful temporal node**: Source A's
  per-frame Lucas-Kanade optical flow (`A[i-1]→A[i]`, reusing
  `pyramidal_lucas_kanade_flow_cpu`) repeatedly advects Source B's *previous
  output* (the carrier is frozen from the last keyframe and smears under A's
  motion). **Recursive accumulate + keyframe refresh** (the chosen model of three):
  `out[0]=B[0]`; `is_datamosh_keyframe(i,K)` ⇒ snap back to `B[i]`, else
  `flow_displace(out[i-1], flowA[i], amount)`. `--keyframe-interval` `1` = exact B
  passthrough, `N` = pulse, `0` = full melt from B[0]; `--amount` scales the flow.
  CPU core `datamosh.rs` (`datamosh_bloom_frame_cpu`, 6 tests, algorithm id
  `flow_reuse_datamosh_bloom_cpu_v1`) delegates the advect branch to the
  parity-gated `flow_displace_cpu`. The recursion carries the previous output as
  **RGBA32F in memory** (unquantized internal state; disk checkpoint/resume
  deferred — the `write_flow_feedback_state` serializers exist to reuse).
  `render-datamosh-sequence` CLI + parity-gated `--backend metal` (reuses
  `flow_displace_metal`, gated per-frame). Persisted `frame_sequence_datamosh`
  queue job (backend serde-default CPU; queue-add/run, manifest carries
  algorithm + keyframe_interval/amount/backend) + a macOS Render-panel section
  (A/B/output pickers, keyframe-interval + amount steppers, CPU/Metal backend).
  **Off-vs-on readout** (high-motion A square over a static stripe+dot B fixture,
  `scripts/make-datamosh-fixture.py` + `scripts/dm-cross-delta.py`): interval 1 =
  **0.000/255** passthrough vs B; interval 0 melts **0 → 17.06/255** as B[0]
  accumulates A's rightward motion (frames Read — the dot stretches, stripes drag).
  **Metal nuance:** per-frame parity gate passes, but the end-to-end Metal sequence
  is **not** byte-identical to CPU (max drift **0.013/255**) because the recursion
  compounds sub-epsilon float diffs across frames — same accepted pattern as the
  recursive `flow_feedback` Metal path; Metal is byte-reproducible across runs
  (determinism-first holds per-backend). Queue add→run byte-identical to direct
  (smoke test pins it + the manifest knobs). Workspace 243 → 250; Swift 42 → 44.
  MVP feature-complete. Contract: `docs/DATAMOSH_MILESTONE.md`.

- **Video-to-Audio Descriptor Routing — HQ tier (3 vertical slices; CPU + CLI +
  queue + SwiftUI, CPU-only).** The three deferred axes of the MVP, built
  incrementally. **(1) Optical-flow descriptor** (`--descriptor flow`): per-frame
  mean Lucas-Kanade flow magnitude (motion) instead of mean luma, reusing the
  parity-gated `lucas_kanade_flow_cpu`; frame 0 = 0 (no prior frame). The gain/pan
  routes were made descriptor-neutral (`descriptor_gain_route`/`_pan_route` take
  arbitrary `(time,value)` samples); the algorithm id is composed in core
  (`video_audio_route_algorithm_id`) as `{descriptor}_{mapping}_route_cpu_v1` —
  **luma ids byte-unchanged**, flow added `flow_gain/flow_pan/flow_filter`.
  **(2) Filter target** (`--mode filter --filter-type lowpass|highpass`): the
  descriptor sweeps a one-pole cutoff on B, reusing a `one_pole_filter_sweep`
  factored out of `centroid_filter_cross_synth` (cross-synth's f64 path
  byte-unchanged). **(3) Time-resampled curves** (`--sampling hold|smooth`):
  `hold` steps (default, byte-identical to the MVP), `smooth` linearly
  interpolates between frames — centralized in `DescriptorEnvelope::resample`,
  shared by gain/pan/filter. New core enums `VideoAudioRouteDescriptor` /
  `VideoAudioRouteFilterType` / `VideoAudioRouteSampling` (all serde-defaulted to
  the MVP meaning) + task fields; manifest records descriptor/filter_type/sampling;
  Render-panel pickers (descriptor, filter-type shown in filter mode, envelope).
  **Off-vs-on readouts:** flow→gain (moving-square fixture) OFF flat 0.5, ON tracks
  motion 0.00→0.11→0.22→0.32→0.43→0.50; luma→lowpass (HF-content metric) OFF flat
  0.9999, ON 0.00 (closed) →0.92 (open); hold-vs-smooth (coarse ramp) max
  consecutive-sample jump 0.1255 (staircase) vs 0.000126 (~1000× smoother).
  Queue add→run byte-identical to direct (3 smoke tests: luma-pan/flow-gain-smooth/
  filter-highpass). Workspace 236 → 243; Swift unchanged at 42 (tests extended).
  Contract: `docs/VIDEO_AUDIO_ROUTE_MILESTONE.md`.

- **Video-to-Audio Descriptor Routing — full vertical slice (CPU + CLI + queue +
  SwiftUI; CPU-only).** The roadmap's "frame-luma controls gain or pan" MVP, the
  cross-modal mirror of Audio-to-Video routing (there A's audio shaped B's video;
  here A's *video* shapes B's *audio*). Source A's **peak-normalized per-frame
  mean Rec.709 luma** envelope (hold-last by frame time at `--fps`) drives Source
  B's WAV: **`gain`** = luma scales B's amplitude (`out = B·lerp(1,luma,amount)`,
  the shape of `rms_gain_cross_synth`); **`pan`** = luma drives an equal-power
  stereo pan of mono-mixed B (`pan=(2·luma−1)·amount`, dark→left, bright→right,
  output 2-channel). CPU-only (audio has no Metal target). The luma is computed
  by the CLI (which owns image decoding) and handed to `morphogen-audio` as raw
  `(time,luma)` samples, keeping the audio crate image-decoupled (the symmetric
  decoupling `audio_route.rs` keeps from audio). `video_route.rs` in
  morphogen-audio (`luma_gain_route` / `luma_pan_route`, 10 tests) +
  `render-video-audio-route` CLI + persisted `video_audio_route` queue job
  (core `VideoAudioRouteMode` enum serde-default Gain;
  `queue-add-/queue-run-video-audio-route` writing `audio/video_audio_route.wav`
  + a manifest carrying algorithm/mode/amount/fps) + a macOS Render-panel section
  (A frames / B WAV / output pickers, mode + amount + fps). Algorithm ids
  `luma_gain_route_cpu_v1` / `luma_pan_route_cpu_v1`. `amount 0` = byte-identical
  passthrough (mono B stays mono). **Off-vs-on readout** (8-frame dark→bright A,
  steady tone B, fps 8): gain off flat 0.354 RMS, on dark **0.035** / bright
  **0.330** (amplitude tracks A's luma ramp); pan off mono flat, on dark
  **L 0.349 / R 0.055** (left), bright **L 0.055 / R 0.349** (right). Queue add→run
  byte-identical to the direct render (smoke test pins it + the manifest knobs,
  pan mode). Workspace 223 → 236; Swift 40 → 42. MVP feature-complete. Contract:
  `docs/VIDEO_AUDIO_ROUTE_MILESTONE.md`.

- **CLI module split (behavior-preserving refactor).** The monolithic
  `crates/morphogen-cli/src/main.rs` (8127 lines) was decomposed into eight
  modules with no logic change — the `run()` dispatch body is unchanged:
  `error.rs` (CliError), `imaging.rs` (PNG/image/fingerprint leaf utils),
  `args.rs` (Cli/Commands + all `Cli*` value-enums + From impls + mode/algorithm
  helpers), `project.rs` (init/probe/extract/cache/inspect/proxy), `audio.rs`
  (cross-synth + impulse-convolution render & queue), `render.rs` (all direct
  `render_*` handlers + granular controls + provenance + feedback + shared render
  consts), `queue.rs` (queue add/run + manifests + checkpoints + bundle writers;
  depends one-directionally on render). `main.rs` is now **786 lines** (imports +
  `main` + `run` dispatch). Cross-module request structs got `pub(crate)` fields.
  Verified: cli tests 34/34, clippy clean, `cargo test --workspace` green
  (baseline unchanged). A new effect now adds its command to `args.rs`, render
  handler to `render.rs`, queue handler to `queue.rs` — bounded files, not a
  monolith.

- **Convolutional AV Blending (per-channel colour kernels + true-stereo IRs +
  large-K Metal verify) — three vertical slices.** The remaining deferred HQ
  items. **Colour kernels** (`--kernel-mode color`): a separate K×K kernel from
  each of A's R/G/B channels, applied channel-wise (chromatic structure transfer);
  parity-gated `convolution_blend_color` Metal kernel (three weight buffers), algo
  id `image_color_kernel_convolution_blend_cpu_v1`; CPU + CLI + queue + SwiftUI.
  Off-vs-on (luma vs colour, K=7): **mean 24/255, max 130**, 0 vs identical.
  **Per-channel IRs** (`--ir-mode per-channel`): each carrier channel convolved
  with its own IR from the matching A channel (cycling when counts differ),
  CPU-only, algo id `per_channel_impulse_response_convolution_blend_cpu_v1`; CPU +
  CLI + queue + SwiftUI. Off-vs-on (mono vs per-channel, stereo identity/smear IR):
  **max abs diff 0.48 (L) / 0.35 (R)**, 0 vs identical. **Large-K Metal:** the
  existing image kernel already convolves arbitrary odd K (no cap) — proved with a
  K=11 CPU + Metal parity test; a tiled perf kernel is deferred (not a correctness
  gap). Both new modes serde-default to luma/mono so existing jobs keep meaning.
  Workspace 208 → 223; Swift 38 → 40. Contract:
  `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Convolutional AV Blending (audio HQ tier: FFT method + IR resampling) —
  full vertical slice (CPU + CLI + queue + SwiftUI; CPU-only).** The two deferred
  audio items. **FFT** (`--method fft`): a new pure-Rust radix-2 Cooley-Tukey FFT
  (`morphogen-audio/src/fft.rs`, forward+inverse over f64, no new deps — the STFT
  is magnitude-only with no inverse) computes the per-channel convolution in the
  frequency domain; same transform as the direct `O(B·L)` loop, gated against it
  within `FFT_DIRECT_PARITY_EPSILON` (1e-4). **IR resampling**
  (`--resample-impulse`, opt-in): a deterministic 3-lobe Lanczos resampler maps
  A's IR to B's rate (L1 after resampling so the gain bound survives), instead of
  the default hard error on a rate mismatch. New `ConvolutionMethod` enum (audio +
  core), serde-default `method`/`resample_impulse` on the `audio_impulse_convolution`
  job, CLI flags on render/queue-add, manifest records both. Algorithm id
  unchanged (`impulse_response_convolution_blend_cpu_v1` — method is an
  implementation choice, the audio analogue of `backend`). **Off-vs-on readout:**
  FFT vs direct on a 400-tap IR/1000-sample carrier = **max abs diff 5.96e-8**
  (≪ 1e-4; identical length/RMS/peak — FFT *is* the direct path); resample off =
  hard error, on = a 24 kHz IR reconstructs the native-48 kHz IR result within
  **7.8e-6**. FFT+resample queue add→run byte-identical to the direct render
  (smoke test pins it + the manifest knobs). Workspace 198 → 208; Swift 37 → 38.
  Contract: `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Convolutional AV Blending (audio impulse) — full vertical slice (CPU + CLI +
  queue + SwiftUI; CPU-only, no Metal like the cross-synth).** The roadmap's
  "tiny direct convolution for audio kernels" MVP — the other half of
  Convolutional AV Blending. Source A is an **impulse response**: downmix to mono,
  optional `--max-impulse-samples` head-truncation, then **L1-normalize** (so
  `Σ|tap| = 1`, which bounds the wet path — no clip blow-up); a silent A falls
  back to a unit-impulse identity. Each Source B channel is convolved with that IR
  (reusing `convolve_mono`), blended wet/dry by `amount`; the output extends past
  B by `L − 1` (the reverb tail). `--amount 0` = exact B passthrough. New logic in
  `morphogen-audio/src/convolution.rs` (`impulse_convolution_blend`, 9 tests) +
  `render-audio-impulse-convolution` CLI + persisted `audio_impulse_convolution`
  queue task (add/run writing `audio/impulse_convolution.wav` + manifest knobs) +
  a macOS Render-panel section (A IR / B / output pickers, amount + max-IR
  steppers). Algorithm id `impulse_response_convolution_blend_cpu_v1`. **Off-vs-on
  readout (audio, not the image's cross-sequence trick):** a straight OFF
  (`--amount 0`) vs ON (`--amount 1`) WAV compare — ON is **longer by L − 1**
  (4800 → 5039 for a 240-tap IR) and a positive lowpass IR drops **RMS
  0.574 → 0.027** / peak 0.90 → 0.08 (L1-bounded), OFF byte-identical to B,
  deterministic re-render byte-identical, queue add→run byte-identical to the
  direct render (smoke test pins it + the manifest knobs). Workspace 186 → 198;
  Swift 34 → 37. Both MVP halves now landed. Contract:
  `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Convolutional AV Blending (image kernel) — full vertical slice (CPU + CLI +
  Metal + queue + SwiftUI).** The roadmap's "tiny direct convolution for image
  kernels" MVP, A→B and **spatial** (the first effect where A modulates B with a
  *kernel*, not a scalar). Each Source A frame is box-downsampled into a normalized
  K×K luma kernel (bright A regions = heavy taps; black A falls back to uniform);
  Source B's frame is directly convolved with it (centered, clamped border,
  correlation-style) and blended by `amount`. `--amount 0` (or `K=1`) = exact
  Source B passthrough. New `conv_blend.rs` in morphogen-render (`ConvolutionKernel`
  + `analyze_convolution_kernel_cpu` + `convolution_blend_cpu`, 7 tests) +
  parity-gated `convolution_blend` Metal kernel (new `.metal` + runtime fn +
  parity/preflight tests) + `render-convolutional-blend-sequence` CLI (A frames +
  B frames → PNG seq, `--kernel-size`/`--amount`/`--backend`) + persisted
  `frame_sequence_convolution_blend` queue job (backend serde-default CPU;
  queue-add/run writing a frames/ bundle + manifest carrying the convolution
  algorithm id + kernel_size/amount/backend) + a macOS Render-panel section
  (A/B pickers, kernel + amount steppers, CPU/Metal backend). Algorithm id
  `image_kernel_convolution_blend_cpu_v1`. **Off-vs-on readout is cross-sequence,
  not within-sequence** — a spatial blur on a static carrier is invisible to
  `frame-delta.py`; instead render `--amount 0` vs `--amount 1` (K=5) on a
  checkerboard carrier + gradient modulator and diff OFF vs ON frame 0: mean
  per-channel **91.5/255** (the 5×5 kernel collapses the Nyquist checkerboard
  toward gray — Read confirms), OFF deterministic across renders, CPU==Metal
  byte-identical, queue add→run byte-identical to the direct render (smoke test
  pins it + the manifest knobs). Workspace 173 → 186; Swift 32 → 34. MVP
  feature-complete for the image carrier. Contract:
  `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Audio-to-Video Descriptor Routing — full vertical slice (CPU + CLI + Metal +
  queue + SwiftUI).** The roadmap's "RMS controls displacement amount" MVP, A→B
  cross-modal (A's *audio* shapes B's *video*, the complement to the cross-synth's
  A-audio→B-audio). The only new logic is **routing**: A's peak-normalized RMS
  envelope, hold-last per output frame at `--fps`, becomes the scalar `amount`
  fed to the **existing, already-parity-gated** flow displace op over a uniform
  displacement field (`--shift-x/--shift-y`). `--amount 0` (or silent A) = exact
  Source B passthrough. Because the pixel transform is the proven
  `flow_displace_cpu`/`flow_displace_metal`, **Metal came nearly free** —
  `--backend metal` reuses the displace kernel, gated per-frame against CPU.
  `audio_route.rs` in morphogen-render (`RmsDisplacementEnvelope` +
  `uniform_displacement_field`, 7 tests) + `render-audio-video-route-sequence`
  CLI (WAV A + PNG-seq B → PNG seq) + persisted `frame_sequence_audio_video_route`
  queue job (backend serde-default CPU; queue-add/run writing a frames/ bundle +
  manifest carrying the routing algorithm id + every knob) + a macOS Render-panel
  section (Source A WAV / Source B frames / amount+shift steppers / CPU-Metal
  backend). Algorithm id `rms_displacement_route_cpu_v1`. Off-vs-on verified on a
  static-gradient readout: amount 0 frame-delta **0.000/255** (passthrough),
  ramped-A on **0.656/255** (displacement tracks the loud→quiet envelope),
  large-shift frame visibly displaced (Read); OFF deterministic, CPU==Metal
  byte-identical, queue add→run byte-identical to the direct render (smoke test
  pins it + the manifest knobs). Workspace 163 → 173; Swift 30 → 32. MVP
  feature-complete. Contract: `docs/AUDIO_VIDEO_ROUTE_MILESTONE.md`.

- **Spectral Audio Cross-Synthesis — full vertical slice (CPU + CLI + queue +
  SwiftUI).** The roadmap's "RMS or centroid controls a simple filter/gain path"
  MVP, A→B, **time-domain by constraint** (our STFT is magnitude-only with no
  inverse, so phase-vocoder resynthesis stays the deferred HQ tier). Two modes
  share the framing (output follows B; A's descriptor resolved by time-based
  hold-last; `amount=0` = byte-identical passthrough): **`gain`** = A's
  peak-normalized RMS envelope scales B's amplitude; **`filter`** = A's
  spectral-centroid envelope (normalized to Nyquist) sweeps a per-sample one-pole
  LP/HP cutoff on B. CPU-only (audio is not a GPU target — no Metal, nothing to
  parity-gate). `cross_synth.rs` in morphogen-audio (5 tests) +
  `render-spectral-cross-synth` CLI (WAV A + WAV B → WAV out) + persisted
  `audio_spectral_cross_synth` queue job (core enums `CrossSynthMode` /
  `CrossSynthFilterType` / `CrossSynthWindow`, all serde-defaulted;
  `queue-add-/queue-run-spectral-cross-synth` writing `audio/cross_synth.wav` +
  a manifest carrying every knob) + a macOS Render-panel section (mode/amount/
  filter-type + WAV pickers). Algorithm ids `rms_gain_cross_synth_cpu_v1` /
  `centroid_filter_cross_synth_cpu_v1`. Off-vs-on verified numerically (audio has
  no PNG): gain half-amplitude ratio **1.00 → 3.11** (output tracks A's
  loud→silent ramp); filter output centroid **5640 → 1962 Hz** (dark A lowpasses
  bright B). Queue add→run byte-identical to the direct render (both modes; smoke
  test pins it + the manifest knobs). Workspace 155 → 163; Swift 28 → 30. This
  effect is now feature-complete for the MVP. Contract:
  `docs/SPECTRAL_CROSS_SYNTH_MILESTONE.md`.

- **Video Vocoder — full vertical slice (CPU + CLI + Metal + queue + SwiftUI).**
  The roadmap's "luma-band gain routing" effect, built A→B. Two modes share the
  framing: **`match`** (default) = histogram specification (remap B's luma
  distribution onto A's via a 256-level CDF tone map — no neutral point, so it
  stays strong on real footage) and **`gain`** = per-band luma-histogram gain
  routing. Both preserve hue, clamp, and treat `amount=0` as a byte-identical
  passthrough. `render-video-vocoder[-sequence]` (CPU + parity-gated
  `--backend metal` for match), persisted `frame_sequence_video_vocoder` queue job
  (`queue-add-/queue-run-video-vocoder-sequence`, manifest carries mode/algorithm/
  bands/amount/backend), and a Render-panel section (mode/bands/amount/backend).
  **Why match over gain:** on harp→cello, gain reads as a timid grade (natural
  histograms keep `N·a_hist≈1`); match imposes A's whole tonal palette (lifts the
  dark cello frame onto harp's daylight palette) — chosen after a side-by-side
  prototype. Verified: amount=0 byte-identical (direct pixel sample); match
  off-vs-on routes correctly; Metal byte-identical to CPU on HD frames (0.0/255);
  queue add→run byte-identical to direct. Algorithm ids
  `luma_histogram_spec_vocoder_cpu_v1` (match) / `luma_band_gain_vocoder_cpu_v1`
  (gain). gain-mode Metal deferred (errors clearly). Workspace 142→155; Swift
  26→28. Contract: `docs/VIDEO_VOCODER_MILESTONE.md`.

- **Granular step 6b luma-variance + gradient texture dims (render/CLI + queue +
  SwiftUI):** the final 6b feature, landed as a full vertical slice. Each pooled
  grain now carries a 2-dim texture descriptor `[luma_variance,
  gradient_magnitude]` over its tile; `--texture-weight W` (0 = off) scales both
  dims in the per-tile nearest match, querying Source A's per-tile texture, so a
  smooth modulator region draws smooth carrier grains and a busy region draws busy
  ones. Off by default ⇒ byte-identical selection. The pool **algorithm id bumped
  v1 → v2** (descriptor schema changed), so stale v1 sidecars regenerate rather
  than read texture as zero. Plumbed through the persisted job (serde default 0),
  queue-add/run, manifest, and the Render panel (Texture Weight stepper). New
  render-crate test (texture breaks a mean-colour tie: a busy modulator query
  picks the checkerboard grain over the flat one; weight 0 leaves the tie). New
  `--readout texture` fixture mode (flat vs striped frames at equal mean colour);
  off-vs-on readout: OFF mean frame-delta **0.0/255** (colour tie pins to the flat
  grain), ON **48.0/255** with the output tracking the modulator's flat↔stripes
  texture demand (frames Read to confirm); `/parity` OK 8/8 (queue == direct,
  manifest carries `texture_weight`); smoke + Swift bridge tests pin the knob.
  Workspace 141 → 142; Swift unchanged at 26 (existing tests extended). **With
  this, granular step 6b is feature-complete — no algorithmic refinements remain.**
- **Granular step 6b spatial-origin coherence (render/CLI + queue + SwiftUI):**
  the spatial complement to frame coherence, landed as a full vertical slice.
  `--spatial-coherence-weight W` (0 = off) adds a second additive term to
  `TemporalCoherence`: a candidate grain whose origin differs from that tile's
  previous pick adds `W*min(dist_tiles,reach)/reach` to its squared feature
  distance (`dist_tiles` = Euclidean origin distance in grain-tile units, sharing
  `--coherence-reach`). Keeps a tile's pick from teleporting across the frame even
  on a nearby source frame. Off by default ⇒ byte-identical; with either coherence
  weight > 0 the scheduler engages (frame zero still a no-op). Plumbed through the
  persisted job (serde default 0), queue-add/run, manifest, and the Render panel
  (Spatial weight stepper sharing Reach). New render-crate test (spatial weight
  overturns the exact-colour grain toward the previous pick's origin; frame-zero
  no-op); `/parity` OK 4/4 with frame + spatial coherence (queue == direct);
  smoke + Swift bridge tests pin the knob. Workspace 140 → 141; Swift unchanged at
  26 (existing tests extended). With this, the last 6b algorithmic refinement
  remaining is luma-variance/gradient feature dims.
- **Granular step 6b pool-selection knobs — queue/SwiftUI exposure sweep:** the
  persisted `frame_sequence_granular_mosaic_pool` job now carries all four
  direct-render pool knobs — centroid (k=2) STFT caches, trailing pool window,
  anti-repeat (weight + cooldown), and temporal coherence (weight + reach). New
  schema fields are `#[serde(default)]` (off), so jobs serialized before this
  sweep keep their whole-clip / no-scheduler meaning.
  `queue-add-granular-mosaic-pool-sequence` gained the matching flags (same
  both-or-neither centroid validation + finite/non-negative weight checks as the
  direct path); `queue-run` threads them into the render request instead of the
  old hardcoded defaults; the bundle manifest + provenance record them. The macOS
  Render panel adds a Spectral Centroid (k=2) toggle (wires the STFT caches from
  proxy extraction, both-or-neither), a pool-window stepper, and anti-repeat /
  coherence weight+span steppers (span steppers disabled when weight = 0).
  Verified e2e: queue add→run with pool-window + anti-repeat + coherence engaged
  is byte-identical to the direct render with the same flags; extended pool queue
  smoke test asserts the knobs round-trip through task + manifest; 3 new Swift
  bridge tests pin the scheduling flags + centroid-cache args (Swift 23 → 26;
  Rust workspace unchanged at 140 — existing tests extended). With this, the last
  deferred 6b follow-on is closed; only spatial-origin coherence + luma-variance/
  gradient feature dims remain noted as algorithmic refinements.
- **Granular step 6b cross-frame scheduling — temporal coherence (render/CLI
  path):** the smooth-motion complement to anti-repeat. `--coherence-weight W`
  (0 = off) + `--coherence-reach R` (default 8) reward source-frame continuity:
  a candidate grain whose source frame differs from that **same tile's** previous
  pick by `delta` adds `W*min(delta,R)/R` to its squared feature distance (0 when
  unchanged, saturating at `W` once `delta>=R`). State is `prev_selection:
  Vec<Option<u32>>` (one global grain index per output tile) — serializable
  checkpoint rep. Frame zero has an empty history ⇒ byte-identical to
  non-scheduled (declared frame-zero behavior); composes additively with
  anti-repeat; Metal path unaffected (CPU-side selection). New render-crate test
  (coherence overturns color-nearest toward the previous pick's frame; frame-zero
  no-op). Verified e2e on solid-gray footage (rearrangement=1.0 ⇒ output color
  reveals source frame): alternating modulator → off jumps f0↔f3 every frame,
  on (W=5, R=1) holds f0 after an identical frame 0. Workspace 139 → 140.
  Queue/SwiftUI exposure deferred. Spatial-origin coherence deferred.
- **Granular step 6b cross-frame scheduling — anti-repeat (render/CLI path):**
  `--anti-repeat-weight W` (0 = off) + `--anti-repeat-cooldown C` (default 8)
  penalize grains used in recent output frames (penalty `W*(C-age)/C`, linear
  decay) to push temporal diversity. State is `last_used_frame: Vec<Option<u32>>`
  (serializable checkpoint rep). Frame zero has empty history ⇒ byte-identical to
  non-scheduled (declared frame-zero behavior); penalty reshapes only the
  nearest-match distance, Metal path unaffected (CPU-side selection). New
  render-crate test (penalty overturns color-nearest; frame-zero no-op). Verified
  e2e on a colorful carrier + static modulator: off = 1 distinct output frame,
  on = 3 distinct, frame 0 identical / frames 1–3 diverge. Render 53 → 54
  (workspace 139). Queue/SwiftUI exposure deferred.
- **Granular step 6b sliding-window pool scope (render/CLI path):**
  `--pool-window N` bounds each output frame to a trailing window of the last `N`
  carrier frames (`0` = whole-clip). Grains are frame-major, so a trailing window
  is a contiguous global-index slice — `PoolSelectionWindow::Trailing` is a
  selection-only filter (whole-clip sidecar stays reusable; Metal render path
  unaffected; `WholeClip` byte-identical to prior behavior). New render-crate test
  pins window membership. Verified e2e: `--pool-window 1` forces each output frame
  onto its own carrier frame (red→green→blue→white) vs the static whole-clip
  mosaic. Render tests 52 → 53 (workspace 138). Queue/SwiftUI exposure deferred.
- **Granular step 6b k>1 audio dims (render/CLI path):**
  `render-granular-mosaic-pool-sequence` accepts optional
  `--modulator-centroid-cache` / `--carrier-centroid-cache` (STFT caches)
  alongside RMS. The audio vector is `[rms?, centroid?]` (each descriptor
  independently both-or-neither across modulator/carrier), k=0..=2; one
  `audio_weight` scales every dim. CPU core was already k-generic; the Metal
  kernel is untouched (audio drives only CPU-side selection). New render-crate
  test proves a centroid dim flips selection vs RMS-only. Verified end-to-end: on
  a 4-frame solid-color carrier + constant-amplitude chirp (flat RMS, rising
  centroid), k=1 vs k=2 give different mosaics (k=1 frame0 mean greenish, k=2
  pulled to blue/white = higher-centroid frames). Render tests 51 → 52
  (workspace 137). Queue/SwiftUI centroid exposure deferred.
- **Granular step 6b Metal backend in queue + SwiftUI:** the persisted
  `frame_sequence_granular_mosaic_pool` job gained a `backend` field (serde
  default CPU). `queue-add-granular-mosaic-pool-sequence --backend metal` is
  parity-gated frame-by-frame in the run path and the manifest records the
  backend; the macOS Render panel exposes a CPU/Metal segmented selector for the
  pool job. Verified end-to-end: a Metal-backed queue run on generated 48×48
  footage rendered 4 frames (per-frame parity gate passed) with `backend: Metal`
  in the manifest. Swift tests 22 → 23; Rust workspace 136 (unchanged count).
- **Granular step 6b Metal render port (temporal grain pool):** a
  `granular_mosaic_pool` compute kernel renders the cross-frame pooled mosaic on
  the GPU — the whole-clip pool uploads as a 2D texture array (slice per frame),
  a flat grain-metadata buffer resolves each global pool index to
  `(frame_index, origin_x, origin_y)`, integer-nearest clamped sampling +
  `rearrangement` value-blend. `granular_mosaic_pool_metal` is parity-gated by a
  multi-frame runtime test; `render-granular-mosaic-pool-sequence --backend metal`
  gates every frame against the CPU reference before export (queue runs stay CPU).
  Verified on generated footage: Metal output byte-identical to CPU (PSNR inf,
  4 frames). Metal tests 11 → 13. SwiftUI/queue exposure of the Metal backend deferred.
- **Granular step 6b SwiftUI exposure (temporal grain pool):** the macOS Render
  panel gains a `Granular Mosaic — Temporal Pool` section (grain size,
  rearrangement, variation, seed, audio weight, Audio-Weighted RMS toggle). The
  dev bridge shells out to `queue-add-/queue-run-granular-mosaic-pool-sequence`;
  the toggle wires the RMS caches from source-proxy extraction (both-or-neither,
  color-only when off). 3 new bridge arg tests (Swift 19 → 22).
- **Granular step 6b queue task (temporal grain pool):** persisted
  `frame_sequence_granular_mosaic_pool` `RenderJob` variant +
  `queue-add-/queue-run-granular-mosaic-pool-sequence`. Writes a ProRes-ready
  bundle (frames + pool sidecar + `frame_sequence_granular_mosaic_pool` manifest
  carrying the pooled algorithm id, `audio_weight`, and RMS-cache provenance).
  Verified: queue add→run on real footage; queued frames are byte-identical to
  the direct render (determinism across the queue path). SwiftUI + Metal deferred.
- **Granular step 6b CLI wiring (temporal grain pool):** new
  `render-granular-mosaic-pool-sequence` subcommand renders the joint-AV pooled
  path end-to-end. `--audio-weight`, optional `--modulator-rms-cache` /
  `--carrier-rms-cache` (both-or-neither, RMS k=1), and a `grain_pool_descriptors.json`
  sidecar keyed on the whole carrier set. On real footage (harp→cello):
  audio-weighted vs audio-off selection differs in ~26% of pixels. CPU-only.
- **Granular step 6b CPU core (temporal grain pool, joint-AV selection):**
  `pooled_av_nearest_grain_cpu_v1`. Grains are drawn from across time (whole-clip
  pool); each carries its frame's carrier-audio descriptor, so audio is finally a
  real matching dimension. `analyze_grain_pool_cpu` / `select_grains_from_pool_cpu`
  (combined `[mean_color | audio]` weighted NN, scalar `audio_weight`) /
  `granular_mosaic_with_pool_selection_cpu` (rearrangement = cross-frame value
  blend). See milestone step 6b.
- **Granular step 6 (selection slice):** multimodal nearest-neighbor grain
  selection on mean RGB (`multimodal_nearest_grain_cpu_v1`), opt-in via
  `--selection rgb` on the direct, sequence, and queue CLI paths; persisted on
  granular queue jobs + provenance; new `grain_color_descriptors.json` sidecar.
  Selection is CPU-side so the Metal render path + parity gate are untouched.
  Verified end-to-end: rgb vs luma give different coherent mosaics; sidecars
  tagged correctly; algorithm-mismatch recompute works.
- (prior) Source A audio descriptors routed into granular-mosaic controls
  (RMS→variation, onset→rearrangement, centroid→grain-size).

## Current direction

The EFFECTS_ROADMAP MVPs are landed. The active direction is making the
strongest experimental render paths easier to run from the app and tightening
visual verification for destructive looks. The fluid/advection SwiftUI exposure
is implemented in the local worktree; commit/push is still pending unless a later
session records otherwise.

Controlled Datamosh / Motion-Vector Reuse is feature-complete for the
deterministic render graph: recursive flow-reuse bloom, codec-simulated
macroblocks, residual haze, per-block refresh, vector remix, reusable Source A
flow sidecars, disk resume, curated presets, parity-gated Metal, queue, and
SwiftUI are all landed. The real bitstream `datamosh-bitstream` path has P-frame
bloom, leading-keyframe removal, and motion-transfer as experimental
non-deterministic CLI carve-outs. Remaining datamosh work is intentionally narrow:
true codec-motion-vector remix (FFglitch-class tooling or pure-Rust MPEG-4 MV
inspection) and optional stateless motion-transfer if a user need appears.

## Candidate next steps

1. **Datamosh true-MV research spike.** Decide whether to integrate an external
   FFglitch-class helper or inspect MPEG-4 motion vectors in Rust; keep it outside
   the deterministic render graph until reproducibility is proven.
2. **Visual regression/contact-sheet command hardening.** Promote the existing
   script path into a stable CLI command only if destructive-look review becomes a
   regular workflow.
3. **Stateless motion-transfer variant.** Add `out[i] = warp(B[i], flowA[i])`
   only if the recursive melt is too destructive for a specific use case.
4. **Lower priority.** Multiscale structure-preserving morph Metal/queue/SwiftUI
   exposure remains deferred because manual testing found it visually marginal on
   real footage.

## Known truths to respect

- Single-scale `--structure-mix` is the keeper for "beyond recognition" feedback;
  multiscale is correct-but-marginal. `--feedback-mix` is the dissolve cliff.
- Every new Metal kernel must parity-gate against the CPU reference before export.
