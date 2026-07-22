use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::enable_reward::{config_fields, default_config, validate_reward_id};
use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel_points.delete_reward";

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

    fn default_config(&self) -> SubActionConfig {
        default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_reward_id(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let reward_id_template = config.str("reward_id").unwrap_or_default();
        let reward_id = ctx.arg_stack.interpolate(reward_id_template);

        let outcome = if reward_id.is_empty() {
            SubActionOutcome::Failed("reward_id is required".to_owned())
        } else {
            self.apply(&reward_id).await
        };

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

impl DeleteRewardRunner {
    async fn apply(&self, reward_id: &str) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };

        let request =
            HelixRequest::new(HelixMethod::Delete, "/helix/channel_points/custom_rewards")
                .query("broadcaster_id", user_id)
                .query("id", reward_id.to_owned());

        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;

    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::helix::HelixError;
    use crate::sub_actions::test_support::{
        MockCreds, MockTransport, SELF_USER_ID, TOKEN_SENTINEL, make_ctx,
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

        let (telemetry, out) = runner.execute(&default_config(), &make_ctx(&stack)).await;

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
    async fn validate_config_requires_non_empty_reward_id() {
        let (_transport, runner) = delete_runner_with(Ok(serde_json::Value::Null));

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("present non-empty", default_config(), true),
            (
                "empty string",
                BTreeMap::from([("reward_id".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            ("missing key", BTreeMap::new(), false),
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

        let (telemetry, _) = runner.execute(&default_config(), &make_ctx(&stack)).await;

        assert!(matches!(
            telemetry.outcome,
            SubActionOutcome::Failed(msg) if !msg.contains(TOKEN_SENTINEL)
        ));
    }
}
