# Task Backlog

Shared, ordered follow-up backlog (history + next steps). Agent-neutral.
The high-level plan lives in [EFFECTS_ROADMAP.md](EFFECTS_ROADMAP.md); per-effect
contracts live in the `*_MILESTONE.md` docs.

## Completed

- Connect SwiftUI app to Rust CLI for dev-only `render-test` invocation.
- Implement real file picking in SwiftUI.
- Implement media probing through AVFoundation placeholder or CLI bridge.
- Extract frame sequences and WAV using external FFmpeg helper.
- Implement first real two-source frame displacement render.
- Add analysis cache files for synthetic and real flow fields.
- Add a first Metal compute implementation of flow displacement.
- Add offline render queue persistence.
- Connect flow cache sidecars to extracted frame-sequence render inputs.
- Add RMS-to-visual-parameter modulation.
- Add basic audio export/stem handling.
- Add project save/load from the SwiftUI shell.
- Add typed node-port compatibility checks in `morphogen-core`.
- Add deterministic fixture media and golden render tests.
- Add image-sequence plus WAV render job output.
- Add Rust-side flow-displace dispatch planning and shader preflight in `morphogen-metal`.
- Add macOS Metal runtime submission for `flow_displace.metal`.
- Add offline render queue execution and resume checkpoints.
- Add AVFoundation media probe implementation behind a Mac backend feature.
- Add STFT cache generation and serialization.
- Add onset-strength detection.
- Add timeline and sample/frame alignment tests.
- Add CoreVideo-to-Metal texture bridge experiments.
- Add ProRes export planning spike with VideoToolbox.
- Wire AVFoundation decoded frames into `CoreVideoMetalTextureBridge`.
- Add first ProRes image-sequence-to-MOV exporter using VideoToolbox and AVAssetWriter.
- Add an app-side preview probe that decodes a selected source frame into a Metal texture and reports dimensions/timing.
- Add a SwiftUI preview surface for the decoded source frame texture.
- Connect render-queue image-sequence output to the ProRes export flow without manual folder selection.
- Add audio muxing to the ProRes export path.
- Add configurable frame-rate/profile controls to the ProRes export panel.
- Carry render timing metadata into queue output manifests and default ProRes export FPS from the bundle.
- Add SwiftUI controls for a real two-source frame-sequence render job.
- Wire the Metal flow-displacement backend into a CLI validation path.
- Broaden render queue metadata beyond the deterministic test job.
- Turn the SwiftUI two-source frame-sequence bridge into a persisted render queue job type.
- Add first app-side media ingest automation from selected movies into frame/WAV proxy directories.
- Add explicit source/cache provenance to persisted frame-sequence queue jobs.
- Route queued frame-sequence flow displacement through the Metal backend with CPU parity checks.
- Add queue cancellation and durable failure records for frame-sequence jobs.
- Persist ingested proxy media and analysis-cache references into project files.
- Add RMS and STFT analysis cache creation to app-side media ingest.
- Add a serializable `flow_feedback` render-node and `frame_sequence_flow_feedback` render-job task.
- Implement CPU float feedback with explicit frame-zero, reset, and prior-output semantics.
- Persist versioned, checksummed RGBA32F feedback state after every frame and prove exact resume output.
- Add deterministic CLI feedback rendering and queued ProRes-ready feedback bundles.
- Implement `advect_feedback.metal` and gate every Metal output frame against the CPU reference.
- Add a temporal Lucas-Kanade optical-flow analysis and make it the feedback job's default flow source.
- Add experimental real-bitstream datamosh keyframe-removal (`datamosh-bitstream --operation remove-keyframe`) on the existing pure-Rust AVI surgery path.
- Add reusable controlled-datamosh Source A optical-flow sidecars, disk resume
  checkpoints, and curated destructive presets across CLI, queue, and SwiftUI.
- Modulation matrix slice 1 (CPU + CLI): generic `--modulate
  <target>=<source>[:<scale>[,<offset>]]` routes normalized analysis envelopes
  (audio RMS/onset/centroid, video luma/flow) onto float knobs of the
  retro-static, pixel-sort, and channel-shift sequence commands. Contract:
  `docs/MODULATION_MATRIX_MILESTONE.md`.
- Modulation matrix slice 2 (queue): routes persist on the retro-static and
  pixel-sort queue jobs (serde-default empty; add-time validation; queue-run
  byte-identical to direct; manifest `modulation` block).
