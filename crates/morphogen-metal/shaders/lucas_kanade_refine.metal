#include <metal_stdlib>
using namespace metal;

// One warp-refinement iteration of dense pyramidal Lucas-Kanade — the GPU port of
// `morphogen_render::optical_flow::refine_level_cpu`'s inner pass. Each thread owns
// one pixel of one pyramid level: it reads its current flow estimate, accumulates the
// 2x2 structure tensor and temporal terms over a (2r+1)^2 window, then applies the
// least-squares update. The surrounding pyramid build, upsample, forward/backward
// filter and resample stay on the CPU, so this kernel is the entire GPU parity surface.
//
// Bilinear sampling uses manual `mix` weights (NOT a hardware sampler) so it matches the
// CPU `sample_bilinear` bit-for-bit, and the dispatch compiles with fast-math disabled.
// Flow is double-buffered (read from `flowIn`, write to `flowOut`) so successive
// iterations dispatched as separate command buffers see each other's writes without an
// in-place read/write hazard — equivalent to the CPU's per-pixel in-place update because
// no thread reads a neighbour's flow.

struct LucasKanadeRefineParams {
  uint width;
  uint height;
  int radius;
};

constant float LK_DETERMINANT_EPSILON = 1e-6;

// Matches `sample_bilinear` / `sample_scalar_clamped`: clamp-to-edge borders, manual
// bilinear with `mix(a, b, t) == a + (b - a) * t` to mirror the CPU arithmetic exactly.
static inline float sampleScalarClamped(
  texture2d<float, access::read> image,
  uint width,
  uint height,
  float x,
  float y
) {
  if (width == 0u || height == 0u) {
    return 0.0;
  }
  float maxX = float(width - 1u);
  float maxY = float(height - 1u);
  float clampedX = clamp(x, 0.0, maxX);
  float clampedY = clamp(y, 0.0, maxY);
  uint x0 = uint(floor(clampedX));
  uint y0 = uint(floor(clampedY));
  uint x1 = min(x0 + 1u, width - 1u);
  uint y1 = min(y0 + 1u, height - 1u);
  float tx = clampedX - float(x0);
  float ty = clampedY - float(y0);
  float c00 = image.read(uint2(x0, y0)).r;
  float c10 = image.read(uint2(x1, y0)).r;
  float c01 = image.read(uint2(x0, y1)).r;
  float c11 = image.read(uint2(x1, y1)).r;
  float top = mix(c00, c10, tx);
  float bottom = mix(c01, c11, tx);
  return mix(top, bottom, ty);
}

// Matches `structure_confidence`: ratio of the smaller to the larger eigenvalue of the
// structure tensor, guarded by the determinant epsilon.
static inline float structureConfidence(float sxx, float sxy, float syy, float determinant) {
  if (determinant <= LK_DETERMINANT_EPSILON) {
    return 0.0;
  }
  float trace = sxx + syy;
  float discriminant = sqrt((sxx - syy) * (sxx - syy) + 4.0 * sxy * sxy);
  float minimum = max((trace - discriminant) * 0.5, 0.0);
  float maximum = max((trace + discriminant) * 0.5, LK_DETERMINANT_EPSILON);
  return clamp(minimum / maximum, 0.0, 1.0);
}

kernel void lucas_kanade_refine(
  texture2d<float, access::read> previous [[texture(0)]],
  texture2d<float, access::read> current [[texture(1)]],
  texture2d<float, access::read> flowIn [[texture(2)]],
  texture2d<float, access::write> flowOut [[texture(3)]],
  texture2d<float, access::write> confidence [[texture(4)]],
  constant LucasKanadeRefineParams& params [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  uint width = params.width;
  uint height = params.height;
  int radius = params.radius;
  float x = float(gid.x);
  float y = float(gid.y);
  float2 estimate = flowIn.read(gid).xy;

  float sxx = 0.0;
  float sxy = 0.0;
  float syy = 0.0;
  float sxt = 0.0;
  float syt = 0.0;

  for (int windowY = -radius; windowY <= radius; windowY += 1) {
    for (int windowX = -radius; windowX <= radius; windowX += 1) {
      float currentX = x + float(windowX);
      float currentY = y + float(windowY);
      float previousX = currentX - estimate.x;
      float previousY = currentY - estimate.y;
      float ix = 0.5
        * (sampleScalarClamped(previous, width, height, previousX + 1.0, previousY)
          - sampleScalarClamped(previous, width, height, previousX - 1.0, previousY));
      float iy = 0.5
        * (sampleScalarClamped(previous, width, height, previousX, previousY + 1.0)
          - sampleScalarClamped(previous, width, height, previousX, previousY - 1.0));
      float it = sampleScalarClamped(current, width, height, currentX, currentY)
        - sampleScalarClamped(previous, width, height, previousX, previousY);

      sxx += ix * ix;
      sxy += ix * iy;
      syy += iy * iy;
      sxt += ix * it;
      syt += iy * it;
    }
  }

  float determinant = sxx * syy - sxy * sxy;
  confidence.write(float4(structureConfidence(sxx, sxy, syy, determinant)), gid);

  // Carry the prior estimate forward by default; only overwrite when the system is
  // well-conditioned, exactly as the CPU reference leaves the flow untouched on a
  // degenerate determinant or a non-finite update.
  float2 result = estimate;
  if (abs(determinant) > LK_DETERMINANT_EPSILON) {
    float deltaX = (-syy * sxt + sxy * syt) / determinant;
    float deltaY = (sxy * sxt - sxx * syt) / determinant;
    if (isfinite(deltaX) && isfinite(deltaY)) {
      result = float2(estimate.x + deltaX, estimate.y + deltaY);
    }
  }
  flowOut.write(float4(result.x, result.y, 0.0, 0.0), gid);
}
