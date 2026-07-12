//! Steady curl-noise **vortex velocity field** — the "flowing fluid" field shared by the
//! faux-fluid dye advection ([`crate::fluid_advect`]) and the fluid colour-sort mosaic's
//! optional vortex flow mode ([`crate::fluid_mosaic`]).
//!
//! The velocity is the analytic curl `(∂ψ/∂y, -∂ψ/∂x)` of a streamfunction `ψ` made of a
//! **steady** low-frequency gradient-noise octave (the persistent large vortices) plus a
//! `detail`-weighted octave at 2× frequency that drifts slowly with `time`. Holding the
//! big octave still is what lets advected material flow *along* the streamlines and spiral
//! into the vortex centres — an evolving field only wobbles in place. The curl is
//! divergence-free by construction, so the field transports without sources/sinks.
//!
//! Deterministic and GPU-safe: 3D gradient (Perlin) noise on a splitmix-hashed lattice
//! (no trig hashing, whose accuracy is GPU-dependent), quintic fade, trilinear interp —
//! round, C2 vortices rather than the grid-aligned blobs of value noise.

const TURBULENCE_SALT_0: u64 = 0x7E12_B0FF_5EED_C0A1;
const TURBULENCE_SALT_1: u64 = 0x9A3C_44D7_1F0B_E215;
/// Angular frequency (radians per detail-lattice cell) of the sinusoidal domain warp —
/// the analog of the reference shader's `QuakeLavaUV` shear.
const WARP_FREQUENCY: f32 = 2.5;
/// Temporal rate of the domain warp relative to the field's `time` axis (the shader
/// warps at `speed 2.0`).
const WARP_TIME_RATE: f32 = 2.0;
/// Slow horizontal drift of the fine detail octave per unit time (lattice cells) — a
/// little life in the texture without unanchoring the steady large vortices.
const VORTEX_DRIFT: f32 = 0.25;
/// The fixed z-slice of the noise used for the steady big-vortex octave (any constant —
/// it just selects one time-independent plane of the 3D field).
const BIG_VORTEX_PLANE: f32 = 0.5;

/// The steady-vortex curl velocity at a point. `scale` is the vortex frequency (lattice
/// cells per pixel; smaller ⇒ larger vortices); `detail` is the fine-octave weight; `time`
/// drifts only that detail octave (the big vortices are steady). The result is normalized
/// by `scale` so the amplitude reads in unit pixels regardless of vortex size — the caller
/// multiplies by its own strength.
pub fn steady_vortex_velocity(
    seed: u64,
    x: f32,
    y: f32,
    time: f32,
    scale: f32,
    detail: f32,
) -> (f32, f32) {
    steady_vortex_velocity_warped(seed, x, y, time, scale, detail, 0.0)
}

/// [`steady_vortex_velocity`] with a sinusoidal domain warp of amplitude `warp` (detail
/// lattice cells) applied to the **detail octave only** — the analog of the reference
/// shader's animated `QuakeLavaUV` shear, which makes advected material *fold* instead of
/// winding forever around fixed centres. The big octave stays steady (an evolving big
/// octave only wobbles in place — see the module docs), so `warp` is invisible when
/// `detail == 0`. `warp == 0.0` is bit-identical to the unwarped field.
pub fn steady_vortex_velocity_warped(
    seed: u64,
    x: f32,
    y: f32,
    time: f32,
    scale: f32,
    detail: f32,
    warp: f32,
) -> (f32, f32) {
    const E: f32 = 1.0;
    let psi_yp = streamfunction(seed, x, y + E, time, scale, detail, warp);
    let psi_ym = streamfunction(seed, x, y - E, time, scale, detail, warp);
    let psi_xp = streamfunction(seed, x + E, y, time, scale, detail, warp);
    let psi_xm = streamfunction(seed, x - E, y, time, scale, detail, warp);
    let dpsi_dy = (psi_yp - psi_ym) / (2.0 * E);
    let dpsi_dx = (psi_xp - psi_xm) / (2.0 * E);
    let inv = if scale != 0.0 { 1.0 / scale } else { 0.0 };
    (dpsi_dy * inv, -dpsi_dx * inv)
}

/// The streamfunction `ψ`: a steady low-frequency octave (the persistent large vortices)
/// plus a `detail`-weighted octave at 2× frequency drifting slowly with `time`, optionally
/// domain-warped by an animated sinusoidal shear of amplitude `warp` (lattice cells).
fn streamfunction(
    seed: u64,
    x: f32,
    y: f32,
    time: f32,
    scale: f32,
    detail: f32,
    warp: f32,
) -> f32 {
    let s = scale;
    // Big, low-frequency, STEADY octave — the persistent large vortices.
    let big = gradient_noise3(seed ^ TURBULENCE_SALT_0, x * s, y * s, BIG_VORTEX_PLANE);
    // Fine octave at 2× frequency, low weight, drifting slowly so the texture has some life.
    let drift = time * VORTEX_DRIFT;
    let mut u = x * 2.0 * s + drift;
    let mut v = y * 2.0 * s;
    if warp != 0.0 {
        // Quake-style shear: each axis offset by a sinusoid of the *other* (pre-warp)
        // axis, animated with time — the fold that keeps layers from staying parallel.
        let u0 = u;
        let phase = time * WARP_TIME_RATE;
        u += warp * (phase + v * WARP_FREQUENCY).sin();
        v += warp * (phase + u0 * WARP_FREQUENCY).sin();
    }
    let small = gradient_noise3(seed ^ TURBULENCE_SALT_1, u, v, time);
    big + detail * small
}

