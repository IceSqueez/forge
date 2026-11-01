use crate::step_failure;
use crate::vocabulary::sub_action_ids;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, StepCatalogEntry, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

pub(crate) struct SnoozeAdRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl SnoozeAdRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn snooze(&self) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/channels/ads/schedule/snooze")
            .query("broadcaster_id", user_id);
        step_failure::outcome(&self.transport.execute(request).await)
    }
}

impl StepCatalogEntry for SnoozeAdRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Integration
    }
    fn label(&self) -> &str {
        "Snooze Next Ad"
    }
    fn summary(&self) -> &str {
        "Delays the next scheduled ad break by 5 minutes."
    }
    fn search_text(&self) -> &str {
        "twitch ad snooze delay schedule next commercial"
    }
    fn icon_name(&self) -> &str {
        "clock-pause"
    }
}

#[async_trait]
impl SubActionRunner for SnoozeAdRunner {
    fn id(&self) -> &str {
        sub_action_ids::CHANNEL_SNOOZE_AD
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        let outcome = self.snooze().await;

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TwitchError;
    use crate::test_support::{
        MockTransport, SELF_USER_ID, TOKEN_SENTINEL, assert_request_refused, make_ctx,
    };
    use forge_types::ArgStack;
    use std::collections::BTreeMap;

    fn runner_with(
        response: Result<serde_json::Value, TwitchError>,
    ) -> (Arc<MockTransport>, SnoozeAdRunner) {
        crate::test_support::runner_with(vec![response], SnoozeAdRunner::new)
    }

    #[tokio::test]
    async fn execute_posts_self_broadcaster_in_query_with_no_body() {
        let (transport, runner) = runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new();

        let (telemetry, output) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(output.is_none());
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(request.path, "/helix/channels/ads/schedule/snooze");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.body.is_none(), "snooze must send no JSON body");
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_outcome_without_token() {
        let (_transport, runner) = runner_with(Err(TwitchError::Http {
            status: 429,
            body: format!("no snoozes remaining {TOKEN_SENTINEL}"),
        }));
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_request_refused(&telemetry.outcome, 429);
    }
}
