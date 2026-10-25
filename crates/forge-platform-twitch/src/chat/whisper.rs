use thiserror::Error;

use super::send::refusal_text;

use crate::helix::{HelixError, HelixMethod, HelixRequest, HelixTransport};
use crate::sub_actions::user_target::UserTarget;
use forge_net::status::BAD_REQUEST;

const WHISPERS_PATH: &str = "/helix/whispers";
const MAX_WHISPER_CHARS: usize = 500;
const WHISPER_BLOCK_MARKERS: [&str; 3] = ["stranger", "block", "doesn't allow"];

#[derive(Debug, Error)]
pub(crate) enum WhisperError {
    #[error("whisper recipient is empty")]
    EmptyRecipient,
    #[error("whisper message is empty")]
    EmptyMessage,
    #[error("whisper message exceeds {MAX_WHISPER_CHARS}-character limit")]
    MessageTooLong,
    #[error("recipient does not accept whispers")]
    RecipientRejectsWhispers,
    #[error(transparent)]
    Helix(#[from] HelixError),
}

fn classify_rejection(error: HelixError) -> WhisperError {
    let HelixError::Http { status, body } = &error else {
        return WhisperError::Helix(error);
    };
    let lowered = refusal_text(body).to_lowercase();
    let blocks_strangers = WHISPER_BLOCK_MARKERS
        .iter()
        .any(|marker| lowered.contains(marker));
    if *status == BAD_REQUEST && blocks_strangers {
        return WhisperError::RecipientRejectsWhispers;
    }
    WhisperError::Helix(error)
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
    let request = WHISPER_BLOCK_MARKERS
        .iter()
        .fold(request, |request, marker| {
            request.expect_status_with_excerpt(BAD_REQUEST, marker)
        });
    transport
        .execute(request)
        .await
        .map_err(classify_rejection)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{
        MockTransport, REQUEST_FAIL_KIND, observed_helix_at, users_fixture,
    };
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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

    #[tokio::test]
    async fn whisper_to_a_user_id_posts_without_resolving_a_login() {
        let transport = MockTransport::returning(Ok(serde_json::Value::Null));

        send_whisper_to(&transport, "100", Some(UserTarget::Id("1001")), "psst")
            .await
            .unwrap();

        assert_eq!(transport.call_count(), 1, "no Get Users lookup for an id");
        let whisper = transport.last_request();
        assert_eq!(whisper.path, WHISPERS_PATH);
        assert!(
            whisper
                .query
                .contains(&("to_user_id".to_owned(), "1001".to_owned()))
        );
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

    #[tokio::test]
    async fn only_a_400_naming_a_stranger_or_block_rule_means_the_recipient_refuses_whispers() {
        let refusal = |status: u16, message: &str| {
            Err(HelixError::Http {
                status,
                body: serde_json::json!({"status": status, "message": message}).to_string(),
            })
        };
        let cases: [(Result<serde_json::Value, HelixError>, bool); 6] = [
            (
                refusal(
                    400,
                    "The recipient's settings prevent this sender from whispering them. Strangers are blocked.",
                ),
                true,
            ),
            (refusal(400, "Recipient has BLOCKED you"), true),
            (refusal(400, "This user doesn't allow whispers"), true),
            (refusal(400, "The message field may not be empty"), false),
            (refusal(403, "Blocked by policy"), false),
            (Err(HelixError::RateLimited), false),
        ];

        for (response, rejects) in cases {
            let label = format!("{response:?}");
            let transport = MockTransport::returning(response);

            let err = send_whisper_to(&transport, "100", Some(UserTarget::Id("1001")), "psst")
                .await
                .unwrap_err();

            assert_eq!(
                matches!(err, WhisperError::RecipientRejectsWhispers),
                rejects,
                "{label} -> {err:?}"
            );
        }
    }

    #[tokio::test]
    async fn only_a_400_naming_a_whisper_block_rule_is_kept_out_of_request_fail() {
        let refusal = |status: StatusCode, message: &str| {
            ResponseTemplate::new(status.as_u16()).set_body_json(
                serde_json::json!({"error": "x", "status": status.as_u16(), "message": message}),
            )
        };
        let cases = [
            (
                refusal(
                    StatusCode::BAD_REQUEST,
                    "Strangers are not allowed to whisper",
                ),
                true,
                0,
            ),
            (
                refusal(StatusCode::BAD_REQUEST, "Recipient has BLOCKED you"),
                true,
                0,
            ),
            (
                refusal(StatusCode::BAD_REQUEST, "This user DOESN'T ALLOW whispers"),
                true,
                0,
            ),
            (
                refusal(
                    StatusCode::BAD_REQUEST,
                    "The message field may not be empty",
                ),
                false,
                1,
            ),
            (
                refusal(StatusCode::FORBIDDEN, "Blocked by policy"),
                false,
                1,
            ),
        ];

        for (reply, rejects, request_fails) in cases {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path(WHISPERS_PATH))
                .respond_with(reply)
                .mount(&server)
                .await;
            let (transport, bus) = observed_helix_at(&server.uri());

            let err = send_whisper_to(
                transport.as_ref(),
                "100",
                Some(UserTarget::Id("1001")),
                "psst",
            )
            .await
            .unwrap_err();

            assert_eq!(
                (
                    matches!(err, WhisperError::RecipientRejectsWhispers),
                    bus.of_kind(REQUEST_FAIL_KIND).len()
                ),
                (rejects, request_fails),
                "{err:?}"
            );
        }
    }
}
