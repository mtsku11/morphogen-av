---
name: preview
description: Render a Morphogen AV effect on a small fixture and view the result — extract source frames if needed, render a short sequence with given params, then Read sampled output PNGs to judge or tune. Use when iterating on an effect's look.
---

# preview (morphogen-av)

Tight render→look loop for tuning effects. Render a *short* sequence to a temp dir,
then Read sampled frames (PNGs render as images). Keep it cheap: small frame counts.

## Step 1 — Get source frames (skip if you already have frame dirs)

Inputs are paired PNG frame directories. The repo has handy test clips at the root
(`cello.mp4`, `cello2.mp4`, `harp.mp4`, gitignored). Extract with the CLI (uses
ffmpeg under the hood):

```sh
cargo run -q -p morphogen-cli -- extract-frames cello.mp4 /tmp/prev/a --fps 12 --max-frames 24
cargo run -q -p morphogen-cli -- extract-frames harp.mp4  /tmp/prev/b --fps 12 --max-frames 24
# audio (for descriptor-driven granular / RMS modulation):
cargo run -q -p morphogen-cli -- extract-audio cello.mp4 /tmp/prev/a.wav --sample-rate 48000 --max-duration-seconds 2
```

Per `[[flow-feedback-levers]]`: feedback/morph effects need motion *in the carrier*,
so for those use a high-motion clip as **both** A and B.

## Step 2 — Render a short sequence

Pick the command for the effect and keep `--max-frames` small (8–24). Examples:

```sh
# two-source displacement
cargo run -q -p morphogen-cli -- render-frame-sequence /tmp/prev/a /tmp/prev/b /tmp/prev/out --amount 16 --max-frames 12

# flow feedback / structure morph (single-scale is the keeper)
cargo run -q -p morphogen-cli -- render-feedback-sequence /tmp/prev/a /tmp/prev/b /tmp/prev/out \
  --flow-source optical-flow --feedback-mix 0.97 --decay 0.97 --structure-mix 0.6 --max-frames 16

# granular mosaic
cargo run -q -p morphogen-cli -- render-granular-mosaic-sequence /tmp/prev/a /tmp/prev/b /tmp/prev/out \
  --grain-size 24 --rearrangement 1 --variation 0.35 --seed 42 --max-frames 12
```

Add `--backend metal` to preview the GPU path (it parity-gates against CPU).

## Step 3 — Look

Read sampled output frames — first, middle, last — to judge the trajectory of the
effect (feedback/morph evolve over time, so one frame isn't enough):

```sh
ls /tmp/prev/out/frames/    # confirm frame names
```

Then Read e.g. `frame_000000.png`, a middle frame, and the last. Describe what you
see. For A/B tuning comparisons, render two param sets into separate dirs and Read
the same frame index from each.

## Step 4 — Optional: assemble a clip to send

```sh
ffmpeg -y -framerate 12 -i /tmp/prev/out/frames/frame_%06d.png -pix_fmt yuv420p /tmp/prev/preview.mp4
```

Use `SendUserFile` to surface a representative frame or the assembled clip when the
look is the deliverable.
