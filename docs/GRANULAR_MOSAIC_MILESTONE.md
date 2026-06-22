# Granular Mosaic Milestone

## Goal

Create the first non-flow visual cross-synthesis effect. Source A's local luminance selects fixed-size visual grains from Source B; Source B remains the material at every output pixel while A decides its recomposition.

## First Render Contract

For every output tile, the renderer averages Source A luminance over the corresponding normalized output area. That value selects one tile from a row-major grain grid over Source B. A seeded hash supplies an alternate selection, blended with the luma selection by `variation`. For each output pixel, the renderer samples between its original Source B coordinate and the selected grain coordinate by `rearrangement`.

- Output dimensions follow Source B.
- Source A may have a different resolution and is bilinearly sampled in output-normalized coordinates.
- Source B sampling uses clamped bilinear borders.
- `rearrangement = 0` exactly preserves Source B.
- `variation = 0` makes selection depend only on Source A luminance.
- Given identical inputs and settings, output is deterministic.

## Initial Scope

- CPU reference renderer and focused synthetic tests.
- `render-granular-mosaic` for still images.
- `render-granular-mosaic-sequence` for paired PNG frame directories.
- Per-frame JSON sidecars for Source B grain descriptors and Source A selection indexes. Reuse requires matching source fingerprints, output dimensions, grain size, variation, seed, and algorithm identifier.
- `frame_sequence_granular_mosaic` persisted queue task, writing a ProRes-ready image-sequence bundle with timing, source, and grain-cache provenance.
- Metal kernel consuming the validated tile-selection map, with every CLI Metal frame gated against the deterministic CPU reference before export.
- Parameters: grain size, rearrangement, variation, and seed.
- Cache-backed, time-addressed Source A audio routing for sequence jobs: RMS raises variation, peak-normalized onset strength raises rearrangement, and Nyquist-normalized spectral centroid offsets grain size in pixels. Each curve uses the last descriptor at or before the output frame time.

The cached audio controls alter per-frame granular settings, not the underlying luma-to-grain selection contract. Their cache paths and scales persist on queued jobs and appear in output provenance.

## Next Steps

1. Done: route Source A RMS, onset, and spectral descriptors into time-addressed grain controls backed by the existing JSON analysis sidecars.
2. Done (selection slice): multimodal nearest-neighbor grain selection on mean RGB — see Step 6 below.
3. Done (6b CPU core + CLI render path + queue task + SwiftUI exposure + Metal render port): temporal grain pool / joint-AV selection — see Step 6b below. Per-grain carrier audio is now a real matching dimension, rendered by `render-granular-mosaic-pool-sequence` (CPU and parity-gated `--backend metal`), the persisted `frame_sequence_granular_mosaic_pool` queue job, and the macOS Render panel. Cross-frame scheduling and k>1 audio dims stay deferred.

## Step 6 — Multimodal Nearest-Neighbor Selection (RGB)

Scope: **selection only.** Widen grain matching from 1-D mean luminance to an
N-D feature vector, populated in this slice with **mean RGB**. Selection stays a
deterministic, stateless per-tile function. The render path is unchanged —
multimodal selection emits the same `GrainSelection` index map the CPU and Metal
renderers already consume, so Metal parity is preserved without touching the
kernel (selection is CPU-side; the GPU only renders from indices).

Contract:

- New algorithm id `multimodal_nearest_grain_cpu_v1`, distinct from
  `luma_nearest_grain_cpu_v1`. The v1 luma path stays byte-identical and remains
  the default; multimodal is opt-in. The differing algorithm id invalidates
  stale luma descriptor/selection sidecars automatically.
- Each Source B grain carries `mean_color` = the per-channel mean (R, G, B) over
  its tile. Each output tile's query is Source A's per-channel mean RGB over the
  corresponding normalized area, using the same sampling convention as the luma
  path (output-normalized bilinear sampling of Source A).
- Selection picks the grain minimizing **weighted Euclidean distance** over the
  feature vector; ties break by ascending grain index. Weights are equal across
  channels in this slice. The distance operates over feature *slices* so audio
  dimensions can be appended later without a rewrite — the stepping stone toward
  the joint audiovisual similarity space in `EFFECTS_ROADMAP.md`.
- `variation` and `rearrangement` keep their current meaning. `variation = 0`
  makes selection depend only on the RGB feature match; `rearrangement = 0`
  exactly preserves Source B.
- A new descriptor sidecar carries `mean_color` and the multimodal algorithm id;
  reuse requires matching algorithm id, dimensions, grain size, and source
  fingerprint.
