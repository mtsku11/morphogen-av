#include <metal_stdlib>
using namespace metal;

struct AdvectFeedbackParams {
  float carrierAmount;
  float feedbackAmount;
  float feedbackMix;
  float decay;
  uint width;
  uint height;
};

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

  constexpr sampler linearClamp(address::clamp_to_edge, filter::linear);
  float2 pixel = float2(gid);
  float2 dimensions = float2(params.width, params.height);
  float2 flow = velocityField.read(gid).xy;
  float2 carrierUv = (pixel + flow * params.carrierAmount + 0.5) / dimensions;
  float2 historyUv = (pixel + flow * params.feedbackAmount + 0.5) / dimensions;

  float4 carrier = currentCarrier.sample(linearClamp, carrierUv);
  float4 history = previousOutput.sample(linearClamp, historyUv) * params.decay;
  output.write(mix(carrier, history, params.feedbackMix), gid);
}
