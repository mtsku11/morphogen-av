# Status

Session-resume checkpoint. Update at the end of any working session so a fresh
session (or a fresh agent) can pick up in seconds. Keep it short; durable detail
lives in `docs/`, cross-session findings live in `/memory/`.

_Last updated: 2026-06-22_

## Baseline (verified)

- `cargo test --workspace`: **208 passing across 7 crates, 0 failing.**
  One benign warning (`block v0.1.6` transitive dep, future-Rust deprecation).
- `swift test`: **38 passing, 0 failing** (Swift shell + service tests).
- Tree clean as of the impulse-convolution HQ-tier commits. Manual-testing clips
  (`cello.mp4`, `cello2.mp4`, `harp.mp4`) are gitignored, not tracked.

## What just landed

- **Convolutional AV Blending (audio HQ tier: FFT method + IR resampling) —
  full vertical slice (CPU + CLI + queue + SwiftUI; CPU-only).** The two deferred
  audio items. **FFT** (`--method fft`): a new pure-Rust radix-2 Cooley-Tukey FFT
  (`morphogen-audio/src/fft.rs`, forward+inverse over f64, no new deps — the STFT
  is magnitude-only with no inverse) computes the per-channel convolution in the
  frequency domain; same transform as the direct `O(B·L)` loop, gated against it
  within `FFT_DIRECT_PARITY_EPSILON` (1e-4). **IR resampling**
  (`--resample-impulse`, opt-in): a deterministic 3-lobe Lanczos resampler maps
  A's IR to B's rate (L1 after resampling so the gain bound survives), instead of
  the default hard error on a rate mismatch. New `ConvolutionMethod` enum (audio +
  core), serde-default `method`/`resample_impulse` on the `audio_impulse_convolution`
  job, CLI flags on render/queue-add, manifest records both. Algorithm id
  unchanged (`impulse_response_convolution_blend_cpu_v1` — method is an
  implementation choice, the audio analogue of `backend`). **Off-vs-on readout:**
  FFT vs direct on a 400-tap IR/1000-sample carrier = **max abs diff 5.96e-8**
  (≪ 1e-4; identical length/RMS/peak — FFT *is* the direct path); resample off =
  hard error, on = a 24 kHz IR reconstructs the native-48 kHz IR result within
  **7.8e-6**. FFT+resample queue add→run byte-identical to the direct render
  (smoke test pins it + the manifest knobs). Workspace 198 → 208; Swift 37 → 38.
  Contract: `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Convolutional AV Blending (audio impulse) — full vertical slice (CPU + CLI +
  queue + SwiftUI; CPU-only, no Metal like the cross-synth).** The roadmap's
  "tiny direct convolution for audio kernels" MVP — the other half of
  Convolutional AV Blending. Source A is an **impulse response**: downmix to mono,
  optional `--max-impulse-samples` head-truncation, then **L1-normalize** (so
  `Σ|tap| = 1`, which bounds the wet path — no clip blow-up); a silent A falls
  back to a unit-impulse identity. Each Source B channel is convolved with that IR
  (reusing `convolve_mono`), blended wet/dry by `amount`; the output extends past
  B by `L − 1` (the reverb tail). `--amount 0` = exact B passthrough. New logic in
  `morphogen-audio/src/convolution.rs` (`impulse_convolution_blend`, 9 tests) +
  `render-audio-impulse-convolution` CLI + persisted `audio_impulse_convolution`
  queue task (add/run writing `audio/impulse_convolution.wav` + manifest knobs) +
  a macOS Render-panel section (A IR / B / output pickers, amount + max-IR
  steppers). Algorithm id `impulse_response_convolution_blend_cpu_v1`. **Off-vs-on
  readout (audio, not the image's cross-sequence trick):** a straight OFF
  (`--amount 0`) vs ON (`--amount 1`) WAV compare — ON is **longer by L − 1**
  (4800 → 5039 for a 240-tap IR) and a positive lowpass IR drops **RMS
  0.574 → 0.027** / peak 0.90 → 0.08 (L1-bounded), OFF byte-identical to B,
  deterministic re-render byte-identical, queue add→run byte-identical to the
  direct render (smoke test pins it + the manifest knobs). Workspace 186 → 198;
  Swift 34 → 37. Both MVP halves now landed. Contract:
  `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Convolutional AV Blending (image kernel) — full vertical slice (CPU + CLI +
  Metal + queue + SwiftUI).** The roadmap's "tiny direct convolution for image
  kernels" MVP, A→B and **spatial** (the first effect where A modulates B with a
  *kernel*, not a scalar). Each Source A frame is box-downsampled into a normalized
  K×K luma kernel (bright A regions = heavy taps; black A falls back to uniform);
  Source B's frame is directly convolved with it (centered, clamped border,
  correlation-style) and blended by `amount`. `--amount 0` (or `K=1`) = exact
  Source B passthrough. New `conv_blend.rs` in morphogen-render (`ConvolutionKernel`
  + `analyze_convolution_kernel_cpu` + `convolution_blend_cpu`, 7 tests) +
  parity-gated `convolution_blend` Metal kernel (new `.metal` + runtime fn +
  parity/preflight tests) + `render-convolutional-blend-sequence` CLI (A frames +
  B frames → PNG seq, `--kernel-size`/`--amount`/`--backend`) + persisted
  `frame_sequence_convolution_blend` queue job (backend serde-default CPU;
  queue-add/run writing a frames/ bundle + manifest carrying the convolution
  algorithm id + kernel_size/amount/backend) + a macOS Render-panel section
  (A/B pickers, kernel + amount steppers, CPU/Metal backend). Algorithm id
  `image_kernel_convolution_blend_cpu_v1`. **Off-vs-on readout is cross-sequence,
  not within-sequence** — a spatial blur on a static carrier is invisible to
  `frame-delta.py`; instead render `--amount 0` vs `--amount 1` (K=5) on a
  checkerboard carrier + gradient modulator and diff OFF vs ON frame 0: mean
  per-channel **91.5/255** (the 5×5 kernel collapses the Nyquist checkerboard
  toward gray — Read confirms), OFF deterministic across renders, CPU==Metal
  byte-identical, queue add→run byte-identical to the direct render (smoke test
  pins it + the manifest knobs). Workspace 173 → 186; Swift 32 → 34. MVP
  feature-complete for the image carrier. Contract:
  `docs/CONVOLUTIONAL_BLEND_MILESTONE.md`.

