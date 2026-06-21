# Status

Session-resume checkpoint. Update at the end of any working session so a fresh
session (or a fresh agent) can pick up in seconds. Keep it short; durable detail
lives in `docs/`, cross-session findings live in `/memory/`.

_Last updated: 2026-06-21_

## Baseline (verified)

- `cargo test --workspace`: **117 passing across 7 crates, 0 failing.**
  One benign warning (`block v0.1.6` transitive dep, future-Rust deprecation).
- Tree clean as of the doc-bootstrap commit. Manual-testing clips
  (`cello.mp4`, `cello2.mp4`, `harp.mp4`) are gitignored, not tracked.

## What just landed

- Source A audio descriptors routed into granular-mosaic controls
  (RMS→variation, onset→rearrangement, centroid→grain-size). Committed.
- Doc bootstrap: `CLAUDE.md` is now the canonical agent guide; `AGENTS.md` is a
  thin pointer; full command/path catalog moved to `docs/REFERENCE.md`;
  `CODEX_TASKS.md` renamed `docs/BACKLOG.md`.

## In flight

Nothing actively in progress — clean handoff point.

## Candidate next steps (awaiting direction on which effect to build)

From `docs/BACKLOG.md` "Next" and `docs/EFFECTS_ROADMAP.md`:

1. **Granular step 6** — multimodal nearest-neighbor audiovisual grain scheduling
   (extends the now-complete luma+audio-descriptor selection).
2. **Next roadmap effect** — Video Vocoder (luma-band gain routing MVP) or
   Spectral Audio Cross-Synthesis (RMS/centroid filter path) are the natural
   next vertical slices.
3. **Deferred / low-priority** — Metal parity port for the multiscale
   structure-preserving morph, then its queue/SwiftUI exposure. Per the manual
   testing finding it's CPU-only and marginal on real footage; don't invest until
   a use case shows it mattering (see `docs/BACKLOG.md` + [[flow-feedback-levers]]).

## Known truths to respect

- Single-scale `--structure-mix` is the keeper for "beyond recognition" feedback;
  multiscale is correct-but-marginal. `--feedback-mix` is the dissolve cliff.
- Every new Metal kernel must parity-gate against the CPU reference before export.
