#include <metal_stdlib>
using namespace metal;

// Faux-fluid dye advection — the GPU port of `morphogen_render::fluid_advect`.
// This kernel is ONE integration substep (the CPU reference's `advect_substep`): the
// dispatch loops it `effective_substeps()` times per frame, ping-ponging dye textures,
// with `advect` = the per-substep step, `reinject` = the per-substep compound rate and
// `time` = that substep's field time. Per output pixel: evaluate the (optionally
// domain-warped) curl-noise vortex velocity, backward-sample the previous dye buffer
// upstream (manual bilinear, matching the CPU `sample_bilinear_clamped`, optionally
// mixed with a 3x3 binomial blur — the `diffuse` faux viscosity), then bleed in the
// reinjection fraction of the current source frame, optionally gated by the animated
// blotch mask (`reinject_blotch`).
// The velocity field reproduces `morphogen_render::vortex_field` bit-for-bit: the
// same splitmix64 lattice hash, 3D gradient (Perlin) noise, quintic fade and curl.
// Compiled with fast-math disabled by the dispatch so the float math (and the integer
// hashing) matches the CPU reference within the project parity tolerance.

struct FluidAdvectParams {
  float advect;
  float turbulence_scale;
  float detail;
  float reinject;
  float time;
  float warp;
  float reinject_blotch;
  float diffuse;
  float blotch_scale;
  uint width;
  uint height;
  uint seed_lo;
  uint seed_hi;
};

// Field salts / constants — identical to morphogen_render::vortex_field.
constant ulong TURBULENCE_SALT_0 = 0x7E12B0FF5EEDC0A1UL;
constant ulong TURBULENCE_SALT_1 = 0x9A3C44D71F0BE215UL;
constant float VORTEX_DRIFT = 0.25;
constant float BIG_VORTEX_PLANE = 0.5;
constant ulong BLOTCH_SALT_0 = 0x51F09A2B7D3EC815UL;
constant ulong BLOTCH_SALT_1 = 0xC4A71E8633B95DF2UL;
constant float BLOTCH_EXPONENT = 5.5;
constant float WARP_FREQUENCY = 2.5;
constant float WARP_TIME_RATE = 2.0;

// splitmix64 finalizer — identical to the Rust hash_u64. Integer ops wrap by default.
static inline ulong hash_u64(ulong x) {
  ulong z = x + 0x9E3779B97F4A7C15UL;
  z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9UL;
  z = (z ^ (z >> 27)) * 0x94D049BB133111EBUL;
  return z ^ (z >> 31);
}

// 3D lattice hash — matches the Rust hash_coords exactly.
static inline ulong hash_coords(ulong seed, ulong a, ulong b, ulong c) {
  return hash_u64(seed
    ^ (a * 0x100000001B3UL)
    ^ (b * 0xD6E8FEB86659FD93UL)
    ^ (c * 0x59E39B1F9A2D7C4BUL));
}

// Perlin's 12 edge-midpoint gradient directions, selected by the low bits of the hash.
static inline float grad3(ulong h, float x, float y, float z) {
  switch (h & 15UL) {
    case 0: return x + y;
    case 1: return -x + y;
    case 2: return x - y;
    case 3: return -x - y;
    case 4: return x + z;
    case 5: return -x + z;
    case 6: return x - z;
    case 7: return -x - z;
    case 8: return y + z;
    case 9: return -y + z;
    case 10: return y - z;
    case 11: return -y - z;
    case 12: return x + y;
    case 13: return -y + z;
    case 14: return -x + y;
    default: return -y - z;
  }
}

