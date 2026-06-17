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

use crate::rewards::{KickRewards, UpdateRewardParams};

const KIND_ID: &str = "kick.reward.update";

pub struct UpdateRewardRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl UpdateRewardRunner {
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
impl SubActionRunner for UpdateRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Update Channel Reward"
    }

    fn summary(&self) -> &str {
        "Updates an existing Kick channel reward. Requires channel:rewards:write scope and reward ownership."
    }

    fn search_text(&self) -> &str {
        "kick channel reward update edit points"
    }

    fn icon_name(&self) -> &str {
        "edit"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("reward_id".to_owned(), Variant::String(String::new())),
            ("title".to_owned(), Variant::String(String::new())),
            ("cost".to_owned(), Variant::String(String::new())),
            ("description".to_owned(), Variant::String(String::new())),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "reward_id",
                label: "Reward ID",
                placeholder: "%reward.id%",
            },
            FormField::Text {
                key: "title",
                label: "New Title (optional)",
                placeholder: "Leave empty to keep current",
            },
            FormField::Text {
                key: "cost",
                label: "New Cost (optional)",
                placeholder: "Leave empty to keep current",
            },
            FormField::Text {
                key: "description",
                label: "New Description (optional)",
                placeholder: "Leave empty to keep current",
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let reward_id = config
            .get("reward_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if reward_id.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'reward_id' must be a non-empty string"
            )));
        }

        let title = config
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let cost = config
            .get("cost")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let description = config
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if title.is_empty() && cost.is_empty() && description.is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: at least one of 'title', 'cost', or 'description' must be provided"
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

        let raw_reward_id = config
            .get("reward_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
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

        let reward_id = ctx.arg_stack.interpolate(raw_reward_id);
        let title_str = ctx.arg_stack.interpolate(raw_title);
        let cost_str = ctx.arg_stack.interpolate(raw_cost);
        let description_str = ctx.arg_stack.interpolate(raw_description);

        if reward_id.is_empty() {
            let outcome =
                SubActionOutcome::Failed("reward_id is empty after interpolation".to_owned());
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

        let title = if title_str.is_empty() {
            None
        } else {
            Some(title_str)
        };

        let cost = if cost_str.is_empty() {
            None
        } else {
            match cost_str.parse::<u64>() {
                Ok(n) if n >= 1 => Some(n),
                Ok(_) => {
                    let outcome = SubActionOutcome::Failed("cost must be at least 1".to_owned());
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
                Err(_) => {
                    let outcome = SubActionOutcome::Failed(format!(
                        "cost '{cost_str}' is not a valid positive integer"
                    ));
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
            }
        };

        let description = if description_str.is_empty() {
            None
        } else {
            Some(description_str)
        };

        if title.is_none() && cost.is_none() && description.is_none() {
            let outcome = SubActionOutcome::Failed(
                "all updatable fields are empty after interpolation; nothing to update".to_owned(),
            );
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
            Ok(token) => match self
                .client
                .update(
                    &reward_id,
                    UpdateRewardParams {
                        title,
                        cost,
                        description,
                        background_color: None,
                        is_enabled: None,
                        is_paused: None,
                        is_user_input_required: None,
                        should_redemptions_skip_request_queue: None,
                    },
                    &token,
                )
                .await
            {
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
    use wiremock::matchers::method;
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

    fn runner_on(server: &MockServer) -> UpdateRewardRunner {
        let client = KickRewards::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
        UpdateRewardRunner::new(Arc::new(client), token_source())
    }

    fn runner_offline() -> UpdateRewardRunner {
        let client = KickRewards::new(Arc::new(GrantLimiter));
        UpdateRewardRunner::new(Arc::new(client), token_source())
    }

    fn config(reward_id: &str, title: &str, cost: &str, description: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "reward_id".to_owned(),
                Variant::String(reward_id.to_owned()),
            ),
            ("title".to_owned(), Variant::String(title.to_owned())),
            ("cost".to_owned(), Variant::String(cost.to_owned())),
            (
                "description".to_owned(),
                Variant::String(description.to_owned()),
            ),
        ])
    }

    #[tokio::test]
    async fn execute_with_reward_id_and_one_field_reaches_server_and_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(
                &config("rw_1", "New Title", "", ""),
                &make_ctx(&ArgStack::new()),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn empty_reward_id_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("id".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner
            .execute(&config("%id%", "New", "", ""), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an empty resolved reward_id must not reach the transport"
        );
    }

    #[tokio::test]
    async fn non_numeric_cost_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let (telemetry, _) = runner
            .execute(&config("rw_1", "", "lots", ""), &make_ctx(&ArgStack::new()))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a non-numeric cost must fail before any HTTP call"
        );
    }

    #[tokio::test]
    async fn all_updatable_fields_empty_after_interpolation_fails_without_request() {
        let server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("e".to_owned(), Variant::String(String::new()));
        let (telemetry, _) = runner
            .execute(&config("rw_1", "%e%", "%e%", "%e%"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "an all-empty resolved update must not reach the transport"
        );
    }

    #[test]
    fn validate_config_requires_reward_id_and_at_least_one_updatable_field() {
        let runner = runner_offline();
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("no reward_id", config("", "New", "", ""), false),
            ("reward_id but no fields", config("rw_1", "", "", ""), false),
            ("reward_id + title", config("rw_1", "New", "", ""), true),
            ("reward_id + cost", config("rw_1", "", "500", ""), true),
            ("reward_id + description", config("rw_1", "", "", "d"), true),
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
