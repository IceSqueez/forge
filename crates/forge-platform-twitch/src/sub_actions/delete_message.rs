use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::ResolvedIdField;
use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHAT_DELETE_MESSAGE;

const MESSAGE_ID: ResolvedIdField = ResolvedIdField {
    key: config_keys::MESSAGE_ID,
    label: "Message ID",
    default: "%message_id%",
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&MESSAGE_ID]);

pub struct DeleteMessageRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl DeleteMessageRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn delete(&self, message_id: &str) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/moderation/chat")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .query("message_id", message_id);
        step_failure::outcome(&self.transport.execute(request).await)
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
        "Deletes a specific message from Twitch chat."
    }

    fn search_text(&self) -> &str {
        "twitch chat delete remove message moderation"
    }

    fn icon_name(&self) -> &str {
        "trash"
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
        (timer.finish(self.delete(&message_id).await), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_request_refused, chat_trigger_args, make_ctx,
    };
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
        creds: MockCreds,
    ) -> (Arc<MockTransport>, DeleteMessageRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = DeleteMessageRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(creds))),
        );
        (transport, runner)
    }

    fn config(message_id: &str) -> SubActionConfig {
        BTreeMap::from([(
            "message_id".to_owned(),
            Variant::String(message_id.to_owned()),
        )])
    }

    #[tokio::test]
    async fn default_config_deletes_the_message_that_fired_the_chat_trigger_as_self_moderator() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = chat_trigger_args("abc-123");

        let (telemetry, _) = runner
            .execute(&runner.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Delete);
        assert_eq!(request.path, "/helix/moderation/chat");
        for (key, value) in [
            ("broadcaster_id", SELF_USER_ID),
            ("moderator_id", SELF_USER_ID),
            ("message_id", "abc-123"),
        ] {
            assert!(
                request.query.contains(&(key.to_owned(), value.to_owned())),
                "query '{key}'"
            );
        }
    }

    #[tokio::test]
    async fn a_message_id_that_does_not_resolve_to_an_id_fails_without_a_helix_call() {
        for (case, stack) in [
            ("empty id", chat_trigger_args("")),
            ("no chat trigger in scope", ArgStack::new()),
        ] {
            let (transport, runner) =
                runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());

            let (telemetry, _) = runner
                .execute(&runner.default_config(), &make_ctx(&stack))
                .await;

            assert!(
                matches!(telemetry.outcome, SubActionOutcome::Failed(_)),
                "{case}"
            );
            assert_eq!(transport.call_count(), 0, "{case}");
        }
    }

    #[tokio::test]
    async fn a_partially_resolved_message_id_fails_without_a_helix_call() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = chat_trigger_args("abc-123");

        let (telemetry, _) = runner
            .execute(&config("%message_id%-%typo_id%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(transport.call_count(), 0);
    }

    #[tokio::test]
    async fn helix_http_failure_maps_to_failed_outcome_without_token_or_url() {
        let (_transport, runner) = runner_with(
            Err(HelixError::Http {
                status: 403,
                body: "moderator scope missing".to_owned(),
            }),
            MockCreds::with_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("abc-123"), &make_ctx(&stack)).await;

        assert_request_refused(&telemetry.outcome, 403);
    }
}