const BLOTCH_SALT_0: u64 = 0x51F0_9A2B_7D3E_C815;
const BLOTCH_SALT_1: u64 = 0xC4A7_1E86_33B9_5DF2;
/// Sharpening exponent for the reinjection blotch mask — the reference shader's
/// `pow(texture, 5.5)`, which turns smooth noise into sparse soft patches.
const BLOTCH_EXPONENT: f32 = 5.5;

/// Animated blotch mask in `[0, 1]` for patchy source reinjection — the analog of the
/// reference shader's `max(pow(maskA, 5.5), pow(maskB, 5.5))`: two gradient-noise layers
/// (one at `scale`, one at 2×) scrolling in different directions with `time`, each
/// sharpened to sparse soft patches, combined with `max`. Mostly near 0 with soft islands
/// near 1, so reinjection driven by it repaints the source in moving patches instead of
/// as a coherent full-frame layer.
pub fn reinjection_blotch_mask(seed: u64, x: f32, y: f32, time: f32, scale: f32) -> f32 {
    let coarse = gradient_noise3(
        seed ^ BLOTCH_SALT_0,
        x * scale + time * 0.35,
        y * scale + time * 0.5,
        0.0,
    );
    let fine = gradient_noise3(
        seed ^ BLOTCH_SALT_1,
        x * scale * 2.0 - time * 0.7,
        y * scale * 2.0 - time * 0.55,
        0.0,
    );
    let sharpen = |n: f32| ((0.5 + 0.5 * n).clamp(0.0, 1.0)).powf(BLOTCH_EXPONENT);
    sharpen(coarse).max(sharpen(fine))
}

/// 3D gradient (Perlin) noise on the splitmix lattice: hash the eight integer cell corners
/// into gradient directions, quintic-fade, trilinearly interpolate the corner dot
/// products. Output ~`[-1, 1]`, smooth (C2), so its curl gives clean round vortices.
fn gradient_noise3(seed: u64, x: f32, y: f32, z: f32) -> f32 {
    let xi = x.floor();
    let yi = y.floor();
    let zi = z.floor();
    let xf = x - xi;
    let yf = y - yi;
    let zf = z - zi;
    let ix = xi as i64 as u64;
    let iy = yi as i64 as u64;
    let iz = zi as i64 as u64;
    let u = fade(xf);
    let v = fade(yf);
    let w = fade(zf);

    let corner = |dx: u64, dy: u64, dz: u64, gx: f32, gy: f32, gz: f32| -> f32 {
        let h = hash_coords(
            seed,
            ix.wrapping_add(dx),
            iy.wrapping_add(dy),
            iz.wrapping_add(dz),
        );
        grad3(h, gx, gy, gz)
    };

    let x00 = lerp(
        corner(0, 0, 0, xf, yf, zf),
        corner(1, 0, 0, xf - 1.0, yf, zf),
        u,
    );
    let x10 = lerp(
        corner(0, 1, 0, xf, yf - 1.0, zf),
        corner(1, 1, 0, xf - 1.0, yf - 1.0, zf),
        u,
    );
    let x01 = lerp(
        corner(0, 0, 1, xf, yf, zf - 1.0),
        corner(1, 0, 1, xf - 1.0, yf, zf - 1.0),
        u,
    );
    let x11 = lerp(
        corner(0, 1, 1, xf, yf - 1.0, zf - 1.0),
        corner(1, 1, 1, xf - 1.0, yf - 1.0, zf - 1.0),
        u,
    );
    let y0 = lerp(x00, x10, v);
    let y1 = lerp(x01, x11, v);
    lerp(y0, y1, w)
}

/// Perlin quintic fade `6t^5 - 15t^4 + 10t^3` (C2-continuous interpolant).
fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Perlin's 12 edge-midpoint gradient directions, selected by the low bits of the hash.
fn grad3(hash: u64, x: f32, y: f32, z: f32) -> f32 {
    match hash & 15 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x + z,
        5 => -x + z,
        6 => x - z,
        7 => -x - z,
        8 => y + z,
        9 => -y + z,
        10 => y - z,
        11 => -y - z,
        12 => x + y,
        13 => -y + z,
        14 => -x + y,
        _ => -y - z,
    }
}

fn hash_coords(seed: u64, a: u64, b: u64, c: u64) -> u64 {
    hash_u64(
        seed ^ a.wrapping_mul(0x100_0000_01B3)
            ^ b.wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ c.wrapping_mul(0x59E3_9B1F_9A2D_7C4B),
    )
}

/// splitmix64 finalizer (matches `fluid_mosaic`/`coagulate`).
fn hash_u64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