- Modulation matrix slice 3 (SwiftUI): per-knob mod slots (source picker with
  Off, scale/offset steppers) + shared modulator media and sampling pickers on
  the retro-static and pixel-sort panel sections; bridge emits the
  `--modulate` flag set on queue-add. Deferred: a channel-shift queue task,
  integer/enum/stateful targets.
- Channel-shift queue task: `frame_sequence_channel_shift` render-job task +
  `queue-add`/`queue-run-channel-shift-sequence` covering constant, flow-driven
  (CPU-only), and modulated modes; add→run byte-identical to the direct render
  (smoke-tested); add-time validation before persisting. Deferred: SwiftUI
  channel-shift panel exposure.
- Channel-shift SwiftUI panel: backend picker (CPU default; flow mode is
  CPU-only), six shift steppers, flow gain/radius, and slice-3 mod slots on all
  six `shift_*` targets (`ModulationSlotRow` gained defaulted range params for
  pixel-unit knobs); bridge emits `--flag=value` shift/flow-gain forms so
  negative pixels survive clap, fail-fast for flow-without-A / flow-on-Metal.
  Deferred: integer/enum/stateful modulation targets.
- Modulation matrix slice 4 (integer targets): palette-quantize `levels` joins
  the registry under the contracted rounding rule (clamp `[2, 256]`, then round
  nearest, ties away from zero; off case reachable at 256);
  `render-palette-quantize-sequence` gains the `--modulate` flag set (direct
  CLI only — no palette-quantize queue/SwiftUI exposure yet). Deferred:
  enum/stateful modulation targets.
- Modulation matrix slice 5 (enum targets): pixel-sort `direction`/`axis`,
  retro-static `filter`, palette-quantize `mode` join the registries under the
  contracted variant-index rule (indices `0..N−1` in declared order, same
  clamp-then-round tie rule; unimplemented variants excluded — `kmeans` stays
  unreachable). Enum routes persist on the existing pixel-sort/retro-static
  queue tasks with no queue changes. Deferred: SwiftUI enum mod slots,
  stateful modulation targets.
- Palette-quantize queue task: `frame_sequence_palette_quantize` render-job
  task + `queue-add`/`queue-run-palette-quantize-sequence` (channel-shift
  precedent — add-time route validation, `mode` persisted as a string label,
  add→run byte-identical smoke-tested with integer + enum routes). Deferred:
  SwiftUI palette-quantize panel section, stateful modulation targets.
- Palette-quantize SwiftUI panel: sticky Metal-default backend picker,
  posterize/palette mode picker, posterize-only levels stepper, and a mod
  slot on the integer `levels` target (wide step-8 ranges); bridge emits the
  `--modulate` flag set on queue-add, validates levels app-side only in
  posterize mode. Deferred: enum mod slots (`mode` has no slot), stateful
  modulation targets.

## Next

### Flow Feedback and Advection Milestone

The next effect is not another independent processor. It is a stateful temporal render primitive that the later datamosh, optical-flow, and video-vocoder work can reuse. The authoritative contract and acceptance criteria are in `docs/FLOW_FEEDBACK_MILESTONE.md`.

1. Done: added a temporal Lucas-Kanade optical-flow analysis (`lucas_kanade_cpu_v2`) and made it the feedback job's default flow source via `--flow-source`, without changing the render-state contract. It uses explicit backward-sampling vectors, output-coordinate scaling, reset-frame zero fields, and validated reusable sidecars.
2. Done: replaced the single-scale solver with deterministic coarse-to-fine pyramidal Lucas-Kanade (`pyramidal_lucas_kanade_cpu_v1`), iterative warped refinement, and forward/backward confidence maps. The reusable flow-field sidecar remains cache format v2; the new algorithm identifier invalidates pre-refinement caches and checkpoints.
3. Done: expose feedback amount, decay, the current one-iteration contract, reset behavior, flow source, and CPU/Metal backend choice in the SwiftUI render panel through a persisted feedback queue bridge.
4. Done: add a first quality-controlled feedback preset library for aggressive degradation, stable trails, and reset-driven cuts.
5. Done: add 16-bit PNG feedback exports and flow-guided temporal supersampling as an export-only pass; the checkpoint remains the exact once-per-frame RGBA32F feedback state.
6. Done: begin the independent-effect phase with deterministic CPU granular mosaicing. Source A luma selects fixed-size Source B tiles; direct image and paired-frame sequence CLI paths expose grain size, rearrangement, variation, and seed. The contract is in `docs/GRANULAR_MOSAIC_MILESTONE.md`.

