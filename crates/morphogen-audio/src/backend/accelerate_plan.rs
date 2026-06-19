//! Future Accelerate/vDSP backend plan.
//!
//! The portable backend defines behavior first. Accelerate can later replace
//! hot paths for FFT, STFT, convolution, spectral centroid, onset detection,
//! and phase-vocoder or cross-synthesis processing on Apple Silicon.

pub const BACKEND_NAME: &str = "accelerate-vdsp-planned";
