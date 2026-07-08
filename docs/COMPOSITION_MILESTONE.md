# Composition Timeline — Milestone Contract (the flagship binder)

Status: **BUILT — S1–S7 landed 2026-07-05** (`cdeab6a`…`52b0c61`, one slice per
commit; cargo 546 → 559, swift 117 → 118, clippy clean per slice). Contract
written 2026-07-04 at the user's request ("plan something spectacular… a new
feature or something that binds features together"). The spec below is the
original contract; the build's accepted deviations and the open follow-ups
from the post-build review are in **§Post-build review (2026-07-06)** at the
end — read that section before extending this feature.

## Why this is the flagship

Every render command today executes **one job**: one effect (or one chain) over
one clip with one settings document. The app has ~20 proven deterministic
effects, a modulation matrix, LFOs, envelopes-in-flight, chains, a preview
loop — and no way to say *"play these in sequence as a piece."* Composing a
finished work currently means rendering jobs by hand and splicing them in an
external editor, which throws away determinism, the manifest trail, and the
checkpoint story at exactly the moment the user is doing their most valuable
work.

A **Composition** is an ordered list of **Scenes** on a global timeline, each
scene a full render job (an effect chain + sources + modulation routes), joined
by deterministic **Transitions**, optionally scored by a **master clock**
(one composition-level audio/video modulator that every scene can route from,
window-offset to its position on the timeline). One spec file in, one
bit-reproducible frame sequence + WAV out, one manifest.

It binds, multiplicatively, what already exists:

- **Chains** become scene bodies — a scene *is* a chain spec (§Spec). Every
  stage the chain engine gains, compositions gain for free.
- **The modulation matrix + LFOs + envelopes** become *scoring* tools: the
  master clock routes one audio track's RMS/onsets across every scene with
  automatic time offsets — the piece is literally driven by its soundtrack.
- **The preview loop** becomes the scene-iteration surface (`--scene` renders
  one scene; preview it as today).
- **Checkpoint/resume + queue** semantics carry over per scene; scene-level
  caching (§Mechanic) makes editing scene 3 of 6 re-render *only* scene 3.
- **Determinism composes for free**: every stage is already deterministic and
  the assembly step is pure arithmetic, so identical spec + sources ⇒
  bit-reproducible output. No invariant is bent.

Relationship to the Tier 5.7 patcher canvas
([`DEFERRED_WORK_HANDOFF.md`](DEFERRED_WORK_HANDOFF.md)): the canvas is
*vertical* composition (signal routing inside one instant); this is
*horizontal* composition (arrangement across time). This one is buildable now
because it reuses the chain spec verbatim; the canvas remains a horizon item.

## Composition spec (the deterministic input)

One JSON document, validated in full before anything renders (the chain-spec
precedent — `{"version": 1, ...}`, `deny_unknown_fields` everywhere):

```jsonc
{
  "version": 1,
  "fps": 12,                      // global; scenes must agree (refuse otherwise)
  "master": {                     // OPTIONAL composition-level modulator media
    "audio": "score.wav",         //   analyzed ONCE into shared sidecars
    "source_a": "master_a/"      //   (video modulator, same idea)
  },
  "scenes": [
    {
      "name": "opening",          // unique; becomes scene_<NN>_<name>/
      "duration_frames": 96,      // pre-overlap length (see Transitions)
      "input_dir": "harp/",      // stage-1 input frames for this scene
      "chain": { "version": 1, "stages": [ /* verbatim render-chain spec */ ] },
      "transition_out": { "type": "crossfade", "frames": 12 }   // to next scene
    },
    { "name": "storm", "duration_frames": 120, "input_dir": "cello/",
      "chain": { ... } }          // last scene: no transition_out
  ]
}
```

Decisions declared up front:

- **A scene body IS a `render-chain` spec document, verbatim.** No parallel
  effect vocabulary. v1 scenes are therefore limited to the stages
  `render-chain` supports (retro_static, channel_shift, palette_quantize,
  rutt_etra, flow_feedback, + whatever has landed since) — this coupling is
  the point: chain growth is composition growth. Do not fork the spec type.
- **`transition_out` types (v1): `cut` and `crossfade`.** `crossfade.frames: 0`
  ≡ `cut` (the off-case). "Morph" transitions (interpolating knobs between
  scenes) are explicitly deferred — different chains have incompatible knob
  spaces; don't attempt it in this milestone.