### AV Granular Mosaicing Milestone

1. Done: implement the deterministic CPU reference renderer and direct image/paired PNG frame-sequence CLI path.
2. Done: persist Source B grain descriptors and Source A selection indexes as validated JSON cache sidecars. Identical inputs/settings reuse both; changed variation, seed, source fingerprint, dimensions, or algorithm regenerates the affected sidecar.
3. Done: add a persisted `frame_sequence_granular_mosaic` task that writes the standard ProRes-ready image-sequence bundle with timing, Source A/B, and grain-cache provenance.
4. Done: add `granular_mosaic.metal`, a macOS runtime dispatcher, shader-binding preflight, and a tiny CPU/Metal parity fixture. Direct, sequence, and queue CLI paths select it with `--backend metal` and reject divergent frames before export.
5. Done: route Source A RMS, onset, and spectral descriptors from cache sidecars into frame-addressed variation, rearrangement, and grain-size controls; persist their paths/scales in granular queue jobs and output provenance.
6. Done (selection slice): multimodal nearest-neighbor grain selection on mean
   RGB (`multimodal_nearest_grain_cpu_v1`), opt-in via `--selection rgb` on the
   direct, sequence, and queue CLI paths and persisted on granular jobs +
   provenance. Weighted distance is written over feature slices so audio
   dimensions append later. See `docs/GRANULAR_MOSAIC_MILESTONE.md` step 6.
7. Done (6b CPU core): temporal grain pool / joint-AV selection
   (`pooled_av_nearest_grain_cpu_v1`). Grains drawn from across time (whole-clip
   pool) each carry their frame's carrier-audio descriptor, making audio a real
   matching dimension over a combined `[mean_color | audio]` weighted distance.
   `rearrangement` is a cross-frame value blend. See
   `docs/GRANULAR_MOSAIC_MILESTONE.md` step 6b.
8. Done (6b CLI wiring): `render-granular-mosaic-pool-sequence` renders the
   pooled path end-to-end (`--audio-weight`, optional both-or-neither
   `--modulator-rms-cache`/`--carrier-rms-cache`, RMS k=1), backed by a
   `grain_pool_descriptors.json` sidecar keyed on the whole carrier set. Verified
   on real footage: audio-weighted vs audio-off selection differs ~26% of pixels.
9. Done (6b queue task): persisted `frame_sequence_granular_mosaic_pool` job +
   `queue-add-/queue-run-granular-mosaic-pool-sequence`. ProRes-ready bundle with
   pool sidecar and a manifest carrying the pooled algorithm id, `audio_weight`,
   and RMS-cache provenance; queued frames byte-identical to the direct render.
10. Done (6b SwiftUI exposure): the macOS Render panel exposes the pooled queue
    job (`Granular Mosaic — Temporal Pool`) — grain size, rearrangement,
    variation, seed, audio weight, and an Audio-Weighted (RMS) toggle that wires
    the source-proxy RMS caches (both-or-neither, color-only when off). Dev bridge
    shells out to `queue-add-/queue-run-granular-mosaic-pool-sequence`; 3 new arg
    tests.
11. Done (6b Metal render port): `granular_mosaic_pool` compute kernel +
    `granular_mosaic_pool_metal` — whole-clip pool as a 2D texture array (slice
    per frame), flat grain-metadata buffer for `(frame_index, origin_x, origin_y)`,
    integer-nearest clamped sampling + `rearrangement` value-blend. Parity-gated by
    a multi-frame runtime test; `render-granular-mosaic-pool-sequence --backend metal`
    gates each frame against the CPU reference before export.
    Verified: Metal output byte-identical to CPU on generated footage (PSNR inf).
12. Done (6b Metal backend in queue/SwiftUI): the persisted
    `frame_sequence_granular_mosaic_pool` job carries a `backend` field (serde
    default CPU); `queue-add-granular-mosaic-pool-sequence --backend metal` is
    parity-gated frame-by-frame in the run path (manifest records the backend),
    and the macOS Render panel has a CPU/Metal selector for the pool job.
    Verified end-to-end on generated footage.
