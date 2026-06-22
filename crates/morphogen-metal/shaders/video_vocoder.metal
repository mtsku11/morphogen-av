#include <metal_stdlib>
using namespace metal;

// Video vocoder — histogram-specification (match) mode.
// Per pixel: read carrier luma, look up the matched tone in a 256-entry LUT
// (built CPU-side from the Source A / Source B luma CDFs), blend by amount, and
// scale RGB by the resulting gain (hue preserved, pure black stays black). The
// CPU reference in morphogen-render evaluates the identical math; this kernel is
// parity-gated against it before export.

struct VideoVocoderParams {
    float amount;
    uint width;
    uint height;
};

kernel void video_vocoder_match(
    texture2d<float, access::read> carrier [[texture(0)]],
    texture2d<float, access::write> output [[texture(1)]],
    constant float *tone [[buffer(0)]],
    constant VideoVocoderParams &params [[buffer(1)]],
    uint2 gid [[thread_position_in_grid]]
) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    float4 color = carrier.read(gid);
    float luma = clamp(color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722, 0.0, 1.0);
    uint index = min((uint)round(luma * 255.0), 255u);
    float target = mix(luma, tone[index], params.amount);
    float gain = luma > 1.0e-4 ? target / luma : 1.0;
    float3 routed = clamp(color.rgb * gain, 0.0, 1.0);
    output.write(float4(routed, color.a), gid);
}
