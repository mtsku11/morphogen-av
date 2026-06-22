---
name: verify
description: Verify a Morphogen AV change before reporting it done — run the cheapest sufficient gates (clippy, targeted tests, shader compile, swift, and a visual PNG check) and print one verdict block with evidence. Project-local; overrides the global verify.
---

# verify (morphogen-av)

Prove the diff works with the cheapest set of gates that covers it. Don't run the
full workspace when targeted checks suffice. Show evidence — never assert success.

## Step 0 — Baseline first (only if you haven't already)

Before relying on "no regressions," you need a number to diff against. If you
captured the pre-change pass/fail counts earlier, reuse them; otherwise note that
you can't claim a delta. Repo baseline is ~117 Rust tests passing across 7 crates.

## Step 1 — Classify the diff

```sh
git status --short && git diff --name-only HEAD
```

Map changed paths to gates (multiple can fire; deduplicate):

| Changed paths | Gates |
|---|---|
| any `crates/**/*.rs` | `clippy` + targeted `cargo test -p <crate>` (workspace if cross-cutting) |
| `crates/morphogen-render/**`, `morphogen-cli` render paths, any renderer math | also the **visual PNG check** (Step 3) |
| `crates/morphogen-metal/**`, `crates/morphogen-metal/shaders/*.metal` | `./scripts/check-shaders.sh` + `cargo test -p morphogen-metal` (runtime CPU/Metal parity) |
| `apps/macos/**/*.swift`, `Package.swift` | `swift build && swift test` |
| docs / `*.md` only | no code gates — just confirm links/commands are accurate |

## Step 2 — Run the code gates

```sh
cargo clippy -p <crate> --all-targets -- -D warnings   # enforces "no unwrap() in libs"; widen to --workspace if cross-cutting
cargo test -p <crate>                                   # targeted; --workspace only for cross-cutting changes
./scripts/check-shaders.sh                              # offline shader compile (skips cleanly if Metal Toolchain absent)
swift build && swift test                               # only if Swift/Package changed
```

Determinism invariant: a renderer change must keep identical inputs+settings
bit-reproducible, and any Metal kernel must hold CPU parity. The
`morphogen-metal` runtime tests are the parity gate; don't skip them for GPU work.

## Step 3 — Visual PNG check (render/effect changes)

The verification loop for this app is visual. Render a deterministic fixture and
actually look at it (Read renders PNGs as images):

```sh
cargo run -q -p morphogen-cli -- render-test /tmp/morphogen-verify.png
```

Then Read `/tmp/morphogen-verify.png` and confirm it looks right. For changes to a
specific effect, render that effect on a small fixture instead (see `/preview`),
or extract a frame from a video output with `ffmpeg -i out.mov -frames:v 1 f.png`.

### Required: off-vs-on readout for any output-affecting feature

A new effect, parameter, or **selection/scheduling knob** is not verified by tests
and parity alone — those prove determinism, not that the knob *does what it
claims*. You must also show it changing the output, both visually and
quantitatively, on a readout fixture:

```sh
scripts/make-fixture.sh /tmp/v --frames 8 --size 96x96 --readout origin   # or default --readout frame
# render the SAME job with the knob off and on — ALWAYS --variation 0 (the pool
# render's 0.25 default injects a per-tile random alternate the schedulers never
# touch; it scatters the readout and hides the knob).
cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence /tmp/v/modulator /tmp/v/carrier /tmp/v/off --rearrangement 1.0 --variation 0 <knob>=off
cargo run -q -p morphogen-cli -- render-granular-mosaic-pool-sequence /tmp/v/modulator /tmp/v/carrier /tmp/v/on  --rearrangement 1.0 --variation 0 <knob>=on
scripts/frame-delta.py /tmp/v/off /tmp/v/on   # quantitative delta
```

Then **Read sampled frames from both** and state the difference — the number alone
never proves a look, and a look without a number is unfalsifiable. Pick the
readout to the knob's axis (`origin` = source location, `frame` = source frame);
see `/fixture`. Report both in the verdict's `visual:` line. Skip this only for
changes that cannot alter output (pure refactors, docs, serialization plumbing
already covered by `/parity`).

## Step 4 — Verdict block

Print exactly one block, then stop:

```
verify — <date>
  gates: <which ran, e.g. clippy + test -p morphogen-render + visual>
  result: <pass / fail>
  tests: <baseline N → now M, or "n/a">
  visual: <what the rendered frame showed; for a knob, off-vs-on delta + what the pixels showed, or "n/a">
  notes: <anything skipped (e.g. shader-check SKIP) or surprising>
```
