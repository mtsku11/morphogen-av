# Effect Chain Milestone — run a chain, not a node

**Status: Slices 1–3 landed and verified; Slice 4 queue half landed**
(`b53acc5` stateless MVP, `328b069` stateful stage + resume semantics,
`1a3284a` per-stage modulation, `cb6bc24` queue task with add→run
byte-identity — the task persists the resolved spec document rather than
typed core mirrors, a declared deviation avoiding a third knob-vocabulary
copy). **Open: the SwiftUI chain-builder panel** — deliberately deferred as
the design decision point this contract flags; the builder UX needs user
input. Datamosh as a stage type also deferred (CLI-side settings, many
knobs — admit on demand). This doc is the acceptance contract
(per the CLAUDE.md "contract first" workflow). Origin: `docs/RECOMMENDATIONS.md`
Part 2 §A — the biggest lever: every render command executes exactly one
effect, so composing today means manually feeding one render's output frames
into the next render. A `render-chain` job turns the effects *catalog* into an
*instrument*, and multiplies the value of every existing effect.

## Origin & Goal

An ordered list of effect stages: stage 1 reads the input frame directory,
each later stage reads the previous stage's output, the final stage's frames
are the chain's output, and one **chain manifest** records the ordered stage
algorithm ids + settings so the whole chain is reproducible. Determinism
composes for free — every stage is already deterministic, so identical inputs
+ chain spec ⇒ bit-reproducible output.

MVP is **CLI-only**, **stateless single-source stages only**, **CPU only**,
**no per-stage modulation**. Each is a later slice (see Slices/Deferred) —
this is deliberately the smallest vertical that produces a real chained look
(e.g. rutt-etra → palette-quantize).

`morphogen-core` already has a schema-level `NodeGraph`/`ModulationRoute`
(`graph.rs`) — the same reconcile-don't-duplicate trap as the modulation
matrix. The MVP chain spec is a **linear list, CLI-side** (the
`DatamoshSequenceSettings` precedent); promoting it into core happens at the
queue slice, and generalizing list → graph is explicitly deferred.

## Chain spec (the deterministic input)

A JSON file passed to the CLI:

```json
{
  "version": 1,
  "stages": [
    { "effect": "rutt_etra",
      "line_pitch": 8, "displacement_depth": 48.0,
      "line_thickness": 1, "mono": false },
    { "effect": "palette_quantize", "mode": "posterize", "levels": 4 }
  ]
}
```

- `stages` is a serde enum tagged by `"effect"`; unknown effect tags, unknown
  fields (`deny_unknown_fields`), and an empty stage list are clear errors.
- Knob names and defaults match the effect's queue-task/CLI spellings exactly
  (one vocabulary per knob everywhere).
- **Slice-1 stage vocabulary** (stateless, single-source, CPU): `retro_static`
  (strength, filter, real_bpp, assumed_bpp), `channel_shift` (the six constant
  shift_*), `palette_quantize` (mode, levels), `rutt_etra` (line_pitch,
  displacement_depth, line_thickness, mono). Growing the vocabulary is
  mechanical (one dispatch arm per effect) and demand-driven — the contract
  does not require more.

## Mechanic

`render-chain <spec.json> <input-frames-dir> <output-dir>`:

1. Parse + validate the **whole spec** (every stage's settings through the
   effect's own `validate()`/CLI checks) **before rendering anything** — a
   stage-3 typo must not leave stage-1 output on disk.
2. Stage `i` (1-based) renders from the previous directory into
   `<output-dir>/stage_<ii>_<effect>/` (zero-padded index, effect tag); the
   per-stage renders reuse the existing `render_*_sequence` handlers
   unchanged, so each stage still writes its own per-effect manifest where the
   effect already does.
3. After the final stage, write `<output-dir>/chain-manifest.json`: spec
   version, frame count, and the ordered stages each carrying the effect tag,
   its **algorithm id** (e.g. `rutt_etra_scanline_cpu_v1`), and the resolved
   settings. The chain manifest is the provenance record — reproducing the
   chain needs nothing else but the input frames.
4. The final stage's directory is the chain's output (print its path; no
   copying/duplication of frames).

## Off / identity anchors (the falsifiable base cases)

- **Single-stage identity:** a 1-stage chain renders frames **byte-identical**
  to the same effect's direct CLI render with the same knobs (it shares the
  same handler — pin with a byte-compare test, the add→run-parity philosophy).
