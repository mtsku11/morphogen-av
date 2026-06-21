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
scripts/make-fixture.sh /tmp/fix --frames 4 [--size WxH] [--with-chirp]
```

- **Carrier**: an ordered grey ramp — frame 0 darkest → frame N−1 lightest.
- **Modulator**: alternates darkest/lightest, so colour-nearest matching wants to
  jump between the extremes every frame (maximally jumpy with no scheduler — the
  baseline a scheduler must visibly tame).

The point: render with `--rearrangement 1.0` and each **output** frame's colour
*is* the carrier frame a tile selected. Source-frame jumpiness, pool-window
membership, and anti-repeat / coherence behaviour are then visible directly.

`--with-chirp` additionally writes, per source, a constant-amplitude linear chirp
WAV (flat RMS, rising spectral centroid — isolates the centroid dim) and the
matching `*-rms.json` / `*-stft.json` caches, ready for `--audio-weight` and
centroid (k=2) runs.

## Typical use

```sh
# 1. scaffold
scripts/make-fixture.sh /tmp/fix --frames 4

# 2. eyeball a knob with /preview (Read the output PNGs)
cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence \
  /tmp/fix/modulator /tmp/fix/carrier /tmp/fix/out \
  --rearrangement 1.0 --coherence-weight 5.0 --coherence-reach 1
#   off: output greys alternate (jumps); on: a grey is held (smooth)

# 3. prove path-independence with /parity
scripts/parity-check.sh /tmp/fix/modulator /tmp/fix/carrier -- \
  --rearrangement 1.0 --coherence-weight 5.0 --coherence-reach 1
```

## Gotchas (learned the hard way)

- A grey differs in all 3 channels, so colour distance is ~3× the single-channel
  intuition — a coherence/anti-repeat `weight` must beat the *actual* colour gap
  to flip a selection (weight 1.0 was too weak on greys; 5.0 was decisive). Tune
  weight to the fixture's contrast, or use `--pool-window`/high weight for a
  decisive readout.
- Output writes to `<output_dir>/frames/` for the queue path but straight into
  `<output_dir>/` for the direct render — `/parity` already accounts for this.
- Needs `ffmpeg` on PATH; `--with-chirp` also shells out to `cache-rms` /
  `cache-stft`. Fixtures live under `target/` or `/tmp` (gitignored) — don't
  commit them.
