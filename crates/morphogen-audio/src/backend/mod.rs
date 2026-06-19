pub mod accelerate_plan;
pub mod portable;

use crate::{AudioBufferF32, AudioDescriptorFrame, AudioError};

pub trait AudioAnalysisBackend {
    fn name(&self) -> &'static str;

    fn rms_envelope(
        &self,
        buffer: &AudioBufferF32,
        window_size: usize,
        hop_size: usize,
    ) -> Result<Vec<AudioDescriptorFrame>, AudioError>;
}
