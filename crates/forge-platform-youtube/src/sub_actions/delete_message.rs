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

use crate::send_chat::YoutubeSendChat;

const KIND_ID: &str = "youtube.chat.delete_message";

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
            Variant::String("%chat.message_id%".to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "message_id",
            label: "Message ID",
            placeholder: "%chat.message_id%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'message_id' must be a non-empty string"
            ))),
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
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use tokio::sync::Mutex;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::quota_state::QuotaState;

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
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let sender =
            YoutubeSendChat::new(token_source(), handle, quota).with_api_base(server.uri());
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
    async fn empty_message_id_after_interpolation_fails_without_delete() {
        let server = MockServer::start().await;
        // No mock mounted; assert via received_requests the transport is untouched.
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("mid".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner.execute(&config("%mid%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty message_id must not reach the delete transport"
        );
    }

    #[test]
    fn validate_config_rejects_empty_or_non_string_id_and_accepts_valid() {
        let runner = DeleteMessageRunner::new(Arc::new(
            YoutubeSendChat::new(
                token_source(),
                LiveChatIdHandle::new(),
                Arc::new(Mutex::new(QuotaState::default())),
            )
            .with_api_base("http://127.0.0.1:0".to_owned()),
        ));

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
