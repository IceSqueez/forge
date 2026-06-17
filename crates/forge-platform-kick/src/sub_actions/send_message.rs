use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::send::KickSendChat;

const KIND_ID: &str = "kick.chat.send_message";

pub struct SendMessageRunner {
    client: Arc<KickSendChat>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_user_id: u64,
}

impl SendMessageRunner {
    pub fn new(
        client: Arc<KickSendChat>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        broadcaster_user_id: u64,
    ) -> Self {
        Self {
            client,
            token_source,
            broadcaster_user_id,
        }
    }
}

#[async_trait]
impl SubActionRunner for SendMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        "Send Message"
    }

    fn summary(&self) -> &str {
        "Posts a message to the Kick channel chat."
    }

    fn search_text(&self) -> &str {
        "kick chat message send say post"
    }

    fn icon_name(&self) -> &str {
        "chat"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("message".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::TextArea {
            key: "message",
            label: "Message",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'message' must be a non-empty string"
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

        let template = config
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let message = ctx.arg_stack.interpolate(template);

        let outcome = if message.is_empty() {
            SubActionOutcome::Failed("message is empty after interpolation".to_owned())
        } else {
            match (self.token_source)().await {
                Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                Ok(token) => match self
                    .client
                    .send(&message, &token, self.broadcaster_user_id)
                    .await
                {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                },
            }
        };

        (
            SubActionTelemetry {
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
    use forge_events::{Event, EventPublisher};
    use forge_platform_core::{RateLimitOutcome, RateLimiter};
    use forge_types::EventId;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    struct GrantLimiter;
    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }
        fn remaining(&self) -> u32 {
            120
        }
        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NoopPublisher,
        }
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> SendMessageRunner {
        let client = KickSendChat::new(Arc::new(GrantLimiter))
            .with_send_endpoint(format!("{}/chat", server.uri()));
        SendMessageRunner::new(Arc::new(client), token_source(), 42)
    }

    fn config(message: &str) -> SubActionConfig {
        BTreeMap::from([("message".to_owned(), Variant::String(message.to_owned()))])
    }

    #[tokio::test]
    async fn execute_posts_interpolated_message_and_reports_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("u".to_owned(), Variant::String("alice".to_owned()));

        let (telemetry, _) = runner.execute(&config("hi %u%"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn empty_message_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("u".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner.execute(&config("%u%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty message must not reach the send transport"
        );
    }

    #[test]
    fn validate_config_accepts_non_empty_and_rejects_empty_missing_non_string() {
        let runner = SendMessageRunner::new(
            Arc::new(KickSendChat::new(Arc::new(GrantLimiter))),
            token_source(),
            42,
        );

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("non-empty", config("hello"), true),
            ("empty", config(""), false),
            ("missing", BTreeMap::new(), false),
            (
                "non-string",
                BTreeMap::from([("message".to_owned(), Variant::Int(7))]),
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
