# Architecture

Morphogen AV is organized around deterministic offline rendering first, with realtime preview treated as a lower-fidelity view of the same project graph.

## 1. Native Core

The native core owns durable project state and render semantics:

- project schema
- scheduling and timeline
- media source declarations
- node graph and typed modulation routes
- frame and audio buffer contracts
- render jobs and queue persistence
- cache manifests
- deterministic frame-addressable offline rendering

The first implementation lives in Rust. The macOS app will call into it through a bridge once the CLI path proves the engine behavior.

Project validation now checks known node-port signal types for modulation routes, so an optical-flow vector field can modulate a displacement vector field while scalar envelopes, spectra, images, and grain indexes are rejected for incompatible parameters.

Stateful temporal render nodes must declare their frame-zero behavior, the exact prior-frame state they consume, and the checkpoint representation needed to resume at a later frame. A render must resume from an unquantized internal state buffer, never from a display PNG, so CPU and Metal jobs remain frame-addressable and reproducible.

## 2. Metal GPU System

Metal is the production GPU backend for Apple Silicon. The initial repo includes `.metal` shader skeletons and Rust placeholder modules for device, pipeline, and texture ownership.

The CPU renderer remains the reference implementation. GPU kernels must match the CPU behavior closely enough for deterministic tests around small fixtures and tolerances.

## 3. Analysis Cache

Analysis is reusable sidecar data. Planned cache types include optical flow, masks, depth maps, audio RMS envelopes, STFT frames, onset maps, spectral descriptors, grain indexes, and future motion-vector data.

The cache manifest is part of project-level orchestration but cache files should remain regenerable from source media and analysis settings. Temporal optical-flow sidecars carry their algorithm, output dimensions, sampling convention, and source fingerprint; renderers may reuse only a matching sidecar and regenerate stale analysis deterministically.

## 4. Node Graph UI

The UI should behave like an audiovisual modular synthesizer. Analysis nodes expose typed signals, carrier/render nodes expose compatible parameters, and modulation routes connect them.

The first SwiftUI shell displays the conceptual routing:

- Source A -> Analysis -> Modulation Signal
- Source B -> Carrier Processing -> Output

## 5. Offline Render Queue

The offline render queue is the quality path. It should eventually support:

- 16-bit and 32-bit float image buffers
- temporal supersampling
- high-quality interpolation
- deterministic frame addressing
- resumable jobs
- EXR and image sequence output
- ProRes export
- WAV and stem export
- sample-accurate audio

Realtime preview should reuse the same graph semantics but may use lower resolution, lower precision, or partial cache data.

The current CLI has a deterministic dev queue executor that writes a single-frame PNG sequence, a 32-bit float WAV stem, a resume checkpoint, and an output manifest with frame/sample timing metadata for the first queued or running test job. `frame_sequence_flow_displace` renders paired source frames into a ProRes-ready `frames/` bundle with flow-cache provenance. `frame_sequence_granular_mosaic` uses the same bundle shape while recording Source A/B plus grain-descriptor cache provenance. `frame_sequence_flow_feedback` adds frame-addressable temporal state: after every completed frame it writes `checkpoint.json` plus a checksummed unquantized RGBA32F previous-output buffer. Its contract includes input frame fingerprints, render settings, reset frame, and analysis provenance, so changed inputs or settings reject stale state rather than silently resuming. This is not the final scheduler, but it proves the intended output bundle shape, provenance handoff, and temporal resume semantics.
