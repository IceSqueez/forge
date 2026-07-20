use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.chat.send_announcement";
/// Twitch counts characters, not bytes; multibyte messages must pass at 500 chars.
const MAX_MESSAGE_CHARS: usize = 500;
const COLORS: &[&str] = &["primary", "blue", "green", "orange", "purple"];
const DEFAULT_COLOR: &str = "primary";

pub struct SendAnnouncementRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl SendAnnouncementRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn send(&self, message: &str, color: &str) -> SubActionOutcome {
        if message.is_empty() {
            return SubActionOutcome::Failed("message is empty after interpolation".to_owned());
        }
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return SubActionOutcome::Failed("message exceeds 500-character limit".to_owned());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/chat/announcements")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .body(serde_json::json!({ "message": message, "color": color }));
        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}

#[async_trait]
impl SubActionRunner for SendAnnouncementRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Send Announcement"
    }

    fn summary(&self) -> &str {
        "Posts a highlighted announcement in the Twitch chat."
    }

    fn search_text(&self) -> &str {
        "twitch chat announcement announce highlight banner notice"
    }

    fn icon_name(&self) -> &str {
        "speakerphone"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("message".to_owned(), Variant::String(String::new())),
            (
                "color".to_owned(),
                Variant::String(DEFAULT_COLOR.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "message",
                label: "Message",
            },
            FormField::Select {
                key: "color",
                label: "Color",
                options: COLORS,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'message' must be a non-empty string"
                )));
            }
        }
        match config.get("color") {
            None => {}
            Some(Variant::String(c)) if COLORS.contains(&c.as_str()) => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'color' must be one of blue, green, orange, purple, primary"
                )));
            }
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let template = config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let message = ctx.arg_stack.interpolate(template);
        let color = config
            .get("color")
            .and_then(|v| v.as_str())
            .filter(|c| COLORS.contains(c))
            .unwrap_or(DEFAULT_COLOR);

        let outcome = self.send(&message, color).await;

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
        creds: MockCreds,
    ) -> (Arc<MockTransport>, SendAnnouncementRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = SendAnnouncementRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(creds))),
        );
        (transport, runner)
    }

    fn config(message: &str, color: Option<&str>) -> SubActionConfig {
        let mut c = BTreeMap::from([("message".to_owned(), Variant::String(message.to_owned()))]);
        if let Some(color) = color {
            c.insert("color".to_owned(), Variant::String(color.to_owned()));
        }
        c
    }

    #[tokio::test]
    async fn execute_posts_announcement_with_self_as_broadcaster_and_moderator() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", Some("blue")), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/chat/announcements");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(
            request
                .query
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        let body = request.body.unwrap();
        assert_eq!(body["message"], "big news");
        assert_eq!(body["color"], "blue");
    }

    #[tokio::test]
    async fn execute_interpolates_message_template_before_send() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("viewer42".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("Welcome %user%!", None), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.last_request().body.unwrap()["message"],
            "Welcome viewer42!"
        );
    }

    #[tokio::test]
    async fn helix_http_failure_maps_to_failed_outcome_without_token_or_url() {
        let (_transport, runner) = runner_with(
            Err(HelixError::Http {
                status: 403,
                body: "moderator scope missing".to_owned(),
            }),
            MockCreds::with_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", None), &make_ctx(&stack))
            .await;

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(
            msg.contains("403"),
            "status must surface for diagnosis: {msg}"
        );
        assert!(
            !msg.contains(TOKEN_SENTINEL),
            "outcome must not leak the token: {msg}"
        );
        assert!(
            !msg.contains("api.twitch.tv"),
            "outcome must not leak the request URL: {msg}"
        );
    }

    #[tokio::test]
    async fn empty_message_after_interpolation_fails_without_transport_call() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new().set("greeting".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%greeting%", None), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty message must not reach Helix"
        );
    }

    #[tokio::test]
    async fn message_limit_enforced_by_character_count_not_byte_count() {
        for (char_count, should_send) in [(500, true), (501, false)] {
            let (transport, runner) =
                runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
            // Cyrillic is 2 bytes per char; the limit must count chars.
            let stack =
                ArgStack::new().set("msg".to_owned(), Variant::String("я".repeat(char_count)));

            let (telemetry, _) = runner
                .execute(&config("%msg%", None), &make_ctx(&stack))
                .await;

            if should_send {
                assert_eq!(
                    telemetry.outcome,
                    SubActionOutcome::Success,
                    "{char_count}-char message must send"
                );
                assert_eq!(transport.call_count(), 1);
            } else {
                assert!(
                    matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                    "{char_count}-char message must fail"
                );
                assert_eq!(
                    transport.call_count(),
                    0,
                    "over-limit message must not reach Helix"
                );
            }
        }
    }

    #[tokio::test]
    async fn missing_credentials_fail_without_transport_call() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null), MockCreds::empty());
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", None), &make_ctx(&stack))
            .await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("no twitch credentials")
        ));
        assert_eq!(transport.call_count(), 0, "no identity, no request");
    }

    #[tokio::test]
    async fn execute_falls_back_to_primary_color_for_unrecognized_stored_color() {
        // A stale stored color must degrade to the default instead of sending
        // a value Helix would reject with 400.
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("big news", Some("magenta")), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.last_request().body.unwrap()["color"], "primary");
    }

    #[test]
    fn validate_config_rejects_bad_message_or_color_and_accepts_valid() {
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid message and color", config("hi", Some("green")), true),
            ("valid message, absent color", config("hi", None), true),
            ("empty message", config("", Some("blue")), false),
            ("missing message", BTreeMap::new(), false),
            ("unknown color", config("hi", Some("magenta")), false),
            (
                "non-string message",
                BTreeMap::from([("message".to_owned(), Variant::Int(3))]),
                false,
            ),
            (
                "non-string color",
                BTreeMap::from([
                    ("message".to_owned(), Variant::String("hi".to_owned())),
                    ("color".to_owned(), Variant::Int(1)),
                ]),
                false,
            ),
        ];
        let (_transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());

        for (label, cfg, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&cfg).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }
}
