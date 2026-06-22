# Video Vocoder Milestone

## Goal

The visual analog of an audio vocoder: impose **Source A's tonal envelope** onto
**Source B**. Source A's per-frame luminance distribution is measured as a gain
per luma band; Source B is then decomposed by luma band and each band's
contribution is reweighted by the matching A-derived gain. Source B stays the
material at every output pixel; A only decides how its tonal bands are emphasized
or suppressed.

This is the roadmap's "luma-band gain routing" MVP for the Video Vocoder
(`docs/EFFECTS_ROADMAP.md`). It is a **stateless, per-frame** effect — no
cross-frame state, no checkpoint representation needed.

## First Render Contract

For each output frame:

1. **Modulator envelope (Source A).** Partition normalized luma `[0,1]` into `N`
   equal bands; band `b` covers `[b/N, (b+1)/N)`. Compute Source A's luma
   histogram over the frame: `a_hist[b]` = fraction of A's pixels whose luma
   falls in band `b` (∑ `a_hist[b]` = 1). A's luma is the Rec. 601 luma already
   used by the granular path. A may be any resolution; every A pixel contributes
   once (no resampling needed for a histogram).
2. **Per-band gain.** `gain[b] = lerp(1.0, N * a_hist[b], amount)`. The `N *`
   normalization makes a **flat/uniform A** yield `gain[b] = 1` for all `b`
   (neutral), so `amount` blends from identity (`0`) to full routing (`1`). A
   band where A has more mass than uniform is boosted (`>1`); less mass is
   attenuated (`<1`).
3. **Apply to carrier (Source B), soft membership.** For each output pixel, take
   the carrier pixel's luma `L`, look up a **continuous** gain `g(L)` by linearly
   interpolating between the two nearest band *centers* (band `b` center =
   `(b + 0.5)/N`; clamp at the end bands), then scale the carrier RGB:
   `out_rgb = clamp(carrier_rgb * g(L), 0, 1)`. Scaling RGB uniformly preserves
   hue and only reshapes brightness. Soft membership keeps the output continuous
   in luma (no posterization).

- Output dimensions follow Source B.
- Source B sampling is the pixel grid directly (no warp); A is only histogrammed.
- `amount = 0` ⇒ output is exactly Source B (byte-identical passthrough).
- Uniform A at `amount = 1` ⇒ all gains 1 ⇒ output ≈ Source B (within rounding).
- Given identical inputs and settings, output is deterministic.

## Initial Scope

- CPU reference renderer and focused synthetic tests, **then** a parity-gated
  Metal kernel (CPU is ground truth; `METAL_CPU_PARITY_EPSILON`).
- `render-video-vocoder` for still images (paired A/B PNG).
- `render-video-vocoder-sequence` for paired PNG frame directories.
- Per-frame JSON sidecar for the Source A luma-band histogram (the modulator
  envelope), carrying algorithm id, band count `N`, dimensions, sampling
  convention, and Source A fingerprint. Reuse requires all of these to match.
  Source B needs no descriptor sidecar (the carrier is consumed directly).
- `frame_sequence_video_vocoder` persisted queue task, writing a ProRes-ready
  image-sequence bundle with timing, source, and histogram-sidecar provenance.
- Metal kernel gated frame-by-frame against the CPU reference before export.
- Parameters: band count `N` (`--bands`, default 8) and `amount`
  (`--amount`, default 1.0; `0` = passthrough).
- macOS Render panel exposure (a `Video Vocoder` section: bands + amount).

## Algorithm Identifiers

- `luma_band_gain_vocoder_cpu_v1` — the CPU reference selection/render id, and the
  histogram-sidecar algorithm id. Distinct from every granular/flow id. Changing
  `N`, the luma convention, or the binning invalidates stale sidecars.

## Acceptance Criteria

1. **Passthrough identity.** `amount = 0` ⇒ output byte-identical to Source B.
2. **Neutral flat modulator.** A uniform-luma A at `amount = 1` ⇒ output equal to
   Source B within ±1/255 (gains all ≈ 1).
3. **Directional routing.** An A concentrated in highlights boosts Source B's
   bright bands and attenuates its dark bands (and the inverse for a shadow-heavy
   A) — visible in a Read frame and measurable by `frame-delta.py` against the
   passthrough.
4. **Soft continuity.** No hard luma steps in the output (gain is `C0`-continuous
   in luma); a smooth carrier ramp stays smooth.
5. **Determinism.** Identical A, B, `N`, `amount` ⇒ identical output; sidecar
   reuse keyed on fingerprint + `N` + algorithm id; a mismatch regenerates.
6. **Metal parity.** The Metal kernel matches the CPU reference within
   `METAL_CPU_PARITY_EPSILON`, gated per frame before export.

## Verification (off-vs-on)

Per the project workflow, tests + parity prove determinism but not that the knob
does what it claims. Render the same job **off** (`--amount 0`) vs **on**
(`--amount 1`) on a fixture whose A is tonally skewed (e.g. a highlight-heavy A
over a full-range B gradient), Read frames from both, and report the
`frame-delta.py` number. A look without a number is unfalsifiable; a number
without the pixels proves nothing.

## Deferred (not this slice)

- Hard/nearest-band membership (the stepped/posterized vocoder aesthetic).
- Spatial-frequency bands (true multiband pyramid) — the roadmap's high-quality
  version; this slice is tonal luma bands only.
- Audio-spectrum-driven bands (A's STFT routing B's tonal bands) — that overlaps
  the Spectral Audio Cross-Synthesis path; keep luma-derived here.
- Per-band chroma/tint, non-uniform band widths, and gain smoothing across frames.
