---
name: fixture
description: Scaffold a synthetic readout fixture for the Morphogen AV granular-pool path — solid-colour carrier frames where the output colour reveals which source frame each tile picked, optional chirp WAVs + RMS/STFT caches for audio/centroid runs. Use to isolate and eyeball what a pool knob does without real footage.
---

# fixture (morphogen-av)

Real clips are noisy for *verifying* what a selection knob did. These fixtures are
designed so the answer is readable straight off the output frame, encoding the
readout tricks from [[granular-audio-needs-temporal-pool]] and the
temporal-coherence work.

## What it builds

```sh
scripts/make-fixture.sh /tmp/fix --frames 4 [--size WxH] [--readout frame|origin|texture] [--with-chirp]
```

Three readout modes — pick the one matching the axis your knob moves:

**`--readout frame` (default) — which source FRAME was picked.**
- **Carrier**: an ordered grey ramp — frame 0 darkest → frame N−1 lightest.
- **Modulator**: alternates darkest/lightest, so colour-nearest matching wants to
  jump between the extremes every frame (maximally jumpy with no scheduler — the
  baseline a scheduler must visibly tame).
- Render with `--rearrangement 1.0` and each **output** frame's colour *is* the
  carrier frame a tile selected. Source-frame jumpiness, pool-window membership,
  and anti-repeat / frame-coherence behaviour are then visible directly.

**`--readout origin` — which source ORIGIN was picked (for spatial/selection knobs).**
- **Carrier**: a *static* coordinate gradient (R=x, G=y) so a tile's output colour
  reveals the carrier *location* it sampled (blue=left edge → yellow=right edge).
- **Modulator**: a horizontal grey gradient whose direction flips every frame, so
  a tile's demanded source origin wants to teleport left↔right each frame.
- This is the readout for **spatial-origin coherence** and other origin-space
  knobs. The solid-colour `frame` readout *cannot* show them — all grains in one
  frame share a colour, so origin only breaks via tie-break. See
  [[spatial-coherence-shares-reach]].

**`--readout texture` — which source TEXTURE was picked (for the texture dims).**
- **Carrier**: frames alternate a *flat* mid-grey and a *busy* vertical-stripe
  frame at the **same mean colour** (0x80), so mean colour ties across frames and
  only the texture descriptor (luma variance + gradient) can decide which a tile
  draws.
- **Modulator**: alternates flat/busy too, so the demanded texture flips each
  frame.
- This is the readout for `--texture-weight`. With it ON the output structure
  tracks the modulator demand (flat↔stripes, high frame-delta); OFF the colour tie
  pins selection to the flat frame (≈0 delta). The `frame`/`origin` readouts cannot
  show texture — solid tiles have zero texture and the coordinate gradient is
  spatially uniform.

`--with-chirp` additionally writes, per source, a constant-amplitude linear chirp
WAV (flat RMS, rising spectral centroid — isolates the centroid dim) and the
matching `*-rms.json` / `*-stft.json` caches, ready for `--audio-weight` and
centroid (k=2) runs.

## Typical use

```sh
# 1. scaffold (origin readout, for a spatial/selection knob)
scripts/make-fixture.sh /tmp/fix --frames 8 --size 96x96 --readout origin

# 2. render the knob OFF vs ON — ALWAYS --variation 0 (see the gotcha below)
cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence \
  /tmp/fix/modulator /tmp/fix/carrier /tmp/fix/off \
  --grain-size 8 --rearrangement 1.0 --variation 0 --coherence-reach 10 --spatial-coherence-weight 0
cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence \
  /tmp/fix/modulator /tmp/fix/carrier /tmp/fix/on \
  --grain-size 8 --rearrangement 1.0 --variation 0 --coherence-reach 10 --spatial-coherence-weight 6

# 3. quantify + eyeball: numbers AND pixels (this is the verify loop)
scripts/frame-delta.py /tmp/fix/off /tmp/fix/on   # OFF strobes (~64/255), ON holds (0/255)
#   then Read sampled OFF and ON frames — the number alone never proves the look.

# 4. prove path-independence with /parity (queue == direct)
scripts/parity-check.sh /tmp/fix/modulator /tmp/fix/carrier -- \
  --rearrangement 1.0 --variation 0 --coherence-reach 10 --spatial-coherence-weight 6
```

(`frame` readout instead: scaffold without `--readout`, then a frame-scheduler
knob like `--coherence-weight 5.0 --coherence-reach 1` — off: output greys
alternate; on: a grey is held.)

## Gotchas (learned the hard way)

- **`--variation` defaults to 0.25, not 0 — always pass `--variation 0`.** Every
  tile's pick is `lerp(feature_match, seeded_random_grain, variation)`. At 0.25 a
  per-tile *random* alternate (fixed by seed/tx/ty) contaminates a quarter of the
  selection, and the schedulers reshape ONLY the feature match — never the
  alternate. So at the default the readout shows fixed spatial scatter and the
  knob's effect is masked (a uniform modulator gives rainbow noise instead of one
  colour). This silently wasted a whole showcase session. See
  [[spatial-coherence-shares-reach]].
- A grey differs in all 3 channels, so colour distance is ~3× the single-channel
  intuition — a coherence/anti-repeat `weight` must beat the *actual* colour gap
  to flip a selection (weight 1.0 was too weak on greys; 5.0 was decisive). Tune
  weight to the fixture's contrast, or use `--pool-window`/high weight for a
  decisive readout.
- The bundled ffmpeg has no `drawtext` (no libfreetype), so you can't burn panel
  labels into a comparison clip — arrange panels positionally and label them in
  the caption instead.
- Output writes to `<output_dir>/frames/` for the queue path but straight into
  `<output_dir>/` for the direct render — `/parity` already accounts for this.
- Needs `ffmpeg` on PATH; `--with-chirp` also shells out to `cache-rms` /
  `cache-stft`. Fixtures live under `target/` or `/tmp` (gitignored) — don't
  commit them.