- **Determinism:** running the same chain twice ⇒ byte-identical frames and
  chain manifest.
- Empty `stages`, unknown effect tag, invalid stage knobs (e.g. palette
  levels 1, rutt-etra pitch 0) ⇒ clear `CliError` before any frame renders.

## Acceptance criteria

Slice 1 — `render-chain` MVP (CPU, stateless stages):

1. Unit/integration tests: spec parse round-trip; unknown tag/unknown field/
   empty-stages/invalid-knob rejection (nothing written to the output dir);
   single-stage byte-identity vs the direct render; two-run determinism;
   2-stage chain — stage directories and chain manifest have the contracted
   names/shape (algorithm ids + settings pinned).
2. **Visual proof (the backbone):** on the gradient fixture, render the
   2-stage chain `rutt_etra → palette_quantize(posterize, levels 4)` and each
   single effect alone; Read frames from all three, report
   `frame-delta.py`/`dm-cross-delta.py` numbers showing chain ≠ either single
   effect while stage 1's directory is byte-identical to the direct rutt-etra
   render. A look without a number is unfalsifiable; a number without the
   pixels proves nothing.

Slice 2 — stateful stages: admit `flow_feedback` (and datamosh if cheap) as
stage types; a stateful stage keeps its standalone checkpoint contract scoped
inside its stage directory; chain re-run resumes/skips completed stages via
the per-stage artifacts (design the stage-complete marker here). Refusal
rules: a changed spec invalidates downstream stage outputs.

Slice 3 — per-stage modulation: stages accept the standard route/named-
modulator/sampling fields (reusing `ModulationCliArgs` semantics); LFO routes
([[docs/LFO_MODULATION_MILESTONE.md]]) need no media and are the natural
chain modulators.

Slice 4 — queue task + SwiftUI: promote the spec types into `morphogen-core`,
`queue-add/run-chain` with add-time whole-spec validation, add→run
byte-identity; a chain builder panel (design decision point — likely a simple
ordered stage list first).

## Build plan (handoff notes)

- New `crates/morphogen-cli/src/chain.rs`: spec types (serde enum over stage
  settings), validation, the stage-dispatch loop, chain-manifest writer.
- `args.rs` + `main.rs`: `RenderChain` command (spec path, input dir, output
  dir).
- Dispatch arms call the existing `render_*_sequence` request structs in
  `render.rs` — reuse, do not reimplement; if a handler demands flags the
  chain doesn't expose (backend pickers), pin the CPU/default path.
- Tests in `crates/morphogen-cli/tests/smoke.rs` beside the queue parity
  tests.

Working agreements (standing, non-negotiable):

- Baseline before touching anything: `cargo test --workspace` (**496** green
  at contract time) and `swift test` (**98** green); report deltas, not
  adjectives.
- `/checkpoint` after each verified slice (local commit, source only, never
  push). `/verify` before calling any slice done.
- Fixture rendering + frame Reads are the proof for every look claim;
  `frame-delta.py` needs matching PNG pixel formats (RGB-vs-RGBA trap —
  `dm-cross-delta.py` handles both).
- Never commit the untracked `scripts/solitaire-cascade-prototype.py` or
  `shader-port-ideas/`.
- Record non-obvious findings in `/memory/`, not in prose docs.

## Deferred (explicitly out of MVP scope)

- **Graph, not list** — branching/merging chains via the core `NodeGraph`
  typed ports; the list is the degenerate case and must stay forward-
  compatible (hence the spec `version` field).
- **Two-source stages** (A→B effects mid-chain need a second input binding —
  a graph problem, not a list problem).
- **Metal stage backends** — CPU is ground truth; per-stage backend selection
  arrives only after the chain semantics are settled.
- **Audio stages / AV chains** — the audio side chains the same way
  eventually; separate contract.
- **Preview integration** (RECOMMENDATIONS Part 2 §C).
