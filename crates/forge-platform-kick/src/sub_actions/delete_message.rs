use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use futures::future::BoxFuture;

use super::step_failure;
use crate::api::chat_messages::KickSendChat;
use crate::vocabulary::messages;
use crate::vocabulary::variables::references;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_DELETE_MESSAGE;
const UNRESOLVED_REFERENCE_MARKER: char = '%';

const MESSAGE_ID: TextField = TextField {
    key: config_keys::MESSAGE_ID,
    label: "Message ID",
    editor: TextEditor::Line {
        placeholder: references::MESSAGE_ID,
    },
    default: references::MESSAGE_ID,
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MESSAGE_ID]);

pub(crate) struct DeleteMessageRunner {
    client: Arc<KickSendChat>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl DeleteMessageRunner {
    pub(crate) fn new(
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

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(step_failure::token_failed(&e)),
            Ok(token) => step_failure::outcome(&self.client.delete(&message_id, &token).await),
        };
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{make_ctx, token_source};
    use forge_events::Event;

    use forge_types::Variant;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

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
        for (case, step_config, stack, expected_code) in [
            (
                "empty id",
                config("%message_id%"),
                chat_trigger_args(""),
                "config.field_problem",
            ),
            (
                "no chat trigger in scope",
                config("%message_id%"),
                ArgStack::new(),
                "kick.unresolved_reference",
            ),
            (
                "missing key and no chat trigger in scope",
                BTreeMap::new(),
                ArgStack::new(),
                "kick.unresolved_reference",
            ),
            (
                "unresolved reference beside a resolved one",
                config("%message_id%-%typo_id%"),
                chat_trigger_args("abc-123"),
                "kick.unresolved_reference",
            ),
        ] {
            let server = MockServer::start().await;
            let runner = runner_on(&server);

            let (telemetry, _) = runner.execute(&step_config, &make_ctx(&stack)).await;

            assert!(
                matches!(&telemetry.outcome, SubActionOutcome::Failed(reason)
                    if reason.code_or_text() == expected_code),
                "{case}: {:?}",
                telemetry.outcome
            );
            assert!(
                server.received_requests().await.unwrap().is_empty(),
                "{case}"
            );
        }
    }

    #[test]
    fn validate_config_accepts_an_id_or_a_missing_key_and_rejects_blank_or_non_string_ids() {
        let runner = DeleteMessageRunner::new(
            Arc::new(KickSendChat::new(
                crate::endpoints_test_support::unreachable_api(),
            )),
            token_source(),
        );

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("non-empty", config("msg-1"), true),
            ("empty", config(""), false),
            ("whitespace only", config(" \t "), false),
            (
                "missing key falls back to the trigger message id",
                BTreeMap::new(),
                true,
            ),
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
