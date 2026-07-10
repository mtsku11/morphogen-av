# Morphogenesis Track A2 — Lenia continuous cellular automata (`--model lenia`)

Status: **A2-S1 COMPLETE.** Core Lenia engine + `--model lenia` on the direct
CLI (`render-morphogenesis-sequence` and `queue-add-morphogenesis-sequence`,
including full queue-run reconstruction — the queue path is already generic
across models post-A1-S2, so no "stub" phase was needed the way A1-S1's was).
Baseline cargo 745/0 → 751/0 (6 new LEN tests: LEN0–LEN5), clippy clean, zero
new fmt diffs (8 pre-existing dirty files, count unchanged). Verified
end-to-end on the real cello fixture: `--model lenia --lenia-preset orbium`
composite view settles carrier-luma-seeded discs into a stable, never-
freezing labyrinth/membrane pattern; field view + `--shade` reads as raised
fingerprint-ridge relief with zero new shading code. Builds on
[MORPHOGENESIS_FHN_MILESTONE.md](MORPHOGENESIS_FHN_MILESTONE.md) and the plan
in [MORPHOGENESIS_EXPANSION_HANDOFF.md](MORPHOGENESIS_EXPANSION_HANDOFF.md)
(Track A, A2).

## Architecture decision (pinned)

Extend `MorphogenesisModel` with a third variant, `Lenia`, alongside
`GrayScott`/`FitzhughNagumo` — no new command family, no `FieldModel` trait
(the handoff's rule-of-three: Lenia is the third model, but the duplication
between GrayScott/FHN/Lenia's dispatch shape is still small enough per-site
that extracting a trait would cost more than it saves).

**State container:** Lenia stores its single scalar channel `A ∈ [0,1]`
directly in [`MorphogenesisField`]'s `.v` — the EXACT display/composite
contract Gray-Scott's `V` already has. `.u` is unused (dummy `1.0`,
mirroring FHN's throwaway-channel convention). This means, unlike FHN,
**Lenia needs no display adapter at all** — `composite_morphogenesis_frame`
and `render_v_field_grayscale_upsampled_with_shading` consume a raw Lenia
field completely unchanged (LEN5 proves this rather than asserting it).

**Settings:** a new `LeniaSettings` struct (own `validate`, own presets:
`orbium`/`geminium`/`soup`), parallel in shape to `FhnSettings`. The sequence
checkpoint contract gains an additive `lenia_settings: LeniaSettings` field
(`#[serde(default)]`, always present, only authoritative when `model ==
Lenia` — the same "always-present, model-selects-meaning" shape as
`fhn_settings`).

**Shared CLI/queue flag reuse:** `dt`/`substeps`/`sim_scale`/
`seed_threshold`/`seed`/`inject`/`erode`/`inject_source` are shared flag
names across all three models, written into whichever struct is active.
Lenia's `inject`/`erode` reuse Gray-Scott's PLAIN `[0,1]` additive/
multiplicative equations verbatim (`apply_lenia_inject_erode` mirrors
`apply_inject_erode`'s formula exactly) — unlike FHN, which needed a
stimulus-scaled discrete kick and a widened legal range, because Lenia's `A`
is already a `[0,1]` density exactly like Gray-Scott's `V`. Lenia-only knobs
(`radius`/`mu`/`sigma`) get their own `--lenia-radius`/`--lenia-mu`/
`--lenia-sigma`/`--lenia-preset` flags on both the direct render command and
`queue-add`, persisted as their own flattened `render_job.rs` fields
(mirroring `epsilon`/`a`/`b`/`stimulus`'s "always-present, Lenia-only"
convention) since neither Gray-Scott nor FHN has an analogue for them.
`du` has no Lenia meaning (placeholder `0.0` in the enqueue tuple, matching
FHN's own placeholder pattern for concepts a given model lacks).

## The model

```text
A(t+dt) = clamp01( A + dt * G( (K * A)(x) ) )
K        = normalized gaussian-shell ring kernel, radius R
G(u)     = 2*exp(-(u-mu)^2 / (2*sigma^2)) - 1
```

- Direct O(W·H·R²) convolution (an FFT/separable port is an optimization
  slice, not the MVP, per the handoff) — measured fine at 320x180 sim res on
  the cello fixture (144 frames renders in well under a minute in release).
- **Seeding is a filled disc, not a single pixel** (`stamp_disc`, radius =
  `settings.radius`), at every carrier-luma-thresholded cell plus a
  deliberately sparse speckle (`LENIA_SPECKLE_DENSITY = 0.0004`, ~5x sparser
  than Gray-Scott/FHN's shared 0.2% — each hit here covers a whole
  neighbourhood, not one cell). A lone active pixel has ~zero mass under the
  normalized kernel, so `G(K*A) = G(~0)` is strongly negative for every
  preset's `mu > 0` and dies before the next substep — Lenia has no reaction
  term to spread an isolated seed, unlike reaction-diffusion.
- **Presets tuned empirically, not from the literature atlas** — this is the
  single most important finding of this slice. The classic Orbium atlas
  (`mu≈0.15, sigma≈0.017, R=13`) is fit to one exact hand-crafted glider
  photograph; seeded instead from a plain disc/bump (this app's own B-luma
  seeding method), it collapses to zero mass within ~20 frames (confirmed by
  direct measurement: mass 529→0 by frame 10 at the literature values). A
  parameter sweep (documented in `morphogenesis.rs`'s test-diagnostic
  history) found `mu=0.2, sigma=0.1` settles the SAME disc seed into a
  stable equilibrium mass (plateaus within ~1% drift over 300+ frames) —
  neither dying nor unboundedly expanding. `orbium`/`geminium` ship with
  these re-tuned values; `soup` just lowers `seed_threshold` to `0.15` so
  most of the carrier's bright structure stamps overlapping discs (dense,
  full-frame texture rather than isolated creatures).
- **The look is a stable, non-translating blob/membrane**, not a literal
  gliding creature — a legitimate member of the Lenia phenomenology (per the
  handoff's own "smooth gliding blobs, orbiting rings, **breathing
  membranes**"), but NOT the same claim as "gliders translate." Rendered on
  real footage, growth reads as an organic labyrinth/coral-adjacent-but-
  distinct pattern that settles into a bounded, never-freezing texture —
  visually distinct from both Gray-Scott (fills/saturates) and FHN
  (expanding rings with a dark refractory interior).

## Aliveness (falsifiable, adapted from the handoff)

The handoff's own criterion is "mass stays in a band AND the centroid
moves (gliders translate; death and explosion both fail it)." Since this
app's disc/luma seeding (not an exact glider photograph) settles to a
STATIC stable blob rather than a translating one (see above), LEN2 adapts
the "stays in a band" half only: early-window mean mass vs. late-window mean
mass must both be nontrivial and within a 2x band of each other — a dying
preset's late mass would be ~0, an exploding preset's late mass would keep
climbing well past the early window mean.

## Anchors

- **LEN0 (continuity):** all three algorithm ids
  (`morphogenesis_cpu_v1`/`morphogenesis_fhn_cpu_v1`/`morphogenesis_lenia_cpu_v1`)
  are pairwise distinct.
- **LEN1 (quiescence):** an entirely empty field (`A = 0` everywhere, no
  seed) stays at exactly zero for every preset — no spontaneous generation.
- **LEN2 (aliveness):** the adapted "bounded mass" criterion above, per
  preset (`orbium`, `geminium`).
- **LEN3:** `substeps == 0` freezes the field byte-identically, same anchor
  shape as Gray-Scott's A2 / FHN's own.
- **LEN4 (checkpoint):** interrupt+resume byte-identical through the
  UNCHANGED RGBA32F codec.
- **LEN5 (composite/field-view reuse, no adapter):** a raw Lenia field feeds
  both existing output-view functions unchanged and produces non-flat
  output — proving no adapter is needed, unlike FHN's `fhn_display_field`.

## Slices

- **A2-S1** (this commit): core Lenia engine (`LeniaSettings`, presets,
  ring-kernel + growth-mapping substep, disc-seeding, `apply_lenia_inject_erode`,
  checkpoint `lenia_settings` field) + `--model lenia`/`--lenia-preset`/
  `--lenia-radius`/`--lenia-mu`/`--lenia-sigma` CLI flags on BOTH the direct
  render command and `queue-add` (queue-run reconstruction included, since
  the queue's model-dispatch machinery is already generic post-A1-S2 — no
  "ignored on the run side" gap the way A1-S1 had) + LEN0–LEN5 tests + a
  real-footage readout (composite view + field/shade view, frames Read,
  flagship clip delivered).
- **A2-S2** (deferred, not yet started): `inject`/`erode` joining the
  modulation registry for Lenia (`apply_morphogenesis_modulation` gaining a
  fourth `&mut LeniaSettings` param) + SwiftUI ride-along (model picker
  extension, Lenia preset picker, 4 numeric knobs) — mirrors A1-S2's own
  scope exactly. Until this lands, modulation routes targeting `inject`/
  `erode` do not affect a Lenia render (silently inert, matching the
  render.rs code comment at the `frame_lenia_settings` declaration).

## Acceptance criteria

LEN0–LEN5 as tests; clippy clean; zero new fmt diffs (8 pre-existing dirty
files, count unchanged); baseline cargo 745 → 751 reported; no `unwrap()`.
A2-S1's deliverable: real-footage frames Read (composite + field/shade) and
an audio-muxed flagship clip — no `inject=audio-rms` reactivity yet (that's
A2-S2's, since the modulation registry isn't wired for Lenia in this slice).
