use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{ChatSent, EventSource, TypedEvent};
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, PlatformId, SubActionOutcome, SubActionTelemetry};

use super::step_failure;
use crate::send_chat::YoutubeSendChat;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_SEND_MESSAGE;

const MESSAGE: TextField = TextField {
    key: config_keys::MESSAGE,
    label: "Message",
    editor: TextEditor::Area,
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MESSAGE]);

pub(crate) struct SendMessageRunner {
    sender: Arc<YoutubeSendChat>,
}

impl SendMessageRunner {
    pub(crate) fn new(sender: Arc<YoutubeSendChat>) -> Self {
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let message = match MESSAGE.read(config, ctx.arg_stack) {
            Ok(message) => message,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let outcome = step_failure::outcome(&self.sender.send(&message).await);
        if matches!(outcome, SubActionOutcome::Success) {
            ChatSent {
                channel: PlatformId::YouTube.as_str().to_owned(),
                message,
                reply_to_message_id: None,
                sent_as: None,
            }
            .publish(
                ctx.publisher,
                EventSource::YouTube,
                Some(ctx.parent_event_id),
            );
        }
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use forge_events::test_support::NullPublisher;
    use std::sync::Arc;

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    const TOKEN_SENTINEL: &str = "yt-secret-token";

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
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
    }

    fn token_source() -> Arc<
        dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
            + Send
            + Sync,
    > {
        Arc::new(|| Box::pin(async { Ok(TOKEN_SENTINEL.to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> SendMessageRunner {
        let handle = LiveChatIdHandle::new();
        handle.set(Some("lc-test".to_owned()));
        let sender = YoutubeSendChat::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token_source(),
            handle,
        );
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
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(json!({"kind": "x"})),
            )
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
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("greeting".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%greeting%"), &make_ctx(&stack))
            .await;

        assert_eq!(
            telemetry.outcome,
            crate::sub_actions::rejected_config(
                &telemetry.kind,
                forge_registry::ConfigProblem::field(
                    crate::vocabulary::config_keys::MESSAGE,
                    forge_registry::ConfigReason::Required
                )
            )
        );
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
                ResponseTemplate::new(reqwest::StatusCode::FORBIDDEN)
                    .set_body_string("forbidden: insufficient scope"),
            )
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("hello"), &make_ctx(&stack)).await;

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        let forge_types::UserText::Message(message) = &msg else {
            panic!("expected a catalogued message, got {msg:?}");
        };
        assert_eq!(message.code().to_string(), "youtube.request_refused");
        assert_eq!(
            message.arg("status"),
            Some(&forge_types::MessageArg::Int(403))
        );
        assert!(
            !serde_json::to_string(&msg)
                .unwrap()
                .contains(TOKEN_SENTINEL),
            "outcome must not leak the bearer token: {msg:?}"
        );
        assert!(
            !serde_json::to_string(&msg).unwrap().contains(&server.uri()),
            "outcome must not leak the request URL: {msg:?}"
        );
        assert!(
            !serde_json::to_string(&msg)
                .unwrap()
                .to_lowercase()
                .contains("googleapis"),
            "outcome must not leak the API host: {msg:?}"
        );
    }

    #[test]
    fn validate_config_rejects_empty_or_non_string_message_and_accepts_valid() {
        let server_uri = "http://127.0.0.1:0".to_owned();
        let handle = LiveChatIdHandle::new();
        let sender = YoutubeSendChat::new(
            crate::endpoints_test_support::data_api_at(&server_uri),
            token_source(),
            handle,
        );
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

    #[tokio::test]
    async fn a_delivered_message_publishes_chat_send_caused_by_the_run() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/messages"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK).set_body_json(json!({"kind": "x"})),
            )
            .mount(&server)
            .await;
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("viewer42".to_owned()));
        let publisher = RecordingPublisher::default();
        let parent = EventId::new();

        runner
            .execute(
                &config("Welcome %user%!"),
                &RunContext::leaf(&stack, 0, parent, &publisher),
            )
            .await;

        assert_eq!(
            publisher.published(),
            vec![(
                EventSource::YouTube,
                "chat.send".to_owned(),
                json!({ "channel": "youtube", "message": "Welcome viewer42!" }),
                Some(parent),
            )]
        );
    }

    #[tokio::test]
    async fn an_undelivered_message_publishes_no_chat_send() {
        for (label, status, template) in [
            (
                "youtube rejects the send",
                reqwest::StatusCode::FORBIDDEN,
                "hello",
            ),
            (
                "empty after interpolation",
                reqwest::StatusCode::OK,
                "%greeting%",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/liveChat/messages"))
                .respond_with(ResponseTemplate::new(status).set_body_json(json!({"kind": "x"})))
                .mount(&server)
                .await;
            let runner = runner_on(&server);
            let stack = ArgStack::new().set("greeting".to_owned(), Variant::String(String::new()));
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
