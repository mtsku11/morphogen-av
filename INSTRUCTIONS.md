# Morphogen AV — Operating Instructions

**Audience: any agent or model that needs to actually *use* this app.** This is
the how-to manual. It assumes the orientation in [`README.md`](README.md) and
the invariants in [`CLAUDE.md`](CLAUDE.md), and it tells you the concrete moves:
how to ingest sources, drive every effect, patch modulation, run the queue,
chain and compose, preview, and export — from both the CLI and the macOS app.

When an exact flag isn't listed here, ask the binary: every subcommand supports
`cargo run -p morphogen-cli -- <command> --help`, and that help text is the
source of truth (this doc is derived from it). [`docs/REFERENCE.md`](docs/REFERENCE.md)
is the fuller flag-level catalog; the per-effect `docs/*_MILESTONE.md` files are
the acceptance contracts.

---

## 1. The mental model (read this first)

- **Two sources, asymmetric roles.** **Source A** is the *modulator* — the
  thing you analyze (its motion, audio, luma, spectrum). **Source B** is the
  *carrier* — the material that gets reshaped. Output is B, transformed by
  analysis derived from A. Single-source effects use only B; audio effects use
  A/B as WAVs. A handful of "mutual" blend effects mangle A and B together.
- **Determinism is the contract.** Same inputs + same settings ⇒ byte-identical
  output. Never introduce a path that breaks this (the one sanctioned exception
  is `datamosh-bitstream`, which is explicitly non-deterministic and lives
  outside the render graph).
- **CPU is ground truth; Metal is gated against it.** `--backend metal` (where
  offered) is validated frame-by-frame against the CPU reference. If parity
  fails, the render aborts — you don't ship an unvalidated GPU frame.
- **Two ways to render, same engine:** a **direct** `render-*` command (one-off,
  scriptable) or the **persisted queue** (`queue-add-*` then `queue-run-*`,
  survives interruption, records provenance, emits a ProRes-ready bundle). The
  queue path is byte-identical to the direct path.
- **Analysis is cached sidecar data**, regenerable from source + settings, and
  fingerprinted so a stale cache is never silently reused.

### The universal workflow

```
ingest → (analyze) → render  →  (preview to tune)  →  export
 §3        §4          §6/§7        app §10             §9
```

---

## 2. Build & run

```sh
cargo build --workspace          # build the Rust engine + CLI
cargo test  --workspace          # full Rust suite (baseline: 753 passing, 0 failing)
cargo run -p morphogen-cli -- <command> [args]     # drive the engine

cd apps/macos                    # (or run from repo root; package is here)
swift build && swift test        # app shell + service tests (baseline: 158 passing)
swift run MorphogenMacApp        # launch the native app
```

FFmpeg/FFprobe are **optional and external** — only the media ingest/probe and
bitstream-datamosh paths need them, and they return a clear error if absent.
Nothing is vendored.

---

## 3. Ingesting sources (proxies)

Effects read **PNG frame directories** (video) and **WAV files** (audio), not
raw `.mov`/`.mp4`. Convert first:

```sh
# Probe a file (uses external ffprobe if present)
cargo run -p morphogen-cli -- probe /path/to/clip.mov

# Video → PNG frame directory
cargo run -p morphogen-cli -- extract-frames /path/to/clip.mov /tmp/a-frames --fps 24 --max-frames 120

# Audio → WAV
cargo run -p morphogen-cli -- extract-audio /path/to/clip.mov /tmp/a.wav --sample-rate 48000

# Deterministic box-average downscale (preview utility; --scale 1 is identity, no manifest)
cargo run -p morphogen-cli -- downscale-frames /tmp/a-frames /tmp/a-quarter --scale 4 --max-frames 48
```

In the **app**, picking Source A/B does this automatically (AVFoundation, with
an FFmpeg fallback) and also generates RMS/STFT sidecars.

---

## 4. Analysis caches (sidecars)

Optional to precompute (renderers regenerate on demand), but caching makes
re-renders fast and lets you inspect the analysis. Each writes a JSON sidecar
tagged with algorithm id + source fingerprint.

