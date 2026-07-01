# Retro Static Milestone — deliberate scanline-filter misread glitch

## Origin & Goal

Discovered by accident: an exploratory Python diagnostic script decoded a PNG's
compressed scanline data while assuming the wrong bytes-per-pixel and no
adaptive-filter reconstruction. Real PNG rows are `[filter-type byte][filtered
pixel bytes at the real stride]`; the buggy reader skipped one byte per row and
walked pixel data at a stride computed from the *wrong* channel count. Two things
compounded: (1) filter residuals (small deltas, not absolute colour) were
displayed as if they were raw pixel values, and (2) the per-row start position
drifted further out of sync every row, shearing the image sideways. The result
looked like analog TV static / a CRT losing sync. The user asked to reproduce it
intentionally and ship it as a real effect.

Reproducing the exact *file-format* bug isn't possible inside the render graph
(by the time an effect runs, PNGs are already correctly decoded to
`ImageBufferF32`). Instead this effect **simulates the same mechanism in pixel
space**: it deterministically (re-)encodes the source as if it were a
scanline-filtered image, then deliberately decodes that simulated stream at a
different, "wrong" stride. Same qualitative look, no file I/O involved, and
because every step is integer byte arithmetic (mod 256, no trig, no float
accumulation), CPU and Metal should be **bit-identical** — a stronger parity
story than most existing effects in this codebase.

---

## Mechanism

Per pixel `(x, y)`, channel slot `c` in `0..real_bpp` (channel `c mod 3` maps to
R/G/B cyclically; slot `3` always encodes a constant 255 "alpha" byte, mirroring
a real RGBA PNG; slots beyond that are 0 padding — a creative-only extension
beyond real PNG semantics):

1. **Encode** (simulate a PNG adaptive filter): `raw(x, y, slot)` is the source's
   quantized 8-bit channel value (or 255 / 0 for the synthetic slots). The
   **filtered** byte is `raw(x,y,slot) - predictor(...) mod 256`, where the
   predictor (`None`/`Sub`/`Up`/`Average`/`Paeth`) reads *raw* (never filtered)
   neighbour bytes — exactly the real PNG filter spec, so this step is
   embarrassingly parallel (no raster dependency chain across filtered output).
   Each row is conceptually `[filter-type marker byte][w * real_bpp filtered
   bytes]`, matching real PNG row layout.
2. **Misread** (the deliberate bug): re-slice that same byte stream assuming a
   row stride built from `assumed_bpp` instead of `real_bpp` (still skipping one
   marker byte per assumed row, but never validating or using it). The resulting
   byte at each output position is a pure, deterministic function of
   `(x, y, real_bpp, assumed_bpp, filter)` — a direct index remap, no state.
3. Reinterpret the misread bytes as an RGB triple (`/255`), alpha `1.0`.
4. Blend the glitched result with the original source by `strength`.

`real_bpp == assumed_bpp` ⇒ no shear, only filter-residual noise (a milder,
non-drifting look). `real_bpp != assumed_bpp` ⇒ progressive per-row shear (the
"analog desync" look). `filter = None` ⇒ filtered bytes equal raw bytes (no
residual noise; shear alone if bpp differs).

Stateless, single-source, per-frame: a frame depends only on `(source frame,
settings)`. Flicker across a real clip comes from the *source* content changing
frame to frame, not from any internal state.

---

## Settings (`RetroStaticSettings`)

- `real_bpp: u32` — simulated encoder's bytes/pixel (3 = RGB, 4 = RGBA typical).
- `assumed_bpp: u32` — the "wrong" decoder's bytes/pixel (the shear knob).
- `filter: ScanlineFilter` — `None | Sub | Up | Average | Paeth`.
- `strength: f32` in `[0, 1]` — blend toward the glitch.

**Off case (byte-identical passthrough):** `strength == 0.0` short-circuits to
the source verbatim (no computation, not just a zero-weighted blend, so it's
exact regardless of float rounding).

---

## Acceptance Criteria

- **A1 Determinism** — identical `(source, settings)` ⇒ byte-identical output.
- **A2 Off case** — `strength = 0.0` ⇒ output equals source exactly.
- **A3 On-vs-off differs** — `strength > 0` measurably changes the frame.
- **A4 Shear vs no-shear** — `real_bpp == assumed_bpp` differs from
  `real_bpp != assumed_bpp` (the drifting-shear knob is real).
- **A5 CPU = ground truth; Metal near-bit-identical** — integer-domain algorithm
  ⇒ gate well under `METAL_CPU_PARITY_EPSILON` (expect ~0, not just within
  tolerance).
- **A6 No `unwrap()` in library code**; errors via
  `RenderError::InvalidRetroStaticSettings`.

## Build Order

1. CPU reference + tests (this slice).
2. CLI (`render-retro-static-sequence`, `--backend cpu|metal`).
3. Metal kernel, parity-gated per frame like `channel_shift`/`palette_quantize`.

## Non-goals

- Not a literal PNG-file decoder; no dependency on real file bytes.
- No cross-frame state (unlike `datamosh`); flicker comes from source motion.
