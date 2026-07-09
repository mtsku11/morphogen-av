# Morphogenesis Live Coupling — perpetual growth driven by every frame

Status: **IN PROGRESS — L-S1 DONE (2026-07-09, cargo 693→711/0); L-S2
(homeostat + mod targets) and L-S3 (queue + SwiftUI + showcase) pending.**
Written 2026-07-09 from the user's observation on the first showcase renders:
*"it animates for ~4 seconds, then it's static when the cells fill the
screen — is there a way to update the reaction-diffusion with each new frame?
Possible with some feedback?"* Follow-up to the completed
[MORPHOGENESIS_MILESTONE.md](MORPHOGENESIS_MILESTONE.md) (S1–S4). The
[`DEFERRED_WORK_HANDOFF.md`](DEFERRED_WORK_HANDOFF.md) ground rules apply
(baseline first, CPU-first, pixels+number verification, `/checkpoint` per
slice; Sonnet slice builds, orchestrator-verified).

## Diagnosis (why it freezes — confirmed on the showcase renders)

The field reads the footage exactly once: frame-0 seeding (S1). After that
the only live coupling is the S3 (f,k) param map — a gentle *rate* nudge, not
a source. Gray-Scott then does what Gray-Scott does: the pattern grows until
it fills the domain and settles into a quasi-static labyrinth (~4 s at coral
defaults on the cello fixture). Two missing mechanisms cause this:

1. **No per-frame source:** new footage content (the bow moving, the subject
   shifting) never injects new chemistry.
2. **No sink:** nothing ever removes V, so the domain saturates and the
   dynamics equilibrate. (Injection alone would *worsen* saturation — source
   and sink must land together.)

**Baseline (CAPTURED 2026-07-09, the falsifiable "freeze" number):** on the
showcase fixture (cello.mp4, `-ss 2.0`, 144 frames @ 24, coral,
`--sim-scale 1`), `render-morphogenesis-field` field frame-delta over the
early window (frames 0–48) = **1.636/255** vs the late window (frames
95–143) = **0.130/255** — a **12.6× collapse**. That collapse is the bug;
the acceptance criteria below must move the late-window number, and the
build reports both windows after each tuning pass.

## Mechanic (three coupled additions, all deterministic per-frame functions)

All three run inside the per-frame update, before the substeps, reading the
CURRENT B frame (already loaded for the composite). All default **off = 0**,
each with a byte-identity continuity anchor to the S1–S4 behaviour.

1. **`--inject <strength>` — the per-frame source.** Adds V where the live
   frame says so: `V += strength * w(x,y)` (then clamp), with the weight
   field chosen by **`--inject-source`**:
   - `luma` — `w = max(0, luma - seed_threshold) / (1 - seed_threshold)`
     (reuses the existing threshold knob; bright regions continuously feed
     growth).
   - `motion` — `w = |luma(frame N) - luma(frame N-1)|` per sim cell,
     clamped [0,1] (**the default when inject > 0**; growth chases movement —
     the bow, the hands — and static regions starve). Frame 0 has no prior ⇒
     w = 0 everywhere (the matte frame-zero precedent: no forward peeking,
     declared). Cheap (no optical flow); a `flow`-magnitude source is a
     listed deferral, not this milestone.
2. **`--erode <strength>` — the sink.** Where the weight field is ~zero the
   field decays: `V *= (1 - strength * (1 - w))` per frame (before substeps,
   after injection; order declared and pinned). Patterns die where the
   subject left and regrow where it arrives — a moving equilibrium instead of
   saturation. Same `w` as injection (one weight computation per frame).
3. **`--coverage-target <0..1>` — the homeostat (the "feedback" ask).**
   Global negative feedback: each frame compute mean(V); if above target,
   shift the *effective* (f,k) toward dissolution, below ⇒ toward growth,
   proportionally (`coverage_gain` internal constant, pinned). This prevents
   fill-and-freeze even under pure-luma injection on bright footage. 0 = off
   (no coverage computation on the hot path). Applied AFTER modulation routes
   and the param map (declared order: routes → param map → homeostat), so
   `feed = audio-rms` still works and the homeostat rides on top.

**Interaction with S3:** the param map and the five existing mod targets are
untouched. `inject`, `erode`, and `coverage_target` become modulation targets
six/seven/eight (clamp ranges: inject/erode [0, 1], coverage [0, 1]) and join
the checkpoint contract exactly like the S3 targets.

## Invariants & contract mechanics

- Stateful node rules unchanged: the three knobs join `MorphogenesisSettings`
  (serde-default 0 ⇒ **pre-milestone checkpoints deserialize and stay
  resumable**, pinned; any nonzero knob is a settings change ⇒ existing
  refusal fires — no new invalidation machinery needed).
