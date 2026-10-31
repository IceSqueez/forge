use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::send_chat::YoutubeSendChat;
use crate::vocabulary::{config_keys, messages, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_DELETE_MESSAGE;
const MESSAGE_ID_VARIABLE: &str = "%message_id%";
const UNRESOLVED_REFERENCE_MARKER: char = '%';

const MESSAGE_ID: TextField = TextField {
    key: config_keys::MESSAGE_ID,
    label: "Message ID",
    editor: TextEditor::Line {
        placeholder: MESSAGE_ID_VARIABLE,
    },
    default: MESSAGE_ID_VARIABLE,
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MESSAGE_ID]);

pub(crate) struct DeleteMessageRunner {
    sender: Arc<YoutubeSendChat>,
}

impl DeleteMessageRunner {
    pub(crate) fn new(sender: Arc<YoutubeSendChat>) -> Self {
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let message_id = match MESSAGE_ID.read(config, ctx.arg_stack) {
            Ok(message_id) => message_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if message_id.contains(UNRESOLVED_REFERENCE_MARKER) {
            return (
                timer.failed(messages::unresolved_reference(config_keys::MESSAGE_ID).into()),
                None,
            );
        }

        let outcome = step_failure::outcome(&self.sender.delete(&message_id).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use forge_events::Event;
    use forge_events::test_support::NullPublisher;
    use forge_types::EventId;
    use futures::future::BoxFuture;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use forge_registry::TriggerKindDescriptor;

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NullPublisher)
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
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::NO_CONTENT))
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
                .respond_with(ResponseTemplate::new(reqwest::StatusCode::NO_CONTENT))
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
        let required = crate::sub_actions::rejected_config(
            KIND_ID,
            forge_registry::ConfigProblem::field(
                crate::vocabulary::config_keys::MESSAGE_ID,
                forge_registry::ConfigReason::Required,
            ),
        );
        let unresolved = SubActionOutcome::Failed(
            crate::vocabulary::messages::unresolved_reference(
                crate::vocabulary::config_keys::MESSAGE_ID,
            )
            .into(),
        );
        for (template, stack, expected) in [
            ("%mid%", blank, required),
            ("%message_id%", ArgStack::new(), unresolved.clone()),
            ("prefix-%missing%", ArgStack::new(), unresolved),
        ] {
            let (telemetry, _) = runner.execute(&config(template), &make_ctx(&stack)).await;

            assert_eq!(telemetry.outcome, expected, "template {template:?}");
        }
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an unusable message_id must not reach the delete transport"
        );
    }

    #[test]
    fn validate_config_accepts_an_id_or_a_missing_key_and_rejects_blank_or_non_string_ids() {
        let runner = DeleteMessageRunner::new(Arc::new(YoutubeSendChat::new(
            crate::endpoints_test_support::unreachable_data_api(),
            token_source(),
            LiveChatIdHandle::new(),
        )));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("non-empty id", config("msg-1"), true),
            ("empty id", config(""), false),
            ("whitespace-only id", config(" \t "), false),
            (
                "missing id falls back to the trigger message id",
                BTreeMap::new(),
                true,
            ),
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
