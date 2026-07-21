use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no default output device available")]
    NoDefaultDevice,

    #[error("cpal host error: {0}")]
    Host(String),

    #[error("resampling failed: {0}")]
    Resample(String),

    #[error("decode failed: {0}")]
    Decode(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("playback task failed: {0}")]
    JoinFailed(String),
}
