use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::api::chat_messages::KickSendChat;
use crate::vocabulary::sub_action_ids;
use crate::vocabulary::variables::references;

const KIND_ID: &str = sub_action_ids::CHAT_DELETE_MESSAGE;
const UNRESOLVED_REFERENCE_MARKER: char = '%';

pub struct DeleteMessageRunner {
    client: Arc<KickSendChat>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl DeleteMessageRunner {
    pub fn new(
        client: Arc<KickSendChat>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
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
        "Removes a chat message from the Kick channel by its message id. Requires moderation:chat_message:manage scope."
    }

    fn search_text(&self) -> &str {
        "kick chat delete remove message moderation"
    }

    fn icon_name(&self) -> &str {
        "delete"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "message_id".to_owned(),
            Variant::String(references::MESSAGE_ID.to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "message_id",
            label: "Message ID",
            placeholder: references::MESSAGE_ID,
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
            match (self.token_source)().await {
                Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                Ok(token) => match self.client.delete(&message_id, &token).await {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                },
            }
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
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> DeleteMessageRunner {
        let client = KickSendChat::new(crate::endpoints_test_support::api_at(&server.uri()));
        DeleteMessageRunner::new(Arc::new(client), token_source())
    }

    fn config(message_id: &str) -> SubActionConfig {
        BTreeMap::from([(
            "message_id".to_owned(),
            Variant::String(message_id.to_owned()),
        )])
    }

    fn chat_trigger_args(message_id: &str) -> ArgStack {
        let event = Event::new(
            forge_events::EventSource::Kick,
            "kick.chat.message.sent",
            serde_json::json!({ "message_id": message_id, "content": "hello" }),
        );
        forge_registry::TriggerKindDescriptor::build_arg_stack(
            &crate::triggers::chat::ChatDescriptor,
            &event,
        )
    }

    #[tokio::test]
    async fn default_config_deletes_the_message_that_fired_the_chat_trigger() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/chat/abc-123"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let runner = runner_on(&server);
        let stack = chat_trigger_args("abc-123");

        let (telemetry, _) = runner
            .execute(&runner.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn a_message_id_that_does_not_resolve_to_an_id_fails_without_request() {
        for (case, template, stack) in [
            ("empty id", "%message_id%", chat_trigger_args("")),
            ("no chat trigger in scope", "%message_id%", ArgStack::new()),
            (
                "unresolved reference beside a resolved one",
                "%message_id%-%typo_id%",
                chat_trigger_args("abc-123"),
            ),
        ] {
            let server = MockServer::start().await;
            let runner = runner_on(&server);

            let (telemetry, _) = runner.execute(&config(template), &make_ctx(&stack)).await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "{case}"
            );
            assert!(
                server.received_requests().await.unwrap().is_empty(),
                "{case}"
            );
        }
    }

    #[test]
    fn validate_config_accepts_non_empty_and_rejects_empty_missing_non_string() {
        let runner = DeleteMessageRunner::new(
            Arc::new(KickSendChat::new(
                crate::endpoints_test_support::unreachable_api(),
            )),
            token_source(),
        );

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("non-empty", config("msg-1"), true),
            ("empty", config(""), false),
            ("missing", BTreeMap::new(), false),
            (
                "non-string",
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
