use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{ChatSent, EventSource, TypedEvent};
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextEditor, TextField, ToggleField, TypedField,
};
use forge_types::{ArgStack, PlatformId, SubActionOutcome, SubActionTelemetry};
use futures::future::BoxFuture;

use super::step_failure;
use crate::api::chat_messages::KickSendChat;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_SEND_MESSAGE;
const BROADCASTER_IGNORED_FOR_BOT: u64 = 0;

const MESSAGE: TextField = TextField {
    key: config_keys::MESSAGE,
    label: "Message",
    editor: TextEditor::Area,
    default: "",
    required: true,
};

const AS_BOT: ToggleField = ToggleField {
    key: config_keys::AS_BOT,
    label: "Send as bot",
    default: false,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MESSAGE, &AS_BOT]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, bool), ConfigProblem> {
    Ok((MESSAGE.read(config, args)?, AS_BOT.read(config, args)?))
}

pub struct SendMessageRunner {
    client: Arc<KickSendChat>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_id_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync>,
}

impl SendMessageRunner {
    pub fn new(
        client: Arc<KickSendChat>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        broadcaster_id_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
            broadcaster_id_source,
        }
    }

    async fn send(&self, message: &str, as_bot: bool) -> SubActionOutcome {
        let token = match (self.token_source)().await {
            Ok(token) => token,
            Err(e) => {
                return SubActionOutcome::Failed(step_failure::token_failed(&e));
            }
        };
        let broadcaster_user_id = if as_bot {
            BROADCASTER_IGNORED_FOR_BOT
        } else {
            match (self.broadcaster_id_source)().await {
                Ok(id) => id,
                Err(e) => {
                    return SubActionOutcome::Failed(step_failure::broadcaster_failed(&e));
                }
            }
        };
        step_failure::outcome(
            &self
                .client
                .send(message, &token, broadcaster_user_id, as_bot)
                .await,
        )
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let (message, as_bot) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let outcome = self.send(&message, as_bot).await;
        if matches!(outcome, SubActionOutcome::Success) {
            ChatSent {
                channel: PlatformId::Kick.as_str().to_owned(),
                message,
                reply_to_message_id: None,
                sent_as: None,
            }
            .publish(ctx.publisher, EventSource::Kick, Some(ctx.parent_event_id));
        }
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use forge_types::Variant;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    #[derive(Default)]
    struct RecordingPublisher(std::sync::Mutex<Vec<Event>>);
    impl EventPublisher for RecordingPublisher {
        fn publish(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl RecordingPublisher {
        fn published(&self) -> Vec<(EventSource, String, serde_json::Value, Option<EventId>)> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|e| (e.source, e.kind.clone(), e.payload.clone(), e.caused_by))
                .collect()
        }
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn broadcaster_id_source(
        id: u64,
    ) -> Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync> {
        Arc::new(move || Box::pin(async move { Ok(id) }))
    }

    fn failing_broadcaster_id_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync> {
        Arc::new(|| {
            Box::pin(async {
                Err(PlatformError::Network {
                    reason: "channel lookup failed".to_owned(),
                })
            })
        })
    }

    fn runner_on(server: &MockServer) -> SendMessageRunner {
        let client = KickSendChat::new(crate::endpoints_test_support::api_at(&server.uri()));
        SendMessageRunner::new(Arc::new(client), token_source(), broadcaster_id_source(42))
    }

    fn config(message: &str) -> SubActionConfig {
        BTreeMap::from([("message".to_owned(), Variant::String(message.to_owned()))])
    }

    fn bot_config(message: &str) -> SubActionConfig {
        BTreeMap::from([
            ("message".to_owned(), Variant::String(message.to_owned())),
            ("as_bot".to_owned(), Variant::Bool(true)),
        ])
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

        assert!(
            matches!(&telemetry.outcome, SubActionOutcome::Failed(reason)
                if super::super::test_support::rejected_field(reason) == Some(config_keys::MESSAGE))
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty message must not reach the send transport"
        );
    }

    #[tokio::test]
    async fn bot_mode_sends_even_when_the_broadcaster_id_cannot_be_resolved() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let client = KickSendChat::new(crate::endpoints_test_support::api_at(&server.uri()));
        let runner = SendMessageRunner::new(
            Arc::new(client),
            token_source(),
            failing_broadcaster_id_source(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&bot_config("hi"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn user_mode_reports_a_broadcaster_id_failure_without_sending() {
        let server = MockServer::start().await;
        let client = KickSendChat::new(crate::endpoints_test_support::api_at(&server.uri()));
        let runner = SendMessageRunner::new(
            Arc::new(client),
            token_source(),
            failing_broadcaster_id_source(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("hi"), &make_ctx(&stack)).await;

        match telemetry.outcome {
            SubActionOutcome::Failed(reason) => {
                assert_eq!(
                    reason.code_or_text(),
                    "kick.broadcaster_unavailable",
                    "unresolved identity must be attributed to the broadcaster id, got: {reason:?}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[test]
    fn validate_config_accepts_non_empty_and_rejects_empty_missing_non_string() {
        let runner = SendMessageRunner::new(
            Arc::new(KickSendChat::new(
                crate::endpoints_test_support::unreachable_api(),
            )),
            token_source(),
            broadcaster_id_source(42),
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

    #[tokio::test]
    async fn a_delivered_message_publishes_chat_send_caused_by_the_run() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("u".to_owned(), Variant::String("alice".to_owned()));
        let publisher = RecordingPublisher::default();
        let parent = EventId::new();

        runner
            .execute(
                &config("hi %u%"),
                &RunContext::leaf(&stack, 0, parent, &publisher),
            )
            .await;

        assert_eq!(
            publisher.published(),
            vec![(
                EventSource::Kick,
                "chat.send".to_owned(),
                serde_json::json!({ "channel": "kick", "message": "hi alice" }),
                Some(parent),
            )]
        );
    }

    #[tokio::test]
    async fn an_undelivered_message_publishes_no_chat_send() {
        for (label, status, broadcaster_ids, template) in [
            (
                "kick rejects the send",
                reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                broadcaster_id_source(42),
                "hi",
            ),
            (
                "broadcaster id unresolved",
                reqwest::StatusCode::OK,
                failing_broadcaster_id_source(),
                "hi",
            ),
            (
                "empty after interpolation",
                reqwest::StatusCode::OK,
                broadcaster_id_source(42),
                "%u%",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/chat"))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
            let client = KickSendChat::new(crate::endpoints_test_support::api_at(&server.uri()));
            let runner = SendMessageRunner::new(Arc::new(client), token_source(), broadcaster_ids);
            let stack = ArgStack::new().set("u".to_owned(), Variant::String(String::new()));
            let publisher = RecordingPublisher::default();

            runner
                .execute(
                    &config(template),
                    &RunContext::leaf(&stack, 0, EventId::new(), &publisher),
                )
                .await;

            assert_eq!(publisher.published(), vec![], "{label}");
        }
    }
}
