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

const KIND_ID: &str = "kick.moderation.unban";

pub struct UnbanUserRunner {
    client: Arc<KickModeration>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_user_id: u64,
}

impl UnbanUserRunner {
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
impl SubActionRunner for UnbanUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Unban User"
    }

    fn summary(&self) -> &str {
        "Lifts an active ban or timeout for a user on the Kick channel. Requires moderation:ban scope."
    }

    fn search_text(&self) -> &str {
        "kick unban user moderation lift remove ban"
    }

    fn icon_name(&self) -> &str {
        "unban"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("user_id".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "user_id",
            label: "Target User ID",
            placeholder: "%user_id%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("user_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'user_id' must be a non-empty string"
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

        let template = config
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(template);

        let outcome = match resolved.parse::<u64>() {
            Err(_) => {
                SubActionOutcome::Failed(format!("user_id '{resolved}' is not a valid numeric id"))
            }
            Ok(target_id) => match (self.token_source)().await {
                Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                Ok(token) => match self
                    .client
                    .unban(target_id, self.broadcaster_user_id, &token)
                    .await
                {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                },
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
        RunContext::leaf(stack, 0, EventId::new(), &NoopPublisher)
    }

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }))
    }

    fn runner_on(server: &MockServer) -> UnbanUserRunner {
        let client = KickModeration::new(Arc::new(GrantLimiter)).with_api_base(server.uri());
        UnbanUserRunner::new(Arc::new(client), token_source(), 42)
    }

    fn config(user_id: &str) -> SubActionConfig {
        BTreeMap::from([("user_id".to_owned(), Variant::String(user_id.to_owned()))])
    }

    #[tokio::test]
    async fn execute_unbans_interpolated_numeric_id_via_delete_and_reports_success() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/moderation/bans"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;

        let runner = runner_on(&server);
        let stack = ArgStack::new().set("uid".to_owned(), Variant::String("777".to_owned()));

        let (telemetry, _) = runner.execute(&config("%uid%"), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn non_numeric_user_id_fails_without_request() {
        let server = MockServer::start().await;
        let runner = runner_on(&server);
        let stack = ArgStack::new().set("uid".to_owned(), Variant::String("alice".to_owned()));

        let (telemetry, _) = runner.execute(&config("%uid%"), &make_ctx(&stack)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "a non-numeric user id must be rejected before any HTTP call"
        );
    }

    #[test]
    fn validate_config_accepts_non_empty_and_rejects_empty_missing_non_string() {
        let runner = UnbanUserRunner::new(
            Arc::new(KickModeration::new(Arc::new(GrantLimiter))),
            token_source(),
            42,
        );

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("non-empty", config("777"), true),
            ("empty", config(""), false),
            ("missing", BTreeMap::new(), false),
            (
                "non-string",
                BTreeMap::from([("user_id".to_owned(), Variant::Int(7))]),
                false,
            ),
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
