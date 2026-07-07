# Performance Capture Milestone — play the preview, keep the render

Tier 5.5 of `docs/DEFERRED_WORK_HANDOFF.md`. Contract written 2026-07-07.
Scope confirmed with the user 2026-07-07: **MVP only** (one-knob capture),
landing on the **Rutt-Etra slots** (the LFO opt-in precedent). Multi-knob
capture and overdub/re-record are explicitly deferred until the user has
played with the MVP.

## Origin & Goal

The app's central tension: preview = play (improvised, hands-on); export =
deterministic. This milestone dissolves it. While the preview loop plays, the
user scrubs a capture control; the app records the gesture as `(time, value)`
breakpoints; the offline render replays it via the **existing**
`breakpoints(t:v;...)` modulation source (Tier 1.7) — bit-exact, forever.

**Key design call (deviation from the handoff's sketch, decided here):** the
handoff imagined a "curve file" + `curve(path)` route. Tier 1.7 actually
landed as *inline* `breakpoints(...)` specs in the route grammar — so capture
emits the inline spec directly. **Zero Rust changes.** No file format, no new
grammar, no media fingerprinting (the knots live in the route spec, which
already joins queue persistence and stateful checkpoint contracts verbatim).
A `curve(path)` file loader is only worth adding if captured specs grow
unwieldy; revisit then, not now.

## Non-goals (MVP)

- Multi-knob capture, overdub, re-record layering (user-deferred).
- Capture on any panel other than Rutt-Etra.
- Editing a captured curve (delete + re-record is the story).
- Any Rust/CLI change. The render half already exists.

## Mechanic

### The recorder (pure model, the `previewFrameIndex` testability precedent)

New pure Swift type `GestureRecorder` (Models/): accumulates `(t, v)` samples
and emits knots. All rules pinned by unit tests:

- `t` = **timeline seconds from frame 0 of the loop** = the preview player's
  elapsed play time since the take started. Hitting Record (re)starts playback
  at frame 0, so `t == 0` is frame 0 by construction — the same origin the
  offline render's `frame / fps` uses.
- The take ends at `min(user stop, one loop duration)` — one pass, no wrap
  (a wrapped second pass would scramble knot ordering).
- `v` is the capture control's value, already normalized to `[0, 1]`
  (the slot's scale/offset map it to knob units, exactly like every other
  source). Clamp on ingest; reject non-finite.
- **Decimation:** append a sample only when `|v - lastRecorded.v| >= 0.005`
  or it is the first/final sample of the take (a held-still knob yields 2
  knots, not hundreds). The final sample is always recorded so the take's end
  value holds (breakpoints clamp after the last knot).
- Empty take (no samples) ⇒ no route change; a one-sample take is legal
  (constant, matches breakpoints single-knot semantics).

### Spec emission

`capturedSourceSpec(points) -> String?` (free function beside
`lfoSourceSpec`): formats `breakpoints(t:v;t:v;...)` with knots sorted
ascending by `t` (recorder order is already ascending; sort anyway, the
parser's contract). Number formatting uses the house `cliNumber` convention.
Returns nil for an empty take. Pinned by tests: exact spec text from a fixed
sample list; clamping; decimation behaviour through the recorder.

### UI (SwiftUI, Rutt-Etra opt-in — the LFO precedent)

- `ModulationSlotRow` on the Rutt-Etra panel gains a **Captured** source
  option (only where opted in — other panels filter it out exactly as they
  filter `.lfo`). A slot with source Captured and no take shows "Record a
  take in the preview band"; run is refused with a status message (the
  missing-media precedent).
- The Workflow preview band gains a **capture strip**, visible when a
  Rutt-Etra slot is armed (source == Captured): a horizontal [0, 1] slider +
  Record/Stop button + a knot-count/duration label after a take.
- Record: restarts preview playback from frame 0, samples the slider through
  the recorder on each change (timestamped from the player's elapsed time),
  auto-stops at one loop duration.
- The take is stored on AppState per armed slot target; re-recording replaces
  it (delete + re-record IS the MVP edit story).

### Bridge

`modulationRoutes` gains a parallel `slotCaptures: [String?]` (the `slotLfos`
churn-avoider precedent — defaulted empty so the other panels' call sites are
untouched): a slot with source Captured takes its pre-formatted
`breakpoints(...)` spec as the source clause, no media, no modulator name
(the LFO branch's shape exactly). Media-flag guards need no change — the
source string never matches the media-source names (the LFO ZERO-changes
finding).

## Anchors (falsifiable)

1. **Spec identity:** the bridge-emitted route for a recorded take is
   **string-identical** to a hand-written `breakpoints(...)` route of the
   same knots (Swift test) — therefore the offline render byte-matches the
   hand-written-curve render *by construction* (same CLI arguments).
2. **Recorder determinism:** the same synthetic sample stream through
   `GestureRecorder` twice ⇒ identical knots (pure model test).
3. **No-regression shape:** with no armed capture slot, every bridge arg
   array is byte-identical to pre-slice (pinned by existing tests remaining
   green + one explicit no-capture test).
4. **End-to-end:** queue-add → run with a captured route on the gradient
   fixture renders; manifest records the breakpoints route; within-off 0.000
   vs within-on nonzero (frame-delta), frames Read-confirmed raked where the
   gesture was high.

## Acceptance criteria

- `swift build` clean; `swift test` green (baseline 123 → report delta; new
  tests: recorder decimation/clamp/end-sample rules, spec emission text,
  bridge token sequence incl. no-media, refusal when armed with no take,
  no-capture byte-identity).
- `cargo test --workspace` untouched (598) — re-run to prove it.
- End-to-end anchor 4 evidence: manifest excerpt, frame-delta number, frames
  Read.

## Slices

Single slice (S1). Later (user-gated): multi-knob arming, overdub, capture on
other panels, a curve editor.
