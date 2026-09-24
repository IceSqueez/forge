use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no default output device available")]
    NoDefaultDevice,

    #[error("cpal host error: {0}")]
    Host(String),

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
    RemoteDestination(String),

    #[error("no live page is listening on audio overlay '{destination}' ({live_players} live)")]
    NoRemoteListener {
        destination: String,
        live_players: usize,
    },
}
