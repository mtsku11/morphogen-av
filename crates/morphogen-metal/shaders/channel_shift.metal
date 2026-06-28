#include <metal_stdlib>
using namespace metal;

// Per-channel spatial offsets in pixels.
// out.R(x,y) = B.R(x - shift_r_x, y - shift_r_y), and so on for G and B.
// Alpha is sampled from the unshifted position.
struct ChannelShiftParams {
    uint  width;
    uint  height;
    float shift_r_x;
    float shift_r_y;
    float shift_g_x;
    float shift_g_y;
    float shift_b_x;
    float shift_b_y;
};

kernel void channel_shift(
    texture2d<float, access::sample> source_b [[texture(0)]],
    texture2d<float, access::write>  output   [[texture(1)]],
    constant ChannelShiftParams&     params   [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]
) {
    if (gid.x >= params.width || gid.y >= params.height) return;

    constexpr sampler linearClamp(address::clamp_to_edge, filter::linear);
    float2 wh = float2(params.width, params.height);
    float2 px = float2(gid);

    // UV convention matches flow_displace.metal: pixel coord p → (p + 0.5) / wh.
    // Shifting channel C by (dx, dy) means sampling at (p - (dx,dy)) in pixel coords.
    float2 uv_r = (px - float2(params.shift_r_x, params.shift_r_y) + 0.5) / wh;
    float2 uv_g = (px - float2(params.shift_g_x, params.shift_g_y) + 0.5) / wh;
    float2 uv_b = (px - float2(params.shift_b_x, params.shift_b_y) + 0.5) / wh;
    float2 uv_a = (px + 0.5) / wh;

    float r = source_b.sample(linearClamp, uv_r).r;
    float g = source_b.sample(linearClamp, uv_g).g;
    float b = source_b.sample(linearClamp, uv_b).b;
    float a = source_b.sample(linearClamp, uv_a).a;

    output.write(float4(r, g, b, a), gid);
}