- **Audio-to-Video Descriptor Routing — full vertical slice (CPU + CLI + Metal +
  queue + SwiftUI).** The roadmap's "RMS controls displacement amount" MVP, A→B
  cross-modal (A's *audio* shapes B's *video*, the complement to the cross-synth's
  A-audio→B-audio). The only new logic is **routing**: A's peak-normalized RMS
  envelope, hold-last per output frame at `--fps`, becomes the scalar `amount`
  fed to the **existing, already-parity-gated** flow displace op over a uniform
  displacement field (`--shift-x/--shift-y`). `--amount 0` (or silent A) = exact
  Source B passthrough. Because the pixel transform is the proven
  `flow_displace_cpu`/`flow_displace_metal`, **Metal came nearly free** —
  `--backend metal` reuses the displace kernel, gated per-frame against CPU.
  `audio_route.rs` in morphogen-render (`RmsDisplacementEnvelope` +
  `uniform_displacement_field`, 7 tests) + `render-audio-video-route-sequence`
  CLI (WAV A + PNG-seq B → PNG seq) + persisted `frame_sequence_audio_video_route`
  queue job (backend serde-default CPU; queue-add/run writing a frames/ bundle +
  manifest carrying the routing algorithm id + every knob) + a macOS Render-panel
  section (Source A WAV / Source B frames / amount+shift steppers / CPU-Metal
  backend). Algorithm id `rms_displacement_route_cpu_v1`. Off-vs-on verified on a
  static-gradient readout: amount 0 frame-delta **0.000/255** (passthrough),
  ramped-A on **0.656/255** (displacement tracks the loud→quiet envelope),
  large-shift frame visibly displaced (Read); OFF deterministic, CPU==Metal
  byte-identical, queue add→run byte-identical to the direct render (smoke test
  pins it + the manifest knobs). Workspace 163 → 173; Swift 30 → 32. MVP
  feature-complete. Contract: `docs/AUDIO_VIDEO_ROUTE_MILESTONE.md`.

