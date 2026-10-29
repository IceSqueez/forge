use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use futures::future::BoxFuture;

use super::fields::REWARD_ID;
use crate::api::rewards::KickRewards;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::REWARD_DELETE;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&REWARD_ID]);

pub struct DeleteRewardRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl DeleteRewardRunner {
    pub fn new(
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
impl SubActionRunner for DeleteRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Delete Channel Reward"
    }

    fn summary(&self) -> &str {
        "Permanently deletes a Kick channel reward. Requires channel:rewards:write scope and reward ownership."
    }

    fn search_text(&self) -> &str {
        "kick channel reward delete remove points"
    }

    fn icon_name(&self) -> &str {
        "trash"
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
        let reward_id = match REWARD_ID.read(config, ctx.arg_stack) {
            Ok(reward_id) => reward_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        if reward_id.is_empty() {
            return (timer.failed("reward_id is empty after interpolation"), None);
        }

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => {
                SubActionOutcome::from_result(&self.client.delete(&reward_id, &token).await)
            }
        };
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> DeleteRewardRunner {
        let client = KickRewards::new(crate::endpoints_test_support::api_at(&server.uri()));
        DeleteRewardRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> DeleteRewardRunner {
        let client = KickRewards::new(crate::endpoints_test_support::unreachable_api());
        DeleteRewardRunner::new(Arc::new(client), token_source())
    }

    fn config(reward_id: &str) -> SubActionConfig {
        BTreeMap::from([(
            "reward_id".to_owned(),
            Variant::String(reward_id.to_owned()),
        )])
    }

    #[tokio::test]
    async fn execute_with_interpolated_reward_id_reaches_delete_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/channels/rewards/rw_7"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("rid".to_owned(), Variant::String("rw_7".to_owned()));
        let (telemetry, _) = runner.execute(&config("%rid%"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn empty_reward_id_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("rid".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner.execute(&config("%rid%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an empty resolved reward_id must not reach the transport"
        );
    }

    #[test]
    fn validate_config_requires_non_empty_reward_id() {
        let runner = runner_offline();
        assert!(runner.validate_config(&config("rw_1")).is_ok());
        assert!(runner.validate_config(&config("")).is_err());
    }
}