13. Done (6b k>1 audio dims, render/CLI path): `render-granular-mosaic-pool-sequence`
    accepts optional `--modulator-centroid-cache` / `--carrier-centroid-cache`
    (STFT caches) beside RMS; the audio vector is `[rms?, centroid?]` (each
    descriptor independently both-or-neither), k=0..=2, one `audio_weight` for
    all dims. CPU core already k-generic; Metal kernel unaffected (audio drives
    only CPU-side selection). Verified: k=1 vs k=2 differ on a solid-color carrier
    + chirp (flat RMS, rising centroid); new render-crate test proves a centroid
    dim flips selection vs RMS-only.
    Deferred: queue/SwiftUI exposure of centroid caches, cross-frame scheduling.
14. Done (6b sliding-window pool scope, render/CLI path): `--pool-window N` bounds
    each output frame to a trailing window of the last `N` carrier frames (0 =
    whole-clip). Frame-major storage makes a trailing window a contiguous
    global-index slice, so `PoolSelectionWindow::Trailing` is a selection-only
    filter — whole-clip sidecar stays reusable, Metal render path unaffected,
    `WholeClip` byte-identical to prior behavior. Verified e2e (`--pool-window 1`
    → own-frame-only red→green→blue→white) + a render-crate membership test.
    Deferred: queue/SwiftUI exposure of centroid caches + pool window.
15. Done (6b cross-frame scheduling — anti-repeat, render/CLI path):
    `--anti-repeat-weight W` (0 = off) + `--anti-repeat-cooldown C` (default 8)
    penalize grains used in recent output frames (`W*(C-age)/C`, linear decay) for
    temporal diversity. State `last_used_frame: Vec<Option<u32>>` is the
    serializable checkpoint rep; frame zero (empty history) is byte-identical to
    non-scheduled; penalty reshapes only the nearest-match distance; Metal path
    unaffected (CPU-side selection). Render-crate test + e2e (static modulator:
    off → 1 distinct output frame, on → 3 distinct; frame 0 identical).
    Deferred: queue/SwiftUI exposure of centroid caches / pool window /
    anti-repeat; temporal-coherence scheduling (complement to anti-repeat).