- **Spectral Audio Cross-Synthesis — full vertical slice (CPU + CLI + queue +
  SwiftUI).** The roadmap's "RMS or centroid controls a simple filter/gain path"
  MVP, A→B, **time-domain by constraint** (our STFT is magnitude-only with no
  inverse, so phase-vocoder resynthesis stays the deferred HQ tier). Two modes
  share the framing (output follows B; A's descriptor resolved by time-based
  hold-last; `amount=0` = byte-identical passthrough): **`gain`** = A's
  peak-normalized RMS envelope scales B's amplitude; **`filter`** = A's
  spectral-centroid envelope (normalized to Nyquist) sweeps a per-sample one-pole
  LP/HP cutoff on B. CPU-only (audio is not a GPU target — no Metal, nothing to
  parity-gate). `cross_synth.rs` in morphogen-audio (5 tests) +
  `render-spectral-cross-synth` CLI (WAV A + WAV B → WAV out) + persisted
  `audio_spectral_cross_synth` queue job (core enums `CrossSynthMode` /
  `CrossSynthFilterType` / `CrossSynthWindow`, all serde-defaulted;
  `queue-add-/queue-run-spectral-cross-synth` writing `audio/cross_synth.wav` +
  a manifest carrying every knob) + a macOS Render-panel section (mode/amount/
  filter-type + WAV pickers). Algorithm ids `rms_gain_cross_synth_cpu_v1` /
  `centroid_filter_cross_synth_cpu_v1`. Off-vs-on verified numerically (audio has
  no PNG): gain half-amplitude ratio **1.00 → 3.11** (output tracks A's
  loud→silent ramp); filter output centroid **5640 → 1962 Hz** (dark A lowpasses
  bright B). Queue add→run byte-identical to the direct render (both modes; smoke
  test pins it + the manifest knobs). Workspace 155 → 163; Swift 28 → 30. This
  effect is now feature-complete for the MVP. Contract:
  `docs/SPECTRAL_CROSS_SYNTH_MILESTONE.md`.

- **Video Vocoder — full vertical slice (CPU + CLI + Metal + queue + SwiftUI).**
  The roadmap's "luma-band gain routing" effect, built A→B. Two modes share the
  framing: **`match`** (default) = histogram specification (remap B's luma
  distribution onto A's via a 256-level CDF tone map — no neutral point, so it
  stays strong on real footage) and **`gain`** = per-band luma-histogram gain
  routing. Both preserve hue, clamp, and treat `amount=0` as a byte-identical
  passthrough. `render-video-vocoder[-sequence]` (CPU + parity-gated
  `--backend metal` for match), persisted `frame_sequence_video_vocoder` queue job
  (`queue-add-/queue-run-video-vocoder-sequence`, manifest carries mode/algorithm/
  bands/amount/backend), and a Render-panel section (mode/bands/amount/backend).
  **Why match over gain:** on harp→cello, gain reads as a timid grade (natural
  histograms keep `N·a_hist≈1`); match imposes A's whole tonal palette (lifts the
  dark cello frame onto harp's daylight palette) — chosen after a side-by-side
  prototype. Verified: amount=0 byte-identical (direct pixel sample); match
  off-vs-on routes correctly; Metal byte-identical to CPU on HD frames (0.0/255);
  queue add→run byte-identical to direct. Algorithm ids
  `luma_histogram_spec_vocoder_cpu_v1` (match) / `luma_band_gain_vocoder_cpu_v1`
  (gain). gain-mode Metal deferred (errors clearly). Workspace 142→155; Swift
  26→28. Contract: `docs/VIDEO_VOCODER_MILESTONE.md`.