- Audio coupling is unchanged from step 5: RMS/onset/centroid still modulate the
  global per-frame settings. Audio does **not** enter the matching in this slice.
- Determinism: identical inputs + settings ⇒ identical selection and output.

Originally deferred from this slice, all since landed in step 6b: luma-variance
and gradient/edge feature dimensions (texture dims, `--texture-weight`); per-grain
carrier-audio descriptors as real matching dimensions (the "audiovisual grains
selected by descriptor similarity" endgame); and cross-frame scheduling —
anti-repeat diversity, frame coherence, and spatial-origin coherence. See the
step-6b subsections below.

## Step 6b — Temporal Grain Pool (Joint-AV Selection)

Scope (CPU core slice): make per-grain carrier audio a **real matching
dimension**. Step 6's audio dim was a no-op because all grains in one carrier
frame share that frame's timestamp, hence one audio descriptor. The fix is to
draw grains from **across time**: a temporal grain pool where each grain carries
the carrier-audio descriptor of *its own* source moment. Selection then matches a
combined `[mean_color | audio]` feature vector, and audio finally discriminates
between grains. This is the joint-AV concatenative step toward the audiovisual
similarity space in `EFFECTS_ROADMAP.md`.

Pool scope for this slice: **whole-clip** — the pool is assembled once from a
fixed set of Source B frames (the global library), independent of output frame.
A bounded sliding window is a deferred knob.

Contract:

- New algorithm id `pooled_av_nearest_grain_cpu_v2` (was `…_v1` before the texture
  dims were added to the descriptor), distinct from the luma and multimodal ids.
  The differing id invalidates stale single-frame sidecars and stale v1 pools.
  Luma and multimodal paths stay byte-identical and remain available.
- A **grain pool** is built from `F` Source B frames that share dimensions and
  grain grid. Each pool grain carries `frame_index`, `origin_x/y`, `mean_color`
  (per-channel tile mean), and an `audio` feature vector = the carrier-audio
  descriptors at that frame's source time (shared by all grains of that frame).
  Grains are globally indexed frame-major then row-major (deterministic).
- Each pool grain's audio vector must have equal length `k` across the pool;
  `k = 0` is allowed and degenerates to multimodal-over-time (color only).
- For each output tile, the query is Source A's per-tile mean color (same
  output-normalized sampling as step 6) concatenated with Source A's
  **frame-time** audio descriptor vector (one query audio vector per output
  frame). Selection minimizes **weighted Euclidean distance** over the combined
  `[color(3) | audio(k)]` vector — equal per-channel color weights and a single
  scalar `audio_weight` applied to every audio dim — with ties broken by
  ascending global grain index. `variation` blends the nearest match with a
  seeded alternate **pool** grain exactly as before.
- Output dimensions follow the current carrier frame; the output grid is that
  frame's grain grid. A selected grain may live in any pool frame.
- Render semantics: because a selected grain and the current carrier pixel live
  in **different frames**, coordinate-lerp is undefined. `rearrangement` is a
  cross-frame **value blend** in this slice: `rearrangement = 0` samples the
  current carrier at the output pixel exactly (preserves Source B);
  `rearrangement = 1` samples the selected grain's pixel from its source frame;
  in between, the two sampled colors are linearly blended. (The single-frame
  coordinate-warp paths are unchanged under their own algorithm ids.)
- Determinism: identical frames, audio, settings, `audio_weight`, and seed ⇒
  identical pool, selection, and output.

CLI (landed): `render-granular-mosaic-pool-sequence` renders the pooled path
CPU-only. `--audio-weight` scales the audio dim; `--modulator-rms-cache` and
`--carrier-rms-cache` supply the Source A query and Source B pool audio
respectively (RMS, k=1) — both-or-neither, omit for color-only matching across
time. A `grain_pool_descriptors.json` sidecar tagged with the pooled algorithm
and keyed on a whole-carrier-set fingerprint (frames + audio) is written/reused
under `--grain-cache-dir`.

Queue (landed): a persisted `frame_sequence_granular_mosaic_pool` `RenderJob`
variant with `queue-add-`/`queue-run-granular-mosaic-pool-sequence`. The run path
writes a ProRes-ready bundle (frames + pool sidecar + a
`frame_sequence_granular_mosaic_pool` manifest carrying the pooled algorithm id,
`audio_weight`, and RMS-cache provenance). Queued frames are byte-identical to
the direct render.

SwiftUI (landed): the native shell's Render panel exposes the pooled queue job
(`Granular Mosaic — Temporal Pool`) — grain size, rearrangement, variation, seed,
audio weight, and an Audio-Weighted (RMS) toggle. The dev bridge shells out to
`queue-add-`/`queue-run-granular-mosaic-pool-sequence`; the toggle wires the RMS
caches produced by source-proxy extraction (both-or-neither, color-only when off).

