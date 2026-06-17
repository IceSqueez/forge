use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use crate::rewards::KickRewards;

const KIND_ID: &str = "kick.reward.redemption_accept";
const MAX_BATCH: usize = 25;

pub struct AcceptRedemptionRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl AcceptRedemptionRunner {
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

fn parse_ids(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

#[async_trait]
impl SubActionRunner for AcceptRedemptionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Accept Reward Redemption(s)"
    }

    fn summary(&self) -> &str {
        "Accepts one or more pending Kick reward redemptions. Requires channel:rewards:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel reward redemption accept approve fulfill points"
    }

    fn icon_name(&self) -> &str {
        "check"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "redemption_ids".to_owned(),
            Variant::String("%redemption_id%".to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "redemption_ids",
            label: "Redemption ID(s)",
            placeholder: "%redemption_id% or id1,id2,id3 (max 25)",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let raw = config
            .get("redemption_ids")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if raw.trim().is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'redemption_ids' must not be empty"
            )));
        }
        let ids = parse_ids(raw);
        if ids.len() > MAX_BATCH {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: config contains {} ids, maximum is {MAX_BATCH}",
                ids.len()
            )));
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw = config
            .get("redemption_ids")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let interpolated = ctx.arg_stack.interpolate(raw);
        let ids = parse_ids(&interpolated);

        if ids.is_empty() {
            let outcome =
                SubActionOutcome::Failed("redemption_ids is empty after interpolation".to_owned());
            return (
                SubActionTelemetry {
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
            Ok(token) => match self.client.accept_redemptions(&ids, &token).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
            },
        };

        (
            SubActionTelemetry {
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
        RunContext {
            arg_stack: stack,
            index: 0,
            parent_event_id: EventId::new(),
            publisher: &NoopPublisher,
        }
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> AcceptRedemptionRunner {
        let client = KickRewards::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
        AcceptRedemptionRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> AcceptRedemptionRunner {
        let client = KickRewards::new(Arc::new(GrantLimiter));
        AcceptRedemptionRunner::new(Arc::new(client), token_source())
    }

    fn config(ids: &str) -> SubActionConfig {
        BTreeMap::from([("redemption_ids".to_owned(), Variant::String(ids.to_owned()))])
    }

    async fn last_body(server: &MockServer) -> serde_json::Value {
        let reqs = server.received_requests().await.unwrap();
        serde_json::from_slice(&reqs.last().unwrap().body).unwrap()
    }

    #[tokio::test]
    async fn execute_with_single_interpolated_id_reaches_accept_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards/redemptions/accept"))
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
            .and(path("/channels/rewards/redemptions/accept"))
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

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "empty resolved redemption_ids must not reach the transport"
        );
    }

    #[test]
    fn validate_config_enforces_presence_and_batch_limit() {
        let runner = runner_offline();
        let twenty_five = (0..25)
            .map(|i| format!("r{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let twenty_six = (0..26)
            .map(|i| format!("r{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let cases: Vec<(&str, String, bool)> = vec![
            ("empty", String::new(), false),
            ("whitespace", "   ".to_owned(), false),
            ("single id", "rd_1".to_owned(), true),
            ("25 ids at limit", twenty_five, true),
            ("26 ids over limit", twenty_six, false),
        ];
        for (label, ids, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&config(&ids)).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }
}
