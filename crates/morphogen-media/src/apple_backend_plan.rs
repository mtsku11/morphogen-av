//! Mac-native media backend plan.
//!
//! FFmpeg is an optional external helper for early CLI workflows. The production
//! Mac path should use AVFoundation for asset inspection and decode
//! coordination, CoreMedia for timing, CoreVideo for pixel buffers, and
//! VideoToolbox for hardware decode/encode and eventual ProRes export.

pub const SUMMARY: &str = "AVFoundation + CoreMedia + CoreVideo + VideoToolbox are the intended Mac media backend layers.";
