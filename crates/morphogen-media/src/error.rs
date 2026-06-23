use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("required external binary '{binary}' was not found on PATH")]
    MissingBinary { binary: String },
    #[error("external command '{binary}' failed with status {code:?}: {stderr}")]
    CommandFailed {
        binary: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("malformed AVI bitstream: {0}")]
    MalformedAvi(String),
    #[error("invalid datamosh request: {0}")]
    InvalidRequest(String),
}
