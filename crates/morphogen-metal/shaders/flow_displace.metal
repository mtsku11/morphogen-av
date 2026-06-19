#include <metal_stdlib>
using namespace metal;

struct FlowDisplaceParams {
  float amount;
  uint width;
  uint height;
};

kernel void flow_displace(
  texture2d<float, access::sample> carrier [[texture(0)]],
  texture2d<float, access::read> flow [[texture(1)]],
  texture2d<float, access::write> output [[texture(2)]],
  constant FlowDisplaceParams& params [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  constexpr sampler linearClamp(address::clamp_to_edge, filter::linear);
  float2 pixel = float2(gid);

  // Flow is an RG32F texture in output pixel coordinates. This mirrors the CPU
  // reference path: each output pixel reads one vector, scales it by amount,
  // then samples the carrier with bilinear filtering and clamp-to-edge borders.
  float2 vector = flow.read(gid).xy;
  float2 displaced = pixel + vector * params.amount;
  float2 uv = (displaced + 0.5) / float2(params.width, params.height);
  float4 color = carrier.sample(linearClamp, uv);
  output.write(color, gid);
}
