//! Complex forward/inverse STFT: analysis + weighted overlap-add synthesis over
//! the crate's pure-Rust radix-2 FFT ([`crate::fft::fft_in_place`]).
//!
//! Unlike [`crate::stft::stft_magnitude_cache`] (magnitude-only, no inverse —
//! kept untouched, sidecars depend on its exact behavior), this module
//! preserves phase and supports resynthesis: the prerequisite for phase-vocoder
//! cross-synthesis (`docs/PHASE_VOCODER_MILESTONE.md`).
//!
//! - **Analysis:** each frame is windowed (the existing [`WindowFunction`] set)
//!   then transformed with the f64 radix-2 FFT. `fft_size` must be a non-zero
//!   power of two (inherited from [`StftConfig::validate`]); `hop_size` must be
//!   `<= fft_size / 2` — the weighted overlap-add below needs that much overlap
//!   for a well-conditioned per-sample normalizer. Frames start at every hop
//!   position `< sample_count` (zero-padding the tail as needed), so even inputs
//!   shorter than one FFT frame round-trip.
//! - **Synthesis:** inverse FFT per frame, windowed again with the same `w`,
//!   accumulated (weighted overlap-add) and normalized by the per-sample
//!   `Σ w²`, clamped away from zero to guard the near-silent normalizer at the
//!   very edges (a lone tapering window's first/last samples). Output is
//!   truncated to the caller-supplied length.

use crate::fft::fft_in_place;
use crate::stft::window_value;
use crate::{AudioError, StftConfig};

/// Floor for the per-sample `Σ w²` weighted-OLA normalizer; positions where the
/// accumulated window energy falls below this are treated as unrecoverable edge
/// samples (the raw accumulated numerator is already ~0 there too).
const NORMALIZER_FLOOR: f64 = 1e-9;

/// One analysis frame: the full `fft_size`-length complex spectrum (both
/// positive- and negative-frequency bins — needed for an exact inverse FFT).
#[derive(Debug, Clone)]
pub(crate) struct ComplexStftFrame {
    pub time_seconds: f64,
    pub real: Vec<f64>,
    pub imag: Vec<f64>,
}

/// Validate a config for complex-STFT use: the general [`StftConfig::validate`]
/// checks plus `hop_size <= fft_size / 2`.
pub(crate) fn validate_complex_stft_config(config: &StftConfig) -> Result<(), AudioError> {
    config.validate()?;
    if config.hop_size > config.fft_size / 2 {
        return Err(AudioError::InvalidSettings(format!(
            "hop_size ({}) must be <= fft_size / 2 ({}) for complex STFT weighted overlap-add",
            config.hop_size,
            config.fft_size / 2
        )));
    }
    Ok(())
}

/// Analyze a mono `f64` signal into windowed complex STFT frames. Frame `k`
/// starts at sample `k * hop_size`; the last frame is zero-padded past
/// `samples.len()`. `sample_rate` is only used to stamp `time_seconds`.
pub(crate) fn stft_complex_mono(
    samples: &[f64],
    config: StftConfig,
    sample_rate: u32,
) -> Result<Vec<ComplexStftFrame>, AudioError> {
    validate_complex_stft_config(&config)?;
    let n = samples.len();
    let mut frames = Vec::new();
    let mut start = 0_usize;
    while start < n {
        let mut re = vec![0.0_f64; config.fft_size];
        let mut im = vec![0.0_f64; config.fft_size];
        for (offset, slot) in re.iter_mut().enumerate() {
            let index = start + offset;
            let sample = if index < n { samples[index] } else { 0.0 };
            *slot = sample * window_value(offset, config.fft_size, config.window);
        }
        fft_in_place(&mut re, &mut im, false);
        frames.push(ComplexStftFrame {
            time_seconds: start as f64 / sample_rate as f64,
            real: re,
            imag: im,
        });
        start += config.hop_size;
    }
    Ok(frames)
}

