use crate::{backend::AudioAnalysisBackend, rms, AudioBufferF32, AudioDescriptorFrame, AudioError};

#[derive(Debug, Default, Clone, Copy)]
pub struct PortableBackend;

impl AudioAnalysisBackend for PortableBackend {
    fn name(&self) -> &'static str {
        "portable"
    }

    fn rms_envelope(
        &self,
        buffer: &AudioBufferF32,
        window_size: usize,
        hop_size: usize,
    ) -> Result<Vec<AudioDescriptorFrame>, AudioError> {
        rms::rms_envelope(buffer, window_size, hop_size)
    }
}
