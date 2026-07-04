# Direction Recommendations

Strategic "where to go next" notes, distinct from the tactical
[`BACKLOG.md`](BACKLOG.md) queue and the per-effect
[`EFFECTS_ROADMAP.md`](EFFECTS_ROADMAP.md) entries. This doc answers two
questions asked of the codebase as a whole: **what is underdeveloped**, and
**what would take the app to the next level**. Written 2026-07-03 against the
466-test baseline. Each item notes rough effort and the invariant tensions to
respect (determinism, CPU-reference parity, sidecar reuse).

Nothing here is committed work — it is a prioritized menu. When an item is
picked up, promote it to a `*_MILESTONE.md` contract first (per the CLAUDE.md
"contract first" workflow) and add a `BACKLOG.md` entry.

**2026-07-04 update:** most of Part 1/2 has since landed (Rutt-Etra incl.
two-source, LFOs, chaining, phase vocoder, preview loop). The living successor
to this doc — every remaining deferred item plus a new tier of
league-elevating proposals (mod-signal algebra, video oscillators, MIDI
source, spatial mattes, performance capture, colour pipeline, patcher canvas)
— is **[DEFERRED_WORK_HANDOFF.md](DEFERRED_WORK_HANDOFF.md)**.

---

## Part 1 — Underdeveloped areas (ranked by payoff ÷ effort)

### 1. Scanline / Rutt-Etra modulation — the only roadmap effect with *nothing* landed
Every other `EFFECTS_ROADMAP.md` entry has at least a shipped MVP; the
"Scanline / Rutt-Etra Style Carrier Modulation" section has zero code. It is
also the single most iconic analog-video-synth look (luma → displaced scanline
geometry), and the MVP (luma-derived vertical displacement) is *simpler* than
most of what already ships. It becomes an immediate modulation-matrix showcase:
`displacement_depth=audio-rms` is the classic Rutt-Etra demo.

- **Effort:** small CPU MVP; a Metal mesh/compute path is the natural HQ tier.
- **Invariants:** standard deterministic CPU-reference-then-Metal vertical.
- **Why first:** fills the only empty roadmap slot, high recognizability, cheap.

### 2. The audio side lags the video side badly
Two concrete gaps:

- **Phase-vocoder spectral cross-synthesis.** `SPECTRAL_CROSS_SYNTH_MILESTONE.md`
  is still the time-domain MVP (RMS/centroid → filter/gain). Imposing A's
  spectral envelope on B's spectrum with a real **inverse STFT** is *the*
  headline audio cross-synth sound — the audio equivalent of what flow-feedback
  is visually. The hardest prerequisite already exists: the pure-Rust radix-2
  FFT shipped for the convolution-blend HQ tier
  (`CONVOLUTIONAL_BLEND_MILESTONE.md`). This is mostly wiring an inverse
  transform + phase handling onto an existing FFT.
- **Granular mosaicing outputs only visual grains.** Audio descriptors *select*
  grains, but the grains don't carry and resynthesize their audio slice.
  "Audiovisual grains" is in the effect's own roadmap description; making grains
  emit their source audio window (overlap-add) turns it from a video effect with
  audio-aware selection into a true AV effect.

- **Effort:** medium each; both are CPU-only (audio is not a GPU target here).
- **Invariants:** determinism holds trivially (pure-Rust FFT, deterministic OLA).

### 3. Exposure debt on already-proven engines
Strong CPU paths stuck at direct-CLI-only, underselling the engine:

- **Cascade collage** — CPU+CLI only; Metal, queue, and A→B all deferred.
- **Fluid colour-sort mosaic** — the `BACKLOG.md` "Next" item explicitly leaves
  its queue task undecided because its raw ~15-knob API stalled it. Resolve with
  a **curated-preset** queue job (a handful of named looks) rather than exposing
  every knob.
- The two modulation-matrix items already scheduled (named-modulator queue/UI,
  per-route-sampling UI).

- **Effort:** small each; the queue/SwiftUI patterns are fully established.
- **Invariants:** each is byte-identical-when-off, add→run byte-identical.

### 4. Uneven modulation-target coverage
The matrix spans stateless + stateful effects, but the newest effects
(field-particles, cascade collage, fluid mosaic, dispersion blend) register no
modulation targets. The pattern is a small per-effect target registry + a
clamped apply-fn slice.

- **Effort:** small per effect.
- **Invariants:** clamp-never-error; zero-route path byte-identical; stateful
  targets must join the checkpoint-invalidation contract (feedback/datamosh
  precedent).

