#include <metal_stdlib>
using namespace metal;

struct GranularMosaicParams {
  float rearrangement;
  uint width;
  uint height;
  uint grainSize;
  uint selectionColumns;
};

kernel void granular_mosaic(
  texture2d<float, access::sample> carrier [[texture(0)]],
  texture2d<float, access::write> output [[texture(1)]],
  constant GranularMosaicParams& params [[buffer(0)]],
  device const uint* selectionIndices [[buffer(1)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  constexpr sampler linearClamp(address::clamp_to_edge, filter::linear);
  uint tileX = gid.x / params.grainSize;
  uint tileY = gid.y / params.grainSize;
  uint selectionIndex = tileY * params.selectionColumns + tileX;
  uint sourceTile = selectionIndices[selectionIndex];
  uint sourceX = (sourceTile % params.selectionColumns) * params.grainSize
    + gid.x % params.grainSize;
  uint sourceY = (sourceTile / params.selectionColumns) * params.grainSize
    + gid.y % params.grainSize;
  float2 sourcePixel = mix(float2(gid), float2(sourceX, sourceY), params.rearrangement);
  float2 uv = (sourcePixel + 0.5) / float2(params.width, params.height);
  output.write(carrier.sample(linearClamp, uv), gid);
}
