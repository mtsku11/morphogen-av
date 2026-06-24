#include <metal_stdlib>
using namespace metal;

// Discrete-carrier particle splat — the GPU port of
// `morphogen_render::render_field_particles`. The particle state (positions + colours) is
// computed on the CPU (the field integration stays CPU); this kernel only rasterizes it.
//
// The CPU render is a SCATTER painted in particle-index order (last writer wins on overlap).
// To match that byte-for-byte without atomics, each output pixel GATHERS: it scans the
// particle buffer and keeps the colour of the LAST (highest-index) particle whose
// `particle_size` square covers it — identical to the CPU last-writer-wins. Positions are the
// CPU-computed floats uploaded verbatim, so `round()` lands on the same integer cells and the
// result is byte-identical to the reference.
//
// Note: this is O(width * height * particle_count). For a dense grid that is more work than
// the CPU scatter (which only touches each particle's size^2 pixels); a tiled/binned scatter
// is the perf follow-up. Correctness-first, like the large-K convolution kernel.

struct FieldParticlesSplatParams {
  uint width;
  uint height;
  uint particle_count;
  uint particle_size;
};

// Each particle occupies 6 floats in the buffer: x, y, r, g, b, a (render/index order).
constant uint PARTICLE_STRIDE = 6u;

kernel void field_particles_splat(
  texture2d<float, access::write> output [[texture(0)]],
  device const float* particles [[buffer(0)]],
  constant FieldParticlesSplatParams& params [[buffer(1)]],
  uint2 gid [[thread_position_in_grid]]
) {
  if (gid.x >= params.width || gid.y >= params.height) {
    return;
  }

  int x = int(gid.x);
  int y = int(gid.y);
  int size = int(params.particle_size);

  // Black background with opaque alpha, matching the CPU canvas [0, 0, 0, 1].
  float4 result = float4(0.0, 0.0, 0.0, 1.0);

  for (uint i = 0; i < params.particle_count; ++i) {
    uint base = i * PARTICLE_STRIDE;
    int px = int(round(particles[base + 0]));
    int py = int(round(particles[base + 1]));
    if (x >= px && x < px + size && y >= py && y < py + size) {
      // A covering particle; later indices overwrite earlier ones (last writer wins).
      result = float4(
          particles[base + 2], particles[base + 3], particles[base + 4], particles[base + 5]);
    }
  }

  output.write(result, gid);
}
