use forge_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SoundboardError {
    #[error("clip `{0}` not found")]
    ClipNotFound(String),

    #[error("source file for `{0}` is missing")]
    SourceMissing(String),

    #[error("import refused: {0}")]
    ImportRefused(StorageError),

    #[error("audio backend error: {0}")]
    Audio(#[from] forge_audio::AudioError),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("blocking task panicked: {0}")]
    JoinError(String),
}
