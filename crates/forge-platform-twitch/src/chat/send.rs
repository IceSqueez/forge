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
    // Twitch limit is by character count, not bytes; multibyte chars (e.g. Cyrillic) must pass.
    if message.chars().count() > MAX_MESSAGE_LEN {
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
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockTransport {
        calls: Mutex<Vec<HelixRequest>>,
        response: Mutex<Option<Result<serde_json::Value, HelixError>>>,
    }

    impl MockTransport {
        fn returning(response: Result<serde_json::Value, HelixError>) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                response: Mutex::new(Some(response)),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }

        fn last_request(&self) -> HelixRequest {
            self.calls.lock().unwrap().last().unwrap().clone()
        }
    }

    #[async_trait]
    impl HelixTransport for MockTransport {
        async fn execute(&self, request: HelixRequest) -> Result<serde_json::Value, HelixError> {
            self.calls.lock().unwrap().push(request);
            self.response
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Ok(serde_json::Value::Null))
        }
    }

    fn sent_fixture() -> serde_json::Value {
        serde_json::json!({"data": [{"message_id": "abc-123"}, {"message_id": "later"}]})
    }

    #[tokio::test]
    async fn send_chat_extracts_message_id_from_first_data_entry() {
        let transport = MockTransport::returning(Ok(sent_fixture()));

        let id = send_chat(&transport, "100", "100", "hello").await.unwrap();

        assert_eq!(id, SentMessageId("abc-123".to_owned()));
    }

    #[tokio::test]
    async fn send_chat_posts_ids_and_message_to_helix_chat_endpoint() {
        let transport = MockTransport::returning(Ok(sent_fixture()));

        send_chat(&transport, "100", "200", "hello").await.unwrap();

        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/chat/messages");
        let body = request.body.unwrap();
        assert_eq!(body["broadcaster_id"], "100");
        assert_eq!(body["sender_id"], "200");
        assert_eq!(body["message"], "hello");
    }

    #[tokio::test]
    async fn message_over_limit_is_rejected_without_invoking_transport() {
        let transport = MockTransport::returning(Ok(sent_fixture()));
        let message = "a".repeat(501);

        let err = send_chat(&transport, "100", "100", &message)
            .await
            .unwrap_err();

        assert!(matches!(err, ChatSendError::MessageTooLong));
        assert_eq!(
            transport.call_count(),
            0,
            "pre-check must not consume a transport call"
        );
    }

    #[tokio::test]
    async fn message_at_exactly_the_limit_is_sent() {
        let transport = MockTransport::returning(Ok(sent_fixture()));
        let message = "a".repeat(500);

        let result = send_chat(&transport, "100", "100", &message).await;

        assert!(result.is_ok());
    }

    type ErrorExpectation = fn(&ChatSendError) -> bool;

    #[tokio::test]
    async fn helix_failures_map_to_matching_chat_send_errors() {
        let cases: Vec<(HelixError, ErrorExpectation)> = vec![
            (HelixError::ReauthRequired, |e| {
                matches!(e, ChatSendError::ReauthRequired)
            }),
            (HelixError::RateLimited, |e| {
                matches!(e, ChatSendError::RateLimited)
            }),
            (
                HelixError::Http {
                    status: 403,
                    body: "denied".to_owned(),
                },
                |e| matches!(e, ChatSendError::Http(msg) if msg.contains("403")),
            ),
        ];

        for (input, is_expected) in cases {
            let label = format!("{input:?}");
            let transport = MockTransport::returning(Err(input));
            let err = send_chat(&transport, "100", "100", "hi").await.unwrap_err();
            assert!(is_expected(&err), "{label} mapped to unexpected {err:?}");
        }
    }

    #[tokio::test]
    async fn malformed_success_payload_maps_to_http_error() {
        let transport = MockTransport::returning(Ok(serde_json::json!({"unexpected": true})));

        let err = send_chat(&transport, "100", "100", "hi").await.unwrap_err();

        assert!(matches!(err, ChatSendError::Http(_)));
    }
}
