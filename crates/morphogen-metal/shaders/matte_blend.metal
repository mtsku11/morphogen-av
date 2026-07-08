#include <metal_stdlib>
using namespace metal;

struct MatteBlendParams {
    uint width;
    uint height;
};

// out = m*effected + (1-m)*original, alpha from `effected` — the exact CPU
// blend in matte.rs::apply_matte, ported as a trivial per-pixel gather (no
// cross-thread coordination). The matte field itself is computed on the CPU
// (docs/SPATIAL_MATTE_MILESTONE.md S2: matte-field compute stays CPU; only the
// blend is a Metal kernel).
kernel void matte_blend(
    texture2d<float, access::read>  effected [[texture(0)]],
    texture2d<float, access::read>  original [[texture(1)]],
    texture2d<float, access::read>  matte    [[texture(2)]],
    texture2d<float, access::write> output   [[texture(3)]],
    constant MatteBlendParams&      params   [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]
) {
    if (gid.x >= params.width || gid.y >= params.height) return;

    float4 fx   = effected.read(gid);
    float4 orig = original.read(gid);
    float  m    = matte.read(gid).r;

    float3 out_rgb = m * fx.rgb + (1.0 - m) * orig.rgb;
    output.write(float4(out_rgb, fx.a), gid);
}
