use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_platform_core::{SENDER_FIELD, SenderPreference};
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::bot_account::TwitchBotAccount;
use crate::chat_sender::{
    ChatDelivery, ChatSenders, StepResult, configured_sender, failed_step, step_result,
};
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.chat.reply";
const MAX_MESSAGE_CHARS: usize = 500;
const DEFAULT_PARENT_TEMPLATE: &str = "%chat.message_id%";

pub struct ReplyChatRunner {
    senders: ChatSenders,
}

impl ReplyChatRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            senders: ChatSenders::new(transport, identity),
        }
    }

    pub fn with_bot(mut self, bot: Option<Arc<TwitchBotAccount>>) -> Self {
        self.senders = self.senders.with_bot(bot);
        self
    }

    async fn reply(
        &self,
        message: &str,
        parent_message_id: &str,
        sender: SenderPreference,
        events: &dyn EventPublisher,
    ) -> StepResult {
        if message.is_empty() {
            return failed_step("message is empty after interpolation");
        }
        if message.chars().count() > MAX_MESSAGE_CHARS {
            return failed_step("message exceeds 500-character limit");
        }
        if parent_message_id.is_empty() {
            return failed_step("parent_message_id is empty after interpolation");
        }
        let delivery = ChatDelivery::Reply {
            text: message,
            parent_message_id,
        };
        step_result(self.senders.deliver(delivery, sender, events).await)
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
            (
                SENDER_FIELD.to_owned(),
                Variant::String(SenderPreference::Default.as_str().to_owned()),
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
            FormField::Select {
                key: SENDER_FIELD,
                label: "Send As",
                options: SenderPreference::WIRE_NAMES,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::invalid_field(
                    "message",
                    ConfigReason::Required,
                ));
            }
        }
        match config.get("parent_message_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::invalid_field(
                    "parent_message_id",
                    ConfigReason::Required,
                ));
            }
        }
        configured_sender(config).map(|_| ())
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

        let (outcome, produced) = match configured_sender(config) {
            Ok(sender) => {
                self.reply(&message, &parent_message_id, sender, ctx.publisher)
                    .await
            }
            Err(e) => (SubActionOutcome::Failed(e.to_string()), BTreeMap::new()),
        };

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced,
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
    use crate::helix::{HelixError, HelixMethod};
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

    fn sent() -> Result<serde_json::Value, HelixError> {
        Ok(serde_json::json!({ "data": [{ "message_id": "m-1", "is_sent": true }] }))
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
        let (transport, runner) = runner_with(sent(), MockCreds::with_identity());
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
            let (transport, runner) = runner_with(sent(), MockCreds::with_identity());
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

    #[tokio::test]
    async fn execute_records_the_acting_sender_only_when_the_reply_is_sent() {
        let cases: [(
            Result<serde_json::Value, HelixError>,
            Option<serde_json::Value>,
        ); 2] = [
            (
                sent(),
                Some(serde_json::json!({ "role": "streamer", "login": "streamer" })),
            ),
            (
                Err(HelixError::Http {
                    status: 400,
                    body: "message_id not found".to_owned(),
                }),
                None,
            ),
        ];

        for (response, expected) in cases {
            let (_transport, runner) = runner_with(response, MockCreds::with_identity());

            let (telemetry, _) = runner
                .execute(&config("hello", "parent-1"), &make_ctx(&ArgStack::new()))
                .await;

            let recorded = telemetry
                .produced
                .get(forge_platform_core::SENT_AS_FIELD)
                .map(|json| serde_json::from_str::<serde_json::Value>(json).unwrap());
            assert_eq!(recorded, expected);
        }
    }

    #[tokio::test]
    async fn unknown_sender_is_rejected_by_validate_and_by_execute_without_a_helix_call() {
        let (transport, runner) = runner_with(sent(), MockCreds::with_identity());
        let mut config = config("hello", "parent-1");
        config.insert(
            SENDER_FIELD.to_owned(),
            Variant::String("moderator".to_owned()),
        );

        let validation = runner.validate_config(&config);
        let (telemetry, _) = runner.execute(&config, &make_ctx(&ArgStack::new())).await;

        assert!(validation.is_err());
        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(telemetry.produced.is_empty());
        assert_eq!(transport.call_count(), 0);
    }
}
