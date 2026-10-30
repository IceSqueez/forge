use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::enable_reward::{REWARD_CONFIG, REWARD_ID};
use crate::account::identity::SelfIdentity;
use crate::custom_rewards::delete_request;
use crate::helix::HelixTransport;
use crate::step_failure;
use crate::vocabulary::messages;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::CHANNEL_POINTS_DELETE_REWARD;

pub struct DeleteRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl DeleteRewardRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, reward_id: &str) -> SubActionOutcome {
        if reward_id.is_empty() {
            return SubActionOutcome::Failed(messages::field_required("reward_id").into());
        }
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = delete_request(user_id, reward_id);
        step_failure::outcome(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for DeleteRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Delete Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Permanently deletes a custom channel point reward."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward delete remove redemption"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(REWARD_CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let reward_id = match REWARD_ID.read(config, ctx.arg_stack) {
            Ok(reward_id) => reward_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.apply(&reward_id).await), None)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::helix::{HelixError, HelixMethod};
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, assert_request_refused, make_ctx,
    };

    fn delete_runner_with(
        response: Result<serde_json::Value, HelixError>,
    ) -> (Arc<MockTransport>, DeleteRewardRunner) {
        let transport = Arc::new(MockTransport::returning(response));
        let runner = DeleteRewardRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        (transport, runner)
    }

    #[tokio::test]
    async fn delete_issues_bodyless_delete_with_both_query_params() {
        let (transport, runner) = delete_runner_with(Ok(serde_json::Value::Null));
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw7".to_owned()));

        let (telemetry, out) = runner
            .execute(&REWARD_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert!(out.is_none(), "delete_reward never pushes an ArgStack");
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Delete);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned())),
            "missing broadcaster_id=self: {:?}",
            request.query
        );
        assert!(
            request.query.contains(&("id".to_owned(), "rw7".to_owned())),
            "id must be the interpolated reward_id: {:?}",
            request.query
        );
        assert_eq!(request.body, None, "DELETE custom_rewards carries no body");
    }

    #[tokio::test]
    async fn empty_reward_id_fails_without_helix_call() {
        let (transport, runner) = delete_runner_with(Ok(serde_json::Value::Null));
        let cfg = BTreeMap::from([("reward_id".to_owned(), Variant::String(String::new()))]);

        let (telemetry, _) = runner.execute(&cfg, &make_ctx(&ArgStack::new())).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            transport.call_count(),
            0,
            "empty reward_id must short-circuit before DELETE"
        );
    }

    #[tokio::test]
    async fn validate_config_requires_a_non_blank_reward_id_and_defaults_a_missing_one() {
        let (_transport, runner) = delete_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("present non-empty", REWARD_CONFIG.default_config(), true),
            (
                "empty string",
                BTreeMap::from([("reward_id".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            (
                "missing key reads the declared template",
                BTreeMap::new(),
                true,
            ),
            (
                "whitespace only",
                BTreeMap::from([("reward_id".to_owned(), Variant::String("  ".to_owned()))]),
                false,
            ),
        ];

        for (label, config, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&config).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }

    #[tokio::test]
    async fn helix_failure_maps_to_failed_without_token() {
        let (_transport, runner) = delete_runner_with(Err(HelixError::Http {
            status: 404,
            body: "not found".to_owned(),
        }));
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw7".to_owned()));

        let (telemetry, _) = runner
            .execute(&REWARD_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_request_refused(&telemetry.outcome, 404);
    }
}
