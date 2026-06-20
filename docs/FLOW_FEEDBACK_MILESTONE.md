# Flow Feedback Milestone

## Goal

Make the first deliberately destructive visual cross-synthesis effect: Source A supplies a vector field that repeatedly advects the previous output while Source B supplies the current carrier material. The result should evolve beyond a one-frame warp without sacrificing deterministic offline rendering, resume behavior, or CPU/Metal parity.

This milestone proves temporal render semantics before the project adds unrelated effect families. The vector-field source is independent of the feedback recurrence, so it can move from luminance gradients to temporal optical flow without changing state or resume semantics.

## Implementation Status

The deterministic first version is complete. `render-feedback-sequence` and the equivalent queue task write a `frames/` bundle, `checkpoint.json`, and checksummed unquantized RGBA32F feedback states. When a flow-cache output is requested, the bundle also records each A-derived field. The CPU renderer is the reference; `advect_feedback.metal` passes per-frame parity checks on the same sequence. `--reset-at-frame` is persisted in the render contract and applies the documented frame-zero behavior at that frame.

The A-derived vector field defaults to temporal coarse-to-fine pyramidal Lucas-Kanade optical flow (`pyramidal_lucas_kanade_cpu_v1`) computed between consecutive modulator frames, selectable with `--flow-source` (`luminance` restores the original single-frame gradient signal). Up to four deterministic pyramid levels use iterative warped refinement; reverse estimation produces forward/backward confidence maps that suppress unreliable vectors before render. Its vectors are scaled to the carrier/output coordinate system and use the renderer's backward-sampling convention. Valid per-frame sidecars are reused only when their algorithm, dimensions, vector convention, and modulator fingerprint match. The flow field is computed on the CPU and shared by both the CPU and Metal advection paths, so Metal parity is unaffected by the flow source. The active source is recorded in the contract's `flow_algorithm`, so switching it invalidates an existing checkpoint.

## Render Contract

For output frame `n`, let `C_n` be the Source B carrier frame, `F_n` the A-derived vector field, and `O_n` the float output buffer.

1. `B_n = displace(C_n, F_n, carrier_amount)`.
2. `O_0 = B_0`. Frame zero does not read an implicit previous image.
3. For `n > 0`, `H_n = displace(O_(n-1), F_n, feedback_amount)`.
4. `O_n = mix(B_n, H_n * decay, feedback_mix)`.

When temporal supersampling is greater than one, the exported PNG is a centered integration of `O_n` along `F_n`. It does not feed back into `O_(n+1)`: checkpoint state is still exactly one unquantized RGBA32F `O_n` per timeline frame.

The MVP uses one feedback iteration per output frame. Future iterations are a deliberate parameter, not an accidental loop. Internal buffers remain float RGBA; clamping and quantization occur only at image export.

## Determinism and Resume

- Frames render in increasing timeline order for a feedback job.
- The checkpoint after frame `n` contains the unquantized `O_n` float buffer, completed frame index, node settings, source/cache provenance, export bit depth, temporal sample count, and a versioned checksum.
- A resumed job loads that exact state and begins at `n + 1`.
- A changed source, flow cache, node setting, or kernel version invalidates the checkpoint. A stale flow sidecar is regenerated before use.
- CPU is the reference implementation. Metal must match it within the established export-precision tolerance before a frame is written.

## Initial Scope

- Inputs: paired PNG frame sequences for Source A and Source B.
- Analysis: temporal pyramidal Lucas-Kanade vector field by default, with forward/backward confidence maps and the luminance-gradient field retained as an explicit alternate source.
- Output: 8-bit or 16-bit PNG frame sequence, canonical RGBA32F feedback-state checkpoints, manifest, and ProRes-ready bundle.
- Parameters: `carrier_amount`, `feedback_amount`, `feedback_mix`, `decay`, `iterations`, reset frame, backend, PNG bit depth, and temporal sample count. The first implementation records `iterations` but intentionally accepts only `1` until a future ping-pong contract is specified.
- No realtime preview, audio synthesis, depth, masks, pyramidal/occlusion-aware flow, or EXR in this milestone.

## Implementation Order

1. Completed: add the project and queue schema without disturbing the current frame-displacement job.
2. Completed: implement the CPU reference function and synthetic tests for frame zero, recurrence, reset, and exact resume.
3. Completed: add CLI queue execution and on-disk float feedback checkpoints.
4. Completed: render a short real harp-to-cello proof sequence and inspect frame provenance.
5. Completed: implement `advect_feedback.metal` using the same texture and sampling convention.
6. Completed: add per-frame Metal-to-CPU parity checks on synthetic and real sequences.
7. Wire controls into SwiftUI after the offline path is stable.
8. Completed: replace luminance gradients with temporal Lucas-Kanade optical flow as the default feedback flow source, keeping the render-state contract and resume semantics unchanged.
9. Completed: define backward-sampling vector semantics, output-coordinate scaling, reset-frame zero fields, and validated sidecar reuse with translated synthetic sequence coverage.
10. Completed: replace the initial single-scale solver with coarse-to-fine pyramidal Lucas-Kanade, iterative warped refinement, and forward/backward reliability maps under a new cache algorithm identifier.

## Acceptance Criteria

- `cargo test` proves that a resumed CPU job produces the same float output as an uninterrupted job.
- A CLI feedback render writes a frame sequence, flow-cache provenance, feedback checkpoints, and a manifest that identifies the frame-state contract version.
- The Metal render passes the CPU parity gate on a real short sequence when a device is available.
- Resetting at a chosen frame produces the documented frame-zero behavior from that point onward.
- A translated synthetic modulator sequence proves optical-flow direction, output-coordinate scaling, cache reuse/invalidation, and reset behavior.
- Renderer tests prove pyramidal recovery of a multi-pixel translation and finite forward/backward confidence maps.
- Two-sample temporal integration affects only exported pixels; the corresponding RGBA32F checkpoint remains the unsupersampled feedback state.
- Completed: the SwiftUI shell submits the effect through the persisted feedback queue with the validated flow source, backend, reset, cache, and feedback controls.
