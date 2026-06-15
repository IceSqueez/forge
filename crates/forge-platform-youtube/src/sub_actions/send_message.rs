use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::send_chat::YoutubeSendChat;

const KIND_ID: &str = "youtube.chat.send_message";

pub struct SendMessageRunner {
    sender: Arc<YoutubeSendChat>,
}

impl SendMessageRunner {
    pub fn new(sender: Arc<YoutubeSendChat>) -> Self {
        Self { sender }
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
        "Posts a message in the active YouTube live chat."
    }

    fn search_text(&self) -> &str {
        "youtube chat message send say post live"
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
            match self.sender.send(&message).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
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
    use std::sync::Arc;

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use serde_json::json;
    use tokio::sync::Mutex;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::quota_state::QuotaState;

    const TOKEN_SENTINEL: &str = "yt-secret-token";

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NoopPublisher,
        }
    }

    fn token_source() -> Arc<
        dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
            + Send
            + Sync,
    > {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    /// Builds a runner whose sender posts to a live wiremock server (no real
    /// YouTube), with an active live-chat id so the send path is reachable.
    fn runner_on(server: &MockServer) -> SendMessageRunner {
        let handle = LiveChatIdHandle::new();
        handle.set(Some("lc-test".to_owned()));
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let sender =
            YoutubeSendChat::new(token_source(), handle, quota).with_api_base(server.uri());
        SendMessageRunner::new(Arc::new(sender))
    }

    fn config(message: &str) -> SubActionConfig {
        BTreeMap::from([("message".to_owned(), Variant::String(message.to_owned()))])
    }

    #[tokio::test]
    async fn execute_interpolates_message_template_before_send() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"kind": "x"})))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("viewer42".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("Welcome %user%!"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let req = &server.received_requests().await.unwrap()[0];
        let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(
            body["snippet"]["textMessageDetails"]["messageText"],
            "Welcome viewer42!"
        );
    }

    #[tokio::test]
    async fn empty_message_after_interpolation_fails_without_send() {
        let server = MockServer::start().await;
        // No mock mounted: any request would 404 and surface as a non-empty
        // body. Assert via received_requests that the transport is untouched.
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("greeting".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%greeting%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty message must not reach the send transport"
        );
    }

    #[tokio::test]
    async fn send_error_maps_to_failed_without_leaking_token_or_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(
                ResponseTemplate::new(403).set_body_string("forbidden: insufficient scope"),
            )
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("hello"), &make_ctx(&stack)).await;

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(
            !msg.contains(TOKEN_SENTINEL),
            "outcome must not leak the bearer token: {msg}"
        );
        assert!(
            !msg.contains(&server.uri()),
            "outcome must not leak the request URL: {msg}"
        );
        assert!(
            !msg.to_lowercase().contains("googleapis"),
            "outcome must not leak the API host: {msg}"
        );
    }

    #[test]
    fn validate_config_rejects_empty_or_non_string_message_and_accepts_valid() {
        let server_uri = "http://127.0.0.1:0".to_owned();
        let handle = LiveChatIdHandle::new();
        let quota = Arc::new(Mutex::new(QuotaState::default()));
        let sender = YoutubeSendChat::new(token_source(), handle, quota).with_api_base(server_uri);
        let runner = SendMessageRunner::new(Arc::new(sender));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("non-empty message", config("hi"), true),
            ("empty message", config(""), false),
            ("missing message", BTreeMap::new(), false),
            (
                "non-string message",
                BTreeMap::from([("message".to_owned(), Variant::Int(3))]),
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
