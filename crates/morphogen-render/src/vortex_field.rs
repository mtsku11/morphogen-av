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
    const E: f32 = 1.0;
    let psi_yp = streamfunction(seed, x, y + E, time, scale, detail);
    let psi_ym = streamfunction(seed, x, y - E, time, scale, detail);
    let psi_xp = streamfunction(seed, x + E, y, time, scale, detail);
    let psi_xm = streamfunction(seed, x - E, y, time, scale, detail);
    let dpsi_dy = (psi_yp - psi_ym) / (2.0 * E);
    let dpsi_dx = (psi_xp - psi_xm) / (2.0 * E);
    let inv = if scale != 0.0 { 1.0 / scale } else { 0.0 };
    (dpsi_dy * inv, -dpsi_dx * inv)
}

/// The streamfunction `ψ`: a steady low-frequency octave (the persistent large vortices)
/// plus a `detail`-weighted octave at 2× frequency drifting slowly with `time`.
fn streamfunction(seed: u64, x: f32, y: f32, time: f32, scale: f32, detail: f32) -> f32 {
    let s = scale;
    // Big, low-frequency, STEADY octave — the persistent large vortices.
    let big = gradient_noise3(seed ^ TURBULENCE_SALT_0, x * s, y * s, BIG_VORTEX_PLANE);
    // Fine octave at 2× frequency, low weight, drifting slowly so the texture has some life.
    let drift = time * VORTEX_DRIFT;
    let small = gradient_noise3(
        seed ^ TURBULENCE_SALT_1,
        x * 2.0 * s + drift,
        y * 2.0 * s,
        time,
    );
    big + detail * small
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
