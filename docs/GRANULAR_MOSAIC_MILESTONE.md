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
3. Done (6b CPU core + CLI render path + queue task + SwiftUI exposure): temporal grain pool / joint-AV selection — see Step 6b below. Per-grain carrier audio is now a real matching dimension, rendered by `render-granular-mosaic-pool-sequence`, the persisted `frame_sequence_granular_mosaic_pool` queue job, and the macOS Render panel. A Metal render port is the remaining 6b increment; cross-frame scheduling stays deferred.

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

Deferred (explicitly not this slice): luma-variance and gradient/edge feature
dimensions; per-grain carrier-audio descriptors as real matching dimensions (the
"audiovisual grains selected by descriptor similarity" endgame, a step-6b
follow-on); and any cross-frame scheduling — anti-repeat diversity or temporal
coherence. Those introduce cross-frame state and are out of the selection-only
scope.

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

- New algorithm id `pooled_av_nearest_grain_cpu_v1`, distinct from the luma and
  multimodal ids. The differing id invalidates stale single-frame sidecars.
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

Deferred: a Metal render port (selection stays CPU-side, but the cross-frame
render samples multiple frames, so the GPU port is its own task); k>1 audio dims
(add spectral centroid); sliding-window pool scope; luma-variance/gradient
feature dims; and cross-frame scheduling (anti-repeat / temporal coherence).