```sh
cargo run -p morphogen-cli -- cache-rms    /tmp/a.wav /tmp/a-rms.json    --window-size 2048 --hop-size 512
cargo run -p morphogen-cli -- cache-stft   /tmp/a.wav /tmp/a-stft.json   --fft-size 1024 --hop-size 256 --window hann
cargo run -p morphogen-cli -- cache-onsets /tmp/a.wav /tmp/a-onsets.json --fft-size 1024 --hop-size 256 --window hann
cargo run -p morphogen-cli -- cache-luminance-flow /tmp/a-frames/frame_000000.png /tmp/luma-flow --width 256 --height 256
```

For optical-flow-driven effects, pass `--flow-cache-dir /tmp/flow` and the
per-frame flow sidecars are written once and reused across re-renders (the
slowest per-frame step, so this matters).

---

## 5. The modulation matrix (the modular-synth core)

Almost every numeric/enum knob can be **driven by a signal** instead of held
constant. This is the single most important feature to understand.

### Route syntax

```
--modulate "<target>=<source>[:<scale>[,<offset>]][@hold|@smooth]"
```

Repeatable — pass `--modulate` once per routed knob. `<target>` is the knob name
(e.g. `feedback_mix`, `displacement_depth`, `strength`, `shift_r_x`). The routed
value is `source_value * scale + offset`, then **clamped to the knob's declared
range** (it never errors — an out-of-range envelope can't abort a render).

### Sources

| Source | Needs | Notes |
|---|---|---|
| `audio-rms` / `audio-onset` / `audio-centroid` | `--modulator-audio a.wav` | peak-normalized envelope from A's audio |
| `luma` / `flow` | `--modulator-frames a-frames/` | per-frame brightness / optical-flow magnitude |
| `edge-density` | `--modulator-frames a-frames/` | Sobel edge density, peak-normalized |
| `lfo(shape[,rate_hz[,phase]])` | *nothing* | `sine\|triangle\|square\|saw`; pure function of frame time |
| `breakpoints(t0:v0;t1:v1;…)` | *nothing* | piecewise-linear envelope in seconds; also the format a recorded gesture becomes |
| `<name>.<source>` | `--named-modulator-audio name=path` / `--named-modulator-frames name=path` | lets different routes read different media on one render |
| MIDI CC | `.mid` file + per-slot CC number (app slot / see MIDI milestone) | a CC lane drives the knob |

### Sampling

- Default per-render sampling is set with `--modulation-sampling hold|smooth`
  (`hold` = step, `smooth` = linear interpolation between envelope points).
- Override **per route** with a trailing `@hold` / `@smooth`.
- Envelope times are sampled against the render's `--frame-rate` (or the
  master clock in a composition).

### Examples

```sh
# LFO on Rutt-Etra depth — no modulator media at all (the canonical demo)
--modulate "displacement_depth=lfo(sine,0.5):128,128"

# A's audio RMS drives datamosh amount, smoothed
--modulator-audio /tmp/a.wav --modulate "amount=audio-rms:2.0@smooth"

# Two knobs, two different named modulator WAVs on one render
--named-modulator-audio bass=/tmp/bass.wav --named-modulator-audio hats=/tmp/hats.wav \
--modulate "feedback_mix=bass.audio-rms:0.5" --modulate "feedback_amount=hats.audio-onset:8"
```

### Rules that will bite you if ignored

- **No `--modulate` flags = the exact unmodulated path** (byte-identical to the
  effect with static knobs). Adding an inert route must not change output.
- **Values clamp, never error.** If a knob's range is `±8` and your scaled
  signal hits 40, it clamps to 8 — silently. Size your `scale`/`offset` to the
  knob's real range or the modulation will look "stuck."
- **Integer/enum knobs**: clamp to range *then* round ties-away-from-zero; enum
  targets map the rounded integer over the declared variant order.
- **Stateful effects checkpoint their routes.** Resuming a feedback/datamosh/
  morphogenesis render with a *changed* route is **refused**, not silently
  applied — change the route ⇒ start a fresh render.

