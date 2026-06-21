#include <metal_stdlib>
using namespace metal;

struct GranularMosaicPoolParams {
  float rearrangement;
  uint width;
  uint height;
  uint grainSize;
  uint selectionColumns;
};

// Temporal grain pool render (granular step 6b). Unlike the single-frame mosaic,
// each selected grain lives in its own pool frame, so coordinate-warp is
// undefined: `rearrangement` is a cross-frame value blend between the current
// carrier pixel and the selected grain's pixel. Samples use integer-nearest
// clamped reads to match the CPU reference's `clamped_pixel` exactly.
kernel void granular_mosaic_pool(
  texture2d<float, access::read> carrier [[texture(0)]],
  texture2d<float, access::write> output [[texture(1)]],
  texture2d_array<float, access::read> poolFrames [[texture(2)]],
  constant GranularMosaicPoolParams& params [[buffer(0)]],
  device const uint* selectionIndices [[buffer(1)]],
  device const uint* grainMeta [[buffer(2)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  uint tileX = gid.x / params.grainSize;
  uint tileY = gid.y / params.grainSize;
  uint selection = selectionIndices[tileY * params.selectionColumns + tileX];
  uint frameIndex = grainMeta[selection * 3u + 0u];
  uint originX = grainMeta[selection * 3u + 1u];
  uint originY = grainMeta[selection * 3u + 2u];

  uint grainX = min(originX + gid.x % params.grainSize, params.width - 1u);
  uint grainY = min(originY + gid.y % params.grainSize, params.height - 1u);
  float4 grainPixel = poolFrames.read(uint2(grainX, grainY), frameIndex);
  float4 carrierPixel = carrier.read(gid);
  output.write(mix(carrierPixel, grainPixel, params.rearrangement), gid);
}
