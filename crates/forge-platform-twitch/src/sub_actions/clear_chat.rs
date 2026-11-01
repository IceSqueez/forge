use crate::step_failure;
use crate::vocabulary::sub_action_ids;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, StepCatalogEntry, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

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

impl StepCatalogEntry for ClearChatRunner {
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
}

#[async_trait]
impl SubActionRunner for ClearChatRunner {
    fn id(&self) -> &str {
        sub_action_ids::CHAT_CLEAR
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        let outcome = self.clear().await;

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TwitchError;
    use crate::test_support::{
        MockTransport, SELF_USER_ID, TOKEN_SENTINEL, assert_request_refused, make_ctx,
        self_identity,
    };
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, TwitchError>,
        identity: Arc<SelfIdentity>,
    ) -> (Arc<MockTransport>, ClearChatRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner =
            ClearChatRunner::new(Arc::clone(&transport) as Arc<dyn HelixTransport>, identity);
        (transport, runner)
    }

    #[tokio::test]
    async fn execute_clears_whole_chat_by_omitting_message_id_query() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null), self_identity());
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
            Err(TwitchError::Http {
                status: 403,
                body: format!("moderator scope missing {TOKEN_SENTINEL}"),
            }),
            self_identity(),
        );
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_request_refused(&telemetry.outcome, 403);
    }
}
