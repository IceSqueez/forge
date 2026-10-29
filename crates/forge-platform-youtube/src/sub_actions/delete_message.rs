use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::send_chat::YoutubeSendChat;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::CHAT_DELETE_MESSAGE;
const MESSAGE_ID_VARIABLE: &str = "%message_id%";
const UNRESOLVED_REFERENCE_MARKER: char = '%';

pub struct DeleteMessageRunner {
    sender: Arc<YoutubeSendChat>,
}

impl DeleteMessageRunner {
    pub fn new(sender: Arc<YoutubeSendChat>) -> Self {
        Self { sender }
    }
}

#[async_trait]
impl SubActionRunner for DeleteMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Delete Message"
    }

    fn summary(&self) -> &str {
        "Removes a message from the active YouTube live chat by its resource id."
    }

    fn search_text(&self) -> &str {
        "youtube chat delete remove message moderation live"
    }

    fn icon_name(&self) -> &str {
        "delete"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "message_id".to_owned(),
            Variant::String(MESSAGE_ID_VARIABLE.to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "message_id",
            label: "Message ID",
            placeholder: MESSAGE_ID_VARIABLE,
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::invalid_field(
                "message_id",
                ConfigReason::Required,
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let template = config.str("message_id").unwrap_or_default();
        let message_id = ctx.arg_stack.interpolate(template);

        let outcome = if message_id.is_empty() {
            SubActionOutcome::Failed("message_id is empty after interpolation".to_owned())
        } else if message_id.contains(UNRESOLVED_REFERENCE_MARKER) {
            SubActionOutcome::Failed("message_id has an unresolved variable reference".to_owned())
        } else {
            SubActionOutcome::from_result(&self.sender.delete(&message_id).await)
        };

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
mod tests {
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use forge_registry::TriggerKindDescriptor;

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source() -> Arc<
        dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
            + Send
            + Sync,
    > {
        Arc::new(|| Box::pin(async { Ok("yt-token".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> DeleteMessageRunner {
        let handle = LiveChatIdHandle::new();
        let sender = YoutubeSendChat::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token_source(),
            handle,
        );
        DeleteMessageRunner::new(Arc::new(sender))
    }

    fn config(message_id: &str) -> SubActionConfig {
        BTreeMap::from([(
            "message_id".to_owned(),
            Variant::String(message_id.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_interpolates_message_id_before_delete() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/liveChat/messages"))
            .and(query_param("id", "abc-123"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("mid".to_owned(), Variant::String("abc-123".to_owned()));

        let (telemetry, _) = runner.execute(&config("%mid%"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn the_default_message_id_deletes_the_message_that_fired_a_chat_trigger() {
        let event = Event::new(
            forge_events::EventSource::YouTube,
            "youtube.chat.message",
            serde_json::json!({
                "message_id": "LCC.abc-123",
                "message_text": "!roll 1d6",
                "author": { "display_name": "Viewer", "channel_id": "UCabc" }
            }),
        );
        let stacks = [
            crate::triggers::chat_message::ChatMessageDescriptor.build_arg_stack(&event),
            crate::triggers::chat_command::ChatCommandDescriptor.build_arg_stack(&event),
        ];
        for stack in stacks {
            let server = MockServer::start().await;
            Mock::given(method("DELETE"))
                .and(path("/liveChat/messages"))
                .and(query_param("id", "LCC.abc-123"))
                .respond_with(ResponseTemplate::new(204))
                .expect(1)
                .mount(&server)
                .await;
            let runner = runner_on(&server);

            let (telemetry, _) = runner
                .execute(&runner.default_config(), &make_ctx(&stack))
                .await;

            assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        }
    }

    #[tokio::test]
    async fn an_empty_or_unresolved_message_id_fails_without_calling_the_api() {
        let server = MockServer::start().await;
        let runner = runner_on(&server);
        let blank = ArgStack::new().set("mid".to_owned(), Variant::String(String::new()));
        for (template, stack) in [
            ("%mid%", blank),
            ("%message_id%", ArgStack::new()),
            ("prefix-%missing%", ArgStack::new()),
        ] {
            let (telemetry, _) = runner.execute(&config(template), &make_ctx(&stack)).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "template {template:?}"
            );
        }
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an unusable message_id must not reach the delete transport"
        );
    }

    #[test]
    fn validate_config_rejects_empty_or_non_string_id_and_accepts_valid() {
        let runner = DeleteMessageRunner::new(Arc::new(YoutubeSendChat::new(
            crate::endpoints_test_support::unreachable_data_api(),
            token_source(),
            LiveChatIdHandle::new(),
        )));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("non-empty id", config("msg-1"), true),
            ("empty id", config(""), false),
            ("missing id", BTreeMap::new(), false),
            (
                "non-string id",
                BTreeMap::from([("message_id".to_owned(), Variant::Int(7))]),
                false,
            ),
        ];

        for (label, cfg, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&cfg).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }
}
