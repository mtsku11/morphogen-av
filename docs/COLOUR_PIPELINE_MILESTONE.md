# Colour Pipeline Milestone — 16-bit interchange + tagged export

Tier 5.6 of `docs/DEFERRED_WORK_HANDOFF.md`. Contract written 2026-07-07.

## Origin & Goal

The internal path is f32 end-to-end, but PNG I/O quantizes to 8-bit between
chain stages and at export — an 8-bit round-trip per stage visibly bands
gradients by stage 3. This milestone: **16-bit PNG as an opt-in interchange
format** for chain stages and sequence renders, and **explicit Rec.709
tagging** on the ProRes export so output lands correctly in Resolve/FCP.

**What already exists (verified 2026-07-07):**
- `save_png_with_bit_depth(image, path, 8|16)` + `float_to_u16` in
  `morphogen-cli/src/imaging.rs`; only the feedback commands expose
  `--output-bit-depth`, and `chain.rs` pins the feedback stage to a const.
- `load_image_f32` decodes via the image crate's `to_rgba32f()` — **16-bit
  PNGs already load at full precision**. The read half needs zero work.
- The SwiftUI ProRes export path
  (`VideoToolboxProResExportPlan.swift` / `ProResImageSequenceExporter.swift`)
  carries **no colour tags today**.

## Invariants restated (untouched by this milestone)

- Stateful checkpoints resume from unquantized RGBA32F state buffers — this
  milestone is about *inter-stage* and *export* fidelity only.
- **Off case:** with 8-bit selected (the default everywhere), every artifact
  is byte-identical to pre-slice output. Pinned per surface.

## Slices

### Status: MILESTONE COMPLETE (2026-07-08)

- **S1+S2** (Sonnet build; the agent died on its session limit during the
  final fmt check with everything built — orchestrator completed
  verification): cargo 652 → **663/0**, clippy clean, zero new fmt diffs.
  Banding proof pinned as a test AND reproduced:
  shallow-gradient two-stage chain at depth 8 collapses to **6 distinct
  values** (max quantization error **0.001953**) vs depth 16 keeping **all
  256 columns distinct** (error **0.000000**). Live shell evidence: IHDR bit
  depth 8 vs 16 confirmed on real chain outputs; a depth flip on the same
  output dir **refuses** via the chain-record spec identity; both manifests
  record `interchange_bit_depth`. Composition 16-bit scene smoke green.
  Off-case pins: chain absent≡8 byte-identity + three sequence-command
  off-case pins + queue add→run at 16 (video-vocoder).
- **S3** (orchestrator-built inline): Rec.709 tagging on the writer settings
  + per-buffer CV attachments. Observed: untagged export probes
  `smpte170m/unknown/unknown` → tagged probes **bt709/bt709/bt709**; decoded
  frame MD5 **identical** pre/post (metadata-only). swift 141 → **142/0**.

### S1 — chain interchange depth (where the loss actually is)

`render-chain` spec gains a top-level optional `"interchange_bit_depth"`:
`8` (default; absent ⇒ 8 ⇒ pre-slice specs byte-identical, pinned) or `16`
— any other value refused at whole-spec validation. When 16, **every stage
writes 16-bit PNGs** (including the final stage — the chain's output is
interchange too), via the existing `save_png_with_bit_depth`. Stage inputs
need no change (`load_image_f32` full-precision). The feedback stage keeps
its own contracted depth const for its checkpoint artifacts; its *stage
output* follows the spec depth.

Reproducibility surface: the depth joins `chain-manifest.json` AND the
`chain-record.json` re-run identity — a changed depth is a changed spec ⇒
the existing refusal semantics fire for free (pin it). The composition path
inherits chain specs verbatim, so scene specs carry the field with zero
composition changes (assert one composition smoke renders with a 16-bit
scene).

**The banding proof (falsifiable):** a two-stage chain on a shallow gradient
(e.g. a 16-step-wide luma ramp through two near-identity stages). Measure
distinct-value counts / max quantization error of the final frame at depth 8
vs 16 — 16 must strictly reduce quantization error vs the f32 reference
(compute the same chain in-process as reference). Report the numbers; Read
the frames (banding visible at 8 on a boosted-contrast crop is a bonus, the
number is the proof).

### S2 — `--output-bit-depth` rollout to stateless sequence renders

The flag (default 8, validated 8|16, the feedback commands' existing
convention) rolls out to the stateless sequence commands that write PNG
sequences (rutt-etra, channel-shift, palette-quantize, retro-static,
pixel-sort, cascade-collage, block-collage, conv-blend, dispersion,
fluid-mosaic, coagulated-blend, video-vocoder, generate-frames, downscale-
frames). Shared helper, mechanical per-command wiring; manifest gains
`output_bit_depth` **only when 16** (skip-when-default ⇒ pre-slice manifests
byte-identical). Queue tasks persist it serde-default-8 (pre-slice JSON
byte-identical, pinned); add→run byte-identity smoke on one representative
command at 16. Off-case pins: one byte-identity test per representative
command shape (not all 14 — pick 3), plus the shared-helper unit tests.
No SwiftUI depth picker in this milestone (export goes through ProRes; the
16-bit PNG surface is a CLI/queue workflow) — revisit on demand.

### S3 — Rec.709 tagging on the ProRes export (Swift)

The VideoToolbox/AVFoundation export attaches explicit colour metadata:
primaries ITU-R 709, transfer function ITU-R 709, YCbCr matrix ITU-R 709
(`AVVideoColorPropertiesKey` on the writer input settings, and/or
`kCVImageBufferColorPrimaries…` attachments on the pixel buffers — **use
context7 for the current API** before writing code; both surfaces must
agree). Proof: export a short clip and read the tags back with
`ffprobe -show_streams` (`color_primaries=bt709`, `color_transfer=bt709`,
`color_space=bt709`) — observed output, not asserted code. Swift tests pin
the settings-dictionary construction (pure fn). 16-bit *input* PNGs into the
exporter are explicitly out of scope here (the exporter reads what the app
renders today); tagging is metadata-only and must not change pixel data
(pin: exported frames byte-identical pre/post tagging via ffmpeg frame
extraction, minus container metadata).

## Acceptance criteria

Per slice: cargo/swift baseline → after with numbers; clippy clean (the ~54
pre-existing fmt-dirty lines stay untouched); the off-case byte-identity
pins named; S1's quantization-error numbers; S3's ffprobe output shown.
No `unwrap()` outside tests.
