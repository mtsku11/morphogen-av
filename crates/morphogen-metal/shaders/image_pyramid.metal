#include <metal_stdlib>
using namespace metal;

struct PyramidParams {
  uint outputWidth;
  uint outputHeight;
};

kernel void downsample_luma_pyramid_level(
  texture2d<float, access::sample> input [[texture(0)]],
  texture2d<float, access::write> output [[texture(1)]],
  constant PyramidParams& params [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.outputWidth || gid.y >= params.outputHeight) {
    return;
  }

  constexpr sampler linearClamp(address::clamp_to_edge, filter::linear);
  float2 outputSize = float2(params.outputWidth, params.outputHeight);
  float2 uv = (float2(gid) + 0.5) / outputSize;
  float4 color = input.sample(linearClamp, uv);
  float luma = dot(color.rgb, float3(0.2126, 0.7152, 0.0722));
  output.write(float4(luma, luma, luma, 1.0), gid);
}
