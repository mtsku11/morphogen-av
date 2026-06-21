# Status

Session-resume checkpoint. Update at the end of any working session so a fresh
session (or a fresh agent) can pick up in seconds. Keep it short; durable detail
lives in `docs/`, cross-session findings live in `/memory/`.

_Last updated: 2026-06-21_

## Baseline (verified)

- `cargo test --workspace`: **128 passing across 7 crates, 0 failing.**
  One benign warning (`block v0.1.6` transitive dep, future-Rust deprecation).
- Tree clean as of the granular step-6b CPU-core commit. Manual-testing clips
  (`cello.mp4`, `cello2.mp4`, `harp.mp4`) are gitignored, not tracked.

## What just landed

- **Granular step 6b CPU core (temporal grain pool, joint-AV selection):**
  `pooled_av_nearest_grain_cpu_v1`. Grains are drawn from across time (whole-clip
  pool); each carries its frame's carrier-audio descriptor, so audio is finally a
  real matching dimension. `analyze_grain_pool_cpu` / `select_grains_from_pool_cpu`
  (combined `[mean_color | audio]` weighted NN, scalar `audio_weight`) /
  `granular_mosaic_with_pool_selection_cpu` (rearrangement = cross-frame value
  blend). CPU-only; sidecar/CLI/queue wiring is the next increment. 5 new tests
  incl. one proving the audio dim breaks a color tie. See milestone step 6b.
- **Granular step 6 (selection slice):** multimodal nearest-neighbor grain
  selection on mean RGB (`multimodal_nearest_grain_cpu_v1`), opt-in via
  `--selection rgb` on the direct, sequence, and queue CLI paths; persisted on
  granular queue jobs + provenance; new `grain_color_descriptors.json` sidecar.
  Selection is CPU-side so the Metal render path + parity gate are untouched.
  Verified end-to-end: rgb vs luma give different coherent mosaics; sidecars
  tagged correctly; algorithm-mismatch recompute works.
- (prior) Source A audio descriptors routed into granular-mosaic controls
  (RMS→variation, onset→rearrangement, centroid→grain-size).

## In flight

Nothing actively in progress — clean handoff point.

## Candidate next steps

From `docs/BACKLOG.md` "Next" and `docs/EFFECTS_ROADMAP.md`:

1. **Granular step 6b wiring** — CPU core landed (temporal grain pool, joint-AV
   selection). Next: persist a pool sidecar (frames fingerprint + audio dims +
   algorithm id), wire a CLI/queue path that assembles the pool and threads
   `audio_weight` + Source A frame-time query audio, mirroring step 6's 3+4.
   Then optionally a Metal render port (selection is CPU-side, but the cross-frame
   render samples multiple frames). Deferred within 6b: sliding-window pool scope;
   cross-frame scheduling (anti-repeat / temporal coherence).
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
