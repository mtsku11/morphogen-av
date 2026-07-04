# Phase-Vocoder Cross-Synthesis Milestone — real spectral resynthesis

**Status: contract — build not started.** This doc is the acceptance contract
(per the CLAUDE.md "contract first" workflow). Origin: `docs/RECOMMENDATIONS.md`
Part 1 §2 and the Deferred list of `docs/SPECTRAL_CROSS_SYNTH_MILESTONE.md` —
"imposing A's spectral envelope on B's spectrum with a real inverse STFT is
*the* headline audio cross-synth sound; the hardest prerequisite already
exists" (the pure-Rust radix-2 forward+inverse FFT in
`crates/morphogen-audio/src/fft.rs`, landed for the convolution-blend HQ tier;
it is private — this milestone exposes it through a complex-STFT module, not
by making callers reach into `fft_in_place`).

## Origin & Goal

The time-domain MVP (`render-spectral-cross-synth --mode gain|filter`) shapes
B with A's scalar descriptors. This milestone adds **`--mode vocode`**: B's
complex spectrum, frame by frame, is reweighted by **A's spectral envelope**
and resynthesized through a real inverse STFT — B speaks with A's timbre.
CPU-only (audio is not a GPU target), deterministic by construction
(pure-Rust FFT, fixed windows, no randomness).

## Mechanic (deterministic CPU reference)

New `morphogen-audio` complex-STFT module (design constraint, not a knob):

- **Analysis:** window `w` (the existing `WindowFunction` set), FFT size
  a power of two, hop `≤ fft/2`. Frames are windowed, transformed with the
  existing f64 radix-2 FFT.
- **Synthesis:** inverse FFT per frame, windowed again with `w`, weighted
  overlap-add, normalized by the precomputed per-sample `Σ w²` (standard
  weighted-OLA; deterministic for any hop ≤ fft/2; guard against ~zero
  normalizer at the edges). The tail beyond B's length is truncated; output
  length, sample rate, and channel count follow **B** exactly.
- **Round-trip anchor:** `istft(stft(x))` must match `x` within
  `1e-5` max-abs on interior samples (pin in a unit test, including a
  non-power-of-two-length input via zero-padding).

Vocode, per B-STFT frame at time `t` (per channel; A is analyzed once, the
way the existing filter mode analyzes A):

1. **A's spectral envelope.** From A's magnitude STFT frame at hold-last
   time `t` (the established A/B time convention): partition **normalized
   frequency** `f/nyquist ∈ [0,1]` into `--vocode-bands` (default 32,
   valid [1, fft/2]) **log-spaced bands**; the envelope value of a band is
   its mean magnitude. Normalize the whole envelope by the **global peak
   band value across all A frames** (the house *relative* peak-norm
   convention — A's loudness contour is preserved; silent A ⇒ zero
   envelope ⇒ silence at amount 1, and an all-silent A yields silence, not
   an error). Normalized frequency makes A/B sample-rate and FFT-size
   mismatches well-defined.
2. **Shape B.** `shaped[k] = B[k] · E_A(f_k)` — complex scale, **B's phase
   is kept verbatim** (B is the carrier; no phase manipulation — this is
   cross-synthesis, not time-stretch).
3. **Blend.** `out[k] = lerp(B[k], shaped[k], amount)`, then inverse STFT.

- **`amount = 0` short-circuits to an exact byte-identical passthrough of
  B** — the transform never runs (the established off-path convention; the
  STFT round-trip is close but not byte-exact, so the off path must not go
  through it).
- Algorithm id: **`phase_vocoder_cross_synth_cpu_v1`**.

## Knobs (`--mode vocode`)

Reuses the existing command surface: `--fft-size`, `--stft-hop`, `--window`,
`--amount` keep their spellings; new `--vocode-bands` (default 32). CLI
validation: bands ≥ 1 and ≤ fft/2, hop ≤ fft/2, clear `CliError`s.

## Acceptance criteria

Slice 1 — complex STFT + vocode mode (CPU + CLI):

1. Unit tests: STFT/ISTFT round-trip anchor (tolerance pinned, interior
   samples, non-power-of-two length); OLA normalizer correctness for two
   hops (fft/2, fft/4); `amount 0` byte-identity; determinism (two runs
   byte-identical); silent-A ⇒ silent output at amount 1; band-count
   edge cases (1 band ≡ broadband gain; bands > fft/2 rejected).
2. **Descriptor proof (the audio analog of Read-the-frames):** white-noise
   B + a low-tone A (energy concentrated in a known band): off
   (`--amount 0`) vs on (`--amount 1`) — the output's spectral centroid
   must drop toward A's; report the centroid numbers (off ≈ B's, on pulled
   markedly down) and the output-vs-A band-envelope agreement. A claim
   without a number is unfalsifiable.
3. No `unwrap()` in library code; errors via `AudioError`/`thiserror`.
   The FFT stays pure-Rust (no Accelerate dependency — determinism first).

Slice 2 — queue + SwiftUI:

4. The existing `AudioSpectralCrossSynth` queue task gains the vocode mode +
   knobs as serde-defaulted fields (pre-slice queue JSON byte-identical —
   assert the serialized form); add-time validation mirrors the CLI checks;
   **add→run byte-identical** to the direct render (smoke-pinned).
5. The existing spectral-cross-synth SwiftUI panel gains the mode option and
   its knobs (mirror how gain/filter mode-specific controls are shown);
   bridge arg tests pin the token sequence.

## Build plan (handoff notes)

- `crates/morphogen-audio/src/stft_complex.rs` (or similar): complex
  forward/inverse STFT over the existing `fft_in_place` (bump its
  visibility to `pub(crate)`); keep the magnitude-only `stft_magnitude_cache`
  untouched (sidecars depend on its exact behavior).
- Vocode mode beside gain/filter in `cross_synth.rs`; CLI wiring in the
  existing `render-spectral-cross-synth` command.
- Queue/SwiftUI: follow the task's existing mode plumbing (grep
  `AudioSpectralCrossSynth` in queue.rs, `RustBridgePlaceholder`, panel).

Working agreements (standing, non-negotiable):

- Baseline before touching anything: `cargo test --workspace` (**510** green
  at contract time) and `swift test` (**98** green); report deltas, not
  adjectives.
- `/checkpoint` after each verified slice (local commit, source only, never
  push). `/verify` before calling any slice done.
- Never commit the untracked `scripts/solitaire-cascade-prototype.py` or
  `shader-port-ideas/`.
- Record non-obvious findings in `/memory/`, not in prose docs. The
  stdlib-wave trap applies: Python's `wave` can't read hound's float WAVs —
  parse RIFF manually or write 16-bit PCM fixtures.

## Deferred (explicitly out of scope)

- Phase manipulation (time-stretch/pitch-shift vocoding) — keep-B-phase only.
- Cepstral/LPC spectral-envelope extraction (log-band mean is the MVP).
- Morphing (A↔B spectral interpolation), per-band amount curves.
- Modulation-matrix routes onto vocode knobs (joins the matrix later, the
  standard target-registry slice).
