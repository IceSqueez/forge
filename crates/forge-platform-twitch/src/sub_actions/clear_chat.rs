use crate::step_failure;
use crate::vocabulary::sub_action_ids;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = sub_action_ids::CHAT_CLEAR;

pub(crate) struct ClearChatRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ClearChatRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn clear(&self) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/moderation/chat")
            .query("broadcaster_id", user_id.clone())
            .query("moderator_id", user_id);
        step_failure::outcome(&self.transport.execute(request).await)
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
                duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
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
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, assert_request_refused, make_ctx,
    };
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, HelixError>,
        creds: forge_storage::test_support::MemoryCredentials,
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
                body: format!("moderator scope missing {TOKEN_SENTINEL}"),
            }),
            MockCreds::with_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_request_refused(&telemetry.outcome, 403);
    }
}
