#include <metal_stdlib>
using namespace metal;

// Retro static — deliberate scanline-filter misread glitch. Mirrors
// morphogen_render::retro_static::render_retro_static_frame exactly: a source
// pixel is (1) simulated as a PNG-style adaptive-filtered scanline (predictor
// reads only RAW source neighbours, never other filtered output — no raster
// dependency chain, so this is safe to recompute per output pixel with zero
// cross-thread state), then (2) deliberately misread at a different
// bytes-per-pixel stride. All integer/mod-256 arithmetic — no trig, no float
// accumulation — so this should be bit-identical to the CPU reference.

struct RetroStaticParams {
  uint width;
  uint height;
  uint real_bpp;
  uint assumed_bpp;
  uint filter;     // 0=None 1=Sub 2=Up 3=Average 4=Paeth
  float strength;
};

inline uint quantize_channel(float v) {
  float c = clamp(v, 0.0, 1.0) * 255.0;
  return uint(round(c));
}

// Raw (unfiltered) simulated byte for the encode pixel (ex, ey), channel c.
// c == 3 is always a constant opaque-alpha byte (255, unconditionally — matches
// the CPU reference's raw_channel exactly, including for out-of-bounds pixels);
// c > 3 is 0 padding; c in 0..3 samples the source (0 if out of bounds).
inline int raw_channel(
    texture2d<float, access::read> source,
    int ex, int ey, uint c, uint width, uint height
) {
  if (c == 3) return 255;
  if (c > 3) return 0;
  if (ex < 0 || ey < 0 || ex >= int(width) || ey >= int(height)) return 0;
  float4 px = source.read(uint2(ex, ey));
  float v = (c == 0) ? px.r : ((c == 1) ? px.g : px.b);
  return int(quantize_channel(v));
}

inline int paeth_predictor(int a, int b, int c) {
  int p = a + b - c;
  int pa = abs(p - a);
  int pb = abs(p - b);
  int pc = abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

// The filtered byte for encode row `ey`, intra-row slot `slot` (0..w*real_bpp).
inline uint filtered_byte(
    texture2d<float, access::read> source,
    int ey, int slot, uint real_bpp, uint filter, uint width, uint height
) {
  int ex = slot / int(real_bpp);
  uint c = uint(slot % int(real_bpp));
  int raw = raw_channel(source, ex, ey, c, width, height);
  int left = raw_channel(source, ex - 1, ey, c, width, height);
  int up = raw_channel(source, ex, ey - 1, c, width, height);
  int up_left = raw_channel(source, ex - 1, ey - 1, c, width, height);
  int predictor = 0;
  if (filter == 1) predictor = left;
  else if (filter == 2) predictor = up;
  else if (filter == 3) predictor = (left + up) / 2;
  else if (filter == 4) predictor = paeth_predictor(left, up, up_left);
  int diff = raw - predictor;
  int m = diff % 256;
  if (m < 0) m += 256;
  return uint(m);
}

kernel void retro_static(
  texture2d<float, access::read> source [[texture(0)]],
  texture2d<float, access::write> output [[texture(1)]],
  constant RetroStaticParams& params [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) return;

  float4 src = source.read(gid);

  if (params.strength <= 0.0) {
    output.write(src, gid);
    return;
  }

  int real_row_len = int(params.width) * int(params.real_bpp);
  long assumed_stride = long(params.width) * long(params.assumed_bpp) + 1;
  long total_filtered_len = long(params.height) * long(real_row_len);

  long start = long(gid.y) * assumed_stride + 1;
  float rgb[3];
  for (int i = 0; i < 3; ++i) {
    long idx = start + long(gid.x) * 3 + long(i);
    uint byte_val = 0;
    if (idx >= 0 && idx < total_filtered_len && real_row_len > 0) {
      int ey = int(idx / real_row_len);
      int slot = int(idx % real_row_len);
      byte_val = filtered_byte(source, ey, slot, params.real_bpp, params.filter, params.width, params.height);
    }
    rgb[i] = float(byte_val) / 255.0;
  }

  float4 glitch = float4(rgb[0], rgb[1], rgb[2], 1.0);
  float4 result = src + (glitch - src) * params.strength;
  result.a = src.a;
  output.write(result, gid);
}