### 5. Coagulated-flow-blend HQ
The roadmap's own future list (multi-class ownership, motion- and audio-driven
coagulation) is a natural fit now that the mod matrix exists — registering
`coagulation_strength` / `edge_hardness` as targets makes
`coagulation_strength=audio-rms` nearly free.

- **Effort:** small (targets) → medium (multi-class ownership).

---

## Part 2 — Next-level features

### A. Effect chaining — run a graph, not a node *(biggest lever)*
The long-term target is a modular AV synthesizer, and `morphogen-core` already
has typed node-port compatibility checks — but every render command executes
**exactly one** effect. Composing today means manually feeding one render's
output frames in as the next render's input.

A `render-chain` job — an ordered list of effect nodes, each stage's output
directory feeding the next, one manifest, per-stateful-stage checkpoint
semantics — turns the app from an effects *catalog* into an *instrument*.
Determinism composes for free since every stage is already deterministic and
parity-gated. This also **multiplies** the value of every effect above:
datamosh → fluid-advect → vocoder chains are where genuinely novel looks live.

- **Effort:** medium-large; deserves its own milestone contract first.
- **Invariants:** each stage keeps its own contract; the chain manifest records
  the ordered stage ids + settings so the whole chain is reproducible; a stateful
  stage checkpoints as it does standalone.

### B. LFOs and drawn envelopes as modulation sources
The matrix is analysis-only. A real mod matrix also has **internal** sources:
deterministic LFOs (`lfo(shape, rate, phase)` — trivially bit-reproducible, no
media needed) and user-drawn breakpoint curves. Grammar-wise these are just new
`ModulationSource` variants; everything downstream (per-route sampling, named
modulators, checkpoint contracts) already generalizes over the source enum.

- **Effort:** small-medium; high leverage.
- **Invariants:** LFO is a pure function of `(frame, fps, params)` — deterministic
  by construction; no sidecar needed. This is the cheapest "feels like a
  synthesizer" upgrade available.

### C. A realtime-ish preview loop
The invariants already reserve space for it ("realtime preview is a
lower-fidelity view of the same project graph, never a separate engine"), but
today the inner loop is render-PNGs-then-Read. Even a non-interactive fast path
— render N seconds at quarter-res straight to the preview surface using the same
engine — would change how it feels to *play* the instrument.

- **Effort:** largest item here (honest caveat). The same-engine invariant is
  exactly what keeps it tractable — no second renderer to keep in parity.
- **Invariants:** must stay a downsampled *view* of the offline graph, never a
  fork of the render logic.

### D. Modulator-source expansion: edge-density and depth
- **Edge-density descriptor** — already on the `VIDEO_AUDIO_ROUTE_MILESTONE.md`
  deferred list; cheap, purely deterministic (Sobel/gradient magnitude).
- **Depth descriptor** — unlocks the Rutt-Etra depth mode and parallax-style
  displacement, but Apple Vision/depth models are **not bit-reproducible across
  OS versions**. It would need the sidecar-fingerprint carve-out treatment
  (deterministic *given* a cached sidecar, like the ffmpeg bitstream path).
  Flag this tension at milestone time, not at discovery time.

---

## Recommended ordering

1. **Named-modulator queue/UI exposure + per-route-sampling UI** — finish the
   modulation-matrix surface already in flight (Part 1 §3).
2. **Rutt-Etra scanline MVP** — fills the only empty roadmap slot, instant
   mod-matrix showcase (Part 1 §1).
3. **LFO modulation sources** — small, high-leverage "synthesizer" upgrade
   (Part 2 §B).
4. **Effect chaining** — the next big milestone; start with a contract
   (Part 2 §A).
5. **Phase-vocoder cross-synth** — the audio-focused stretch, whenever wanted
   (Part 1 §2).

## Explicitly *not* recommended (with rationale)
- **Multiscale structure-morph Metal/queue/SwiftUI** — proven mathematically
  correct but practically marginal on real footage (dense low-contrast footage
  makes the mask near-uniform). Stays a correct, opt-in, CPU-only path until a
  use case shows it mattering. See `BACKLOG.md` structure-morph note.
- **FFglitch integration** — the non-deterministic bitstream carve-out
  (`datamosh-bitstream`) already reproduces the authentic codec-artifact looks
  that matter (P-frame bloom, keyframe removal, motion transfer). A hard external
  dependency for the richer FFglitch vocabulary isn't worth the invariant cost.
