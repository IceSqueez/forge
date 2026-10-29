use crate::vocabulary::sub_action_ids;
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

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = sub_action_ids::CHAT_DELETE_MESSAGE;
const DEFAULT_MESSAGE_ID_TEMPLATE: &str = "%message_id%";
const UNRESOLVED_REFERENCE_MARKER: char = '%';

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
        if message_id.is_empty() {
            return SubActionOutcome::Failed("message_id is empty after interpolation".to_owned());
        }
        if message_id.contains(UNRESOLVED_REFERENCE_MARKER) {
            return SubActionOutcome::Failed(
                "message_id has an unresolved variable reference".to_owned(),
            );
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/moderation/chat")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id)
            .query("message_id", message_id);
        SubActionOutcome::from_result(&self.transport.execute(request).await)
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "message_id".to_owned(),
            Variant::String(DEFAULT_MESSAGE_ID_TEMPLATE.to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "message_id",
            label: "Message ID",
            placeholder: DEFAULT_MESSAGE_ID_TEMPLATE,
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::invalid_field(
                    "message_id",
                    ConfigReason::Required,
                ));
            }
        }
        Ok(())
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

        let outcome = self.delete(&message_id).await;

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
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, chat_trigger_args, make_ctx,
    };

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

        let SubActionOutcome::Failed(msg) = telemetry.outcome else {
            panic!("expected Failed, got {:?}", telemetry.outcome);
        };
        assert!(
            msg.contains("403"),
            "status must surface for diagnosis: {msg}"
        );
        assert!(
            !msg.contains(TOKEN_SENTINEL),
            "outcome must not leak the token: {msg}"
        );
        assert!(
            !msg.contains("api.twitch.tv"),
            "outcome must not leak the request URL: {msg}"
        );
    }
}
