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

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.chat.delete_message";
const DEFAULT_MESSAGE_ID_TEMPLATE: &str = "%chat.message_id%";

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
            placeholder: "%chat.message_id%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("message_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::InvalidConfig(format!(
                    "{KIND_ID}: 'message_id' must be a non-empty string"
                )));
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
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
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
    async fn execute_deletes_interpolated_message_id_as_self_moderator() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new().set(
            "chat.message_id".to_owned(),
            Variant::String("abc-123".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&config("%chat.message_id%"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.last_request();
        assert_eq!(request.method, HelixMethod::Delete);
        assert_eq!(request.path, "/helix/moderation/chat");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(
            request
                .query
                .contains(&("moderator_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(
            request
                .query
                .contains(&("message_id".to_owned(), "abc-123".to_owned())),
            "message_id query must carry the interpolated value"
        );
    }

    #[tokio::test]
    async fn empty_message_id_after_interpolation_fails_without_transport_call() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack =
            ArgStack::new().set("chat.message_id".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%chat.message_id%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty message_id must not reach Helix"
        );
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
