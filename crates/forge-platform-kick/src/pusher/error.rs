use thiserror::Error;

#[derive(Debug, Error)]
pub enum KickError {
    #[error("network error: {reason}")]
    Network { reason: String },

    #[error("HTTP {status} ({})", forge_net::status_class(*status))]
    Http { status: u16, body: String },

    #[error("channel info unavailable for slug '{slug}': {reason}")]
    ChannelInfoUnavailable { slug: String, reason: String },

    #[error("chatroom_id missing in channel response for slug '{slug}'")]
    ChatroomIdNotFound { slug: String },

    #[error("WebSocket error: {reason}")]
    WebSocket { reason: String },

    #[error("chat connect cancelled")]
    ConnectCancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_error_display_names_status_and_class_and_never_the_body() {
        let e = KickError::Http {
            status: 403,
            body: "<html>secret-slug</html>".into(),
        };
        assert_eq!(e.to_string(), "HTTP 403 (client error)");
    }
}
