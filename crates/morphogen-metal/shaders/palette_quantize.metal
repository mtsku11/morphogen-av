#include <metal_stdlib>
using namespace metal;

struct PaletteQuantizeParams {
    uint  width;
    uint  height;
    uint  mode;    // 0 = posterize, 1 = neon palette
    uint  levels;  // posterize: discrete steps per channel (mode 0 only)
};

// Mirror of NEON_PALETTE in palette_quantize.rs — values are exact in f32.
constant float3 NEON_PALETTE[4] = {
    float3(1.0,  0.0,  1.0 ),  // magenta
    float3(1.0,  0.5,  0.0 ),  // neon orange
    float3(0.0,  0.75, 0.75),  // teal
    float3(0.0,  0.0,  0.0 ),  // black
};

kernel void palette_quantize(
    texture2d<float, access::read>  source_b [[texture(0)]],
    texture2d<float, access::write> output   [[texture(1)]],
    constant PaletteQuantizeParams& params   [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]
) {
    if (gid.x >= params.width || gid.y >= params.height) return;

    float4 pixel = source_b.read(gid);
    float3 rgb   = pixel.rgb;
    float3 out_rgb;

    if (params.mode == 0u) {
        // Posterize: round each channel to 'levels' discrete steps.
        float scale = float(params.levels - 1u);
        out_rgb = round(rgb * scale) / scale;
    } else {
        // Neon palette: nearest colour by RGB L2, tie-break = lowest index.
        float best_dist = 1.0e9;
        int   best_idx  = 0;
        for (int i = 0; i < 4; i++) {
            float3 diff = rgb - NEON_PALETTE[i];
            // Explicit separate multiplies — avoids FMA reordering vs CPU.
            float d = diff.r * diff.r + diff.g * diff.g + diff.b * diff.b;
            if (d < best_dist) {
                best_dist = d;
                best_idx  = i;
            }
        }
        out_rgb = NEON_PALETTE[best_idx];
    }

    output.write(float4(out_rgb, pixel.a), gid);
}