Full contract: [`docs/MODULATION_MATRIX_MILESTONE.md`](docs/MODULATION_MATRIX_MILESTONE.md),
[`docs/LFO_MODULATION_MILESTONE.md`](docs/LFO_MODULATION_MILESTONE.md),
[`docs/MIDI_MODULATION_MILESTONE.md`](docs/MIDI_MODULATION_MILESTONE.md).

---

## 6. The effect catalog (direct command + queue pair)

Every effect has a direct `render-*` command; most also have a `queue-add-* …`
/ `queue-run-*` pair (§7). Run `<command> --help` for the exact flags — the
**off-case** column is the knob setting that makes the effect a passthrough
(use it as the "off" half of an off-vs-on check, §11).

### Displacement

| Effect | Direct command | Sources | Off-case | Metal |
|---|---|---|---|---|
| Flow displace | `render-frame-sequence` (two-source) / `render-two-source` (still) | A+B | `--amount 0` | ⚡ |
| Flow feedback | `render-feedback-sequence` | A+B | `--feedback-amount 0` | ⚡ |
| Rutt-Etra | `render-rutt-etra-sequence` | B (or A+B via two-source) | `--displacement-depth 0` | ⚡ |

Flow feedback is the reference stateful effect: `--flow-source optical-flow`
(default) vs `luminance`, `--structure-mix` re-injects carrier detail,
`--stop-after-frame N` + re-run proves resume, `--reset-at-frame N` restarts the
loop. Use **small** `--feedback-amount` with optical flow (real pixel motion).

### Fluid / Advection

| Effect | Direct command | Sources | Off-case | Metal |
|---|---|---|---|---|
| Procedural fluid dye | `render-fluid-advect-sequence` | B | `--advect 0 --reinject 0` (holds frame 0); `--reinject 1` = source verbatim | ⚡ |
| Two-source flow advect | `render-fluid-advect-two-source-sequence` | A+B | `--reinject 1` | ⚡ |
| Self optical-flow advect | `render-optical-flow-advect-sequence` | B | `--reinject 1` (static clip ⇒ verbatim) | ⚡ |
| Field particles | `render-field-particles-sequence` | B | `--advect 0` (static grid) | ⚡ |

Velocity field must be **steady** for coherent swirls; match vortex scale to
canvas size (see [[faux-fluid-advect]] memory).

### Blend / Mosaic (mutual A×B)

| Effect | Direct command | Sources | Off-case | Metal |
|---|---|---|---|---|
| Convolutional blend (video) | `render-convolutional-blend-sequence` | A+B | `--amount 0` | ⚡ |
| Coagulated flow blend | `render-coagulated-blend-sequence` | A+B | `--coagulation-strength 0 --randomness 0 --bias 0` | CPU |
| Dispersion blend | `render-dispersion-blend-sequence` | A+B | (see `--help`; low strength) | CPU |
| Fluid colour-sort mosaic | `render-fluid-mosaic-sequence` | A+B | `--cohesion 0 --repulsion 0 --fluid-strength 0 --jitter 0 --settle-iterations 0` | CPU |
| Block collage | `render-block-collage-sequence` | A+B | `--threshold 0` = all A | CPU |

### Feedback / Datamosh

| Effect | Direct command | Sources | Off-case | Metal |
|---|---|---|---|---|
| Controlled datamosh | `render-datamosh-sequence` | A+B | `--keyframe-interval 1` (snaps to B every frame) | ⚡ |
| Real bitstream datamosh | `datamosh-bitstream` | one video file | — (non-deterministic, needs ffmpeg) | n/a |
| Cascade collage | `render-cascade-collage-sequence` | source-less / B tiles | `--scrib-amp-scale 0 --morph-rate 0 --frame-hue-rate 0` | CPU |
| Trail cascade | `render-cascade-trails-sequence` | B | `--advect 0` (static grid) | CPU |

