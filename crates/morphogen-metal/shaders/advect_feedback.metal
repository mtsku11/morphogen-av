#include <metal_stdlib>
using namespace metal;

struct AdvectFeedbackParams {
  float feedbackGain;
  float flowAmount;
  uint width;
  uint height;
};

kernel void advect_feedback(
  texture2d<float, access::sample> currentCarrier [[texture(0)]],
  texture2d<float, access::sample> previousOutput [[texture(1)]],
  texture2d<float, access::sample> velocityField [[texture(2)]],
  texture2d<float, access::write> output [[texture(3)]],
  constant AdvectFeedbackParams& params [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  constexpr sampler linearClamp(address::clamp_to_edge, filter::linear);
  float2 pixel = float2(gid);
  float2 uv = (pixel + 0.5) / float2(params.width, params.height);
  float2 velocity = velocityField.sample(linearClamp, uv).xy;
  float2 historyUv = (pixel - velocity * params.flowAmount + 0.5) / float2(params.width, params.height);

  float4 carrier = currentCarrier.sample(linearClamp, uv);
  float4 history = previousOutput.sample(linearClamp, historyUv);
  output.write(mix(carrier, history, params.feedbackGain), gid);
}
