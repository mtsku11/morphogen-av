#![forbid(unsafe_code)]

pub mod backend;
pub mod buffer;
pub mod convolution;
pub mod cross_synth;
pub mod descriptors;
pub mod error;
pub mod fft;
pub mod onset;
pub mod rms;
pub mod spectral;
pub mod stft;
pub mod video_route;
pub mod wav;

pub use buffer::AudioBufferF32;
pub use convolution::{
    convolve_mono, impulse_convolution_blend, ConvolutionMethod, IrMode,
    IMPULSE_CONVOLUTION_BLEND_ALGORITHM, PER_CHANNEL_IMPULSE_CONVOLUTION_BLEND_ALGORITHM,
};
pub use cross_synth::{
    centroid_filter_cross_synth, rms_gain_cross_synth, FilterType,
    CENTROID_FILTER_CROSS_SYNTH_ALGORITHM, RMS_GAIN_CROSS_SYNTH_ALGORITHM,
};
pub use descriptors::{AudioAnalysisCache, AudioDescriptorFrame};
pub use error::AudioError;
pub use fft::convolve_via_fft;
pub use onset::{onset_strength_from_stft, OnsetStrengthCache, OnsetStrengthFrame};
pub use rms::rms_envelope;
pub use spectral::{spectral_centroid, spectral_centroid_from_magnitudes};
pub use stft::{stft_magnitude_cache, StftAnalysisCache, StftConfig, StftFrame, WindowFunction};
pub use video_route::{descriptor_gain_route, descriptor_pan_route};
pub use wav::{load_wav_f32, save_wav_f32};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_envelope_calculates_stereo_signal() {
        let buffer = AudioBufferF32::new(2, 48_000, vec![1.0, -1.0, 1.0, -1.0])
            .expect("valid stereo buffer");
        let frames = rms_envelope(&buffer, 2, 2).expect("calculate RMS");

        assert_eq!(frames.len(), 1);
        assert!((frames[0].rms - 1.0).abs() < 0.000_001);
    }

    #[test]
    fn spectral_centroid_detects_nyquist_energy() {
        let centroid =
            spectral_centroid(&[1.0, -1.0, 1.0, -1.0], 4).expect("calculate spectral centroid");

        assert!((centroid - 2.0).abs() < 0.000_001);
    }

    #[test]
    fn spectral_centroid_of_silence_is_zero() {
        let centroid =
            spectral_centroid(&[0.0, 0.0, 0.0, 0.0], 48_000).expect("calculate spectral centroid");

        assert_eq!(centroid, 0.0);
    }
}