Datamosh presets: `custom`, `codec-bloom`, `structured-melt`, `macroblock-rot`,
`vector-shuffle`, `scanline-smear`, `codec-engrave`. Non-custom presets print
their resolved knobs (overrides aren't silent). `datamosh-bitstream` operations:
`pframe-duplicate` (bloom), `remove-keyframe` (void mosh), motion transfer — it
is **intentionally outside** the queue/parity system and writes
`deterministic: false`.

### Generative

| Effect | Direct command | Sources | Off-case | Metal |
|---|---|---|---|---|
| Morphogenesis | `render-morphogenesis-sequence` (`render-morphogenesis-field` = debug V dump) | B (+ A for live inject) | `--pattern-mix 0 --displace 0` | CPU |
| Granular mosaic | `render-granular-mosaic-sequence` / `render-granular-mosaic-pool-sequence` (temporal pool) | A+B | `--rearrangement 0 --variation 0` | ⚡ (pool = CPU) |

Morphogenesis `--model gray-scott|fhn|lenia`; seeds where B's luma crosses
`--seed-threshold`; `--pattern-mix` colourizes growth, `--displace` pushes B
along ∇V. Model tuning traps are recorded in the morphogenesis memory files —
read them before re-tuning. Granular selection: `--selection luma|rgb`; the
pool variant (`…-pool-sequence`, CPU-only) adds whole-clip temporal matching by
colour+texture+audio (`--audio-weight`, `--texture-weight`, needs matching
`--readout` fixtures to verify — see [[granular-texture-dims]]).

### Post / Look

| Effect | Direct command | Sources | Off-case | Modulation targets |
|---|---|---|---|---|
| Retro static | `render-retro-static-sequence` | B | `--strength 0` | `strength` |
| Channel shift | `render-channel-shift-sequence` | B (+A for flow) | all `--shift-*-x/y 0` | `shift_r_x`…`shift_b_y` |
| Palette quantize | `render-palette-quantize-sequence` | B | `--mode posterize --levels 256` | `levels` (integer) |
| Pixel sort | `render-pixel-sort-sequence` | B (+A) | `--threshold-low > --threshold-high` (empty mask) | `threshold_low`, `threshold_high`, `direction`, `axis` |

### Audio / Cross-Synth

| Effect | Direct command | Sources | Off-case |
|---|---|---|---|
| Video vocoder | `render-video-vocoder-sequence` (`render-video-vocoder` = still) | A+B video | `--amount 0` |
| Spectral cross-synth | `render-spectral-cross-synth` | A+B WAV | `--amount 0` |
| Audio impulse convolution | `render-audio-impulse-convolution` | A(IR)+B WAV | silent A ⇒ identity |
| Audio→video route | `render-audio-video-route-sequence` | A WAV + B frames | `--amount 0` |
| Video→audio route | `render-video-audio-route` | A frames + B WAV | `--amount 0` |

Video vocoder `--mode match` (histogram-spec, the strong one) vs `gain`.
Spectral `--mode gain|filter|vocode` (vocode = real phase-vocoder, complex
inverse STFT keeping B's phase). Impulse convolution `--use-fft`,
`--use-per-channel-ir`, `--resample-impulse`.

---

## 7. The render queue (the quality/resume path)

Same engine as the direct commands, but persisted and interruption-safe, and it
emits the ProRes-ready bundle. Every effect above that has a queue pair follows
this lifecycle:

```sh
cargo run -p morphogen-cli -- queue-init /tmp/q.json

# Add a job (validated + rejected BEFORE it's written to the queue)
cargo run -p morphogen-cli -- queue-add-feedback-sequence /tmp/q.json \
  /tmp/a-frames /tmp/b-frames /tmp/out \
  --feedback-amount 2 --feedback-mix 0.72 --backend metal

cargo run -p morphogen-cli -- queue-inspect /tmp/q.json          # list jobs + status
cargo run -p morphogen-cli -- queue-run-feedback-sequence /tmp/q.json   # execute / resume next job
cargo run -p morphogen-cli -- queue-cancel /tmp/q.json job-0001  # skip a job
```

- `queue-add-*` mirrors the direct command's flags **plus** `--frame-rate`;
  `--modulate` routes persist onto the job and validate at add time.
- `queue-run-*` output is **byte-identical** to the direct render.
- The output bundle: `frames/` (PNG sequence), audio stems, `manifest.json`
  (algorithm id, resolved knobs, source/cache provenance, modulation block),
  and — for stateful effects — `checkpoint.json` + an RGBA32F state buffer.
- **Resume:** `--stop-after-frame N` on a stateful job writes a checkpoint;
  re-running continues from it. A changed input/setting/route is **refused**.

The full `queue-add-*` / `queue-run-*` list (one per effect, plus `-test`,
`-chain`, `-composition`, `-datamosh-bitstream`) is in
[`docs/REFERENCE.md`](docs/REFERENCE.md); the names follow the effect commands
in §6 verbatim (`render-X-sequence` → `queue-add-X-sequence` / `queue-run-X-sequence`).

---

## 8. Chains, composition, and generators

### Effect chains (`render-chain`)

Compose single-source stages into a pipeline from one JSON spec; each stage's
output feeds the next. Stages available: `retro_static`, `channel_shift`,
`palette_quantize`, `rutt_etra`, and the stateful `flow_feedback`. Each stage
takes an optional `modulation` block (LFO routes need no media). Re-running the
same spec into the same output dir **skips completed stages** and resumes an
interrupted stateful stage; a changed spec/input refuses.

```sh
cargo run -p morphogen-cli -- render-chain chain.json /tmp/b-frames /tmp/chain-out
# queue: queue-add-chain / queue-run-chain
```

Contract + spec schema: [`docs/EFFECT_CHAIN_MILESTONE.md`](docs/EFFECT_CHAIN_MILESTONE.md).

### Composition timeline (`render-composition`)

Arrange finished effect-chain **scenes** on a global timeline. Each scene is a
verbatim `render-chain` spec over its own `input_dir`; scenes assemble via hard
cuts or crossfades into `<output_dir>/frames/`, with a scene-fingerprint cache
and a reserved `master.` clock modulator (note the **dot**, e.g.
`displacement_depth=master.audio-rms@smooth` — a `:` collides with the
grammar). The whole spec validates before anything
renders.

```sh
cargo run -p morphogen-cli -- render-composition composition.json /tmp/comp-out
# queue: queue-add-composition / queue-run-composition
```

Contract: [`docs/COMPOSITION_MILESTONE.md`](docs/COMPOSITION_MILESTONE.md).

### Video oscillators (`generate-frames`)

A source-less deterministic pattern generator — writes an ordinary PNG frame
dir, so **any** effect/route/queue/chain can consume it as a synthetic Source A
or B. Off-case: `--rate 0` holds every frame at frame 0.

```sh
# <preset> and <output-dir> are positional; --rate 0 is the off-case
cargo run -p morphogen-cli -- generate-frames <preset> /tmp/osc-frames --rate 0.5 --frames 120
```

---

## 9. Export

Completed queue bundles export to ProRes `.mov` (VideoToolbox, audio stems
muxed in) from the app's export button. To inspect any video/ProRes output as
an image:

```sh
ffmpeg -i out.mov -frames:v 1 frame.png     # then Read frame.png
```

For a shareable H.264 preview of the "character" of a patch, `render-showcase`
writes named stills + a contact sheet + an optional MP4.

---

## 10. Driving the macOS app

Launch: `swift run MorphogenMacApp`. Layout is **persistent header + sidebar +
detail**:

1. **Header (always visible):** pick **Source A** and **Source B** (auto-extracts
   proxies + RMS/STFT sidecars). Set global **Render Quality**, **Export
   Format**, **ProRes FPS/Profile**. A collapsible **Sources & Proxies**
   disclosure exposes proxy fps/frame-limit and manual re-extraction.
2. **Sidebar:** every effect, grouped exactly as §6 (Displacement, Fluid/
   Advection, Blend/Mosaic, Feedback/Datamosh, Generative, Post/Look, Audio/
   Cross-Synth, Composition, Tools). Click one.
3. **Detail pane (selected effect only):** its knobs, a **More knobs**
   disclosure, per-slot **modulation routing** (pick a source; set scale/offset;
   add named modulators; choose LFO/MIDI/captured; attach a matte), a **Run**
   button (submits a **real persisted queue job**, identical to the CLI), and a
   status line.

### Quick Preview + performance capture

On effects that use the shared proxy pipeline (most video effects — not the
audio-file effects, bitstream datamosh, or composition), the detail pane ends
with a **Quick Preview** band:

- Set **Scale** (Full/½/¼/⅛) and **seconds**, hit **Quick Preview**: it
  downscales the proxies once and renders a few seconds of *this* effect through
  the same engine, then **loops** it (play/pause, frame counter). ~13× faster
  than full-res on an expensive effect. Navigating to a different effect clears
  the preview (press Quick Preview again for the new one).
- **Performance capture:** in a Rutt-Etra modulation slot, set a slot's source
  to **Captured** (arming it). Then in *any* eligible effect's Quick Preview,
  hit **record** and scrub the capture slider against the loop — the gesture is
  stored as a `breakpoints(…)` route on the armed slot, bit-exact. Recording
  restarts the loop at frame 0 so `t=0` aligns with frame 0.

Contract: [`docs/PERFORMANCE_CAPTURE_MILESTONE.md`](docs/PERFORMANCE_CAPTURE_MILESTONE.md),
[`docs/QUICK_PREVIEW_RESTORE_MILESTONE.md`](docs/QUICK_PREVIEW_RESTORE_MILESTONE.md).

---

## 11. Verifying your work (the project's evidence discipline)

Tests + parity prove **determinism**, but *not* that a knob does what it claims.
For any output-affecting change, also prove the look:

1. **Baseline first.** Capture pass/fail counts *before* touching anything
   (`cargo test --workspace`, `swift test`), then report the delta — "no
   regressions" needs a number.
2. **Off-vs-on on a readout fixture.** Render the effect twice — once at its
   off-case (§6 tables), once on — with `--variation 0` where a pool render
   would otherwise scatter output. **Read the PNG frames from both** and report
   the `scripts/frame-delta.py` number. A look without a number is
   unfalsifiable; a number without the pixels proves nothing.
3. **Static carrier when needed.** Some effects only show a spatial diff against
   a static carrier or a cross-sequence diff — within-sequence delta on a static
   carrier is 0. (Recorded per-effect in `/memory/`.)
4. **Parity / path-independence** for granular-pool work: `/parity` renders the
   same job via direct CLI and queue add→run and byte-compares every frame.
5. **Project skills:** `/verify` (clippy + targeted tests + offline shader
   compile + visual PNG check), `/preview` (render an effect on a fixture and
   look), `/fixture` (scaffold readout fixtures). **Visual verification —
   render to PNG, then Read it as an image — is the backbone.**

Non-obvious empirical findings (lever sweeps, "looks right but isn't" traps,
tuning dead-ends) go in `/memory/`, not prose docs — and are auto-recalled next
session. Check there before re-deriving a tuning.

---

## 12. Invariants you must not break

- **No new non-deterministic path** in the render graph (bitstream datamosh is
  the one sanctioned, quarantined exception).
- **Never ship a Metal path that hasn't passed CPU parity.** Metal is the only
  GPU target — no Vulkan/CUDA/WebGPU/WGSL.
- **Stateful nodes resume from the unquantized RGBA32F state buffer, never a
  display PNG**; changing algorithm id / inputs / settings / routes must
  invalidate the checkpoint (refuse, don't drift).
- **Analysis sidecars reuse only on a fingerprint match**; adding a descriptor
  dimension bumps the algorithm id (don't serde-default into a stale sidecar).
- **FFmpeg stays external and optional**; missing tools return a clear error.
- **No `unwrap()` in library code** (tests excepted); errors via `thiserror`.
- **Surgical changes**, simplest solution first, match existing style; mention
  unrelated issues rather than fixing them.

---

## 13. Where to look next

| I want to… | Go to |
|---|---|
| See exact flags for a command | `<command> --help`, then [`docs/REFERENCE.md`](docs/REFERENCE.md) |
| Understand an effect's acceptance criteria | the matching `docs/*_MILESTONE.md` |
| Know what's built vs deferred | [`docs/EFFECTS_ROADMAP.md`](docs/EFFECTS_ROADMAP.md), [`docs/BACKLOG.md`](docs/BACKLOG.md) |
| Resume where the last session left off | [`STATUS.md`](STATUS.md) |
| Recall a tuning finding / known trap | `/memory/` (auto-recalled) |
| Understand the system shape | [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) |
| The invariants + workflow (agent guide) | [`CLAUDE.md`](CLAUDE.md) |
</content>
