use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};
use serde::Deserialize;
use thiserror::Error;

const SEND_CHAT_PATH: &str = "/helix/chat/messages";
const MAX_MESSAGE_LEN: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMessageId(pub String);

#[derive(Debug, Error)]
pub enum ChatSendError {
    #[error("rate limited")]
    RateLimited,
    #[error("message exceeds 500-character limit")]
    MessageTooLong,
    #[error("not connected")]
    NotConnected,
    #[error("send failed: {0}")]
    Http(String),
    #[error("reauth required")]
    ReauthRequired,
}

impl From<HelixError> for ChatSendError {
    fn from(err: HelixError) -> Self {
        match err {
            HelixError::RateLimited => Self::RateLimited,
            HelixError::ReauthRequired => Self::ReauthRequired,
            HelixError::Http { status, body } => Self::Http(format!("HTTP {status}: {body}")),
            HelixError::Credentials(msg) | HelixError::Transport(msg) => Self::Http(msg),
        }
    }
}

#[derive(Debug, Deserialize)]
struct SendChatResponse {
    data: Vec<SentData>,
}

#[derive(Debug, Deserialize)]
struct SentData {
    message_id: String,
}

/// `message` must be ≤500 chars (Twitch limit); returns `ChatSendError::MessageTooLong`
/// otherwise, without consuming a rate-limit token.
pub async fn send_chat(
    transport: &dyn HelixTransport,
    broadcaster_id: &str,
    sender_id: &str,
    message: &str,
) -> Result<SentMessageId, ChatSendError> {
    if message.len() > MAX_MESSAGE_LEN {
        return Err(ChatSendError::MessageTooLong);
    }

    let body = serde_json::json!({
        "broadcaster_id": broadcaster_id,
        "sender_id": sender_id,
        "message": message
    });

    let response = transport
        .execute(HelixRequest::new(HelixMethod::Post, SEND_CHAT_PATH).body(body))
        .await?;

    let parsed: SendChatResponse =
        serde_json::from_value(response).map_err(|e| ChatSendError::Http(e.to_string()))?;

    let message_id = parsed
        .data
        .into_iter()
        .next()
        .map(|d| d.message_id)
        .unwrap_or_default();

    Ok(SentMessageId(message_id))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {

    #[tokio::test]
    async fn send_network_error_strips_url() {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(1))
            .build()
            .unwrap();
        let err = client
            .post("https://192.0.2.1/helix/chat/messages")
            .send()
            .await
            .unwrap_err();
        assert!(!err.without_url().to_string().contains("192.0.2.1"));
    }
}