- Algorithm id **stays `morphogenesis_cpu_v1`**: the all-off case is
  byte-identical by construction (pinned), and checkpoint contracts already
  fingerprint the full settings, so no stale reuse is possible (this is the
  matte/output-bit-depth precedent, not the granular-sidecar trap — there is
  no settings-blind sidecar here).
- Determinism: `w` is a pure function of B's frames; injection/erosion/
  homeostat are fixed-order per-frame passes. Two-run byte-identity extends
  the existing smoke.

## Anchors (falsifiable)

- **L1 (off identity):** `--inject 0 --erode 0 --coverage-target 0` ⇒
  byte-identical field AND composite to the pre-milestone build (pin against
  a checked-in expectation or a twin render on the old code path — in
  practice: the existing S1–S4 tests stay green and one explicit
  all-zero-vs-defaults byte compare).
- **L2 (motion tracking):** on a fixture with a moving bright bar over black
  (write it with a tiny generator or `write_texture_sequence`), with
  inject+erode on: late-window V mass must be concentrated near the bar's
  CURRENT position, not its frame-0 position (assert center-of-mass tracks
  within a tolerance).
- **L3 (no freeze):** the headline. On the cello showcase fixture with
  inject+erode at defaults-to-be-tuned: late-window (95–143) field
  frame-delta ≥ 50% of the early-window (0–48) delta, where the baseline
  build collapses toward ~0. Report all four numbers.
- **L4 (homeostat):** with coverage-target 0.3 and pure-luma injection on a
  bright carrier, mean(V) settles within ±0.1 of target over the last 48
  frames (vs saturating toward the injection ceiling without it).
- **L5 (resume):** a mid-render interrupt with all three knobs active resumes
  byte-identically; changed inject/erode/coverage refuses (contract
  equality); pre-milestone checkpoint resumes with knobs defaulted to 0.

## Acceptance criteria

1. Anchors L1–L5 as tests (L3 may live as a smoke with pinned loose bounds).
2. Baseline-first: the freeze numbers (early/late window) reported BEFORE the
   first code change, then after (the delta is the proof).
