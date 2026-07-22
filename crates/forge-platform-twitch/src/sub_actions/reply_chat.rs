use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.chat.reply";
/// Twitch counts characters, not bytes; multibyte messages must pass at 500 chars.
const MAX_MESSAGE_CHARS: usize = 500;
const DEFAULT_PARENT_TEMPLATE: &str = "%chat.message_id%";

pub struct ReplyChatRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ReplyChatRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn reply(&self, message: &str, parent_message_id: &str) -> SubActionOutcome {
        if message.is_empty() {
            return SubActionOutcome::Failed("message is empty after interpolation".to_owned());
        }
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return SubActionOutcome::Failed("message exceeds 500-character limit".to_owned());
        }
        if parent_message_id.is_empty() {
            return SubActionOutcome::Failed(
                "parent_message_id is empty after interpolation".to_owned(),
            );
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request =
            HelixRequest::new(HelixMethod::Post, "/helix/chat/messages").body(serde_json::json!({
                "broadcaster_id": user_id,
                "sender_id": user_id,
                "message": message,
                "reply_parent_message_id": parent_message_id,
            }));
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for ReplyChatRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Reply to Message"
    }

    fn summary(&self) -> &str {
        "Sends a reply to a specific chat message."
    }

    fn search_text(&self) -> &str {
        "twitch chat reply respond message thread"
    }

    fn icon_name(&self) -> &str {
        "reply"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("message".to_owned(), Variant::String(String::new())),
            (
                "parent_message_id".to_owned(),
                Variant::String(DEFAULT_PARENT_TEMPLATE.to_owned()),
            ),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::TextArea {
                key: "message",
                label: "Message",
            },
            FormField::Text {
                key: "parent_message_id",
                label: "Parent Message ID",
                placeholder: "%chat.message_id%",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'message' must be a non-empty string"
                )));
            }
        }
        match config.get("parent_message_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'parent_message_id' must be a non-empty string"
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

        let msg_template = config.str("message").unwrap_or_default();
        let message = ctx.arg_stack.interpolate(msg_template);

        let parent_template = config
            .str("parent_message_id")
            .unwrap_or(DEFAULT_PARENT_TEMPLATE);
        let parent_message_id = ctx.arg_stack.interpolate(parent_template);

        let outcome = self.reply(&message, &parent_message_id).await;

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
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
        creds: MockCreds,
    ) -> (Arc<MockTransport>, ReplyChatRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = ReplyChatRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(creds))),
        );
        (transport, runner)
    }

    fn config(message: &str, parent_message_id: &str) -> SubActionConfig {
        BTreeMap::from([
            ("message".to_owned(), Variant::String(message.to_owned())),
            (
                "parent_message_id".to_owned(),
                Variant::String(parent_message_id.to_owned()),
            ),
        ])
    }

    #[tokio::test]
    async fn execute_posts_reply_with_broadcaster_and_sender_both_as_self() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new()
            .set("msg".to_owned(), Variant::String("hello!".to_owned()))
            .set(
                "parent_id".to_owned(),
                Variant::String("msg-abc-123".to_owned()),
            );

        let (telemetry, _) = runner
            .execute(&config("%msg%", "%parent_id%"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/chat/messages");
        let body = request.body.unwrap();
        assert_eq!(body["broadcaster_id"], SELF_USER_ID);
        assert_eq!(body["sender_id"], SELF_USER_ID);
        assert_eq!(body["message"], "hello!");
        assert_eq!(body["reply_parent_message_id"], "msg-abc-123");
    }

    #[tokio::test]
    async fn empty_interpolated_message_or_parent_id_fails_before_transport_call() {
        let cases: &[(&str, &str, &str, &str)] = &[
            (
                "%m%",
                "fixed-parent",
                "",
                "empty message after interpolation",
            ),
            (
                "fixed text",
                "%p%",
                "",
                "empty parent_message_id after interpolation",
            ),
        ];

        for (msg_tpl, parent_tpl, var_val, label) in cases {
            let (transport, runner) =
                runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
            let stack = ArgStack::new()
                .set("m".to_owned(), Variant::String((*var_val).to_owned()))
                .set("p".to_owned(), Variant::String((*var_val).to_owned()));

            let (telemetry, _) = runner
                .execute(&config(msg_tpl, parent_tpl), &make_ctx(&stack))
                .await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "expected Failed for case: {label}"
            );
            assert_eq!(
                transport.call_count(),
                0,
                "transport must not be called for case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn message_limit_enforced_by_character_count_not_byte_count() {
        for (char_count, should_send) in [(500usize, true), (501, false)] {
            let (transport, runner) =
                runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
            let stack =
                ArgStack::new().set("msg".to_owned(), Variant::String("я".repeat(char_count)));

            let (telemetry, _) = runner
                .execute(&config("%msg%", "parent-id-fixed"), &make_ctx(&stack))
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
    async fn helix_4xx_maps_to_failed_outcome_without_token_or_url() {
        let (_transport, runner) = runner_with(
            Err(HelixError::Http {
                status: 400,
                body: "message_id not found".to_owned(),
            }),
            MockCreds::with_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&config("hello", "msg-parent-123"), &make_ctx(&stack))
            .await;

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(
            msg.contains("400"),
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
}
