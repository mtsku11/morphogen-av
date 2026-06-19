use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("invalid image buffer: {0}")]
    InvalidImageBuffer(String),
    #[error("invalid flow field: {0}")]
    InvalidFlowField(String),
    #[error("invalid flow cache: {0}")]
    InvalidFlowCache(String),
    #[error("render inputs are incompatible: {0}")]
    IncompatibleInputs(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
