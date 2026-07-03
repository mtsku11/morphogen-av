# LFO Modulation Sources Milestone — internal deterministic modulators

**Status: contract — build not started.** This doc is the acceptance contract
(per the CLAUDE.md "contract first" workflow). Origin: `docs/RECOMMENDATIONS.md`
Part 2 §B ("LFOs and drawn envelopes as modulation sources") — the cheapest
"feels like a synthesizer" upgrade. Drawn breakpoint curves are **out of scope**
(a later slice); this milestone is LFOs only.

## Origin & Goal

The modulation matrix is analysis-only: every source (audio-rms/onset/centroid,
luma, flow) is extracted from modulator *media*. A real mod matrix also has
**internal** sources. An LFO is a pure function of `(frame_time, params)` —
deterministic by construction, no media, no sidecar, no fingerprint. After this
milestone, `displacement_depth=lfo(sine,0.5)` oscillates the Rutt-Etra lines
with **zero modulator files**, on every modulatable command, through the direct
CLI, the queue, and (one panel first) SwiftUI.

## Grammar

The route grammar gains one source family (everything else unchanged):

```
<target>=lfo(<shape>[,<rate_hz>[,<phase>]])[:<scale>[,<offset>]][@hold|@smooth]
```

- `<shape>` ∈ `sine | triangle | square | saw`. Unknown shape → clear parse
  error listing the available shapes.
- `<rate_hz>` — cycles per second on the envelope timeline. Default `1.0`.
  Must be finite and `> 0` (parse error otherwise; clamp-never-error governs
  envelope *values*, not spec parsing).
- `<phase>` — phase offset in **cycles** (0.25 = quarter cycle). Default `0.0`.
  Must be finite; any value legal (`fract` is applied).
- Examples: `lfo(sine)`, `lfo(square,2)`, `lfo(saw,0.5,0.25)`,
  `lfo(triangle,1):64,-32@hold`.

**Parser traps (both must be handled and unit-tested):**

1. The named-modulator split in `parse_modulation_route`
   (`modulation.rs:158`) takes the first `.` — but `lfo(sine,0.5)` contains a
   `.` inside the parens. The parser must **skip the modulator split when the
   source text starts with `lfo(`**. A named prefix on an LFO
   (`wob.lfo(sine)`) is meaningless (no media) and must be a clear parse
   error, not silently treated as a modulator name.
2. The scale/offset split (`rest.split_once(':')`) and the `@` sampling rsplit
   already compose correctly with the paren body (no `:` or `@` inside it) —
   pin this with a round-trip test on a full spec like
   `depth=lfo(saw,0.5,0.25):64,-32@smooth`.

## Semantics (the deterministic reference)

Evaluated **exactly** at each output frame's envelope time `t = index / fps`
(the same `fps` the command already uses — job `frame_rate` or
`--modulation-fps` per the established per-command rules). All math in `f64`,
cast to `f32` at the end (same-platform determinism; Mac-first).

```
p = fract(rate_hz * t + phase)        // fract(x) = x - x.floor(), so p ∈ [0,1)
sine:     0.5 - 0.5*cos(2π*p)
triangle: if p < 0.5 { 2p } else { 2 - 2p }
saw:      p
square:   if p < 0.5 { 0.0 } else { 1.0 }
```

Every shape emits **[0,1]** (the house normalized-envelope convention) and
every shape's value at `p = 0` is `0.0` (square is low-first; use `phase 0.5`
for high-first). These formulas are contract — changing any changes rendered
frames.

- **No sparse envelope, no sampling.** LFO routes bypass envelope extraction
  and `sample_envelope` entirely: `modulated_value` (the single seam shared by
  direct + queue paths) branches on the LFO source and computes
  `lfo(p)·scale + offset` directly. `@hold`/`@smooth` are accepted by the
  grammar but are **documented no-ops** on LFO routes (nothing sparse to
  sample) — pin with a test that `@hold` ≡ `@smooth` ≡ unsuffixed, byte-equal.
- **No media, no sidecar, no fingerprint.** `needs_audio()`/`needs_frames()`
  return `false`; a pure-LFO route set requires no `--modulator-*` flags at
  all. `--modulation-cache-dir` never caches LFO (pure function, nothing to
  cache). Checkpoint contracts need **no new fingerprint fields**: the LFO
  params live on the route, the route list is already in the
  feedback/datamosh sequence contract, so changing `rate_hz` on resume must
  refuse via the existing "settings changed" path — verify, don't rebuild.

## Type changes (minimum churn)

- `morphogen-render` `ModulationSource` gains
  `Lfo { shape: LfoShape, rate_hz: f32, phase: f32 }`; new `LfoShape` unit
  enum (`sine`/`triangle`/`square`/`saw`, kebab-case serde). The f32 fields
  force **dropping the `Eq` derive** on `ModulationSource` (keep `Copy`,
  `PartialEq`); nothing requires `Eq` — `EnvelopeKey` comparisons are `==` in
  a `Vec`, there are no map keys (verified at contract time). Same change on
  the `morphogen-core` mirror (`render_job.rs:1228`) + new arms in the two
  queue.rs conversion fns.
