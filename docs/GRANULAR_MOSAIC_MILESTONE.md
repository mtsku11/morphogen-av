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
2. Add multimodal nearest-neighbor grain selection and audiovisual grain scheduling.

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
