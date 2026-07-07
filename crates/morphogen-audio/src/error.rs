use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("invalid audio buffer: {0}")]
    InvalidBuffer(String),
    #[error("invalid analysis settings: {0}")]
    InvalidSettings(String),
    #[error("WAV error: {0}")]
    Wav(#[from] hound::Error),
    #[error("MIDI file error: {0}")]
    Midi(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
