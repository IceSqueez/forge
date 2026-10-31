use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextField, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::TARGET_LOGIN;
use crate::account::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::lifecycle::TwitchLifecycle;
use crate::step_failure;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::CHANNEL_START_RAID;

const TO_BROADCASTER_LOGIN: TextField = TextField {
    key: config_keys::TO_BROADCASTER_LOGIN,
    label: "Target Channel",
    ..TARGET_LOGIN
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TO_BROADCASTER_LOGIN]);

pub(crate) struct StartRaidRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
    lifecycle: TwitchLifecycle,
}

impl StartRaidRunner {
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

    async fn start_raid(&self, to_broadcaster_login: &str) -> SubActionOutcome {
        let self_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let to_broadcaster_id =
            match resolve_user_id(self.transport.as_ref(), to_broadcaster_login).await {
                Ok(id) => id,
                Err(e) => return step_failure::failed(&e),
            };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/raids")
            .query("from_broadcaster_id", self_id)
            .query("to_broadcaster_id", to_broadcaster_id);
        let outcome = step_failure::outcome(&self.transport.execute(request).await);
        if outcome == SubActionOutcome::Success {
            self.lifecycle.raid_started();
        }
        outcome
    }
}

#[async_trait]
impl SubActionRunner for StartRaidRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Start Raid"
    }

    fn summary(&self) -> &str {
        "Starts a raid to another broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch raid start send viewers channel"
    }

    fn icon_name(&self) -> &str {
        "raid"
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
        let to_broadcaster_login = match TO_BROADCASTER_LOGIN.read(config, ctx.arg_stack) {
            Ok(login) => login,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (
            timer.finish(self.start_raid(&to_broadcaster_login).await),
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockTransport, SELF_USER_ID, assert_request_refused, make_ctx, users_fixture,
    };
    use forge_platform_core::QuickActionLiveness;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner_with(
        responses: Vec<Result<serde_json::Value, HelixError>>,
    ) -> (Arc<MockTransport>, StartRaidRunner) {
        runner_tracking(responses, TwitchLifecycle::new())
    }

    fn runner_tracking(
        responses: Vec<Result<serde_json::Value, HelixError>>,
        lifecycle: TwitchLifecycle,
    ) -> (Arc<MockTransport>, StartRaidRunner) {
        let transport = Arc::new(MockTransport::returning_sequence(responses));
        let runner = StartRaidRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            crate::sub_actions::test_support::self_identity(),
            lifecycle,
        );
        (transport, runner)
    }

    fn config(login: &str) -> SubActionConfig {
        BTreeMap::from([(
            "to_broadcaster_login".to_owned(),
            Variant::String(login.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_resolves_login_then_posts_raid_from_self_to_resolved_id() {
        let (transport, runner) =
            runner_with(vec![users_fixture("555"), Ok(serde_json::Value::Null)]);
        let stack = ArgStack::new().set(
            "user_login".to_owned(),
            Variant::String("target".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&config("%user_login%"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(transport.call_count(), 2, "resolve then raid");

        let resolve = transport.request(0);
        assert_eq!(resolve.method, HelixMethod::Get);
        assert_eq!(resolve.path, "/helix/users");
        assert!(
            resolve
                .query
                .contains(&("login".to_owned(), "target".to_owned())),
            "resolve must look up the interpolated login: {:?}",
            resolve.query
        );

        let act = transport.last_request();
        assert_eq!(act.method, HelixMethod::Post);
        assert_eq!(act.path, "/helix/raids");
        assert!(
            act.query
                .contains(&("from_broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "from must be self: {:?}",
            act.query
        );
        assert!(
            act.query
                .contains(&("to_broadcaster_id".to_owned(), "555".to_owned())),
            "to must be the RESOLVED id, not the login: {:?}",
            act.query
        );
        assert_eq!(act.body, None, "raid carries no JSON body");
    }

    #[tokio::test]
    async fn empty_login_after_interpolation_fails_before_any_helix_call() {
        let (transport, runner) = runner_with(vec![users_fixture("555")]);
        let stack = ArgStack::new().set("user_login".to_owned(), Variant::String(String::new()));

        let (telemetry, _) = runner
            .execute(&config("%user_login%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty login must fail before the resolve call"
        );
    }

    #[tokio::test]
    async fn resolve_failure_skips_the_raid_call() {
        let (transport, runner) = runner_with(vec![Err(HelixError::Http {
            status: 404,
            body: "user not found".to_owned(),
        })]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("ghost"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            1,
            "resolve failure must not issue the raid POST"
        );
    }

    #[tokio::test]
    async fn raid_http_failure_maps_to_failed_without_token_or_url() {
        let (transport, runner) = runner_with(vec![
            users_fixture("555"),
            Err(HelixError::Http {
                status: 409,
                body: "raid already pending".to_owned(),
            }),
        ]);
        let stack = ArgStack::new();

        let (telemetry, _) = runner.execute(&config("target"), &make_ctx(&stack)).await;

        assert_eq!(
            transport.call_count(),
            2,
            "failure must come from the raid call, not the resolve"
        );
        assert_request_refused(&telemetry.outcome, 409);
    }

    #[tokio::test]
    async fn a_started_raid_records_a_pending_raid() {
        let lifecycle = TwitchLifecycle::new();
        let (_, runner) = runner_tracking(
            vec![users_fixture("555"), Ok(serde_json::Value::Null)],
            lifecycle.clone(),
        );
        let stack = ArgStack::new();

        runner.execute(&config("target"), &make_ctx(&stack)).await;

        assert_eq!(
            lifecycle.snapshot().raid_in_flight(),
            QuickActionLiveness::Live
        );
    }

    #[tokio::test]
    async fn a_rejected_raid_records_no_pending_raid() {
        let lifecycle = TwitchLifecycle::new();
        let (_, runner) = runner_tracking(
            vec![
                users_fixture("555"),
                Err(HelixError::Http {
                    status: 409,
                    body: "raid already pending".to_owned(),
                }),
            ],
            lifecycle.clone(),
        );
        let stack = ArgStack::new();

        runner.execute(&config("target"), &make_ctx(&stack)).await;

        assert_eq!(
            lifecycle.snapshot().raid_in_flight(),
            QuickActionLiveness::Unknown
        );
    }
}
