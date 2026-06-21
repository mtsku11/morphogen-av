---
name: parity
description: Prove a Morphogen AV granular-pool render is path-independent — render the same job via the direct CLI and the queue add→run path with shared knobs, byte-compare every frame, and show the persisted manifest knobs. Use in the inner loop when adding or tuning a pool knob, before the durable smoke-test assertion exists.
---

# parity (morphogen-av)

The non-negotiable invariant: identical inputs + settings ⇒ bit-reproducible
output, whichever path produced it. This is the **exploratory** complement to the
per-feature determinism assertions in `crates/morphogen-cli/tests/smoke.rs` —
reach for it while iterating on a new pool knob, then encode the proven case as a
smoke-test assertion. It does not replace that gate.

## Step 1 — Get a fixture

Any paired modulator/carrier PNG frame dirs work. For a fixture whose output
*colour reveals the selected source frame* (so you can eyeball what a knob did,
not just that two paths agree), scaffold one with `/fixture`:

```sh
scripts/make-fixture.sh /tmp/fix --frames 4              # solid-colour readout
scripts/make-fixture.sh /tmp/fix-av --frames 4 --with-chirp   # + RMS/STFT caches for audio/centroid
```

## Step 2 — Run the cross-path check

Pass the **selection knobs** after `--`; they go to *both* the direct render and
the queue add→run. The script renders both, byte-compares every frame, and prints
the queue bundle's persisted knob block.

```sh
scripts/parity-check.sh /tmp/fix/modulator /tmp/fix/carrier -- \
  --rearrangement 1.0 --pool-window 3 \
  --anti-repeat-weight 0.4 --coherence-weight 0.5 --coherence-reach 4
```

k=2 centroid / audio-weighted (needs a `--with-chirp` fixture):

```sh
scripts/parity-check.sh /tmp/fix-av/modulator /tmp/fix-av/carrier -- \
  --rearrangement 1.0 --audio-weight 2.0 \
  --modulator-rms-cache /tmp/fix-av/modulator-rms.json \
  --carrier-rms-cache  /tmp/fix-av/carrier-rms.json \
  --modulator-centroid-cache /tmp/fix-av/modulator-stft.json \
  --carrier-centroid-cache  /tmp/fix-av/carrier-stft.json
```

Add `--backend metal` to the flags to run both paths on the GPU (queue-run gates
Metal vs CPU per frame internally). `KEEP=1 scripts/parity-check.sh ...` keeps the
temp workdir (path printed) so you can Read the diverging frames.

Note: pass selection knobs only — not `--grain-cache-dir` (direct-only) or
`--project-path` (queue-only). The script already adds `--no-grain-cache`.

## Step 3 — Read the verdict

- `parity: OK   N/N frames byte-identical (direct == queue)` — the knob is
  path-independent; exit 0. Safe to promote into a smoke-test assertion.
- `parity: FAIL ... first divergent: frame_00000X.png` — exit 1. A queue-vs-direct
  divergence is a real determinism bug (a knob the queue task doesn't carry, or a
  non-deterministic selection). Re-run with `KEEP=1` and Read both copies of the
  named frame. See [[f32-json-roundtrip-test-trap]] before assuming a mismatch is
  the renderer's fault when you later assert persisted f32 knobs in tests.

This covers the queue↔direct axis. CPU↔Metal parity is gated separately by the
`morphogen-metal` runtime tests during `cargo test` (see `/verify`).
