use thiserror::Error;

#[derive(Debug, Error)]
pub enum KickError {
    #[error("network error: {reason}")]
    Network { reason: String },

    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    #[error("channel info unavailable for slug '{slug}': {reason}")]
    ChannelInfoUnavailable { slug: String, reason: String },

    #[error("chatroom_id missing in channel response for slug '{slug}'")]
    ChatroomIdNotFound { slug: String },

    #[error("WebSocket error: {reason}")]
    WebSocket { reason: String },
}
