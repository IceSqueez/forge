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

use crate::moderation::KickModeration;

const KIND_ID: &str = "kick.moderation.timeout";

pub struct TimeoutUserRunner {
    client: Arc<KickModeration>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_user_id: u64,
}

impl TimeoutUserRunner {
    pub fn new(
        client: Arc<KickModeration>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        broadcaster_user_id: u64,
    ) -> Self {
        Self {
            client,
            token_source,
            broadcaster_user_id,
        }
    }
}

#[async_trait]
impl SubActionRunner for TimeoutUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Timeout User"
    }

    fn summary(&self) -> &str {
        "Temporarily bans a user from the Kick channel. Duration 1–10080 minutes. Requires moderation:ban scope."
    }

    fn search_text(&self) -> &str {
        "kick timeout user moderation temporary ban"
    }

    fn icon_name(&self) -> &str {
        "timeout"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("user_id".to_owned(), Variant::String(String::new())),
            ("duration_minutes".to_owned(), Variant::Int(10)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "user_id",
                label: "Target User ID",
                placeholder: "%user_id%",
            },
            FormField::Integer {
                key: "duration_minutes",
                label: "Duration (minutes)",
                min: 1,
                max: 10080,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("user_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'user_id' must be a non-empty string"
                )));
            }
        }

        match config.get("duration_minutes") {
            Some(Variant::Int(n)) if (1..=10080).contains(n) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'duration_minutes' must be an integer 1–10080"
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

        let user_template = config
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved_uid = ctx.arg_stack.interpolate(user_template);

        let duration_minutes = config
            .get("duration_minutes")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(10);

        let outcome = match resolved_uid.parse::<u64>() {
            Err(_) => SubActionOutcome::Failed(format!(
                "user_id '{resolved_uid}' is not a valid numeric id"
            )),
            Ok(target_id) => {
                let duration_u32 = duration_minutes.clamp(1, 10080) as u32;
                match (self.token_source)().await {
                    Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                    Ok(token) => match self
                        .client
                        .timeout(target_id, self.broadcaster_user_id, duration_u32, &token)
                        .await
                    {
                        Ok(()) => SubActionOutcome::Success,
                        Err(e) => SubActionOutcome::Failed(e.to_string()),
                    },
                }
            }
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
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> TimeoutUserRunner {
        let client = KickModeration::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
        TimeoutUserRunner::new(Arc::new(client), token_source(), 42)
    }

    fn config(user_id: &str, duration_minutes: i64) -> SubActionConfig {
        BTreeMap::from([
            ("user_id".to_owned(), Variant::String(user_id.to_owned())),
            (
                "duration_minutes".to_owned(),
                Variant::Int(duration_minutes),
            ),
        ])
    }

    #[tokio::test]
    async fn execute_times_out_numeric_id_sending_duration_in_body() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/moderation/bans"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("uid".to_owned(), Variant::String("777".to_owned()));

        let (telemetry, _) = runner.execute(&config("%uid%", 5), &make_ctx(&stack)).await;
        assert_eq!(telemetry.outcome, SubActionOutcome::Success);

        let reqs = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
        assert_eq!(
            body["duration"], 5,
            "timeout must forward duration_minutes; this is the signal distinguishing it from a permanent ban"
        );
    }

    #[tokio::test]
    async fn non_numeric_user_id_fails_without_request() {
        let server = MockServer::start().await;
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("uid".to_owned(), Variant::String("alice".to_owned()));

        let (telemetry, _) = runner.execute(&config("%uid%", 5), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a non-numeric user id must be rejected before any HTTP call"
        );
    }

    #[test]
    fn validate_config_enforces_user_id_and_duration_bounds() {
        let runner = TimeoutUserRunner::new(
            Arc::new(KickModeration::new(Arc::new(GrantLimiter))),
            token_source(),
            42,
        );

        let missing_user: SubActionConfig =
            BTreeMap::from([("duration_minutes".to_owned(), Variant::Int(5))]);
        let non_string_user: SubActionConfig = BTreeMap::from([
            ("user_id".to_owned(), Variant::Int(7)),
            ("duration_minutes".to_owned(), Variant::Int(5)),
        ]);

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid in-range", config("777", 5), true),
            ("duration lower bound", config("777", 1), true),
            ("duration upper bound", config("777", 10080), true),
            ("duration zero rejected", config("777", 0), false),
            ("duration over max rejected", config("777", 10081), false),
            ("empty user_id", config("", 5), false),
            ("missing user_id", missing_user, false),
            ("non-string user_id", non_string_user, false),
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
