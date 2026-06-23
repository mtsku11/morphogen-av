#include <metal_stdlib>
using namespace metal;

// Descriptor-coagulated flow blend — composite stage.
// Per pixel: optionally apply a per-cell coherent block-jitter offset, bilinearly
// sample the low-resolution A/B ownership field (cols x rows weights, built
// CPU-side), apply the per-pixel dithered hard/soft edge blend, and lerp Source A
// over Source B by the resulting weight (alpha taken from B). The CPU reference in
// morphogen-render (composite_with_field) evaluates the identical math; this kernel
// is parity-gated against it before export. Compiled with fast-math disabled so the
// float arithmetic — and therefore the hard-edge threshold decision — matches the
// CPU bit-for-bit.

struct CoagulatedCompositeParams {
    uint width;
    uint height;
    uint cols;
    uint rows;
    uint patch_size;
    uint seed_lo;
    uint seed_hi;
    float edge_hardness;
    float edge_dither;
    float block_jitter;
};

// splitmix64 finalizer — identical to the Rust hash_u64. Integer ops wrap by default.
static inline ulong hash_u64(ulong x) {
    ulong z = x + 0x9E3779B97F4A7C15UL;
    z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9UL;
    z = (z ^ (z >> 27)) * 0x94D049BB133111EBUL;
    return z ^ (z >> 31);
}

// hash01(seed, a, b) -> [0, 1), matching the Rust reference exactly.
static inline float hash01(ulong seed, ulong a, ulong b) {
    ulong h = hash_u64(seed ^ (a * 0x100000001B3UL) ^ (b * 0xD6E8FEB86659FD93UL));
    return (float)(h >> 40) / 16777216.0f; // 2^24
}

constant ulong EDGE_SALT = 0xA5A55A5AC3C33C3CUL;
constant ulong JITTER_SALT_X = 0x123456789ABCDEF0UL;
constant ulong JITTER_SALT_Y = 0x0FEDCBA987654321UL;

// Bilinear ownership-field sample at fractional pixel coordinates, clamped at the
// grid borders — the scalar analogue of sample_bilinear_clamped / sample_pixel.
static inline float sample_field(
    constant float *weights,
    int cols,
    int rows,
    uint patch_size,
    float px,
    float py
) {
    float fx = (px + 0.5f) / (float)patch_size - 0.5f;
    float fy = (py + 0.5f) / (float)patch_size - 0.5f;
    float x0 = floor(fx);
    float y0 = floor(fy);
    float tx = fx - x0;
    float ty = fy - y0;
    int cx0 = clamp((int)x0, 0, cols - 1);
    int cy0 = clamp((int)y0, 0, rows - 1);
    int cx1 = clamp((int)(x0 + 1.0f), 0, cols - 1);
    int cy1 = clamp((int)(y0 + 1.0f), 0, rows - 1);
    float w00 = weights[cy0 * cols + cx0];
    float w10 = weights[cy0 * cols + cx1];
    float w01 = weights[cy1 * cols + cx0];
    float w11 = weights[cy1 * cols + cx1];
    float top = w00 + (w10 - w00) * tx;
    float bottom = w01 + (w11 - w01) * tx;
    return top + (bottom - top) * ty;
}

kernel void coagulated_composite(
    texture2d<float, access::read> source_a [[texture(0)]],
    texture2d<float, access::read> source_b [[texture(1)]],
    texture2d<float, access::write> output [[texture(2)]],
    constant float *weights [[buffer(0)]],
    constant CoagulatedCompositeParams &params [[buffer(1)]],
    uint2 gid [[thread_position_in_grid]]
) {
    if (gid.x >= params.width || gid.y >= params.height) {
        return;
    }
    ulong seed = ((ulong)params.seed_hi << 32) | (ulong)params.seed_lo;

    float px = (float)gid.x;
    float py = (float)gid.y;
    if (params.block_jitter != 0.0f) {
        ulong cx = (ulong)(gid.x / params.patch_size);
        ulong cy = (ulong)(gid.y / params.patch_size);
        float span = params.block_jitter * (float)params.patch_size;
        float ox = (hash01(seed ^ JITTER_SALT_X, cx, cy) - 0.5f) * 2.0f * span;
        float oy = (hash01(seed ^ JITTER_SALT_Y, cx, cy) - 0.5f) * 2.0f * span;
        px = (float)gid.x + ox;
        py = (float)gid.y + oy;
    }

    float w_soft = clamp(
        sample_field(weights, (int)params.cols, (int)params.rows, params.patch_size, px, py),
        0.0f,
        1.0f
    );

    float w_eff = w_soft;
    float hardness = clamp(params.edge_hardness, 0.0f, 1.0f);
    if (hardness > 0.0f) {
        float dither =
            (hash01(seed ^ EDGE_SALT, (ulong)gid.x, (ulong)gid.y) - 0.5f) * params.edge_dither;
        float hard = (w_soft + dither >= 0.5f) ? 1.0f : 0.0f;
        w_eff = w_soft + (hard - w_soft) * hardness;
    }

    float4 a = source_a.read(gid);
    float4 b = source_b.read(gid);
    float4 out = float4(
        b.x + (a.x - b.x) * w_eff,
        b.y + (a.y - b.y) * w_eff,
        b.z + (a.z - b.z) * w_eff,
        b.w + (a.w - b.w) * w_eff
    );
    output.write(out, gid);
}
