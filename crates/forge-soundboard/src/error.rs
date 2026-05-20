use thiserror::Error;

#[derive(Debug, Error)]
pub enum SoundboardError {
    #[error("clip `{0}` not found")]
    ClipNotFound(String),

    #[error("clip file `{0}` is missing on disk")]
    ClipFileMissing(String),

    #[error("audio backend error: {0}")]
    Audio(#[from] forge_audio::AudioError),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("blocking task panicked: {0}")]
    JoinError(String),
}
