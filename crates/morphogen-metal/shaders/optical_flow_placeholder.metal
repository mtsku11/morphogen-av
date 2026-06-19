#include <metal_stdlib>
using namespace metal;

// Real optical flow is future work. The first production path should likely be
// a deterministic multiscale classical method using image pyramids, gradients,
// and iterative refinement. Optional ML/neural flow backends can be evaluated
// later, but they should not replace a testable CPU/GPU reference path.

kernel void optical_flow_placeholder(
  texture2d<float, access::sample> previousFrame [[texture(0)]],
  texture2d<float, access::sample> nextFrame [[texture(1)]],
  texture2d<float, access::write> outputFlow [[texture(2)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= outputFlow.get_width() || gid.y >= outputFlow.get_height()) {
    return;
  }

  outputFlow.write(float4(0.0, 0.0, 0.0, 1.0), gid);
}
