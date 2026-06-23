# Video-to-Audio Descriptor Routing Milestone

## Goal

The cross-modal mirror of Audio-to-Video Descriptor Routing: there Source A's
**audio** shaped Source B's **video**; here Source A's **video** shapes Source
B's **audio**. Source B's WAV stays the material you hear; **Source A's per-frame
luma envelope decides how loud each moment is (`gain`) or where it sits in the
stereo field (`pan`)** over time. This is the roadmap's **"frame-luma controls
gain or pan"** MVP for Video-to-Audio Descriptor Routing
(`docs/EFFECTS_ROADMAP.md`).

CPU-only: audio is not a GPU target in this project (no Metal path, nothing to
parity-gate), exactly like Spectral Audio Cross-Synthesis and Convolutional
audio blending.

## Routing (the new logic)

1. **Modulator envelope (Source A).** For each Source A frame `k` at time
   `t_k = k / fps`, compute its **mean Rec.709 luma**
   `luma[k] = mean over pixels of (0.2126·R + 0.7152·G + 0.0722·B)` ∈ `[0,1]`,
   then **peak-normalize**: `a_norm[k] = luma[k] / max(luma)` (all zeros if the
   peak is ~0). A's brightest frame maps to full effect, its darkest (relative)
   to none — the same peak-normalize + hold-last convention as the cross-synth
   `gain` mode and the RMS-displacement route. The luma is computed by the CLI
   (which owns image decoding) and handed to `morphogen-audio` as raw
   `(time_seconds, luma)` samples, keeping the audio crate decoupled from the
   image crate (the symmetric decoupling `audio_route.rs` keeps from audio).
2. **Per-output-sample lookup.** Output follows Source B (sample rate, channels
   for `gain`; always stereo for `pan`). For B sample frame `i` at
   `t = i / sample_rate_B`, hold-last `a_norm` at `t` (latest A frame at or
   before `t`). A and B stay independent in rate and length.
3. **Apply.**
   - **`gain`:** `out[i] = B[i] · lerp(1.0, a_norm, amount)` per channel — a
     bright A frame keeps B, a dark A frame attenuates it toward silence
     (identical shape to `rms_gain_cross_synth`).
   - **`pan`:** mono-mix B's channels to `m`, place it with an equal-power law at
     `pan = (2·a_norm − 1) · amount` ∈ `[−amount, amount]` (`−1` hard left, `+1`
     hard right, `0` center): `θ = (pan+1)·π/4`, `L = m·cos θ`, `R = m·sin θ`.
     A dark frame steers energy left, a bright frame steers it right. Output is
     2-channel.

- `amount = 0` (or `--amount 0`) ⇒ **byte-identical** Source B passthrough in
  both modes (the routing is short-circuited before any channel reshaping, so a
  mono B stays mono).
- Determinism: identical A frames, B, fps, mode, amount ⇒ identical output
  samples.

## Initial Scope

- CPU reference in `morphogen-audio` (`video_route.rs`): the peak-normalized
  luma envelope (`LumaEnvelope`, hold-last by frame time), `luma_gain_route`,
  and `luma_pan_route` (equal-power), with focused synthetic tests.
- `render-video-audio-route` CLI: `--modulator-dir` (A PNG sequence),
  `--carrier-wav` (B), `--output-wav` (out), `--mode gain|pan`, `--amount`
  (base, default `1.0`; `0` = passthrough), `--fps` (frame→time mapping for the
  luma lookup, default `30`), `--max-frames`. The CLI reads A's frames, computes
  per-frame mean luma, and routes them into the audio op.
- Output is a single WAV following Source B (`gain`: B's channels; `pan`:
  stereo).
- Queue task + macOS Render-panel exposure follow once the CPU + CLI slice is
  proven (an audio job like the cross-synth / impulse-convolution ones).

## Algorithm Identifiers

- `luma_gain_route_cpu_v1` — `gain` mode (per-frame luma → B amplitude).
- `luma_pan_route_cpu_v1` — `pan` mode (per-frame luma → equal-power stereo
  position).

No new reusable analysis sidecar: A's per-frame mean luma is cheap to recompute;
a luma sidecar can be added later if reuse matters.

## Acceptance Criteria

1. **Passthrough identity.** `--amount 0` ⇒ output byte-identical to Source B
   (both modes; mono B stays mono).
2. **Envelope transfer.** `gain`: a dark→bright A ramp over a steady B ⇒ output
   amplitude tracks A (quiet where A is dark, full where A is bright). `pan`: a
   dark A frame ⇒ energy to the left channel, a bright A frame ⇒ energy to the
   right.
3. **Determinism.** Identical inputs + settings ⇒ identical output samples.
4. **No `unwrap()` in library code**; errors via `AudioError`/`thiserror`.

## Verification (off-vs-on)

Audio has no PNG to Read, so verify numerically (as the cross-synth /
impulse-convolution slices did). Render the same job **off** (`--amount 0`) vs
**on** (`--amount 1`) on a synthetic readout (A = dark→bright frame ramp, B =
steady tone): `gain` ⇒ off RMS flat, on RMS rises with A's brightness; `pan` ⇒
off L≈R, on L-energy dominates on dark frames and R-energy on bright frames.
Report the numbers. A look without a number is unfalsifiable.

## HQ Tier (landed — CPU + CLI + queue + SwiftUI)

Three deferred axes of the MVP, each a verified vertical slice. The routes are
descriptor-neutral and the algorithm id is composed in `morphogen-core`
(`video_audio_route_algorithm_id`) as `{descriptor}_{mapping}_route_cpu_v1` (the
project convention, cf. `rms_gain_cross_synth`); the `filter_type` and `sampling`
knobs are recorded parameters, not part of the id (like cross-synth's filter).

1. **Optical-flow descriptor** (`--descriptor flow`): per-frame mean Lucas-Kanade
   flow magnitude (motion) instead of mean luma, reusing the parity-gated
   `lucas_kanade_flow_cpu` (`LUCAS_KANADE_WINDOW_RADIUS = 3`); frame zero has no
   prior frame ⇒ `0`. Peak-normalized like luma (relative motion). New ids
   `flow_gain/flow_pan/flow_filter_route_cpu_v1`; luma ids unchanged.
2. **Filter audio target** (`--mode filter --filter-type lowpass|highpass`): the
   descriptor sweeps a one-pole LP/HP cutoff on B (strong ⇒ open toward Nyquist),
   reusing a shared `one_pole_filter_sweep` factored out of
   `centroid_filter_cross_synth`. Ids `*_filter_route_cpu_v1`.
3. **Time-resampled curves** (`--sampling hold|smooth`): `hold` steps the
   envelope at frame boundaries (default, byte-identical to the MVP); `smooth`
   linearly interpolates between frames (a continuous curve). Centralized in
   `DescriptorEnvelope::resample`, shared by gain/pan/filter.

## Still Deferred

- **Edge-density descriptor** (per-frame Sobel mean) — a near-free third
  descriptor; not yet wired.
- **Pitch / playback-rate target** — needs deterministic resampling and changes
  output length/timing (bit-repro risk); intentionally out of scope.
- **Depth descriptor** — no depth pipeline exists; a monocular-depth estimator
  would be a heavy non-deterministic dependency (dropped, not deferred).
- **Phase-vocoder spectral processing** driven by the descriptor curves — the
  deeper "drive spectral audio processing" reading of the roadmap HQ line; gated
  on a complex-STFT + inverse path (shared with the cross-synth HQ tier).
