# Codex Task Backlog

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

## Next

### Flow Feedback and Advection Milestone

The next effect is not another independent processor. It is a stateful temporal render primitive that the later datamosh, optical-flow, and video-vocoder work can reuse. The authoritative contract and acceptance criteria are in `docs/FLOW_FEEDBACK_MILESTONE.md`.

1. Add a real temporal optical-flow analysis cache, then replace the luminance-gradient signal in the feedback job without changing its render-state contract.
2. Expose feedback amount, decay, iterations, reset behavior, and CPU/Metal backend choice in the SwiftUI render panel.
3. Add a first quality-controlled feedback preset library for aggressive degradation, stable trails, and reset-driven cuts.
4. Add higher-precision feedback exports and temporal supersampling without changing checkpoint semantics.
5. Only then resume independent effect families such as video vocoder, granular mosaicing, and codec-aware datamosh.
