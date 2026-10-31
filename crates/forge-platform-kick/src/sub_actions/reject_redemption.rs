use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use futures::future::BoxFuture;

use super::fields::REDEMPTION_IDS;
use super::step_failure;
use crate::api::rewards::KickRewards;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::REWARD_REDEMPTION_REJECT;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&REDEMPTION_IDS]);

pub(crate) struct RejectRedemptionRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl RejectRedemptionRunner {
    pub(crate) fn new(
        client: Arc<KickRewards>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

#[async_trait]
impl SubActionRunner for RejectRedemptionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Reject Reward Redemption(s)"
    }

    fn summary(&self) -> &str {
        "Rejects one or more pending Kick reward redemptions. Requires channel:rewards:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel reward redemption reject cancel deny points"
    }

    fn icon_name(&self) -> &str {
        "x-circle"
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
        let ids = match REDEMPTION_IDS.read(config, ctx.arg_stack) {
            Ok(ids) => ids,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(step_failure::token_failed(&e)),
            Ok(token) => step_failure::outcome(&self.client.reject_redemptions(&ids, &token).await),
        };
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sub_actions::test_support::{make_ctx, token_source};

    use forge_types::Variant;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn runner_on(server: &MockServer) -> RejectRedemptionRunner {
        let client = KickRewards::new(crate::endpoints_test_support::api_at(&server.uri()));
        RejectRedemptionRunner::new(Arc::new(client), token_source())
    }

    fn config(ids: &str) -> SubActionConfig {
        BTreeMap::from([("redemption_ids".to_owned(), Variant::String(ids.to_owned()))])
    }

    async fn last_body(server: &MockServer) -> serde_json::Value {
        let reqs = server.received_requests().await.unwrap();
        serde_json::from_slice(&reqs.last().unwrap().body).unwrap()
    }

    #[tokio::test]
    async fn execute_with_single_interpolated_id_reaches_reject_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards/redemptions/reject"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let stack = ArgStack::new().set(
            "redemption_id".to_owned(),
            Variant::String("rd_7".to_owned()),
        );
        let (telemetry, _) = runner_on(&server)
            .execute(&config("%redemption_id%"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn execute_with_comma_separated_batch_sends_all_parsed_ids() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards/redemptions/reject"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let (telemetry, _) = runner_on(&server)
            .execute(&config("a, b, c"), &make_ctx(&ArgStack::new()))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let body = last_body(&server).await;
        assert_eq!(body["ids"], serde_json::json!(["a", "b", "c"]));
    }

    #[tokio::test]
    async fn execute_empty_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let stack = ArgStack::new().set("x".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner_on(&server)
            .execute(&config("%x%"), &make_ctx(&stack))
            .await;

        assert_eq!(
            telemetry.outcome,
            SubActionOutcome::Failed(super::super::test_support::step_problem(
                "kick.reward.redemption_reject",
                &forge_registry::ConfigProblem::field(
                    "redemption_ids",
                    forge_registry::ConfigReason::Required
                )
            ))
        );
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty resolved redemption_ids must not reach the transport"
        );
    }
}
