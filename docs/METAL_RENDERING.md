# Metal Rendering

The first working renderer is CPU-based and deterministic. Metal is the intended production backend for performance on Apple Silicon.

## CPU Reference Renderer

The CPU reference renderer uses float RGBA buffers, float flow fields, bilinear sampling, and clamped borders. It is the correctness baseline for tests and offline determinism.

## Metal Backend Goals

The Metal backend should provide:

- flow displacement
- advection and video feedback
- image pyramids for multiscale analysis
- optical-flow preprocessing and future optical-flow kernels
- float texture pipelines for 16-bit and 32-bit internal buffers
- deterministic offline render behavior where practical
- lower-latency realtime preview variants

## Flow Displacement

Flow displacement samples the carrier at coordinates offset by a vector field derived from the modulator. A flow vector is a `backward_sampling_offset`: it is added to the output coordinate before sampling. Temporal Lucas-Kanade analysis converts its forward motion estimate into this convention and scales it into output pixels before the shared CPU/Metal displacement path consumes it. The CPU and Metal implementations should agree on coordinate conventions and border behavior.

`crates/morphogen-metal/shaders/flow_displace.metal` now contains the first concrete compute kernel body. It expects a carrier texture, an RG32F flow texture in output pixel coordinates, an output texture, and an `amount` parameter. It mirrors the CPU reference behavior by reading one flow vector per output pixel, sampling the carrier with linear filtering, and clamping at texture borders.

`morphogen-metal` also exposes a Rust-side `FlowDisplaceDispatchPlan` that validates dimensions and amount, defines the expected texture roles, calculates 16x16 threadgroup coverage, embeds the shader source, and preflights that the checked-in shader still has the expected kernel and texture binding layout.

On macOS, `flow_displace_metal` compiles the checked-in shader source, creates shared RGBA32F/RG32F textures, uploads `morphogen-render` buffers, dispatches the compute pass, and reads back an `ImageBufferF32`. The parity test compares the Metal output against the CPU reference when a Metal device is available, and skips only the no-device case so non-GPU CI remains usable.

## Advection and Feedback

Flow feedback is now a CPU/Metal temporal render path with one shared contract:

- frame zero outputs the flow-displaced carrier with no prior feedback state;
- each later frame advects the previous unquantized output through the current A-derived field;
- the renderer blends that history with the current displaced carrier using explicit feedback and decay parameters;
- the queue persists the float previous-output buffer after every completed frame, so resume never depends on a quantized PNG.

`advect_feedback.metal` samples the current carrier with `carrier_amount`, samples the previous output with `feedback_amount`, applies `decay`, then blends the two with `feedback_mix`. Frame zero reuses the existing Metal flow-displacement kernel, because there is no history texture. The runtime compiles with fast math disabled and compares every Metal frame to the CPU reference before writing its export image. The MVP validates `iterations == 1`; future multi-iteration behavior must add explicit ping-pong texture semantics before it is enabled. Realtime preview may reduce resolution, but it must not change state-update ordering.

## Granular Mosaic

`granular_mosaic.metal` receives the float Source B carrier texture and the CPU-generated row-major grain-selection index map. It maps every output pixel to its selected carrier tile, blends original and selected coordinates by `rearrangement`, then uses the same linear, clamp-to-edge sampling convention as the CPU renderer. Descriptor analysis and Source A luma matching remain on the CPU in this milestone, preserving the validated cache contract.

`granular_mosaic_metal` checks the selection map dimensions and indices before submission, compiles with fast math disabled, and returns a float image for comparison. The granular CLI paths accept `--backend metal`; each rendered frame is compared to `granular_mosaic_with_selection_cpu` at the established float tolerance before any PNG is written.

## Image Pyramids and Optical Flow

The current optical-flow source is deterministic coarse-to-fine pyramidal Lucas-Kanade. It uses iterative warping at each level and forward/backward confidence checks before emitting the renderer's backward-sampling field; this remains a testable CPU reference before a Metal analysis implementation. A future quality pass can add robust weighting and use confidence maps for explicit occlusion masks. Horn-Schunck can later be an intentionally smooth optional field, but it is not the production replacement because its global regularization blurs motion boundaries and does not solve large displacement alone. Optional neural backends can be evaluated later.

## Offline vs Preview

Offline renders prioritize quality, determinism, float precision, and resumability. Feedback's 8/16-bit PNG export can optionally use CPU flow-guided temporal integration after the CPU/Metal state parity gate; its RGBA32F checkpoint remains the unfiltered canonical state. The current ProRes handoff still converts image sequences through an 8-bit pixel buffer, so a 16-bit PNG sequence is the archival output until a high-bit-depth VideoToolbox path lands. Realtime preview may use lower resolution, fewer iterations, or cached approximations, but must preserve graph semantics.
