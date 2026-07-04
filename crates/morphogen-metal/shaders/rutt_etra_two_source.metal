#include <metal_stdlib>
using namespace metal;

// Rutt-Etra TWO-SOURCE gather kernel — the GPU port of
// `morphogen_render::render_rutt_etra_two_source_frame`. Identical gather proof
// to `rutt_etra_scanline.metal` (each output pixel scans scanlines in REVERSE
// order and stops at the first covering span = last-writer-wins without
// atomics), with one change: the displacement luma is read from Source A while
// the drawn colour is read from Source B. With source_a == source_b this is
// byte-identical to the single-source kernel (the continuity identity).
//
// Metal's round() is round-half-away-from-zero (MSL spec §2.12), matching
// Rust's f32::round() for all finite inputs — no special-casing needed.

struct RuttEtraParams {
    uint  width;
    uint  height;
    uint  line_pitch;
    float displacement_depth;
    uint  line_thickness;
    uint  mono; // 0 or 1
};

kernel void rutt_etra_two_source(
    texture2d<float, access::read>  source_a [[texture(0)]],
    texture2d<float, access::read>  source_b [[texture(1)]],
    texture2d<float, access::write> output   [[texture(2)]],
    constant RuttEtraParams&        params   [[buffer(0)]],
    uint2 gid [[thread_position_in_grid]]
) {
    if (gid.x >= params.width || gid.y >= params.height) return;

    int px = int(gid.x);
    int py = int(gid.y);

    // Black background with opaque alpha, matching the CPU canvas [0, 0, 0, 1].
    float4 result = float4(0.0, 0.0, 0.0, 1.0);

    // Compute the number of scanlines: ceil(height / pitch).
    uint num_scanlines = (params.height + params.line_pitch - 1) / params.line_pitch;

    // Scan scanlines in reverse order (bottom → top = last writer wins equivalent).
    for (int s = int(num_scanlines) - 1; s >= 0; s--) {
        uint y0 = uint(s) * params.line_pitch;

        // Displacement luma comes from Source A (column px at scanline y0).
        float4 luma_pixel_a = source_a.read(uint2(uint(px), y0));
        float luma_a = clamp(
            0.2126 * luma_pixel_a.r + 0.7152 * luma_pixel_a.g + 0.0722 * luma_pixel_a.b,
            0.0, 1.0);
        int y_a = int(y0) - int(round(params.displacement_depth * luma_a));

        // ...and for column px+1 (clamped to last column) at scanline y0.
        uint px_b = min(uint(px + 1), params.width - 1);
        float4 luma_pixel_b = source_a.read(uint2(px_b, y0));
        float luma_b = clamp(
            0.2126 * luma_pixel_b.r + 0.7152 * luma_pixel_b.g + 0.0722 * luma_pixel_b.b,
            0.0, 1.0);
        int y_b = int(y0) - int(round(params.displacement_depth * luma_b));

        int span_lo = min(y_a, y_b);
        int span_hi = max(y_a, y_b) + int(params.line_thickness) - 1;

        if (span_lo <= py && py <= span_hi) {
            // First covering scanline from the bottom — this is the winner.
            // The drawn colour comes from Source B (column px at scanline y0).
            float4 colour_b = source_b.read(uint2(uint(px), y0));
            result = (params.mono != 0u) ? float4(1.0, 1.0, 1.0, 1.0) : colour_b;
            break;
        }
    }

    output.write(result, gid);
}
