use thiserror::Error;

use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};
use crate::sub_actions::user_target::UserTarget;

const WHISPERS_PATH: &str = "/helix/whispers";
const MAX_WHISPER_CHARS: usize = 500;

#[derive(Debug, Error)]
pub(crate) enum WhisperError {
    #[error("whisper recipient is empty")]
    EmptyRecipient,
    #[error("whisper message is empty")]
    EmptyMessage,
    #[error("whisper message exceeds {MAX_WHISPER_CHARS}-character limit")]
    MessageTooLong,
    #[error(transparent)]
    Helix(#[from] HelixError),
}

pub(crate) async fn send_whisper(
    transport: &dyn HelixTransport,
    from_user_id: &str,
    to_user_login: &str,
    message: &str,
) -> Result<(), WhisperError> {
    let recipient = (!to_user_login.is_empty()).then_some(UserTarget::Login(to_user_login));
    send_whisper_to(transport, from_user_id, recipient, message).await
}

pub(crate) async fn send_whisper_to(
    transport: &dyn HelixTransport,
    from_user_id: &str,
    recipient: Option<UserTarget<'_>>,
    message: &str,
) -> Result<(), WhisperError> {
    let Some(recipient) = recipient else {
        return Err(WhisperError::EmptyRecipient);
    };
    if message.is_empty() {
        return Err(WhisperError::EmptyMessage);
    }
    if message.chars().count() > MAX_WHISPER_CHARS {
        return Err(WhisperError::MessageTooLong);
    }
    let to_user_id = recipient.user_id(transport).await?;
    let request = HelixRequest::new(HelixMethod::Post, WHISPERS_PATH)
        .query("from_user_id", from_user_id)
        .query("to_user_id", to_user_id)
        .body(serde_json::json!({ "message": message }));
    transport.execute(request).await?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{MockTransport, users_fixture};

    #[tokio::test]
    async fn whisper_resolves_the_login_then_posts_to_the_resolved_user_id() {
        let transport = MockTransport::returning_sequence(vec![
            users_fixture("555"),
            Ok(serde_json::Value::Null),
        ]);

        send_whisper(&transport, "100", "viewer", "psst")
            .await
            .unwrap();

        assert_eq!(transport.call_count(), 2);
        assert!(
            transport
                .request(0)
                .query
                .contains(&("login".to_owned(), "viewer".to_owned())),
            "first call must resolve the recipient login"
        );
        let whisper = transport.last_request();
        assert_eq!(whisper.method, HelixMethod::Post);
        assert_eq!(whisper.path, WHISPERS_PATH);
        assert!(
            whisper
                .query
                .contains(&("from_user_id".to_owned(), "100".to_owned()))
        );
        assert!(
            whisper
                .query
                .contains(&("to_user_id".to_owned(), "555".to_owned())),
            "to_user_id must be the resolved id, not the login"
        );
        assert_eq!(whisper.body, Some(serde_json::json!({ "message": "psst" })));
    }

    type WhisperExpectation = fn(&WhisperError) -> bool;

    #[tokio::test]
    async fn invalid_whisper_is_rejected_without_any_helix_call() {
        let cases: [(&str, String, WhisperExpectation); 3] = [
            ("", "hello".to_owned(), |e| {
                matches!(e, WhisperError::EmptyRecipient)
            }),
            ("viewer", String::new(), |e| {
                matches!(e, WhisperError::EmptyMessage)
            }),
            ("viewer", "я".repeat(MAX_WHISPER_CHARS + 1), |e| {
                matches!(e, WhisperError::MessageTooLong)
            }),
        ];
        for (login, message, is_expected) in cases {
            let transport = MockTransport::returning(users_fixture("555"));

            let err = send_whisper(&transport, "100", login, &message)
                .await
                .unwrap_err();

            assert!(is_expected(&err), "login {login:?}: got {err:?}");
            assert_eq!(transport.call_count(), 0, "login {login:?}");
        }
    }

    #[tokio::test]
    async fn whisper_at_the_character_limit_is_sent() {
        let transport = MockTransport::returning_sequence(vec![
            users_fixture("555"),
            Ok(serde_json::Value::Null),
        ]);

        let result =
            send_whisper(&transport, "100", "viewer", &"я".repeat(MAX_WHISPER_CHARS)).await;

        assert!(result.is_ok(), "got {result:?}");
    }

    #[tokio::test]
    async fn unknown_recipient_fails_before_posting_a_whisper() {
        let transport = MockTransport::returning(Ok(serde_json::json!({ "data": [] })));

        let err = send_whisper(&transport, "100", "ghost", "hello")
            .await
            .unwrap_err();

        assert!(matches!(err, WhisperError::Helix(_)), "got {err:?}");
        assert_eq!(transport.call_count(), 1, "only the lookup may run");
    }
}
