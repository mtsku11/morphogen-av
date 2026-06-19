use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("project validation failed: {0}")]
    InvalidProject(String),
    #[error("timeline validation failed: {0}")]
    InvalidTimeline(String),
    #[error("render queue error: {0}")]
    InvalidRenderQueue(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
