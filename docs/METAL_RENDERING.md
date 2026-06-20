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

Flow displacement samples the carrier at coordinates offset by a vector field derived from the modulator. The CPU and Metal implementations should agree on coordinate conventions and border behavior.

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

## Image Pyramids and Optical Flow

Image pyramids support multiscale analysis such as coarse-to-fine optical flow. The first serious optical-flow implementation should probably be a classical deterministic method before optional neural flow backends are considered.

## Offline vs Preview

Offline renders prioritize quality, determinism, float precision, and resumability. Realtime preview may use lower resolution, fewer iterations, or cached approximations, but must preserve graph semantics.