- `ModulationSource::name() -> &'static str` cannot spell an LFO. Add a
  `spec_text(&self) -> String` (or equivalent) returning the exact
  round-trippable spelling — `lfo(<shape>,<rate>,<phase>)` with f32 `Display`
  (exact round-trip, the established queue-identity mechanism) — and use it in
  `describe()` and the queue spec reconstruction
  (`modulation_specs_from_routes`). Media variants keep their current
  spellings and **pre-slice error wording must not change** (smoke tests pin
  some of it).
- Serde: the unit variants still serialize as plain strings (`"audio-rms"`),
  so pre-slice checkpoints/manifests/queue JSON are byte-identical; the LFO
  variant serializes as an object — assert its shape in a test using
  0.5/0.25-style literals (the f32 JSON round-trip trap).
- `extract_envelope` returns an empty vec for LFO without touching media
  resolution; `envelope_cache_algorithm` must not gain an LFO arm.

## Acceptance criteria

Slice 1 — engine + direct CLI (works on **all** modulatable commands for free,
since `build_modulation_plan`/`modulated_value` are shared):

1. Unit tests: each shape's value pinned at `p = 0, 0.25, 0.5, 0.75` (from the
   formulas above); phase/fract wrap (`phase 1.25` ≡ `phase 0.25`); parse
   round-trip of the full grammar incl. defaults (`lfo(sine)` → rate 1, phase
   0); parse errors (unknown shape, `rate 0`, negative rate, non-finite,
   named prefix on lfo); the sampling-suffix no-op identity; serde shape.
2. Continuity identity: `target=lfo(sine,1):0,K` byte-identical to passing
   constant `K` directly (the established `scale 0, offset K` proof, on
   rutt-etra `displacement_depth`).
3. A pure-LFO route with **no** `--modulator-*` flags renders (the
   no-media-needed point of the milestone); zero routes stays the exact
   unmodulated path (existing tests must not regress).
4. **Visual proof (the backbone):** rutt-etra on a **static** gradient carrier
   (`--variation` n/a here; static so the LFO is the only source of change),
   off (no route) vs on (`displacement_depth=lfo(sine,0.5):<big scale>`), fps
   chosen so the render spans ≥ 1 full cycle. Report within-off (must be
   0.000) and within-on `frame-delta.py` numbers, and Read frames at ~0, ~¼,
   ~½ cycle — flat → raked → flat. A look without a number is unfalsifiable; a
   number without the pixels proves nothing.

Slice 2 — queue + stateful contracts:

5. LFO routes persist on queue tasks through the existing core route field
   (new source variant only — no new task fields); add-time validation
   accepts LFO routes without demanding modulator media, and still rejects
   unknown targets before persisting. Pre-slice queue JSON byte-identical
   (unit-variant serde unchanged — assert serialized form).
6. **add→run byte-identical to the direct CLI render** (smoke test, rutt-etra
   or channel-shift with an LFO route; frames + manifest) — the spec
   reconstruction must round-trip `lfo(...)` exactly.
7. Stateful contract: a feedback (or datamosh) render with an LFO route —
   stop-after-frame + resume byte-identical to uninterrupted; resume with a
   changed `rate_hz` (or shape) **refuses** via the existing
   contract-equality path; a legacy checkpoint (no modulation block) still
   resumes.

Slice 3 — SwiftUI (one-panel vertical, the named-modulator precedent):

8. `ModulationSlotRow` gains an **opt-in** LFO capability (defaulted params so
   the other panels' call sites stay untouched and byte-identical): source
   picker gains "LFO"; when selected, a shape picker + rate/phase fields
   appear and the media pickers are not required. Wire it on the **Rutt-Etra
   panel only** (`displacement_depth` slot is the showcase); the 6-panel sweep
   is post-milestone, like named modulators.
9. Bridge emits the exact `lfo(shape,rate,phase)` route token; tests pin the
   arg shape (LFO route with no media flags; LFO + media routes coexisting;
   invalid rate rejected app-side before launch).

## Build plan (handoff notes)

- Engine: `LfoShape` + variant + parse + `spec_text` + the `modulated_value`
  branch, all in `crates/morphogen-render/src/modulation.rs`.
- CLI: `extract_envelope` LFO short-circuit in
  `crates/morphogen-cli/src/modulate.rs`; nothing else on the direct path.
- Queue: core mirror variant in `crates/morphogen-core/src/render_job.rs`,
  conversion arms + `modulation_specs_from_routes` in
  `crates/morphogen-cli/src/queue.rs`. Grep for exhaustive matches on
  `ModulationSource` — the compiler finds every site once the variant lands.
- SwiftUI: `ModulationSlotRow` in `RenderPanelView.swift`, per-slot LFO state
  on `AppState` (rutt-etra slots only), spec assembly in
  `RustBridgePlaceholder.swift` (`modulationRoutes(slots:)` path).

Working agreements (standing, non-negotiable):

- Baseline before touching anything: `cargo test --workspace` (**481** green
  at contract time) and `swift test` (**95** green); report deltas, not
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

## Deferred (explicitly out of scope)

- **Drawn breakpoint envelopes** (the other half of RECOMMENDATIONS Part 2 §B)
  — needs a curve file format + editor UI; separate contract.
- **6-panel SwiftUI sweep** of the LFO slot capability (mechanical clone of
  the named-modulator sweep, once the one-panel pattern is user-confirmed).
- **LFO-on-LFO** (rate/phase themselves modulated), one-shot envelopes,
  smoothing/slew — synthesizer depth beyond MVP.
