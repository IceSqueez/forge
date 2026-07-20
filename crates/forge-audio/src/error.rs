use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("output device `{0}` not found")]
    DeviceNotFound(String),

    #[error("no default output device available")]
    NoDefaultDevice,

    #[error("cpal host error: {0}")]
    Host(String),

    #[error("cpal stream error: {0}")]
    Stream(String),

    #[error("unsupported source format: {0}")]
    UnsupportedFormat(String),

    #[error("resampling failed: {0}")]
    Resample(String),

    #[error("decode failed: {0}")]
    Decode(String),

    #[error("playback aborted: {0}")]
    Aborted(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("playback task failed: {0}")]
    JoinFailed(String),
}
