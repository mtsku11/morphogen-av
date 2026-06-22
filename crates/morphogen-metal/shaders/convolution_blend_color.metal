#include <metal_stdlib>
using namespace metal;

// Convolutional AV Blending — per-channel colour mode.
// Like convolution_blend, but each carrier channel (R, G, B) is convolved with
// its OWN KxK kernel (built CPU-side from Source A's matching colour channel),
// so each channel takes on the structure of A's channel (chromatic structure
// transfer). Clamped border, taps applied without flip, blended with the carrier
// by amount, alpha preserved. The CPU reference convolution_blend_color_cpu in
// morphogen-render evaluates the identical math; this kernel is parity-gated
// against it before export.

struct ConvolutionBlendParams {
    float amount;
    uint width;
    uint height;
    uint kernel_size;
};

kernel void convolution_blend_color(
    texture2d<float, access::read> carrier [[texture(0)]],
    texture2d<float, access::write> output [[texture(1)]],
    constant float *weights_r [[buffer(0)]],
    constant float *weights_g [[buffer(1)]],
    constant float *weights_b [[buffer(2)]],
    constant ConvolutionBlendParams &params [[buffer(3)]],
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
            uint tap = ky * params.kernel_size + kx;
            float4 sample = carrier.read(uint2((uint)sx, (uint)sy));
            accum.r += weights_r[tap] * sample.r;
            accum.g += weights_g[tap] * sample.g;
            accum.b += weights_b[tap] * sample.b;
        }
    }

    float3 blended = clamp(mix(here.rgb, accum, params.amount), 0.0, 1.0);
    output.write(float4(blended, here.a), gid);
}
