use thiserror::Error;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("invalid image buffer: {0}")]
    InvalidImageBuffer(String),
    #[error("invalid flow field: {0}")]
    InvalidFlowField(String),
    #[error("invalid flow cache: {0}")]
    InvalidFlowCache(String),
    #[error("invalid flow feedback settings: {0}")]
    InvalidFlowFeedbackSettings(String),
    #[error("invalid flow feedback state: {0}")]
    InvalidFlowFeedbackState(String),
    #[error("invalid granular mosaic settings: {0}")]
    InvalidGranularMosaicSettings(String),
    #[error("invalid granular mosaic cache: {0}")]
    InvalidGranularMosaicCache(String),
    #[error("invalid video vocoder settings: {0}")]
    InvalidVideoVocoderSettings(String),
    #[error("invalid convolution blend settings: {0}")]
    InvalidConvolutionSettings(String),
    #[error("render inputs are incompatible: {0}")]
    IncompatibleInputs(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