- **Granular step 6b luma-variance + gradient texture dims (render/CLI + queue +
  SwiftUI):** the final 6b feature, landed as a full vertical slice. Each pooled
  grain now carries a 2-dim texture descriptor `[luma_variance,
  gradient_magnitude]` over its tile; `--texture-weight W` (0 = off) scales both
  dims in the per-tile nearest match, querying Source A's per-tile texture, so a
  smooth modulator region draws smooth carrier grains and a busy region draws busy
  ones. Off by default ⇒ byte-identical selection. The pool **algorithm id bumped
  v1 → v2** (descriptor schema changed), so stale v1 sidecars regenerate rather
  than read texture as zero. Plumbed through the persisted job (serde default 0),
  queue-add/run, manifest, and the Render panel (Texture Weight stepper). New
  render-crate test (texture breaks a mean-colour tie: a busy modulator query
  picks the checkerboard grain over the flat one; weight 0 leaves the tie). New
  `--readout texture` fixture mode (flat vs striped frames at equal mean colour);
  off-vs-on readout: OFF mean frame-delta **0.0/255** (colour tie pins to the flat
  grain), ON **48.0/255** with the output tracking the modulator's flat↔stripes
  texture demand (frames Read to confirm); `/parity` OK 8/8 (queue == direct,
  manifest carries `texture_weight`); smoke + Swift bridge tests pin the knob.
  Workspace 141 → 142; Swift unchanged at 26 (existing tests extended). **With
  this, granular step 6b is feature-complete — no algorithmic refinements remain.**
- **Granular step 6b spatial-origin coherence (render/CLI + queue + SwiftUI):**
  the spatial complement to frame coherence, landed as a full vertical slice.
  `--spatial-coherence-weight W` (0 = off) adds a second additive term to
  `TemporalCoherence`: a candidate grain whose origin differs from that tile's
  previous pick adds `W*min(dist_tiles,reach)/reach` to its squared feature
  distance (`dist_tiles` = Euclidean origin distance in grain-tile units, sharing
  `--coherence-reach`). Keeps a tile's pick from teleporting across the frame even
  on a nearby source frame. Off by default ⇒ byte-identical; with either coherence
  weight > 0 the scheduler engages (frame zero still a no-op). Plumbed through the
  persisted job (serde default 0), queue-add/run, manifest, and the Render panel
  (Spatial weight stepper sharing Reach). New render-crate test (spatial weight
  overturns the exact-colour grain toward the previous pick's origin; frame-zero
  no-op); `/parity` OK 4/4 with frame + spatial coherence (queue == direct);
  smoke + Swift bridge tests pin the knob. Workspace 140 → 141; Swift unchanged at
  26 (existing tests extended). With this, the last 6b algorithmic refinement
  remaining is luma-variance/gradient feature dims.
- **Granular step 6b pool-selection knobs — queue/SwiftUI exposure sweep:** the
  persisted `frame_sequence_granular_mosaic_pool` job now carries all four
  direct-render pool knobs — centroid (k=2) STFT caches, trailing pool window,
  anti-repeat (weight + cooldown), and temporal coherence (weight + reach). New
  schema fields are `#[serde(default)]` (off), so jobs serialized before this
  sweep keep their whole-clip / no-scheduler meaning.
  `queue-add-granular-mosaic-pool-sequence` gained the matching flags (same
  both-or-neither centroid validation + finite/non-negative weight checks as the
  direct path); `queue-run` threads them into the render request instead of the
  old hardcoded defaults; the bundle manifest + provenance record them. The macOS
  Render panel adds a Spectral Centroid (k=2) toggle (wires the STFT caches from
  proxy extraction, both-or-neither), a pool-window stepper, and anti-repeat /
  coherence weight+span steppers (span steppers disabled when weight = 0).
  Verified e2e: queue add→run with pool-window + anti-repeat + coherence engaged
  is byte-identical to the direct render with the same flags; extended pool queue
  smoke test asserts the knobs round-trip through task + manifest; 3 new Swift
  bridge tests pin the scheduling flags + centroid-cache args (Swift 23 → 26;
  Rust workspace unchanged at 140 — existing tests extended). With this, the last
  deferred 6b follow-on is closed; only spatial-origin coherence + luma-variance/
  gradient feature dims remain noted as algorithmic refinements.
