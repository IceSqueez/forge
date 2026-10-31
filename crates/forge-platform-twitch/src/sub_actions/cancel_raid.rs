use crate::step_failure;
use crate::vocabulary::sub_action_ids;
use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::account::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::lifecycle::TwitchLifecycle;

pub(crate) struct CancelRaidRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
    lifecycle: TwitchLifecycle,
}

impl CancelRaidRunner {
    pub(crate) fn new(
        transport: Arc<dyn HelixTransport>,
        identity: Arc<SelfIdentity>,
        lifecycle: TwitchLifecycle,
    ) -> Self {
        Self {
            transport,
            identity,
            lifecycle,
        }
    }

    async fn cancel_raid(&self) -> SubActionOutcome {
        let self_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request =
            HelixRequest::new(HelixMethod::Delete, "/helix/raids").query("broadcaster_id", self_id);
        let outcome = step_failure::outcome(&self.transport.execute(request).await);
        if outcome == SubActionOutcome::Success {
            self.lifecycle.raid_canceled();
        }
        outcome
    }
}

#[async_trait]
impl SubActionRunner for CancelRaidRunner {
    fn id(&self) -> &str {
        sub_action_ids::CHANNEL_CANCEL_RAID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Cancel Raid"
    }

    fn summary(&self) -> &str {
        "Cancels a pending raid initiated from this channel."
    }

    fn search_text(&self) -> &str {
        "twitch raid cancel abort stop"
    }

    fn icon_name(&self) -> &str {
        "raid-cancel"
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        let outcome = self.cancel_raid().await;

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::TwitchError;
    use crate::sub_actions::test_support::{
        MockTransport, SELF_USER_ID, TOKEN_SENTINEL, assert_request_refused, make_ctx,
    };
    use forge_platform_core::QuickActionLiveness;
    use forge_types::ArgStack;
    use std::collections::BTreeMap;

    fn runner_with(
        responses: Vec<Result<serde_json::Value, TwitchError>>,
    ) -> (Arc<MockTransport>, CancelRaidRunner) {
        runner_tracking(responses, TwitchLifecycle::new())
    }

    fn runner_tracking(
        responses: Vec<Result<serde_json::Value, TwitchError>>,
        lifecycle: TwitchLifecycle,
    ) -> (Arc<MockTransport>, CancelRaidRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = CancelRaidRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            crate::sub_actions::test_support::self_identity(),
            lifecycle,
        );
        (transport, runner)
    }

    #[tokio::test]
    async fn execute_deletes_raid_scoped_to_self_broadcaster() {
        let (transport, runner) = runner_with(vec![Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.call_count(),
            1,
            "cancel issues a single call, no login resolution"
        );
        let act = transport.last_request();
        assert_eq!(act.method, HelixMethod::Delete);
        assert_eq!(act.path, "/helix/raids");
        assert_eq!(
            act.query,
            vec![("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())],
            "only broadcaster_id=self may be sent"
        );
        assert_eq!(act.body, None, "DELETE carries no JSON body");
    }

    #[tokio::test]
    async fn cancel_http_failure_maps_to_failed_without_token_or_url() {
        let (transport, runner) = runner_with(vec![Err(TwitchError::Http {
            status: 404,
            body: format!("no raid in progress {TOKEN_SENTINEL}"),
        })]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(transport.call_count(), 1);
        assert_request_refused(&telemetry.outcome, 404);
    }

    #[tokio::test]
    async fn a_successful_cancel_clears_the_pending_raid() {
        let lifecycle = TwitchLifecycle::new();
        lifecycle.raid_started();
        let (_, runner) = runner_tracking(vec![Ok(serde_json::Value::Null)], lifecycle.clone());
        let stack = ArgStack::new();

        runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(
            lifecycle.snapshot().raid_in_flight(),
            QuickActionLiveness::Unknown
        );
    }

    #[tokio::test]
    async fn a_rejected_cancel_leaves_the_pending_raid() {
        let lifecycle = TwitchLifecycle::new();
        lifecycle.raid_started();
        let (_, runner) = runner_tracking(
            vec![Err(TwitchError::Http {
                status: 404,
                body: format!("no raid in progress {TOKEN_SENTINEL}"),
            })],
            lifecycle.clone(),
        );
        let stack = ArgStack::new();

        runner.execute(&BTreeMap::new(), &make_ctx(&stack)).await;

        assert_eq!(
            lifecycle.snapshot().raid_in_flight(),
            QuickActionLiveness::Live
        );
    }
}
