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

This first slice does not use Metal, audio descriptors, a render-queue task, or SwiftUI controls. Those additions must preserve this CPU contract rather than revise it implicitly.

## Next Steps

1. Route Source A RMS, onset, and spectral descriptors into time-addressed grain controls.
2. Add multimodal nearest-neighbor grain selection and audiovisual grain scheduling.