- **Granular step 6b cross-frame scheduling — temporal coherence (render/CLI
  path):** the smooth-motion complement to anti-repeat. `--coherence-weight W`
  (0 = off) + `--coherence-reach R` (default 8) reward source-frame continuity:
  a candidate grain whose source frame differs from that **same tile's** previous
  pick by `delta` adds `W*min(delta,R)/R` to its squared feature distance (0 when
  unchanged, saturating at `W` once `delta>=R`). State is `prev_selection:
  Vec<Option<u32>>` (one global grain index per output tile) — serializable
  checkpoint rep. Frame zero has an empty history ⇒ byte-identical to
  non-scheduled (declared frame-zero behavior); composes additively with
  anti-repeat; Metal path unaffected (CPU-side selection). New render-crate test
  (coherence overturns color-nearest toward the previous pick's frame; frame-zero
  no-op). Verified e2e on solid-gray footage (rearrangement=1.0 ⇒ output color
  reveals source frame): alternating modulator → off jumps f0↔f3 every frame,
  on (W=5, R=1) holds f0 after an identical frame 0. Workspace 139 → 140.
  Queue/SwiftUI exposure deferred. Spatial-origin coherence deferred.
- **Granular step 6b cross-frame scheduling — anti-repeat (render/CLI path):**
  `--anti-repeat-weight W` (0 = off) + `--anti-repeat-cooldown C` (default 8)
  penalize grains used in recent output frames (penalty `W*(C-age)/C`, linear
  decay) to push temporal diversity. State is `last_used_frame: Vec<Option<u32>>`
  (serializable checkpoint rep). Frame zero has empty history ⇒ byte-identical to
  non-scheduled (declared frame-zero behavior); penalty reshapes only the
  nearest-match distance, Metal path unaffected (CPU-side selection). New
  render-crate test (penalty overturns color-nearest; frame-zero no-op). Verified
  e2e on a colorful carrier + static modulator: off = 1 distinct output frame,
  on = 3 distinct, frame 0 identical / frames 1–3 diverge. Render 53 → 54
  (workspace 139). Queue/SwiftUI exposure deferred.
- **Granular step 6b sliding-window pool scope (render/CLI path):**
  `--pool-window N` bounds each output frame to a trailing window of the last `N`
  carrier frames (`0` = whole-clip). Grains are frame-major, so a trailing window
  is a contiguous global-index slice — `PoolSelectionWindow::Trailing` is a
  selection-only filter (whole-clip sidecar stays reusable; Metal render path
  unaffected; `WholeClip` byte-identical to prior behavior). New render-crate test
  pins window membership. Verified e2e: `--pool-window 1` forces each output frame
  onto its own carrier frame (red→green→blue→white) vs the static whole-clip
  mosaic. Render tests 52 → 53 (workspace 138). Queue/SwiftUI exposure deferred.
- **Granular step 6b k>1 audio dims (render/CLI path):**
  `render-granular-mosaic-pool-sequence` accepts optional
  `--modulator-centroid-cache` / `--carrier-centroid-cache` (STFT caches)
  alongside RMS. The audio vector is `[rms?, centroid?]` (each descriptor
  independently both-or-neither across modulator/carrier), k=0..=2; one
  `audio_weight` scales every dim. CPU core was already k-generic; the Metal
  kernel is untouched (audio drives only CPU-side selection). New render-crate
  test proves a centroid dim flips selection vs RMS-only. Verified end-to-end: on
  a 4-frame solid-color carrier + constant-amplitude chirp (flat RMS, rising
  centroid), k=1 vs k=2 give different mosaics (k=1 frame0 mean greenish, k=2
  pulled to blue/white = higher-centroid frames). Render tests 51 → 52
  (workspace 137). Queue/SwiftUI centroid exposure deferred.
