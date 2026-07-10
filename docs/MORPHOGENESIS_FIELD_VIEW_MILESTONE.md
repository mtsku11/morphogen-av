# Morphogenesis Field View — the raw chemistry as a first-class output

Status: **MILESTONE COMPLETE (2026-07-10).** Sonnet build to ~95% (died on
its session limit re-running the readout), orchestrator finished + verified:
cargo 724 → **733/0**, swift 150 → **152/0**, clippy clean, zero new fmt
diffs. All anchors pinned:
`render_morphogenesis_sequence_output_view_composite_matches_omitting_the_flag`
(FV1), `…field_view_matches_debug_field_command_at_sim_scale_one` (FV2, the
shared-renderer pin), `…field_view_ignores_composite_knobs`,
`…refuses_resume_on_changed_output_view` +
`…legacy_checkpoint_without_output_view_resumes_as_composite` (FV4),
`queue_morphogenesis_field_view_matches_direct_and_records_output_view`
(FV5), swift Output-picker token tests. FV3 readout
(orchestrator-run: field view + `inject=audio-rms:0.15,0@smooth` + erode
0.03 on the cello fixture): early/late window deltas **1.891/1.079**
(ratio 0.57, no freeze); loud frame 112 = dense full-frame labyrinth,
quiet frame 120 = the lower third burned to sparse dots — the raw field
surges with the music (frames orchestrator-Read). Deliverable clip sent.

Origin: written 2026-07-10 from the user's reaction to the
live-coupling showcase: the black-and-white raw V field *is the look* — make
it a rendered output option with the full modulation surface (the composite
clip's `inject = audio-rms` surge, but in the raw field view). Follow-up to
[MORPHOGENESIS_MILESTONE.md](MORPHOGENESIS_MILESTONE.md) (S1–S4) and
[MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md](MORPHOGENESIS_LIVE_COUPLING_MILESTONE.md)
(L-S1–L-S3), both complete. Ground rules as ever.

## Design (one decision, everything else falls out)

**`--output-view <composite|field>` on `render-morphogenesis-sequence`**
(default `composite` — pre-milestone byte-identity). NOT a growth of the
`render-morphogenesis-field` debug scaffold: the sequence command already
owns the modulation plan (all eight targets incl. the live-coupling three),
the checkpoint/resume contract, the queue task, and the SwiftUI panel — an
output mode inherits every one of those for free, and there is exactly one
render path so the views cannot drift.

- **Field view output:** the V field rendered greyscale (the debug command's
  exact per-pixel mapping — extract/share the function, do not duplicate),
  **upsampled bilinearly to carrier resolution** (the composite's existing
  upsample) so output dimensions are stable regardless of `--sim-scale`.
  The debug command stays sim-res raw (its niche; unchanged).
- **Composite knobs in field view:** `pattern_mix`/`displace`/`hue`/`mode`
  do not affect field output. They stay *legal* (clamp-never-error: routes
  targeting them simply modulate an unused copy — declared harmless), and
  the manifest records them as given.
- **Contract/resume:** `output_view` joins the checkpoint contract (ride the
  composite-settings struct or a sibling field — wherever the existing
  contract equality picks it up). Frames written in one view then switching
  views MUST refuse resume (inconsistent outputs on disk). Pre-milestone
  checkpoints (no field) deserialize as composite and stay resumable —
  serde-default, pinned.
- **Queue:** the task persists `output_view` (serde-default composite ⇒
  pre-slice queue JSON parses, pinned); add-time validation unchanged;
  add→run byte-identical in field view (smoke).
- **SwiftUI:** an "Output" picker (Composite | Field) on the Morphogenesis
  panel; default-composite arg arrays byte-identical (pinned).

## Anchors (falsifiable)

- **FV1 (default identity):** `--output-view composite` (and the flag
  absent) ⇒ byte-identical to pre-milestone renders; existing tests green.
- **FV2 (shared-renderer pin):** at `--sim-scale 1`, unmodulated, identical
  knobs: sequence field-view frames **byte-identical** to
  `render-morphogenesis-field` frames (the can't-drift proof). At
  sim-scale > 1 they legitimately differ (upsample vs raw) — declared.
- **FV3 (the payoff readout):** field view + `inject=audio-rms:0.15,0@smooth`
  + erode 0.03 on the cello fixture: the raw field visibly surges with the
  music (frames Read at loud/quiet + early/late window deltas — expect the
  L-S2 readout's shape, ~0.86 ratio, now in the deliverable view).
- **FV4 (resume):** field-view interrupt+resume byte-identical; switching
  output_view on an existing dir refuses; pre-milestone checkpoint resumes
  as composite.
- **FV5 (queue):** add→run byte-identical with `--output-view field` +
  live-coupling knobs.

## Acceptance criteria

FV1–FV5 as tests/smokes; clippy clean; zero new fmt diffs (8 pre-existing
dirty files); baselines cargo 724 / swift 150 → delta reported; no
`unwrap()`. Final deliverable (orchestrator, not the agent): the 6 s
audio-muxed raw-field clip rendered through the NEW path with the audio
route — the thing the user asked for.

## Build plan

Single slice (the machinery all exists): core output branch + shared field
renderer extraction + CLI flag + contract field + queue field + SwiftUI
picker + tests + the FV3 readout.
