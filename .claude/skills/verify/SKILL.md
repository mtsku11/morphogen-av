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

## Step 4 — Verdict block

Print exactly one block, then stop:

```
verify — <date>
  gates: <which ran, e.g. clippy + test -p morphogen-render + visual>
  result: <pass / fail>
  tests: <baseline N → now M, or "n/a">
  visual: <what the rendered frame showed, or "n/a">
  notes: <anything skipped (e.g. shader-check SKIP) or surprising>
```
