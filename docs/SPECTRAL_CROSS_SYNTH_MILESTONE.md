# Spectral Audio Cross-Synthesis Milestone

## Goal

The audio analog of the Video Vocoder: impose **Source A's audio descriptor
envelope** onto **Source B's audio**. Source B stays the material you hear at
every sample; Source A only decides how B's amplitude or spectral brightness
moves over time. This is the roadmap's **"RMS or centroid controls a simple
filter/gain path"** MVP for Spectral Audio Cross-Synthesis
(`docs/EFFECTS_ROADMAP.md`).

It is a **time-domain** effect, by deliberate constraint. Our STFT
(`stft_magnitude_cache`) is **magnitude-only** — it discards phase and has no
inverse transform — so *true* spectral resynthesis (multiply B's complex
spectrum by A's envelope, invert back to audio) is unbuildable today. That
phase-vocoder cross-synthesis is the roadmap's **Future high-quality** tier and
needs a complex-spectrum + inverse-STFT + Accelerate-FFT path first. This MVP
shapes B in the time domain from A's analysis envelopes: deterministic,
sample-accurate, CPU-only (audio DSP is not a GPU target — no Metal kernel,
nothing to parity-gate).

## Modes

Two descriptor→target modes share the same A→B framing, selected by `--mode`:

- **`gain`** — **A's RMS envelope drives B's amplitude.** B speaks in A's
  loudness contour (envelope transfer / ducking). See "Mode: gain" below.
- **`filter`** — **A's spectral-centroid envelope sweeps a one-pole filter on
  B.** B takes A's brightness contour. See "Mode: filter" below.

Both follow Source B for the output's sample rate, channel count, and length;
both resolve A's descriptor by **time-based hold-last** lookup (the latest A
descriptor at or before each B sample's time — the same convention the granular
path uses, so A and B stay independent in rate and length); and both treat
`amount = 0` as an exact Source B passthrough.

## Mode: gain (`--mode gain`)

1. **Modulator envelope (Source A).** Compute A's RMS envelope
   (`rms_envelope`, `--rms-window` / `--rms-hop`). Normalize by the envelope's
   peak: `a_norm[k] = rms[k] / max(rms)` ∈ `[0,1]` (all zeros if the peak is
   ~0). A's loudest moment maps to full gain, its silence to zero.
2. **Per-sample gain.** For output sample `i` at time `t = i / sr_B`, hold-last
   `a_norm` at `t`, then `g = lerp(1.0, a_norm, amount)`. `amount = 0` ⇒ `g = 1`
   (identity); `amount = 1` ⇒ B fully follows A's envelope (silent where A is
   silent).
3. **Apply.** `out[i] = B[i] * g`, per channel.

- `amount = 0` ⇒ `g = 1.0` exactly ⇒ output byte-identical to B.
- Determinism: identical A, B, window/hop, `amount` ⇒ identical output.

## Mode: filter (`--mode filter`)

1. **Modulator envelope (Source A).** STFT A (`--fft-size` / `--stft-hop` /
   `--window`); per frame compute the spectral centroid in Hz
   (`spectral_centroid_from_magnitudes`) and normalize to A's Nyquist:
   `c_norm[k] = centroid_hz / (sr_A / 2)` ∈ `[0,1]`.
2. **Map to cutoff (Source B).** For output sample `i` at `t = i / sr_B`,
   hold-last `c_norm`, then `fc = c_norm * (sr_B / 2)` and a standard one-pole
   coefficient `alpha = 1 - exp(-2π · fc / sr_B)` (clamped to `[0,1]`). High A
   brightness ⇒ high cutoff ⇒ B passes bright; low brightness ⇒ heavy lowpass ⇒
   B darkens.
3. **One-pole filter, per channel.** `lp[i] = lp[i-1] + alpha·(B[i] - lp[i-1])`,
   `lp[-1] = 0` (declared frame-zero state). `--filter-type`:
   `lowpass` ⇒ `filtered = lp`; `highpass` ⇒ `filtered = B[i] - lp`.
4. **Blend.** `out[i] = lerp(B[i], filtered, amount)`.

- `amount = 0` ⇒ output byte-identical to B (filter is skipped entirely).
- Determinism: identical A, B, STFT settings, filter type, `amount` ⇒ identical
  output.

## Initial Scope

- CPU reference module in `morphogen-audio` (`cross_synth.rs`) with focused
  synthetic tests. No Metal (audio is CPU-only here).
- `render-spectral-cross-synth` CLI: `--modulator-wav` (A), `--carrier-wav` (B),
  `--output-wav` (out), `--mode gain|filter` (default `gain`), `--amount`
  (default 1.0; `0` = passthrough), `--filter-type lowpass|highpass`
  (`filter` mode), RMS window/hop (`gain` mode), STFT fft/hop/window
  (`filter` mode). Reuses the `export-audio-stem` WAV-in/WAV-out idiom.
- Output WAV follows Source B (sample rate, channels, length).
- Queue task + macOS Render-panel exposure follow once the CPU + CLI slice is
  proven (a `frame_sequence`-style audio job; deferred below).

## Algorithm Identifiers

- `rms_gain_cross_synth_cpu_v1` — the `gain` mode render id.
- `centroid_filter_cross_synth_cpu_v1` — the `filter` mode render id.

Both are distinct from every granular / flow / vocoder id. There is no reusable
analysis sidecar in this slice (A's RMS / STFT are cheap and the existing
`cache-rms` / `cache-stft` sidecars already cover reuse if needed later).

## Acceptance Criteria

1. **Passthrough identity.** `amount = 0` ⇒ output byte-identical to Source B
   (both modes).
2. **Envelope transfer (gain).** A loud→quiet ramp over a steady B tone ⇒ the
   output's amplitude tracks A (loud where A is loud, ~silent where A is silent);
   a flat A ⇒ uniform scaling.
3. **Brightness transfer (filter).** A bright (high-centroid) A passes B largely
   intact (high cutoff); a dark (low-centroid) A audibly lowpasses B — the
   output's own spectral centroid drops relative to B.
4. **Determinism.** Identical inputs + settings ⇒ identical output samples.
5. **No `unwrap()` in library code**; errors via `AudioError`/`thiserror`.

## Verification (off-vs-on)

Audio has no PNG to Read, so the numeric analog stands in for the off-vs-on
pixel check: render the same job **off** (`--amount 0`) vs **on**
(`--amount 1`), then compare output **descriptor envelopes**, not raw bytes —
for `gain`, the output RMS contour is flat off / tracks A's RMS on; for
`filter`, the output spectral centroid is unchanged off / pulled toward A's
centroid on. Report the measured delta. A look without a number is
unfalsifiable; a number without the descriptor curve proves nothing.

## Deferred (not this slice)

- **Phase-vocoder cross-synthesis** (the roadmap's high-quality tier): complex
  STFT + phase policy + inverse STFT + Accelerate/vDSP FFT. The MVP is
  time-domain only; this is the real spectral-resynthesis path.
- Multi-pole / biquad / resonant filters; per-band spectral gain (B's STFT bands
  reweighted by A's spectrum) — that is the spectral cousin of the Video
  Vocoder's multiband tier.
- Stereo-aware descriptor routing, envelope smoothing, and look-ahead.
- Queue + SwiftUI exposure land after the CPU + CLI slice is verified.
