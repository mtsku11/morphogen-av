#include <metal_stdlib>
using namespace metal;

// Maximum line length (pixels) handled in threadgroup memory.
// float4 × 1024 = 16 KB — within the 32 KB threadgroup memory floor on Apple Silicon.
// Lines wider than PS_MAX_LINE are passed through unchanged (documented limitation).
#define PS_MAX_LINE 1024

struct PixelSortParams {
    uint  width;
    uint  height;
    uint  axis;           // 0 = row, 1 = col
    uint  key;            // 0=luma, 1=hue, 2=sat, 3=red, 4=green, 5=blue
    uint  direction;      // 0 = ascending, 1 = descending
    float threshold_low;
    float threshold_high;
    uint  max_span;       // 0 = unbounded
};

// ─── Sort-key helpers — mirror pixel_sort.rs exactly (same constants, same order) ──

static float ps_luma(float r, float g, float b) {
    // Explicit fma() matches Rust's mul_add order → bit-identical sort keys on CPU and GPU.
    return fma(0.2126f, r, fma(0.7152f, g, 0.0722f * b));
}

// HSV hue in [0, 1]; returns 0 for achromatic pixels.
static float ps_hue(float r, float g, float b) {
    float maxc  = max(max(r, g), b);
    float minc  = min(min(r, g), b);
    float delta = maxc - minc;
    if (delta < 1e-6f) return 0.0f;
    float h;
    if (abs(maxc - r) < 1e-6f) {
        h = fmod((g - b) / delta, 6.0f);
    } else if (abs(maxc - g) < 1e-6f) {
        h = (b - r) / delta + 2.0f;
    } else {
        h = (r - g) / delta + 4.0f;
    }
    if (h < 0.0f) h += 6.0f;
    return h / 6.0f;
}

// HSV saturation in [0, 1].
static float ps_saturation(float r, float g, float b) {
    float maxc = max(max(r, g), b);
    if (maxc < 1e-6f) return 0.0f;
    float minc = min(min(r, g), b);
    return (maxc - minc) / maxc;
}

static float ps_sort_key(float4 px, uint key_type) {
    switch (key_type) {
        case 0:  return ps_luma(px.r, px.g, px.b);
        case 1:  return ps_hue(px.r, px.g, px.b);
        case 2:  return ps_saturation(px.r, px.g, px.b);
        case 3:  return px.r;
        case 4:  return px.g;
        case 5:  return px.b;
        default: return ps_luma(px.r, px.g, px.b);
    }
}

// ─── Stable insertion sort ───────────────────────────────────────────────────
// Called only by thread 0. Stable because equal-key elements never swap,
// matching Rust's slice.sort_by() (which uses a stable merge sort with the same
// comparison predicate). Tie-breaking is therefore identical on both paths.

static void ps_insertion_sort(
    threadgroup float4* line,
    uint start,
    uint end,
    uint key_type,
    uint direction
) {
    for (uint i = start + 1; i < end; i++) {
        float4 key_px = line[i];
        float  key_k  = ps_sort_key(key_px, key_type);
        uint   j      = i;
        while (j > start) {
            float4 prev   = line[j - 1];
            float  prev_k = ps_sort_key(prev, key_type);
            bool   do_swap;
            if (direction == 0) {       // ascending: swap if prev > current
                do_swap = (prev_k > key_k);
            } else {                    // descending: swap if prev < current
                do_swap = (prev_k < key_k);
            }
            if (!do_swap) break;
            line[j] = prev;
            j--;
        }
        line[j] = key_px;
    }
}

// ─── Kernel ──────────────────────────────────────────────────────────────────

kernel void pixel_sort(
    texture2d<float, access::read>  source [[texture(0)]],
    texture2d<float, access::write> output [[texture(1)]],
    constant PixelSortParams&       params [[buffer(0)]],
    uint3 tgid [[threadgroup_position_in_grid]],
    uint3 tid3 [[thread_position_in_threadgroup]],
    uint3 tgw3 [[threads_per_threadgroup]]
) {
    // Threads are always dispatched as (T, 1, 1); x gives the linear index / count.
    uint tid = tid3.x;
    uint tgw = tgw3.x;

    threadgroup float4 tg_line[PS_MAX_LINE];

    uint line_index, line_len;
    if (params.axis == 0) {
        line_index = tgid.y;
        line_len   = params.width;
        if (line_index >= params.height) return;
    } else {
        line_index = tgid.x;
        line_len   = params.height;
        if (line_index >= params.width) return;
    }

    // Lines longer than PS_MAX_LINE: copy through unchanged.
    if (line_len > PS_MAX_LINE) {
        if (params.axis == 0) {
            for (uint x = tid; x < line_len; x += tgw)
                output.write(source.read(uint2(x, line_index)), uint2(x, line_index));
        } else {
            for (uint y = tid; y < line_len; y += tgw)
                output.write(source.read(uint2(line_index, y)), uint2(line_index, y));
        }
        return;
    }

    // Load line into threadgroup memory (all threads share the load).
    if (params.axis == 0) {
        for (uint x = tid; x < line_len; x += tgw)
            tg_line[x] = source.read(uint2(x, line_index));
    } else {
        for (uint y = tid; y < line_len; y += tgw)
            tg_line[y] = source.read(uint2(line_index, y));
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Thread 0: span detection and stable insertion sort.
    if (tid == 0) {
        float low  = params.threshold_low;
        float high = params.threshold_high;
        uint  ms   = params.max_span;
        uint  kt   = params.key;
        uint  dir  = params.direction;

        if (low <= high) {      // off case: low > high → nothing to sort
            uint i = 0;
            while (i < line_len) {
                float k = ps_sort_key(tg_line[i], kt);
                if (k < low || k > high) { i++; continue; }
                uint span_start = i;
                while (i < line_len) {
                    float k2 = ps_sort_key(tg_line[i], kt);
                    if (k2 < low || k2 > high) break;
                    i++;
                }
                if (ms == 0 || (i - span_start) <= ms) {
                    ps_insertion_sort(tg_line, span_start, i, kt, dir);
                } else {
                    uint pos = span_start;
                    while (pos < i) {
                        uint chunk_end = min(pos + ms, i);
                        ps_insertion_sort(tg_line, pos, chunk_end, kt, dir);
                        pos = chunk_end;
                    }
                }
            }
        }
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);

    // Write sorted line back.
    if (params.axis == 0) {
        for (uint x = tid; x < line_len; x += tgw)
            output.write(tg_line[x], uint2(x, line_index));
    } else {
        for (uint y = tid; y < line_len; y += tgw)
            output.write(tg_line[y], uint2(line_index, y));
    }
}