// Perlin quintic fade 6t^5 - 15t^4 + 10t^3.
static inline float fade(float t) {
  return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

static inline float lerpf(float a, float b, float t) {
  return a + (b - a) * t;
}

static inline float corner(
  ulong seed, ulong ix, ulong iy, ulong iz,
  ulong dx, ulong dy, ulong dz,
  float gx, float gy, float gz
) {
  ulong h = hash_coords(seed, ix + dx, iy + dy, iz + dz);
  return grad3(h, gx, gy, gz);
}

// 3D gradient (Perlin) noise — matches the Rust gradient_noise3 bit-for-bit. The
// float->int cell index uses a two's-complement reinterpret, mirroring `as i64 as u64`.
static inline float gradient_noise3(ulong seed, float x, float y, float z) {
  float xi = floor(x);
  float yi = floor(y);
  float zi = floor(z);
  float xf = x - xi;
  float yf = y - yi;
  float zf = z - zi;
  ulong ix = (ulong)(long)xi;
  ulong iy = (ulong)(long)yi;
  ulong iz = (ulong)(long)zi;
  float u = fade(xf);
  float v = fade(yf);
  float w = fade(zf);

  float x00 = lerpf(
    corner(seed, ix, iy, iz, 0, 0, 0, xf, yf, zf),
    corner(seed, ix, iy, iz, 1, 0, 0, xf - 1.0, yf, zf),
    u);
  float x10 = lerpf(
    corner(seed, ix, iy, iz, 0, 1, 0, xf, yf - 1.0, zf),
    corner(seed, ix, iy, iz, 1, 1, 0, xf - 1.0, yf - 1.0, zf),
    u);
  float x01 = lerpf(
    corner(seed, ix, iy, iz, 0, 0, 1, xf, yf, zf - 1.0),
    corner(seed, ix, iy, iz, 1, 0, 1, xf - 1.0, yf, zf - 1.0),
    u);
  float x11 = lerpf(
    corner(seed, ix, iy, iz, 0, 1, 1, xf, yf - 1.0, zf - 1.0),
    corner(seed, ix, iy, iz, 1, 1, 1, xf - 1.0, yf - 1.0, zf - 1.0),
    u);
  float y0 = lerpf(x00, x10, v);
  float y1 = lerpf(x01, x11, v);
  return lerpf(y0, y1, w);
}

// The streamfunction psi: a steady low-frequency octave (the persistent large vortices)
// plus a detail-weighted octave at 2x frequency drifting slowly with time, optionally
// domain-warped by an animated sinusoidal shear (the Quake-style fold) — mirrors the
// Rust `streamfunction` including the warp branch.
static inline float streamfunction(
  ulong seed, float x, float y, float time, float scale, float detail, float warp
) {
  float s = scale;
  float big = gradient_noise3(seed ^ TURBULENCE_SALT_0, x * s, y * s, BIG_VORTEX_PLANE);
  float drift = time * VORTEX_DRIFT;
  float u = x * 2.0 * s + drift;
  float v = y * 2.0 * s;
  if (warp != 0.0) {
    float u0 = u;
    float phase = time * WARP_TIME_RATE;
    u += warp * sin(phase + v * WARP_FREQUENCY);
    v += warp * sin(phase + u0 * WARP_FREQUENCY);
  }
  float small = gradient_noise3(seed ^ TURBULENCE_SALT_1, u, v, time);
  return big + detail * small;
}

// The steady-vortex curl velocity (dpsi/dy, -dpsi/dx), normalized by scale.
static inline float2 steady_vortex_velocity(
  ulong seed, float x, float y, float time, float scale, float detail, float warp
) {
  const float E = 1.0;
  float psi_yp = streamfunction(seed, x, y + E, time, scale, detail, warp);
  float psi_ym = streamfunction(seed, x, y - E, time, scale, detail, warp);
  float psi_xp = streamfunction(seed, x + E, y, time, scale, detail, warp);
  float psi_xm = streamfunction(seed, x - E, y, time, scale, detail, warp);
  float dpsi_dy = (psi_yp - psi_ym) / (2.0 * E);
  float dpsi_dx = (psi_xp - psi_xm) / (2.0 * E);
  float inv = scale != 0.0 ? 1.0 / scale : 0.0;
  return float2(dpsi_dy * inv, -dpsi_dx * inv);
}

// Animated blotch mask for patchy reinjection — mirrors the Rust
// `reinjection_blotch_mask`: two gradient-noise layers scrolling in different
// directions, each sharpened by pow(., 5.5), combined with max.
static inline float reinjection_blotch_mask(
  ulong seed, float x, float y, float time, float scale
) {
  float coarse = gradient_noise3(
    seed ^ BLOTCH_SALT_0, x * scale + time * 0.35, y * scale + time * 0.5, 0.0);
  float fine = gradient_noise3(
    seed ^ BLOTCH_SALT_1, x * scale * 2.0 - time * 0.7, y * scale * 2.0 - time * 0.55, 0.0);
  float sharp_coarse = pow(clamp(0.5 + 0.5 * coarse, 0.0, 1.0), BLOTCH_EXPONENT);
  float sharp_fine = pow(clamp(0.5 + 0.5 * fine, 0.0, 1.0), BLOTCH_EXPONENT);
  return max(sharp_coarse, sharp_fine);
}

// Bilinear sample with border clamping that matches the CPU `sample_bilinear_clamped`
// reference bit-for-bit (manual weights, no hardware sampler quantization).
static inline float4 sampleBilinearClamped(
  texture2d<float, access::sample> image,
  float2 position,
  float2 dimensions
) {
  float maxX = dimensions.x - 1.0;
  float maxY = dimensions.y - 1.0;
  float clampedX = clamp(position.x, 0.0, maxX);
  float clampedY = clamp(position.y, 0.0, maxY);
  uint x0 = uint(floor(clampedX));
  uint y0 = uint(floor(clampedY));
  uint x1 = min(x0 + 1u, uint(maxX));
  uint y1 = min(y0 + 1u, uint(maxY));
  float tx = clampedX - float(x0);
  float ty = clampedY - float(y0);
  float4 c00 = image.read(uint2(x0, y0));
  float4 c10 = image.read(uint2(x1, y0));
  float4 c01 = image.read(uint2(x0, y1));
  float4 c11 = image.read(uint2(x1, y1));
  float4 top = mix(c00, c10, tx);
  float4 bottom = mix(c01, c11, tx);
  return mix(top, bottom, ty);
}

kernel void fluid_advect(
  texture2d<float, access::sample> source [[texture(0)]],
  texture2d<float, access::sample> previous [[texture(1)]],
  texture2d<float, access::write> output [[texture(2)]],
  constant FluidAdvectParams& params [[buffer(0)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  ulong seed = ((ulong)params.seed_hi << 32) | (ulong)params.seed_lo;
  float2 dimensions = float2(params.width, params.height);
  float xf = float(gid.x);
  float yf = float(gid.y);

  float2 velocity = steady_vortex_velocity(
    seed, xf, yf, params.time, params.turbulence_scale, params.detail, params.warp);

  // Semi-Lagrangian: read the dye that was upstream so colour flows downstream.
  float sx = xf - velocity.x * params.advect;
  float sy = yf - velocity.y * params.advect;
  float4 advected = sampleBilinearClamped(previous, float2(sx, sy), dimensions);
  if (params.diffuse > 0.0) {
    // 3x3 binomial (1-2-1)^2/16 blur of the bilinear samples — the faux viscosity.
    // Taps and accumulation order match the CPU `binomial_blur_sample`.
    float4 sum = float4(0.0);
    sum += sampleBilinearClamped(previous, float2(sx - 1.0, sy - 1.0), dimensions) * 1.0;
    sum += sampleBilinearClamped(previous, float2(sx, sy - 1.0), dimensions) * 2.0;
    sum += sampleBilinearClamped(previous, float2(sx + 1.0, sy - 1.0), dimensions) * 1.0;
    sum += sampleBilinearClamped(previous, float2(sx - 1.0, sy), dimensions) * 2.0;
    sum += sampleBilinearClamped(previous, float2(sx, sy), dimensions) * 4.0;
    sum += sampleBilinearClamped(previous, float2(sx + 1.0, sy), dimensions) * 2.0;
    sum += sampleBilinearClamped(previous, float2(sx - 1.0, sy + 1.0), dimensions) * 1.0;
    sum += sampleBilinearClamped(previous, float2(sx, sy + 1.0), dimensions) * 2.0;
    sum += sampleBilinearClamped(previous, float2(sx + 1.0, sy + 1.0), dimensions) * 1.0;
    float4 blurred = sum / 16.0;
    advected += (blurred - advected) * params.diffuse;
  }

  // Source reinjection (the "frame refresh"): bleed in a fraction of the live frame,
  // optionally gated by the animated blotch mask (mix(1, mask, blotch)).
  float4 src = source.read(gid);
  float r = params.reinject;
  if (params.reinject_blotch > 0.0) {
    float mask = reinjection_blotch_mask(seed, xf, yf, params.time, params.blotch_scale);
    r *= 1.0 + (mask - 1.0) * params.reinject_blotch;
  }
  float4 result = advected + (src - advected) * r;

  output.write(result, gid);
}