- **Granular step 6b Metal backend in queue + SwiftUI:** the persisted
  `frame_sequence_granular_mosaic_pool` job gained a `backend` field (serde
  default CPU). `queue-add-granular-mosaic-pool-sequence --backend metal` is
  parity-gated frame-by-frame in the run path and the manifest records the
  backend; the macOS Render panel exposes a CPU/Metal segmented selector for the
  pool job. Verified end-to-end: a Metal-backed queue run on generated 48×48
  footage rendered 4 frames (per-frame parity gate passed) with `backend: Metal`
  in the manifest. Swift tests 22 → 23; Rust workspace 136 (unchanged count).
- **Granular step 6b Metal render port (temporal grain pool):** a
  `granular_mosaic_pool` compute kernel renders the cross-frame pooled mosaic on
  the GPU — the whole-clip pool uploads as a 2D texture array (slice per frame),
  a flat grain-metadata buffer resolves each global pool index to
  `(frame_index, origin_x, origin_y)`, integer-nearest clamped sampling +
  `rearrangement` value-blend. `granular_mosaic_pool_metal` is parity-gated by a
  multi-frame runtime test; `render-granular-mosaic-pool-sequence --backend metal`
  gates every frame against the CPU reference before export (queue runs stay CPU).
  Verified on generated footage: Metal output byte-identical to CPU (PSNR inf,
  4 frames). Metal tests 11 → 13. SwiftUI/queue exposure of the Metal backend deferred.
- **Granular step 6b SwiftUI exposure (temporal grain pool):** the macOS Render
  panel gains a `Granular Mosaic — Temporal Pool` section (grain size,
  rearrangement, variation, seed, audio weight, Audio-Weighted RMS toggle). The
  dev bridge shells out to `queue-add-/queue-run-granular-mosaic-pool-sequence`;
  the toggle wires the RMS caches from source-proxy extraction (both-or-neither,
  color-only when off). 3 new bridge arg tests (Swift 19 → 22).
- **Granular step 6b queue task (temporal grain pool):** persisted
  `frame_sequence_granular_mosaic_pool` `RenderJob` variant +
  `queue-add-/queue-run-granular-mosaic-pool-sequence`. Writes a ProRes-ready
  bundle (frames + pool sidecar + `frame_sequence_granular_mosaic_pool` manifest
  carrying the pooled algorithm id, `audio_weight`, and RMS-cache provenance).
  Verified: queue add→run on real footage; queued frames are byte-identical to
  the direct render (determinism across the queue path). SwiftUI + Metal deferred.
- **Granular step 6b CLI wiring (temporal grain pool):** new
  `render-granular-mosaic-pool-sequence` subcommand renders the joint-AV pooled
  path end-to-end. `--audio-weight`, optional `--modulator-rms-cache` /
  `--carrier-rms-cache` (both-or-neither, RMS k=1), and a `grain_pool_descriptors.json`
  sidecar keyed on the whole carrier set. On real footage (harp→cello):
  audio-weighted vs audio-off selection differs in ~26% of pixels. CPU-only.
- **Granular step 6b CPU core (temporal grain pool, joint-AV selection):**
  `pooled_av_nearest_grain_cpu_v1`. Grains are drawn from across time (whole-clip
  pool); each carries its frame's carrier-audio descriptor, so audio is finally a
  real matching dimension. `analyze_grain_pool_cpu` / `select_grains_from_pool_cpu`
  (combined `[mean_color | audio]` weighted NN, scalar `audio_weight`) /
  `granular_mosaic_with_pool_selection_cpu` (rearrangement = cross-frame value
  blend). See milestone step 6b.
