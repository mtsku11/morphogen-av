# Spatial Matte Milestone — analysis-derived per-pixel modulation

Tier 5.4 of `docs/DEFERRED_WORK_HANDOFF.md`. Contract written 2026-07-07.

## Origin & Goal

Every modulation route today is one scalar per frame — the whole frame gets the
same knob value. This milestone makes the CV-to-pixel leap: a **matte** is a
per-pixel `[0,1]` field computed from analysis of a source, and the effect is
blended in only where the matte says:

```
out(x,y) = matte(x,y) * effect(B)(x,y) + (1 - matte(x,y)) * B(x,y)
```

The A-modulates-B thesis at per-pixel resolution: the effect strikes only
where A moves / is bright / has edges.

**Reality check vs the handoff sketch:** there is no single shared
sequence-render loop in `render.rs` — every effect owns its frame loop. So the
*machinery* is shared (one module), but the *integration* is per-command flags
on named commands, exactly like `--modulate` was rolled out.

## Non-goals (MVP)

- Stateful effects. The matte-gates-composite-vs-gates-state-update question
  is real; **composite-only is the safe answer but even that is deferred** —
  S1/S2 are stateless commands only. Declared explicitly so nobody wires
  `--matte` onto feedback/datamosh without a contract extension.
- The chain-stage `matte_blend` form. It intersects the Tier 3.2 two-source
  chain-graph design (per-stage second-source media). **Flagged here, not
  solved** — the handoff's own instruction. Build it with 3.2.
- Drawn/painted mattes, matte preview UI, softness/blur knobs (`--matte-gain`
  is the only shaping knob; more only on demand).

## Mechanic

New module `crates/morphogen-render/src/matte.rs`:

- `MatteSource` enum: `ALuma`, `AFlow`, `AEdge`.
- `compute_matte_field(prev: Option<&ImageBufferF32>, current: &ImageBufferF32, source, gain) -> MatteField`
  (a `width × height × f32` field, values clamped `[0,1]`):
  - **`a-luma`** — Rec.709 luma of the matte frame, already absolute `[0,1]`,
    then `* gain`, clamp.
  - **`a-flow`** — per-pixel Lucas–Kanade flow magnitude between
    `prev → current` (reuse `pyramidal_lucas_kanade_flow_cpu`), normalized by
    the declared full scale `MATTE_FLOW_FULL_SCALE_PX = 8.0` px, `* gain`,
    clamp. **Frame-zero rule (declared):** frame 0 has no prior ⇒ the matte is
    all zeros ⇒ frame 0 is passthrough B. No peeking at frame 1 — a matte
    frame's field depends only on frames ≤ its index (keeps the door open for
    stateful integration later).
  - **`a-edge`** — per-pixel Sobel gradient magnitude on luma (the
    `frame_mean_edge_density` kernel, kept per-pixel instead of averaged;
    border pixels 0), lifted by the declared `MATTE_EDGE_GAIN = 5.0` (the
    cascade-collage `EDGE_DETECT_GAIN` precedent — raw Sobel magnitudes are
    ~0.05–0.3), then `* gain`, clamp. **Fixed gains, not per-frame peak
    normalization** — per-frame peaks flicker; determinism and temporal
    stability beat auto-levels. (The relative-normalization trap from
    `video-audio-route-readout` is thereby avoided entirely.)
- `apply_matte(effected: &ImageBufferF32, original: &ImageBufferF32, matte: &MatteField) -> ImageBufferF32`
  — the blend above, f32, alpha from `effected`. Dimension mismatch (matte
  media vs carrier) ⇒ `IncompatibleInputs`.
- Algorithm id `matte_blend_cpu_v1` recorded in the manifest **alongside** the
  effect's own id (the matte is a post-blend, not a new effect); the manifest
  gains a `matte` block (source, gain, media path) only when a matte is
  active — absent block ⇒ pre-slice manifests byte-identical.

## CLI (S1 commands)

Three stateless commands covering both media shapes:

- `render-rutt-etra-sequence` — has `--source-a-dir` (two-source A→B): matte
  media defaults to Source A when present.
- `render-channel-shift-sequence` — same A-present shape (flow-driven mode).
- `render-palette-quantize-sequence` — single-source: matte media must be
  given explicitly.

Flags (identical on all three): `--matte <a-luma|a-flow|a-edge>`,
`--matte-frames <dir>` (the analysis media; **required** unless the command
has a Source A dir given, which is the default), `--matte-gain <f32>`
(default 1.0, finite ≥ 0). `--matte-frames` without `--matte` is an error
(dead flag = user confusion). Matte frame count must cover the rendered
range (shorter ⇒ clear error; longer ⇒ excess ignored, the modulator-media
convention).

## Slices

- **S1 — CPU matte blend, direct CLI** on the three commands + unit tests +
  the half-frame readout (below). **DONE (2026-07-08,** Sonnet build,
  orchestrator-verified: cargo 633 → **647/0**, clippy clean, zero new fmt
  diffs; matte-1/matte-0 byte-identity + frame-zero flow rule pinned as
  tests; half-frame crop-compare **28800/28800 both halves** re-run
  independently, frame Read (clean gradient|displaced split). Declared
  deviation, accepted: channel-shift/palette-quantize had NO manifest
  pre-slice (stdout-only convention), so they write `manifest.json` only
  when a matte is active — the off case stays exactly pre-slice (no file);
  rutt-etra's existing manifest gains the block only when active.)
- **S2 — Metal + queue.** A trivial per-pixel blend kernel, parity-gated
  frame-by-frame like every kernel (`METAL_CPU_PARITY_EPSILON`); matte-field
  computation stays CPU (LK flow on GPU is already backend-segregated — reuse
  only if free, do not port). Queue tasks gain serde-skip matte fields
  (pre-slice JSON byte-identical, pinned); add-time validation; add→run
  byte-identity smoke. SwiftUI: matte picker row on the three panels
  (source/gain + frames dir), no-matte arg arrays pinned byte-identical.

## Anchors (falsifiable)

1. **Matte-1 identity:** an all-white `a-luma` matte (gain 1 on a white
   matte dir) ⇒ output byte-identical to the effect **without** `--matte`.
2. **Matte-0 identity:** all-black matte ⇒ output byte-identical to the plain
   carrier (pure passthrough).
3. **The half-frame readout (the cleanest possible visual proof):** a
   half-black/half-white A as `a-luma` matte over a strong effect (rutt-etra
   at a big displacement) ⇒ the white half shows scanline displacement, the
   black half is pixel-identical to the carrier. Assert programmatically
   (crop-compare both halves) AND Read the frame.
4. **Flow matte frame-zero rule:** frame 0 byte-identical to the carrier.
5. **Determinism:** two runs byte-identical; S2 adds add→run and CPU↔Metal
   parity.

## Acceptance criteria

Per slice: cargo (and swift for S2) baseline → after with numbers; clippy
clean (fmt: the ~54 pre-existing dirty lines stay untouched); anchor evidence
(byte-identity assertions in tests, the half-frame Read, frame-delta numbers
for matte-on vs matte-off). No `unwrap()` outside tests.
