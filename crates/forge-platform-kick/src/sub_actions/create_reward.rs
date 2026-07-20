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

use crate::rewards::{CreateRewardParams, KickRewards};

const KIND_ID: &str = "kick.reward.create";

pub struct CreateRewardRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl CreateRewardRunner {
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
impl SubActionRunner for CreateRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Create Channel Reward"
    }

    fn summary(&self) -> &str {
        "Creates a new channel point reward on Kick. Requires channel:rewards:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel reward create points redeem"
    }

    fn icon_name(&self) -> &str {
        "gift"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(String::new())),
            ("cost".to_owned(), Variant::String(String::new())),
            ("description".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "title",
                label: "Reward Title",
                placeholder: "e.g. Hydrate",
            },
            FormField::Text {
                key: "cost",
                label: "Cost (channel points)",
                placeholder: "e.g. 500",
            },
            FormField::Text {
                key: "description",
                label: "Description (optional)",
                placeholder: "Leave empty to omit",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if title.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'title' must be a non-empty string"
            )));
        }

        let cost_raw = config
            .get("cost")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if cost_raw.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'cost' must be provided"
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

        let raw_title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_cost = config
            .get("cost")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let raw_description = config
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        let title = ctx.arg_stack.interpolate(raw_title);
        let cost_str = ctx.arg_stack.interpolate(raw_cost);
        let description_str = ctx.arg_stack.interpolate(raw_description);

        if title.is_empty() {
            let outcome = SubActionOutcome::Failed("title is empty after interpolation".to_owned());
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

        let cost = match cost_str.parse::<u64>() {
            Ok(n) if n >= 1 => n,
            Ok(_) => {
                let outcome = SubActionOutcome::Failed("cost must be at least 1".to_owned());
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
            Err(_) => {
                let outcome = SubActionOutcome::Failed(format!(
                    "cost '{cost_str}' is not a valid positive integer"
                ));
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
        };

        let description = if description_str.is_empty() {
            None
        } else {
            Some(description_str)
        };

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => match self
                .client
                .create(
                    CreateRewardParams {
                        title,
                        cost,
                        description,
                        background_color: None,
                        is_enabled: None,
                        is_user_input_required: None,
                        should_redemptions_skip_request_queue: None,
                    },
                    &token,
                )
                .await
            {
                Ok(_created_id) => SubActionOutcome::Success,
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

    fn runner_on(server: &MockServer) -> CreateRewardRunner {
        let client = KickRewards::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
        CreateRewardRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> CreateRewardRunner {
        let client = KickRewards::new(Arc::new(GrantLimiter));
        CreateRewardRunner::new(Arc::new(client), token_source())
    }

    fn config(title: &str, cost: &str, description: &str) -> SubActionConfig {
        BTreeMap::from([
            ("title".to_owned(), Variant::String(title.to_owned())),
            ("cost".to_owned(), Variant::String(cost.to_owned())),
            (
                "description".to_owned(),
                Variant::String(description.to_owned()),
            ),
        ])
    }

    #[tokio::test]
    async fn execute_with_interpolated_title_and_cost_reaches_server_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/channels/rewards"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "x"})))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("g".to_owned(), Variant::String("Hydrate".to_owned()));

        let (telemetry, _) = runner
            .execute(&config("%g%", "500", ""), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let body = server.received_requests().await.unwrap();
        assert_eq!(body.len(), 1);
    }

    #[tokio::test]
    async fn non_numeric_cost_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config("Hydrate", "lots", ""), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a non-numeric cost must fail before any HTTP call"
        );
    }

    #[tokio::test]
    async fn zero_cost_fails_the_minimum_guard_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config("Hydrate", "0", ""), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a cost below 1 must fail before any HTTP call"
        );
    }

    #[tokio::test]
    async fn empty_title_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("g".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner
            .execute(&config("%g%", "500", ""), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an empty resolved title must not reach the transport"
        );
    }

    #[test]
    fn validate_config_requires_non_empty_title_and_cost() {
        let runner = runner_offline();
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("both empty", config("", "", ""), false),
            ("title only", config("Hydrate", "", ""), false),
            ("cost only", config("", "500", ""), false),
            ("both present", config("Hydrate", "500", ""), true),
        ];
        for (label, cfg, expect_ok) in cases {
            assert_eq!(
                runner.validate_config(&cfg).is_ok(),
                expect_ok,
                "case: {label}"
            );
        }
    }
}