/// Resynthesize `frames` (analyzed with `config`, frame `k` at `k * hop_size`)
/// back to a mono `f64` signal of exactly `output_len` samples via windowed
/// weighted overlap-add, normalized by the per-sample `Σ w²`.
pub(crate) fn istft_complex_mono(
    frames: &[ComplexStftFrame],
    config: StftConfig,
    output_len: usize,
) -> Result<Vec<f64>, AudioError> {
    validate_complex_stft_config(&config)?;
    let mut output = vec![0.0_f64; output_len];
    let mut normalizer = vec![0.0_f64; output_len];

    for (frame_index, frame) in frames.iter().enumerate() {
        let start = frame_index * config.hop_size;
        let mut re = frame.real.clone();
        let mut im = frame.imag.clone();
        fft_in_place(&mut re, &mut im, true);
        for (offset, &value) in re.iter().enumerate() {
            let pos = start + offset;
            if pos >= output_len {
                break;
            }
            let w = window_value(offset, config.fft_size, config.window);
            output[pos] += value * w;
            normalizer[pos] += w * w;
        }
    }

    for (sample, norm) in output.iter_mut().zip(normalizer.iter()) {
        *sample /= norm.max(NORMALIZER_FLOOR);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stft::WindowFunction;

    fn round_trip(samples: &[f64], config: StftConfig) -> Vec<f64> {
        let frames = stft_complex_mono(samples, config, 48_000).expect("stft");
        istft_complex_mono(&frames, config, samples.len()).expect("istft")
    }

    /// Interior samples (away from the first/last window's taper) must round
    /// trip within 1e-5 max-abs.
    fn assert_interior_round_trip(samples: &[f64], config: StftConfig, margin: usize) {
        let recovered = round_trip(samples, config);
        assert_eq!(recovered.len(), samples.len());
        let lo = margin.min(samples.len());
        let hi = samples.len().saturating_sub(margin);
        let mut max_abs_err = 0.0_f64;
        for i in lo..hi {
            max_abs_err = max_abs_err.max((recovered[i] - samples[i]).abs());
        }
        assert!(
            max_abs_err <= 1e-5,
            "interior round-trip max-abs error {max_abs_err} exceeds 1e-5"
        );
    }

    fn synthetic_signal(len: usize) -> Vec<f64> {
        (0..len)
            .map(|i| {
                let t = i as f64;
                0.6 * (0.05 * t).sin() + 0.3 * (0.13 * t + 0.4).sin()
            })
            .collect()
    }

    #[test]
    fn round_trip_hop_half_fft_size() {
        let config = StftConfig {
            fft_size: 64,
            hop_size: 32,
            window: WindowFunction::Hann,
        };
        assert_interior_round_trip(&synthetic_signal(512), config, 64);
    }

    #[test]
    fn round_trip_hop_quarter_fft_size() {
        let config = StftConfig {
            fft_size: 64,
            hop_size: 16,
            window: WindowFunction::Hann,
        };
        assert_interior_round_trip(&synthetic_signal(512), config, 64);
    }

    #[test]
    fn round_trip_survives_non_power_of_two_length_input() {
        // 517 is not a power of two and not a multiple of fft_size or hop_size,
        // exercising the zero-padded tail frame.
        let config = StftConfig {
            fft_size: 64,
            hop_size: 32,
            window: WindowFunction::Hann,
        };
        assert_interior_round_trip(&synthetic_signal(517), config, 64);
    }

    #[test]
    fn stft_rejects_hop_larger_than_half_fft_size() {
        let config = StftConfig {
            fft_size: 64,
            hop_size: 33,
            window: WindowFunction::Hann,
        };
        assert!(stft_complex_mono(&[0.0; 128], config, 48_000).is_err());
    }

    #[test]
    fn empty_input_round_trips_to_empty_output() {
        let config = StftConfig {
            fft_size: 32,
            hop_size: 16,
            window: WindowFunction::Hann,
        };
        let frames = stft_complex_mono(&[], config, 48_000).expect("stft");
        assert!(frames.is_empty());
        let out = istft_complex_mono(&frames, config, 0).expect("istft");
        assert!(out.is_empty());
    }
}
