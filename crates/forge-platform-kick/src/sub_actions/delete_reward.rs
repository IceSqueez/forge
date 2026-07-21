use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::rewards::KickRewards;

const KIND_ID: &str = "kick.reward.delete";

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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("reward_id".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "reward_id",
            label: "Reward ID",
            placeholder: "%reward_id%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("reward_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::InvalidConfig(format!(
                "{KIND_ID}: 'reward_id' must be a non-empty string"
            ))),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw_reward_id = config.str("reward_id").unwrap_or_default();
        let reward_id = ctx.arg_stack.interpolate(raw_reward_id);

        if reward_id.is_empty() {
            let outcome =
                SubActionOutcome::Failed("reward_id is empty after interpolation".to_owned());
            return (
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
            );
        }

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => match self.client.delete(&reward_id, &token).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            },
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_platform_core::{RateLimitOutcome, RateLimiter};
    use forge_types::EventId;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    struct GrantLimiter;
    #[async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }
        fn remaining(&self) -> u32 {
            120
        }
        async fn observe_remote_throttle(&self, _retry_after: Duration) {}
    }

    fn make_ctx(stack: &ArgStack) -> RunContext<'_> {
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> DeleteRewardRunner {
        let client = KickRewards::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
        DeleteRewardRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> DeleteRewardRunner {
        let client = KickRewards::new(Arc::new(GrantLimiter));
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