- **Overlap model:** a crossfade of N frames blends the *last N frames of
  scene k* with the *first N frames of scene k+1*. Composition length =
  Σ duration_frames − Σ transition frames. Refuse if any transition is longer
  than either adjacent scene.
- **Master routes** use a `master:` prefix in the existing route grammar
  (e.g. `displacement_depth = master:audio-rms @smooth`), resolved against the
  composition-level sidecars with the scene's global start frame as the window
  offset. Scenes may still use their own per-scene modulator media exactly as
  chains do today; `master:` is additive surface, not a replacement.
- **fps/dims must match across scenes and master media** →
  `RenderError::IncompatibleInputs` (the two-source precedent). Per-scene
  resolutions are deferred.

## Mechanic

1. **Validate** the whole spec (every scene's chain spec included) before any
   frame renders — the chain precedent.
2. **Render each scene** into `<out>/scene_<NN>_<name>/` via the *existing*
   chain execution path (same code, not a mirror). A stateful stage checkpoints
   inside the scene directory exactly as it does in a standalone chain.
3. **Scene fingerprint + cache:** each scene directory records a fingerprint =
   hash of (resolved scene spec incl. chain stages and routes, source
   fingerprints, fps, dims, master-media fingerprints *if the scene routes from
   master*, the scene's global start frame *iff master-routed — offsets change
   the signal*). On re-run into the same output dir: matching fingerprint ⇒
   skip the scene entirely; mismatch ⇒ re-render that scene only; a *changed
   spec name/order* refuses rather than guessing (chain refusal semantics).
4. **Assemble**: walk the timeline, copying scene frames into `<out>/frames/`
   with global numbering; inside a crossfade window blend per-pixel in f32 on
   the decoded RGBA with weight `w = (i+1)/(N+1)` for blend frame i (declared
   exactly so it is testable), round half away from zero back to 8-bit. Audio
   (if any scene chain emits WAVs): sample-accurate concat, **equal-power**
   crossfade (`cos`/`sin` quarter-period gains) over the same wall-clock
   window, mixed in f32, requantized once. Mismatched sample rates refuse.
5. **Write `composition-manifest.json`**: global timeline (scene name, start
   frame, length, transition), each scene's chain manifest content or path,
   fingerprints, master sidecar ids. The whole piece is reproducible from
   manifest + sources.

## Off / identity anchors (the falsifiable base cases)

- **A1 (same-engine):** a composition of ONE scene with no transitions is
  byte-identical, frame for frame, to `render-chain` run directly on that
  scene's chain spec + input. This is the invariant that keeps compositions a
  *view over* the engine, never a second engine.
- **A2 (cut ≡ concat):** with all transitions `cut`, `<out>/frames/` is
  byte-identical to the concatenation (renumbering only) of the per-scene
  renders.
- **A3 (crossfade off-case):** `crossfade.frames: 0` produces byte-identical
  output to `cut`.
- **A4 (cache identity):** re-running an unchanged spec re-renders nothing and
  re-assembles byte-identical output; changing only scene k's settings leaves
  every other scene's frames byte-identical (assert on bytes, not mtimes).
- **A5 (master ≡ local at offset 0):** a one-scene composition whose route is
  `master:audio-rms` over `score.wav` is byte-identical to the same scene
  routed per-scene to the same file. Offsets: a scene starting at global frame
  F sampling `master:` must equal a per-scene route over the same media
  trimmed by F frames (unit-level equality on the sampled envelope).

## Acceptance criteria

1. All five anchors hold as automated tests (A5's offset half may be
   unit-level on the envelope sampler rather than a full render).
2. Determinism: rendering the same composition twice into fresh dirs is
   byte-identical across every frame and WAV (extend the smoke-test pattern in
   `crates/morphogen-cli/tests/smoke.rs`).
3. A real two-scene composition (e.g. rutt_etra scene → flow_feedback scene,
   12-frame crossfade) renders end to end; **Read the frames** at the boundary
   window and report `frame-delta.py` across it — the delta must ramp through
   the crossfade, not step (the number half of the proof; the Read is the
   look half).
4. Stateful resume: kill a run mid-scene-2, re-run, and get byte-identical
   output to an uninterrupted run (the chain stage-marker precedent).
5. Refusals are exercised: fps mismatch, transition longer than a scene,
   changed spec into an existing output dir, mismatched WAV rates.
6. No `unwrap()` outside tests; errors via `thiserror`; baseline test counts
   captured before the first change and the delta reported per slice.

## Build plan (slices — /checkpoint after each)

- **S1 — spec + validation + single-scene passthrough.** Types (core or a
  CLI-local mirror, whichever the chain spec chose — follow that precedent
  exactly), full-document validation, `render-composition <spec> <out>`
  delegating one scene to the existing chain path. Anchor A1.
- **S2 — multi-scene cut assembly + manifest.** Global renumbering, WAV
  concat, `composition-manifest.json`. Anchor A2.
- **S3 — crossfade transitions.** Video f32 lerp + equal-power audio, exact
  formulas above. Anchors A3 + acceptance 3 (the visual/number proof).
- **S4 — scene fingerprint cache + resume/refusal.** Anchor A4, acceptance 4–5.
- **S5 — master clock.** Composition-level sidecar generation (reuse the
  existing analysis/sidecar machinery — ids, fingerprints, sampling
  convention), `master:` route prefix, per-scene window offsets. Anchor A5.
- **S6 — queue pair.** `QueueAddComposition`/`QueueRunComposition` persisting
  the resolved spec document (the chain queue precedent — resolved document,
  not typed mirrors); add→run byte-identical to direct.
- **S7 (user-gated — ask first) — SwiftUI composition panel + per-scene
  preview.** `--scene <name>` single-scene render lands earlier (S1) as the
  CLI iteration path; the panel and preview-loop wiring wait for the user's
  go, alongside the chain-builder panel decision (Tier 2.1).

Estimated shape: S1–S2 small (mostly plumbing over existing paths), S3–S5
medium, S6 small. Each slice is independently shippable and independently
verifiable — do not start S3 before A1/A2 are green.

## Deferred (explicitly out of this milestone)

- Morph/knob-interpolation transitions between scenes.
- Per-scene resolutions or fps (refuse for now).
- Nested compositions / composition-as-scene.
- Audio-only scenes; generative (source-less) scenes gated on Tier 5.2 landing
  in chains first.
- Any patcher-canvas UI (Tier 5.7 stays a horizon item).

## Post-build review (2026-07-06) — deviations + follow-ups

Independent review of the landed S1–S7 (`crates/morphogen-cli/src/composition.rs`,
13 composition smoke tests, queue pair, SwiftUI runner panel). Per-slice test
deltas in the commit messages are internally consistent (546 → 559); the four
platform-independent crates re-verified green (385/0) — the CLI smoke suite and
Metal parity gates compile only on macOS.

### Accepted deviations (the contract above is superseded on these points)

- **`master:` prefix → reserved named modulator `master.<source>`.** The `:`
  collides with the route grammar's `:scale` separator, so master routes use
  the named-modulator dot spelling (`displacement_depth = master.audio-rms`).
  The offset is implemented by *trimming* the master WAV to the scene's global
  start frame, which makes anchor A5 exact by construction. Only engine touch:
  the additive `ChainStage::inject_named_modulator_media` hook.
- **Audio assembly (WAV concat + equal-power crossfade) deferred, not built.**
  No chain stage emits WAVs yet, so there is no producer to blend. The
  mechanic-step-4 formulas stand and activate with the first audio-emitting
  scene type (e.g. audiovisual granular grains landing in chains).
- **Video master deferred.** `master.luma`/`master.flow` (a `source_a` master)
  refuses with a clear message; audio master only in v1.
- **S7 shipped as a spec-file *runner* panel, not a scene-authoring UI.** The
  user gate was about authoring UX (it depends on the still-open chain-builder
  decision); the runner adds no authoring surface — pick a spec JSON + output
  dir, queue-add→run, preview the assembled frames. The chain-builder panel
  question remains open and user-gated.
- **Acceptance-5 refusal list partly moot by design:** scenes have no
  per-scene fps (global-only spec field), so there is no fps mismatch to
  refuse; the WAV-rate refusal waits on audio scenes. Exercised instead:
  overlapping-transition bounds, duration/render mismatch, structural change
  into an existing dir, master misuse ×3, spec-grammar rejections.

### Follow-ups (ordered; none block using the feature)

- **F1 — cross-scene dims validation (correctness gap). DONE (2026-07-07).**
  Dimensions were checked only inside `crossfade_frame`, so a **cut-only**
  composition of scenes with different dimensions assembled a mixed-dimension
  `frames/` silently — downstream ProRes/preview broke late instead of the
  render refusing early. Fixed: pass 1 now reads each scene's first final frame
  and refuses a dims mismatch against the first scene, which establishes the
  composition's dimensions
  (`render_composition_refuses_cross_scene_dimension_mismatch`).
- **F2 — `--scene <name>` single-scene render. DONE (2026-07-07).** Promised in
  S1 as the CLI iteration path and never built. `render-composition <spec> <out>
  --scene <name>` renders that one scene into its `scene_NN_name` directory with
  its master binding at the scene's composition timeline offset, and skips
  timeline assembly (no `frames/`, no `composition-manifest.json`). The offset is
  summed from the *declared* lengths of the earlier scenes (owned length =
  `duration_frames` − outgoing crossfade), so no earlier scene is rendered —
  exact because the full loop pins each rendered length to its `duration_frames`.
  The per-scene render/reuse body (master bind → fingerprint-before-render →
  chain run → length check → F1 dims) was extracted to a shared
  `render_composition_scene` helper so the full loop and `--scene` can't drift;
  the record keeps the full skeleton so a later full run reuses the scene
  rendered here. Tests: `render_composition_single_scene_matches_full_composition_scene`
  (byte-identical to the scene inside the full piece, no prior scene rendered, no
  assembly) and `render_composition_single_scene_rejects_unknown_name`.
- **F3 — mid-scene resume on first runs. DONE (2026-07-07).** A scene's
  fingerprint was persisted only *after* the scene completed, so a first run
  killed mid-scene re-ran with recorded `""` ≠ computed → the partial scene dir
  was cleared and the scene restarted from frame 0. Output stayed byte-identical
  (everything is deterministic) but the chain's stage markers and any stateful
  checkpoint were discarded — the "chain stage-marker precedent" of acceptance 4
  was honoured only for completed-then-lost scenes (what the smoke test
  simulates). Fixed: the computed fingerprint is now persisted to
  `composition-record.json` *before* the scene renders (the clear-on-mismatch
  guard already runs first, off the prior record's value). `chain-manifest.json`
  presence stays the completeness gate, so reuse semantics are unchanged and the
  same-fingerprint-but-incomplete branch (dir left intact → chain resumes) is
  now reachable for real interruptions. Regression test
  `render_composition_persists_fingerprint_before_rendering` provokes a
  post-fingerprint failure and asserts the record already carries the scene's
  fingerprint.
- **F4 — master-clock fps alignment guard. DONE (2026-07-08).** The trim
  offset assumes the scene's modulation timeline runs at the composition fps;
  a master-routed stage whose envelope fps differs (stateless default 12, or
  flow_feedback's pinned frame rate) read the master off-time, silently.
  Fixed: `ChainStage::effective_envelope_fps()` (single source of truth,
  shared with `modulation_args` so the guard and the render can't drift) +
  a per-stage check in `validate_scene_chain` — a `master.` route on a stage
  whose effective envelope fps ≠ the composition fps refuses at validation
  with the alignment remedy, before anything is written. Fires on the direct,
  `--scene`, and queue-add paths (all validate through
  `validate_composition_spec`). Test
  `render_composition_master_route_refuses_misaligned_envelope_fps`:
  24-fps composition × default-12 stateless stage refuses; stage
  `modulation.fps: 24` renders; feedback stage (pinned 12) under 24 refuses.
  Per-stage trimming stays deferred (the "or, later" clause).
- **F5 — acceptance 3 on real footage.** The ramp-not-step proof ran on the
  warm|cool synthetic fixture (cut = one 116.1 boundary spike; 6-frame
  crossfade = ~16.5 across 7 pairs). The contract's real two-scene piece
  (rutt_etra scene → flow_feedback scene, 12-frame crossfade on the cello/harp
  clips) is still owed — needs macOS + the gitignored clips; Read the boundary
  frames and report `frame-delta.py` across the window.
- **F6 — unchanged deferrals:** morph transitions, per-scene resolutions/fps,
  nested compositions, audio-only/generative scenes, video master (above).