Metal (landed): `granular_mosaic_pool_metal` ports the cross-frame render to a
`granular_mosaic_pool` compute kernel. The whole-clip pool uploads as a 2D
texture array (one slice per frame); a flat grain-metadata buffer resolves each
global pool index to its `(frame_index, origin_x, origin_y)`. Sampling is
integer-nearest clamped and `rearrangement` value-blends carrier vs. selected
grain, matching the CPU reference within the 1/255 parity tolerance (a multi-frame
runtime parity test plus the CLI's per-frame gate). `render-granular-mosaic-pool-sequence`
accepts `--backend metal`, gating every frame against
`granular_mosaic_with_pool_selection_cpu` before export. The persisted
`frame_sequence_granular_mosaic_pool` queue job now carries a `backend` field
(`queue-add-granular-mosaic-pool-sequence --backend metal`, parity-gated per
frame in the run path; the manifest records the backend), and the macOS Render
panel exposes a CPU/Metal selector for the pool job.

k>1 audio dims (landed on the render/CLI path): `render-granular-mosaic-pool-sequence`
accepts optional `--modulator-centroid-cache` / `--carrier-centroid-cache` (STFT
caches) alongside the RMS caches. The audio feature vector is built in fixed
order `[rms?, centroid?]` (each descriptor independently both-or-neither across
modulator/carrier), so k ranges 0..=2; `audio_weight` scales every dim equally.
The CPU core was already k-generic and the Metal kernel is unaffected (audio only
drives CPU-side selection). Verified: on a 4-frame solid-color carrier with a
constant-amplitude chirp (flat RMS, rising centroid), k=1 (RMS) and k=2
(RMS+centroid) produce different mosaics — the centroid query pulls selection
toward the higher-centroid frames. (Queue/SwiftUI exposure landed later — see the
pool-selection-knob exposure paragraph below.)

Sliding-window pool scope (landed, render/CLI path): `--pool-window N` bounds each
output frame to a **trailing** window of the last `N` carrier frames (`0` =
whole-clip, the default). Because grains are stored frame-major, the trailing
window is a contiguous global-index slice, so it is a selection-only filter:
`PoolSelectionWindow::Trailing { current_frame, frames }` restricts both the
nearest match and the seeded alternate, the whole-clip pool sidecar stays
reusable, and the Metal render path is unaffected (it renders whatever index map
selection produces; `WholeClip` is byte-identical to the prior behavior).
Verified: `--pool-window 1` forces each output frame onto its own carrier frame
(red→green→blue→white on a 4-solid-color carrier) vs the static whole-clip
mosaic; a render-crate test pins the window membership.

Cross-frame scheduling — anti-repeat (landed, render/CLI path): `--anti-repeat-weight W`
(`0` = off) with `--anti-repeat-cooldown C` (default 8) penalizes grains selected
in recent output frames, pushing temporal diversity. A grain used `age` frames
ago adds `W * (C - age) / C` to its squared feature distance while `age < C`,
decaying linearly. State is `last_used_frame: Vec<Option<u32>>` (the most recent
selecting frame per global grain index) — a plain serializable buffer, the
checkpoint representation for this stateful temporal node. Frame zero has an
empty history, so it is byte-identical to the non-scheduled selection (declared
frame-zero behavior); the penalty reshapes only the nearest-match distance, not
the seeded alternate. Metal render path unaffected (selection is CPU-side).
Verified: render-crate test (penalty overturns the color-nearest grain; frame
zero is a no-op) plus e2e on a colorful carrier with a **static** modulator —
anti-repeat off yields 1 distinct output frame (max repetition), on yields 3
distinct, frame 0 identical and frames 1–3 diverge.

Cross-frame scheduling — temporal coherence (landed, render/CLI path): the
smooth-motion complement to anti-repeat. `--coherence-weight W` (`0` = off) with
`--coherence-reach R` (default 8) rewards source-frame continuity: a candidate
grain whose source frame differs from that **same tile's** previous pick by
`delta` adds `W * min(delta, R) / R` to its squared feature distance (zero when
the source frame is unchanged, saturating at `W` once `delta >= R`). Each tile's
source frame therefore drifts smoothly through the pool instead of jumping across
the clip (the dominant flicker source in a per-tile nearest-neighbour mosaic).
State is `prev_selection: Vec<Option<u32>>` — the global grain index each output
tile selected last frame, one entry per tile — a plain serializable buffer, the
checkpoint representation for this stateful temporal node. Frame zero has an
empty history, so it is byte-identical to the non-scheduled selection (declared
frame-zero behaviour); the penalty reshapes only the nearest-match distance, not
the seeded alternate, and composes additively with anti-repeat (continuity vs
diversity). Metal render path unaffected (selection is CPU-side). Verified: a
render-crate test (coherence pulls selection back to the previous pick's frame,
overturning the colour-nearest grain; frame zero is a no-op).

Cross-frame scheduling — spatial-origin coherence (landed, render/CLI + queue +
SwiftUI): the spatial complement to frame coherence. `--spatial-coherence-weight W`
(`0` = off) adds a second additive continuity term to [`TemporalCoherence`]: a
candidate grain whose origin differs from that same tile's previous pick adds
`W * min(dist_tiles, reach) / reach` to its squared feature distance, where
`dist_tiles` is the Euclidean distance between origins in grain-tile units
(`origin / grain_size`) and `reach` is the **shared** `--coherence-reach`. This
keeps a tile's pick from teleporting across the *frame* even when it stays on a
nearby source frame. Both terms default off; with either weight > 0 the scheduler
engages (frame zero still empty-history ⇒ byte-identical). The new weight is
plumbed through the persisted `frame_sequence_granular_mosaic_pool` job (serde
default 0), `queue-add-`/`queue-run`, the bundle manifest, and the macOS Render
panel (a Spatial weight stepper sharing the Reach control). Verified: a
render-crate test (spatial weight overturns the exact-colour grain toward the
previous pick's origin with frame weight 0; frame zero a no-op); `/parity` OK 4/4
with frame + spatial coherence engaged (queue == direct); the pool queue smoke
test and Swift bridge test pin the knob through task/manifest/args.

Queue/SwiftUI exposure of the pool-selection knobs (landed): the persisted
`frame_sequence_granular_mosaic_pool` job now carries the centroid (k=2) STFT
caches, trailing pool window, anti-repeat (weight + cooldown), and temporal
coherence (weight + reach). New schema fields are `#[serde(default)]` (off), so
jobs serialized before this sweep keep their whole-clip / no-scheduler meaning.
`queue-add-granular-mosaic-pool-sequence` gained the matching flags (same
both-or-neither centroid validation and finite/non-negative weight checks as the
direct path); `queue-run` threads them into the render request; the bundle
manifest and provenance record them. The macOS Render panel adds a Spectral
Centroid (k=2) toggle (wires the STFT caches from proxy extraction), a pool
window stepper, and anti-repeat / coherence weight+span steppers. Verified: a
queue add→run with pool window + anti-repeat + coherence engaged is byte-
identical to the direct render with the same flags; extended pool queue smoke
test asserts the knobs round-trip through task + manifest; three new Swift bridge
tests pin the scheduling flags and centroid-cache args.

Texture feature dims — luma-variance + gradient (landed, render/CLI + queue +
SwiftUI): each pooled grain carries a 2-dim texture descriptor `[luma_variance,
gradient_magnitude]` computed over its tile (population variance of per-pixel
luminance, and the mean of `sqrt(dx²+dy²)` of forward luma differences within the
tile). `--texture-weight W` (`0` = off) scales **both** dims in the per-tile
nearest match against Source A's per-tile texture query, so a smooth modulator
region draws smooth carrier grains and a busy region draws busy ones — a
discriminator orthogonal to mean colour and audio. Off by default ⇒ byte-identical
selection. Because the sidecar descriptor schema changed, the pool **algorithm id
bumped `pooled_av_nearest_grain_cpu_v1` → `…_v2`**: a stale v1 sidecar genuinely
lacks the texture dims and is regenerated rather than silently read as zero. The
weight is plumbed through the persisted job (serde default 0), `queue-add-`/
`queue-run`, the bundle manifest, and the macOS Render panel (a Texture Weight
stepper). Verified: a render-crate test (a busy modulator query selects the
checkerboard grain over a flat grain of equal mean colour; weight 0 leaves the
colour tie); a `--readout texture` fixture (flat vs striped carrier frames at
equal mean colour) off-vs-on — OFF mean frame-delta 0.0/255 (colour tie pins to
the flat grain), ON 48.0/255 with the output tracking the modulator's flat↔stripes
demand (frames Read to confirm); `/parity` OK 8/8 (queue == direct, manifest
carries `texture_weight`); the pool queue smoke test and Swift bridge test pin the
knob through task/manifest/args.

With this, granular step 6b is feature-complete — no algorithmic refinements
remain.
