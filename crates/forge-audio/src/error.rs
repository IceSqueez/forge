use thiserror::Error;

use crate::remote::RemoteFailure;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no default output device available")]
    NoDefaultDevice,

    #[error("audio device error: {0}")]
    Host(String),

    #[error("audio device '{id}' not found")]
    DeviceNotFound { id: String },

    #[error("no matching audio input device found")]
    NoInputDevice,

    #[error("unsupported audio sample format: {0}")]
    UnsupportedSampleFormat(String),

    #[error("output device stopped during playback: {0}")]
    DeviceLost(String),

    #[error("resampling failed: {0}")]
    Resample(String),

    #[error("decode failed: {0}")]
    Decode(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("playback task failed: {0}")]
    JoinFailed(String),

    #[error("no audio route is configured")]
    NoRoute,

    #[error("unknown audio route: {0}")]
    UnknownRoute(String),

    #[error("every audio route failed: {0}")]
    AllRoutesFailed(String),

    #[error("remote audio destination failed: {0}")]
    RemoteDestination(RemoteFailure),

    #[error("no live page is listening on audio overlay '{destination}'")]
    NoRemoteListener { destination: String },
}
