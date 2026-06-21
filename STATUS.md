# Status

Session-resume checkpoint. Update at the end of any working session so a fresh
session (or a fresh agent) can pick up in seconds. Keep it short; durable detail
lives in `docs/`, cross-session findings live in `/memory/`.

_Last updated: 2026-06-21_

## Baseline (verified)

- `cargo test --workspace`: **136 passing across 7 crates, 0 failing.**
  One benign warning (`block v0.1.6` transitive dep, future-Rust deprecation).
- `swift test`: **22 passing, 0 failing** (Swift shell + service tests).
- Tree clean as of the granular step-6b Metal-port commits. Manual-testing clips
  (`cello.mp4`, `cello2.mp4`, `harp.mp4`) are gitignored, not tracked.

## What just landed

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

On branch `granular-6b-deferred-features`: working through the deferred 6b items.
Done: Metal backend in queue + SwiftUI; k>1 audio dims on the render/CLI path
(both above). Next in this branch: sliding-window pool scope, then cross-frame
scheduling — both have design forks to confirm before building. Also deferred:
queue/SwiftUI exposure of the centroid (k=2) caches.

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