3. Showcase evidence: 144-frame field render + composite render on the cello
   fixture with tuned defaults, frames Read at t = 0 / 2 s / 4 s / 6 s — the
   pattern must be visibly *different and moving* in the last two seconds
   (the current build's frozen window), tracking the bow/subject motion.
4. Tuned defaults declared and pinned (expect inject ≈ 0.02–0.1 /
   erode ≈ 0.01–0.05 per frame — TUNE EMPIRICALLY on the real fixture, the
   S3 probe precedent; do not ship guesses).
5. No `unwrap()`; clippy clean; zero new fmt diffs (8 pre-existing dirty
   files); baselines (cargo 693 / swift 147 at contract time) → delta per
   slice.

## Build plan (slices — /checkpoint each)

- **L-S1 — weight field + inject + erode (CPU core + direct CLI). DONE
  (2026-07-09,** Sonnet build, orchestrator-verified: cargo 693 → **711/0**,
  clippy clean, zero new fmt diffs. `MorphogenesisSettings` gains `inject`,
  `erode` (both `#[serde(default)]` = `0.0`) and `inject_source`
  (`InjectSource::Luma | Motion`, `#[serde(default)]` = `Motion`); `validate()`
  requires both in `[0, 1]`. Weight field: `injection_weight_luma` (the
  declared `seed_threshold`-anchored rescale, guarded against the
  `seed_threshold == 1.0` divide-by-zero) and `injection_weight_motion`
  (`|luma(N) - luma(N-1)|`, `None` prior ⇒ all-zero, the matte frame-zero
  precedent). Passes (`apply_inject_erode`, pure on `MorphogenesisField`):
  `V += inject * w` clamped, then `V *= (1 - erode * (1 - w))` clamped —
  declared order inject → erode → substeps, gated behind `index > 0` (frame 0
  stays exactly the seed, matching the existing param-map/substep gate).
  **Checkpoint design:** the previous frame's carrier luma grid rides the
  RGBA32F checkpoint's spare `B` channel (`pack_morphogenesis_state`/
  `unpack_morphogenesis_prev_luma` in `morphogen-cli`, NOT part of the core
  `MorphogenesisField` — keeps the pure-algorithm struct/tests untouched);
  packing is a no-op (`B` stays the pre-existing hardcoded `0.0`) whenever
  `inject == 0.0 && erode == 0.0`, so anchor L1 holds at the checkpoint-byte
  level too, not just the rendered frame. Both CLI commands sample the
  carrier's CURRENT-frame luma once per frame (shared between the weight
  field and the S3 param map's `cell_luma` when both are active) and only
  when `inject`/`erode` are nonzero — the off path never reads a carrier
  frame beyond frame 0, byte-identical to the pre-milestone build by
  construction, not float coincidence. Anchors: L1 (explicit `--inject 0
  --erode 0` CLI invocation byte-identical to omitting the flags, both
  commands, frames AND checkpoint state); L2 (unit-level: a synthetic moving
  bright bar's `V` column-center-of-mass tracks the bar's CURRENT column,
  not frame 0's, with `substeps=0` isolating inject/erode from Gray-Scott's
  own diffusion); L5 (resume with inject+erode active byte-identical to an
  uninterrupted run — the prev-luma round-trip proof — plus changed
  inject/erode refuses, plus a hand-stripped pre-milestone checkpoint missing
  `inject`/`erode`/`inject_source` entirely still resumes with defaults).
  **Tuning pass** (cello showcase fixture, 144 frames @ 24 fps,
  `render-morphogenesis-field --sim-scale 1 --preset coral`, motion source):
  baseline (`inject=erode=0`) reproduced the diagnosis numbers exactly
  (early 1.636 / late 0.130 /255, confirming continuity). Swept `inject`
  0.02–0.15 × `erode` 0.01–0.05; late/early ratio rose monotonically with
  both knobs but early-window aliveness (the coral seed's own growth
  character) degraded past `inject≈0.12`. **Recommended defaults (NOT serde
  defaults — those stay `0.0`/off; these are the values documented here and
  used for the showcase/tuning table): `inject = 0.1`, `erode = 0.03`,
  `inject_source = motion`** — early 1.463 (89% of the 1.636 baseline, growth
  character intact) vs late 1.091 (**8.4× the 0.130 baseline**, ratio
  late/early = 0.746, well clear of anchor L3's `>= 0.5`). Full sweep:
  (0.02,0.01)→0.378/1.521=0.25; (0.05,0.02)→0.799/1.466=0.55;
  (0.06,0.025)→0.852/1.424=0.60; (0.08,0.02)→1.026/1.553=0.66;
  (0.08,0.03)→0.961/1.403=0.69; (0.1,0.025)→1.129/1.540=0.73;
  **(0.1,0.03)→1.091/1.463=0.75 (chosen)**; (0.1,0.05)→0.818/1.156=0.71 (early
  visibly weakened); (0.12,0.03)→1.212/1.520=0.80; (0.15,0.03)→1.371/1.611=0.85
  (both windows still rising — 0.1/0.03 chosen as the documented default to
  stay inside the contract's own expected prior range rather than chase the
  last few points of ratio). Read frames confirm the visual: on
  `render-morphogenesis-sequence` with the recommended defaults, frame 0 is
  the seed (cellist silhouette in orange); frame 48 shows a fine coral
  stipple with a distinct bright streak along the bow/arm (the motion
  weight visibly concentrating on the moving bow); frame 96 (the old build's
  frozen-labyrinth window) still shows active fine-grained stipple following
  the player's hands/bow; frame 143 — where the source footage itself CUTS
  to a wide static shot — shows the pattern has THINNED to a sparse dot
  scatter near the new frame's only motion (a few orange flecks by the
  cellist's feet on the newly-visible stage floor), proof the field is
  still reading and reacting to brand-new footage content in the render's
  final second, not replaying a saturated freeze. On the bare `V`-field view
  (`render-morphogenesis-field`), the same defaults show the maze breaking up
  into a travelling dot/fragment pattern with a visible dark "silhouette"
  blob that shifts position between frames 95 and 143 — directly, visually
  confirming L2's mechanism on real footage, not just the synthetic unit
  test.)
- **L-S2 — homeostat + mod targets.** `coverage_target` + registry entries
  for inject/erode/coverage_target (checkpoint contract join, the S3
  pattern), anchors L4/L5, `inject = audio-rms` readout (growth surging on
  the soundtrack — the payoff route; Read + numbers).
- **L-S3 — queue + SwiftUI + showcase.** The three knobs on the queue task
  (serde-default, add→run byte-identity) and panel steppers + mod slots;
  final showcase render for the user (field view + composite, 6 s, audio
  muxed — the orchestrator delivers it with SendUserFile).

## Deferred (listed so nobody scope-creeps)

- Optical-flow-magnitude inject source (LK is available but costs; luma-diff
  motion is the MVP).
- Per-channel / chroma-driven injection; A-driven (modulator-frames) spatial
  injection — that is Tier 5.4 matte territory crossed with RD; own contract.
- The composite-mode gap found in the same showcase session (luma-preserving
  tint is invisible on dark footage — needs an additive/screen mode): a
  SEPARATE small follow-up, contract it independently; do not fold it in
  here.
