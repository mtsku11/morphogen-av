#include <metal_stdlib>
using namespace metal;

// Convolutional AV Blending — image-kernel mode.
// Per pixel: accumulate the centered KxK weighted sum of the carrier (clamped
// border, taps applied without flip), blend it with the carrier by amount, and
// write the result (alpha preserved). The weights buffer is the normalized KxK
// kernel built CPU-side from Source A's luma. The CPU reference in
// morphogen-render (convolution_blend_cpu) evaluates the identical math; this
// kernel is parity-gated against it before export.

struct ConvolutionBlendParams {
    float amount;
    uint width;
    uint height;
    uint kernel_size;
};

kernel void convolution_blend(
    texture2d<float, access::read> carrier [[texture(0)]],
    texture2d<float, access::write> output [[texture(1)]],
    constant float *weights [[buffer(0)]],
    constant ConvolutionBlendParams &params [[buffer(1)]],
    uint2 gid [[thread_position_in_grid]]
) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    int radius = (int)(params.kernel_size / 2u);
    int max_x = (int)params.width - 1;
    int max_y = (int)params.height - 1;
    float4 here = carrier.read(gid);

    float3 accum = float3(0.0);
    for (uint ky = 0; ky < params.kernel_size; ky++) {
        int sy = clamp((int)gid.y + (int)ky - radius, 0, max_y);
        for (uint kx = 0; kx < params.kernel_size; kx++) {
            int sx = clamp((int)gid.x + (int)kx - radius, 0, max_x);
            float weight = weights[ky * params.kernel_size + kx];
            float4 sample = carrier.read(uint2((uint)sx, (uint)sy));
            accum += weight * sample.rgb;
        }
    }

    float3 blended = clamp(mix(here.rgb, accum, params.amount), 0.0, 1.0);
    output.write(float4(blended, here.a), gid);
}
