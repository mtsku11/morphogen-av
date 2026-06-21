# Status

Session-resume checkpoint. Update at the end of any working session so a fresh
session (or a fresh agent) can pick up in seconds. Keep it short; durable detail
lives in `docs/`, cross-session findings live in `/memory/`.

_Last updated: 2026-06-21_

## Baseline (verified)

- `cargo test --workspace`: **140 passing across 7 crates, 0 failing.**
  One benign warning (`block v0.1.6` transitive dep, future-Rust deprecation).
- `swift test`: **22 passing, 0 failing** (Swift shell + service tests).
- Tree clean as of the granular step-6b Metal-port commits. Manual-testing clips
  (`cello.mp4`, `cello2.mp4`, `harp.mp4`) are gitignored, not tracked.

## What just landed

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

## In flight

On `main`. The `granular-6b-deferred-features` branch (Metal backend in
queue+SwiftUI, k>1 audio dims, trailing pool window, anti-repeat) is merged
(PR #2). Temporal-coherence scheduling now also landed on the render/CLI path
(see "What just landed") — local commit, not yet pushed. With coherence in, the
remaining cross-frame scheduler is built; the one outstanding deferred follow-on
is a single queue/SwiftUI **exposure sweep** that plumbs all the direct-render
pool knobs (centroid caches, pool window, anti-repeat, temporal coherence)
through the persisted job + Render panel together. Spatial-origin coherence is a
deferred algorithmic refinement.

## Candidate next steps

From `docs/BACKLOG.md` "Next" and `docs/EFFECTS_ROADMAP.md`:

1. **Granular step 6b remaining** — CPU core + CLI render path + pool sidecar +
   queue task + SwiftUI exposure + Metal render port (`--backend metal`,
   parity-gated) + Metal backend in queue/SwiftUI all landed. Deferred within 6b:
   k>1 audio dims (add centroid), sliding-window pool scope, and cross-frame
   scheduling (anti-repeat / temporal coherence).
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
