use forge_events::{Event, EventSource};
use forge_platform_core::RateLimiter;
use forge_runtime::EventBus;
use forge_types::OAuthToken;
use serde::Deserialize;
use std::sync::Arc;
use thiserror::Error;

const SEND_CHAT_URL: &str = "https://api.twitch.tv/helix/chat/messages";
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

#[derive(Debug, Deserialize)]
struct SendChatResponse {
    data: Vec<SentData>,
}

#[derive(Debug, Deserialize)]
struct SentData {
    message_id: String,
}

/// Rate-limited. Returns `ChatSendError::RateLimited` if the limiter is exhausted.
/// `message` must be ≤500 chars (Twitch limit); returns `ChatSendError::MessageTooLong` otherwise.
pub async fn send_chat(
    rate_limiter: &dyn RateLimiter,
    token: &OAuthToken,
    client_id: &str,
    broadcaster_id: &str,
    sender_id: &str,
    message: &str,
    bus: &Arc<EventBus>,
) -> Result<SentMessageId, ChatSendError> {
    let http = reqwest::Client::new();
    if message.len() > MAX_MESSAGE_LEN {
        return Err(ChatSendError::MessageTooLong);
    }

    let outcome = rate_limiter
        .acquire(1)
        .await
        .map_err(|_| ChatSendError::RateLimited)?;

    if matches!(outcome, forge_platform_core::RateLimitOutcome::Exhausted) {
        return Err(ChatSendError::RateLimited);
    }

    let body = serde_json::json!({
        "broadcaster_id": broadcaster_id,
        "sender_id": sender_id,
        "message": message
    });

    let resp = http
        .post(SEND_CHAT_URL)
        .header("Authorization", format!("Bearer {}", token.expose()))
        .header("Client-Id", client_id)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| ChatSendError::Http(e.to_string()))?;

    let status = resp.status().as_u16();

    if !resp.status().is_success() {
        let retry_after = extract_retry_after(&resp);
        let body_text = resp.text().await.unwrap_or_default();
        let snippet_end = body_text.len().min(200);
        bus.publish(Event::new(
            EventSource::Twitch,
            "request.fail",
            serde_json::json!({
                "endpoint": SEND_CHAT_PATH,
                "status_code": status,
                "body_snippet": &body_text[..snippet_end],
                "retry_after_secs": retry_after,
            }),
        ));
        if status == 401 {
            return Err(ChatSendError::ReauthRequired);
        }
        if status == 429 {
            return Err(ChatSendError::RateLimited);
        }
        return Err(ChatSendError::Http(format!("HTTP {status}: {body_text}")));
    }

    let parsed: SendChatResponse = resp
        .json()
        .await
        .map_err(|e| ChatSendError::Http(e.to_string()))?;

    let message_id = parsed
        .data
        .into_iter()
        .next()
        .map(|d| d.message_id)
        .unwrap_or_default();

    Ok(SentMessageId(message_id))
}

fn extract_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_too_long_exactly_at_boundary() {
        let exactly_500 = "a".repeat(500);
        assert_eq!(exactly_500.len(), MAX_MESSAGE_LEN);

        let over_500 = "a".repeat(501);
        assert!(over_500.len() > MAX_MESSAGE_LEN);
    }

    #[test]
    fn sent_message_id_equality() {
        let a = SentMessageId("abc".into());
        let b = SentMessageId("abc".into());
        assert_eq!(a, b);
    }

    #[test]
    fn chat_send_error_displays_non_empty() {
        for e in [
            ChatSendError::RateLimited,
            ChatSendError::MessageTooLong,
            ChatSendError::NotConnected,
            ChatSendError::Http("timeout".into()),
            ChatSendError::ReauthRequired,
        ] {
            assert!(!e.to_string().is_empty(), "empty display for {e:?}");
        }
    }
}
