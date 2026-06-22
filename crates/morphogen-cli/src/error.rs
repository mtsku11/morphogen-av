use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum CliError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Core(#[from] morphogen_core::CoreError),
    #[error(transparent)]
    Media(#[from] morphogen_media::MediaError),
    #[error(transparent)]
    Audio(#[from] morphogen_audio::AudioError),
    #[error(transparent)]
    Render(#[from] morphogen_render::RenderError),
    #[cfg(target_os = "macos")]
    #[error(transparent)]
    Metal(#[from] morphogen_metal::MetalDispatchError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}
