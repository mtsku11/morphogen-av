#include <metal_stdlib>
using namespace metal;

struct AdvectFeedbackParams {
  float carrierAmount;
  float feedbackAmount;
  float feedbackMix;
  float decay;
  float structureMix;
  uint width;
  uint height;
};

// Bilinear sample with border clamping that matches the CPU
// `sample_bilinear_clamped` reference bit-for-bit (manual weights, no hardware
// sampler quantization). Used for the structure-preserving high-frequency band
// so the GPU structure term stays within Metal/CPU parity tolerance.
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

kernel void advect_feedback(
  texture2d<float, access::sample> currentCarrier [[texture(0)]],
  texture2d<float, access::sample> previousOutput [[texture(1)]],
  texture2d<float, access::read> velocityField [[texture(2)]],
  texture2d<float, access::write> output [[texture(3)]],
  constant AdvectFeedbackParams& params [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  float2 pixel = float2(gid);
  float2 dimensions = float2(params.width, params.height);
  float2 flow = velocityField.read(gid).xy;

  // Manual bilinear sampling (not the hardware linear sampler) so every term
  // matches the CPU `sample_bilinear_clamped` reference to float precision. The
  // hardware sampler quantizes interpolation weights to 8 bits, which at high
  // feedback-mix amplifies into a >1/255 divergence on real fractional flow.
  float2 carrierPosition = pixel + flow * params.carrierAmount;
  float2 historyPosition = pixel + flow * params.feedbackAmount;
  float4 carrier = sampleBilinearClamped(currentCarrier, carrierPosition, dimensions);
  float4 history =
      sampleBilinearClamped(previousOutput, historyPosition, dimensions) * params.decay;
  float4 result = mix(carrier, history, params.feedbackMix);

  // Structure-preserving morph: re-inject the displaced carrier's
  // high-frequency band (carrier minus its low-pass). The low-pass is a
  // separable binomial blur (radius 2, weights [1,4,6,4,1]/16); the equivalent
  // 5x5 outer-product sum below matches the CPU reference because each axis is
  // clamped independently.
  if (params.structureMix != 0.0) {
    const float weights[5] = {1.0, 4.0, 6.0, 4.0, 1.0};
    int maxX = int(params.width) - 1;
    int maxY = int(params.height) - 1;
    float4 lowPass = float4(0.0);
    float4 displacedCenter = float4(0.0);
    for (int dy = -2; dy <= 2; ++dy) {
      int ny = clamp(int(gid.y) + dy, 0, maxY);
      for (int dx = -2; dx <= 2; ++dx) {
        int nx = clamp(int(gid.x) + dx, 0, maxX);
        float2 neighborFlow = velocityField.read(uint2(uint(nx), uint(ny))).xy;
        float2 samplePosition = float2(nx, ny) + neighborFlow * params.carrierAmount;
        float4 displaced = sampleBilinearClamped(currentCarrier, samplePosition, dimensions);
        float weight = (weights[dx + 2] * weights[dy + 2]) / 256.0;
        lowPass += displaced * weight;
        if (dx == 0 && dy == 0) {
          displacedCenter = displaced;
        }
      }
    }
    result += params.structureMix * (displacedCenter - lowPass);
  }

  output.write(result, gid);
}
