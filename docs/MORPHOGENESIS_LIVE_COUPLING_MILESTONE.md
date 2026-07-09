# Morphogenesis Live Coupling — perpetual growth driven by every frame

Status: **PLANNED — handoff contract for an Opus-orchestrated build session.**
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

- **L-S1 — weight field + inject + erode (CPU core + direct CLI).** The
  per-frame `w` (luma|motion), injection and erosion passes with declared
  order, the three settings fields, anchors L1/L2/L3 + determinism +
  pre-milestone-checkpoint compat. Tuning pass on the cello fixture (probe
  like S3 did — render, Read, adjust, pin).
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