16. Done (6b remaining — closes step 6b): (a) **frame coherence** scheduling
    (`--coherence-weight`/`--coherence-reach`, rewards per-tile source-frame
    continuity); (b) **queue/SwiftUI exposure sweep** plumbing centroid caches,
    pool window, anti-repeat, and frame coherence through the persisted job +
    manifest + Render panel; (c) **spatial-origin coherence**
    (`--spatial-coherence-weight`, shares `--coherence-reach`, penalizes grain
    origins that teleport across the frame); (d) **luma-variance + gradient texture
    dims** (`--texture-weight`, a 2-dim `[variance, gradient]` per-grain descriptor
    matched against Source A's per-tile texture) — this bumped the pool algorithm
    id `pooled_av_nearest_grain_cpu_v1 → _v2` (descriptor schema change). All four
    are off-by-default selection-only knobs (byte-identical when off, Metal render
    path unaffected), each plumbed through render/CLI + queue + SwiftUI with
    render-crate tests, `/parity`, smoke, and Swift bridge coverage, plus off-vs-on
    readouts on `/fixture` modes (`origin` for spatial coherence, `texture` for the
    texture dims). With this, granular step 6b is feature-complete.

### Structure-Preserving Morph (Flow Feedback Enhancement)

Motivation: the current flow-feedback renderer can erase but not transform. Its feedback model is additive accumulation, so once the carrier stops re-asserting (high `--feedback-mix`), the frame collapses toward flat fog rather than reorganizing into new structure. An empirical lever sweep (cello@4fps carrier, harp modulator, optical flow) confirmed: `--feedback-mix` is the dissolve cliff (recognizable below ~0.94, gone by ~0.99), `--feedback-amount` past ~60 only adds haze, and the usable "unrecognizable but alive" window is a narrow transition band (mix ~0.96-0.98, decay ~0.97) that decays to fog within ~30 frames. The goal here is "beyond recognition" as a *structured morph into something new*, not a wash-out.

The core idea: decouple carrier *texture* from carrier *position*. Re-inject the carrier's high-frequency structure (edges, grain, local contrast) every frame so detail keeps regenerating, while letting the accumulated optical-flow displacement own the *layout*. The original stops re-asserting its composition (so it goes beyond recognition) but the frame never collapses to uniform haze (because fresh high-frequency energy is continuously re-seeded).

1. CPU reference: add a `--structure-mix` (or carrier high-pass re-injection) path that splits the carrier into low/high spatial-frequency bands, advects the accumulated feedback state as today, and re-injects only the carrier high-frequency band into the displaced result. Preserve the once-per-frame RGBA32F checkpoint contract, deterministic output, and a new algorithm identifier that invalidates prior checkpoints. Prove that at high feedback-mix the output retains regenerating structure instead of trending to flat fog.
2. Expose the new control(s) in the CLI `render-feedback-sequence` path and document the interaction with `--feedback-mix`/`--decay`. Add a fixture-based test asserting the high-pass re-injection keeps per-frame high-frequency energy above a floor (i.e. it does not wash out) across a long sequence.
3. Done: `advect_feedback.metal` re-injects the carrier high-frequency band (5x5 binomial low-pass) and now samples carrier, history, and the structure band with manual bilinear matching `sample_bilinear_clamped`, so the whole GPU path holds CPU parity at `METAL_CPU_PARITY_EPSILON` even at high feedback-mix where the hardware sampler previously diverged. The `--backend metal` direct path gates every frame against the CPU reference.
4. Done: `structure_mix` now threads through the persisted `frame_sequence_flow_feedback` queue task (`#[serde(default)]`, so legacy queue files still load), the CLI `queue-add-feedback-sequence` flag, and the SwiftUI feedback panel (a `Structure` stepper plus a new "Structured Morph" preset at mix 0.97 / decay 0.97 / structure 0.6). A queue add→run smoke test asserts the field round-trips into the queue JSON and renders.
5. Done (CPU): added a `StructureMode::Multiscale` path selected by `--structure-mode multiscale` on the direct `render-feedback-sequence` CLI. It splits the displaced carrier into three full-resolution Burt-Adelson detail bands (repeated binomial blurs, differenced) and gates each band by a structure mask taken from the *morphed* (advected) frame — sharp mask for fine detail, progressively blurred mask for coarse — so re-seeded detail concentrates along the evolving geometry instead of the static carrier grid. `structure_mix` stays the single master gain; level count, mask floor (0.25), and gain (6.0) are fixed internal constants. `StructureMode::SingleScale` remains the default and is bitwise-unchanged, so existing outputs/checkpoints and the Metal parity path are untouched. The Metal backend rejects multiscale (`--backend metal` errors) since it has no shader port yet. Tests cover zero-mix identity across modes, washout resistance, and the mask biasing re-injection toward a morphed edge.

   Manual-testing finding (cello self-feedback, mix 0.97, structure-mix 0.8): multiscale is **mathematically correct but practically marginal on real footage** — single-scale vs multiscale differ by ~1.5% mean (concentrated in a handful of edge pixels), and the renders are visually indistinguishable. The mask earns its keep only when the morphed frame has large flat regions to separate from edges (as in the synthetic test); dense, low-contrast footage has gradient nearly everywhere, so the mask reads near-uniform and multiscale degenerates toward single-scale. Aggressively retuning the mask (floor 0.05, gain 12) did not change this, so the constants were left at 0.25 / 6.0. By contrast, single-scale `structure-mix` itself is a clear keeper — it visibly rescues the mix-0.99 fog collapse with regenerating edge detail. Conclusion: do **not** invest in the Metal port / queue / SwiftUI exposure for multiscale until a use case shows it mattering; it stands as a correct, opt-in, CPU-only path. Deferred (low priority): Metal parity port for multiscale, then queue/SwiftUI exposure.

### Fluid Advect Family Queue Exposure

1. Done: persisted queue task/schema + CLI add/run commands for the compact,
   parity-gated fluid/advection jobs:
   `frame_sequence_fluid_advect`,
   `frame_sequence_fluid_advect_two_source`,
   `frame_sequence_optical_flow_advect`, and
   `frame_sequence_field_particles`. Each writes the standard ProRes-ready
   `frames/`, `manifest.json`, and `checkpoint.json` bundle, records timing,
   backend, source provenance, and the relevant algorithm id, and persists
   failures back to the queue. Smoke tests prove queued output is byte-identical
   to the direct CLI render for all four CPU paths.
2. Done: exposed these four queued jobs in the SwiftUI render panel with compact
   controls and the existing CPU/Metal selector. The bridge now has typed queue
   requests/argument builders for each command, plus Swift tests covering command
   shape and invalid values. Source B is used as the carrier/material input for
   the single-source procedural fluid, self-flow advection, and field-particles
   jobs; the two-source job uses Source A motion over Source B dye.
3. Next: decide whether `render-fluid-mosaic-sequence` deserves a queue task now
   or should wait for a narrower preset surface. It is CPU-only and has a much
   larger tuning API than the four compact fluid/advection jobs above.
