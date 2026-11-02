use forge_net::{GatewayBuildError, GatewayFailure};

#[derive(Debug, thiserror::Error)]
pub enum EmotesError {
    #[error("emote provider client unavailable: {0}")]
    ClientUnavailable(#[from] GatewayBuildError),
    #[error("emote provider unreachable: {0}")]
    Unreachable(#[from] GatewayFailure),
    #[error("emote provider answer unreadable")]
    Unreadable,
}