- **Granular step 6 (selection slice):** multimodal nearest-neighbor grain
  selection on mean RGB (`multimodal_nearest_grain_cpu_v1`), opt-in via
  `--selection rgb` on the direct, sequence, and queue CLI paths; persisted on
  granular queue jobs + provenance; new `grain_color_descriptors.json` sidecar.
  Selection is CPU-side so the Metal render path + parity gate are untouched.
  Verified end-to-end: rgb vs luma give different coherent mosaics; sidecars
  tagged correctly; algorithm-mismatch recompute works.
- (prior) Source A audio descriptors routed into granular-mosaic controls
  (RMS→variation, onset→rearrangement, centroid→grain-size).

## In flight

On `main` (local commits, not yet pushed). The **Video Vocoder** MVP is now
feature-complete end-to-end (CPU + CLI + parity-gated Metal for match mode + queue
job + SwiftUI). Granular step 6b remains feature-complete. The vocoder's
deferred items: gain-mode Metal port, a reusable Source-A luma-band histogram
sidecar (currently recomputed per frame), spatial-frequency (multiband) routing,
and the reverse/cross-clip look exploration. **Spectral Audio Cross-Synthesis**
is now a feature-complete MVP vertical slice (CPU + CLI + queue + SwiftUI, gain +
filter modes). Its deferred HQ tier is phase-vocoder cross-synthesis (needs a
complex-STFT + inverse + Accelerate-FFT path first). **Audio-to-Video Descriptor
Routing** (RMS→displacement) is now a feature-complete MVP vertical slice too
(CPU + CLI + parity-gated Metal + queue + SwiftUI); its deferred items are
spatially varying displacement fields (sine/radial/Source-A flow), other
descriptor targets (centroid→hue, onset→cut), and sample-accurate descriptor
curves (HQ tier). **Convolutional AV Blending** is now feature-complete across
both MVP halves (image kernel: CPU + parity-gated Metal; audio impulse: CPU-only)
**and the audio HQ tier** (FFT method + Lanczos IR resampling, CPU + CLI + queue +
SwiftUI). Its remaining deferred items are image Metal *spatial* kernels for
larger K (the direct loop ships now), per-channel/color or separable image
kernels, and per-channel / true-stereo audio IRs (this MVP downmixes A to one
mono IR). The next unstarted roadmap effect is **Video-to-Audio Descriptor
Routing** (frame-luma → audio gain/pan) or **Controlled Datamosh / Motion-Vector
Reuse** (flow-field reuse on decoded float frames).

## Candidate next steps

From `docs/BACKLOG.md` "Next" and `docs/EFFECTS_ROADMAP.md`:

1. **Granular step 6b remaining** — CPU core + CLI render path + pool sidecar +
   queue task + SwiftUI exposure + Metal render port (`--backend metal`,
   parity-gated) + Metal backend in queue/SwiftUI all landed. Deferred within 6b:
   k>1 audio dims (add centroid), sliding-window pool scope, and cross-frame
   scheduling (anti-repeat / temporal coherence).
2. **Next roadmap effect** — Video Vocoder (luma-band gain routing MVP) or
   Spectral Audio Cross-Synthesis (RMS/centroid filter path) are the natural
   next vertical slices.
3. **Deferred / low-priority** — Metal parity port for the multiscale
   structure-preserving morph, then its queue/SwiftUI exposure. Per the manual
   testing finding it's CPU-only and marginal on real footage; don't invest until
   a use case shows it mattering (see `docs/BACKLOG.md` + [[flow-feedback-levers]]).

## Known truths to respect

- Single-scale `--structure-mix` is the keeper for "beyond recognition" feedback;
  multiscale is correct-but-marginal. `--feedback-mix` is the dissolve cliff.
- Every new Metal kernel must parity-gate against the CPU reference before export.
