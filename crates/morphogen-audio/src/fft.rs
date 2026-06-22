//! Deterministic pure-Rust radix-2 FFT (forward + inverse) and FFT-based linear
//! convolution.
//!
//! The crate's STFT is a naive magnitude-only DFT with no inverse, so the HQ
//! tier of audio-impulse convolution (`docs/CONVOLUTIONAL_BLEND_MILESTONE.md`)
//! needs a real invertible transform. This is an iterative Cooley-Tukey FFT over
//! `f64` (so it stays a faithful reference for the `f32` direct path it is gated
//! against): identical inputs ⇒ identical output, no platform-dependent
//! intrinsics. `convolve_via_fft` produces the same full linear convolution as
//! `convolution::convolve_mono`, up to floating-point rounding.

use crate::AudioError;

/// In-place iterative radix-2 Cooley-Tukey FFT over split real/imag buffers.
///
/// `re.len()` must equal `im.len()` and be a power of two. `inverse` runs the
/// inverse transform (conjugate twiddles + `1/n` scaling).
fn fft_in_place(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    debug_assert_eq!(n, im.len());
    if n <= 1 {
        return;
    }

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterfly stages.
    let mut len = 2usize;
    while len <= n {
        let angle = if inverse { 2.0 } else { -2.0 } * std::f64::consts::PI / len as f64;
        let (wr_step, wi_step) = (angle.cos(), angle.sin());
        let half = len / 2;
        let mut block = 0usize;
        while block < n {
            let (mut wr, mut wi) = (1.0_f64, 0.0_f64);
            for k in 0..half {
                let a = block + k;
                let b = block + k + half;
                let tr = wr * re[b] - wi * im[b];
                let ti = wr * im[b] + wi * re[b];
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let next_wr = wr * wr_step - wi * wi_step;
                wi = wr * wi_step + wi * wr_step;
                wr = next_wr;
            }
            block += len;
        }
        len <<= 1;
    }

    if inverse {
        let scale = 1.0 / n as f64;
        for value in re.iter_mut() {
            *value *= scale;
        }
        for value in im.iter_mut() {
            *value *= scale;
        }
    }
}

/// Full linear convolution of `input` with `impulse`, computed in the frequency
/// domain. Output length is `input.len() + impulse.len() - 1` — identical to
/// [`crate::convolve_mono`] up to floating-point rounding.
pub fn convolve_via_fft(input: &[f32], impulse: &[f32]) -> Result<Vec<f32>, AudioError> {
    if impulse.is_empty() {
        return Err(AudioError::InvalidSettings(
            "impulse response must contain at least one sample".to_string(),
        ));
    }
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let out_len = input
        .len()
        .checked_add(impulse.len())
        .and_then(|len| len.checked_sub(1))
        .ok_or_else(|| {
            AudioError::InvalidSettings("convolution output is too large".to_string())
        })?;
    // Zero-pad to a power of two ≥ out_len so the circular FFT product equals the
    // linear convolution (no time-domain wraparound).
    let n = out_len
        .checked_next_power_of_two()
        .ok_or_else(|| AudioError::InvalidSettings("convolution size overflow".to_string()))?;

    let mut ar = vec![0.0_f64; n];
    let mut ai = vec![0.0_f64; n];
    let mut br = vec![0.0_f64; n];
    let mut bi = vec![0.0_f64; n];
    for (slot, &sample) in ar.iter_mut().zip(input) {
        *slot = sample as f64;
    }
    for (slot, &sample) in br.iter_mut().zip(impulse) {
        *slot = sample as f64;
    }

    fft_in_place(&mut ar, &mut ai, false);
    fft_in_place(&mut br, &mut bi, false);
    for i in 0..n {
        let real = ar[i] * br[i] - ai[i] * bi[i];
        let imag = ar[i] * bi[i] + ai[i] * br[i];
        ar[i] = real;
        ai[i] = imag;
    }
    fft_in_place(&mut ar, &mut ai, true);

    Ok(ar[..out_len].iter().map(|&value| value as f32).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convolve_mono;

    #[test]
    fn fft_round_trip_recovers_input() {
        let mut re = vec![1.0_f64, -2.0, 3.0, 0.5, -1.0, 2.0, 0.0, 4.0];
        let mut im = vec![0.0_f64; re.len()];
        let original = re.clone();
        fft_in_place(&mut re, &mut im, false);
        fft_in_place(&mut re, &mut im, true);
        for (got, want) in re.iter().zip(&original) {
            assert!((got - want).abs() < 1e-9, "got {got}, want {want}");
        }
        for value in im {
            assert!(value.abs() < 1e-9, "imag residue {value}");
        }
    }

    #[test]
    fn fft_convolution_matches_direct_tiny_fixture() {
        let out = convolve_via_fft(&[1.0, 2.0], &[0.5, 0.5]).expect("fft convolve");
        let expected = [0.5_f32, 1.5, 1.0];
        assert_eq!(out.len(), expected.len());
        for (got, want) in out.iter().zip(&expected) {
            assert!((got - want).abs() < 1e-5, "got {got}, want {want}");
        }
    }

    #[test]
    fn fft_convolution_matches_direct_on_longer_signal() {
        // A deterministic pseudo-signal and impulse where the lengths do not sum
        // to a power of two (exercises the zero-padding path).
        let input: Vec<f32> = (0..37).map(|i| ((i * 7 % 11) as f32 / 5.0) - 1.0).collect();
        let impulse: Vec<f32> = (0..13).map(|i| ((i * 3 % 5) as f32 / 4.0) - 0.5).collect();
        let direct = convolve_mono(&input, &impulse).expect("direct");
        let fft = convolve_via_fft(&input, &impulse).expect("fft");
        assert_eq!(direct.len(), fft.len());
        for (d, f) in direct.iter().zip(&fft) {
            assert!((d - f).abs() < 1e-4, "fft {f} vs direct {d}");
        }
    }

    #[test]
    fn fft_convolution_rejects_empty_impulse_and_passes_empty_input() {
        assert!(convolve_via_fft(&[1.0, 2.0], &[]).is_err());
        assert_eq!(
            convolve_via_fft(&[], &[1.0]).expect("empty input"),
            Vec::<f32>::new()
        );
    }
}
