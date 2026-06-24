#include <metal_stdlib>
using namespace metal;

// Two-source faux-fluid advection — the GPU port of
// `morphogen_render::fluid_advect_two_source_frame_cpu`. Source A's optical flow advects
// Source B's dye, then a fraction of the current B frame is reinjected. This is
// `flow_displace` (the parity-gated displace) plus the reinject composite in one pass:
//   advected = sampleBilinearClamped(previous, pixel + flow * advect)
//   result   = advected + (carrierB - advected) * reinject
// Compiled with fast-math disabled so the float math matches the CPU reference; the CLI
// gates this output against `fluid_advect_two_source_frame_cpu` per frame.

struct FluidAdvectTwoSourceParams {
  float advect;
  float reinject;
  uint width;
  uint height;
};

// Bilinear sample with border clamping that matches the CPU `sample_bilinear_clamped`
// reference bit-for-bit (manual weights, no hardware sampler quantization) — identical to
// the helper in advect_feedback.metal / fluid_advect.metal.
static inline float4 sampleBilinearClamped(
  texture2d<float, access::sample> image,
  float2 position,
  float2 dimensions
) {
  float maxX = dimensions.x - 1.0;
  float maxY = dimensions.y - 1.0;
  float clampedX = clamp(position.x, 0.0, maxX);
  float clampedY = clamp(position.y, 0.0, maxY);
  uint x0 = uint(floor(clampedX));
  uint y0 = uint(floor(clampedY));
  uint x1 = min(x0 + 1u, uint(maxX));
  uint y1 = min(y0 + 1u, uint(maxY));
  float tx = clampedX - float(x0);
  float ty = clampedY - float(y0);
  float4 c00 = image.read(uint2(x0, y0));
  float4 c10 = image.read(uint2(x1, y0));
  float4 c01 = image.read(uint2(x0, y1));
  float4 c11 = image.read(uint2(x1, y1));
  float4 top = mix(c00, c10, tx);
  float4 bottom = mix(c01, c11, tx);
  return mix(top, bottom, ty);
}

kernel void fluid_advect_two_source(
  texture2d<float, access::sample> carrierB [[texture(0)]],
  texture2d<float, access::sample> previous [[texture(1)]],
  texture2d<float, access::read> flow [[texture(2)]],
  texture2d<float, access::write> output [[texture(3)]],
  constant FluidAdvectTwoSourceParams& params [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  float2 dimensions = float2(params.width, params.height);
  float2 velocity = flow.read(gid).xy;

  // flow_displace convention: read the dye at pixel + flow * advect.
  float2 position = float2(gid) + velocity * params.advect;
  float4 advected = sampleBilinearClamped(previous, position, dimensions);

  // Source reinjection (the "frame refresh"): bleed in a fraction of the current B frame.
  float4 b = carrierB.read(gid);
  float r = params.reinject;
  float4 result = advected + (b - advected) * r;

  output.write(result, gid);
}
