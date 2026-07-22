use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.chat.clear";

pub struct ClearChatRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ClearChatRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn clear(&self) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/moderation/chat")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id);
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for ClearChatRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Clear Chat"
    }

    fn summary(&self) -> &str {
        "Clears all messages from Twitch chat."
    }

    fn search_text(&self) -> &str {
        "twitch chat clear wipe moderation purge"
    }

    fn icon_name(&self) -> &str {
        "eraser"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = self.clear().await;

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
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
    };

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
        creds: MockCreds,
    ) -> (Arc<MockTransport>, ClearChatRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = ClearChatRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(creds))),
        );
        (transport, runner)
    }

    #[tokio::test]
    async fn execute_clears_whole_chat_by_omitting_message_id_query() {
        let (transport, runner) =
            runner_with(Ok(serde_json::Value::Null), MockCreds::with_identity());
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

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
            request.query.iter().all(|(key, _)| key != "message_id"),
            "clear must not scope the delete to one message: {:?}",
            request.query
        );
    }

    #[tokio::test]
    async fn helix_http_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(
            Err(HelixError::Http {
                status: 403,
                body: "moderator scope missing".to_owned(),
            }),
            MockCreds::with_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if msg.contains("403") && !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
