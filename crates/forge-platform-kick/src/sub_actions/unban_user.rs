use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use futures::future::BoxFuture;

use super::fields::USER_ID;
use crate::api::moderation::KickModeration;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::MODERATION_UNBAN;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&USER_ID]);

pub struct UnbanUserRunner {
    client: Arc<KickModeration>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_id_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync>,
}

impl UnbanUserRunner {
    pub fn new(
        client: Arc<KickModeration>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
        broadcaster_id_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
            broadcaster_id_source,
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let target_id = match USER_ID.read(config, ctx.arg_stack) {
            Ok(target_id) => target_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => match (self.broadcaster_id_source)().await {
                Err(e) => SubActionOutcome::Failed(format!("broadcaster id error: {e}")),
                Ok(broadcaster_user_id) => SubActionOutcome::from_result(
                    &self
                        .client
                        .unban(target_id, broadcaster_user_id, &token)
                        .await,
                ),
            },
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

    fn broadcaster_id_source(
        id: u64,
    ) -> Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync> {
        Arc::new(move || Box::pin(async move { Ok(id) }))
    }

    fn runner_on(server: &MockServer) -> UnbanUserRunner {
        let client = KickModeration::new(
            crate::endpoints_test_support::api_at(&server.uri()),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        );
        UnbanUserRunner::new(Arc::new(client), token_source(), broadcaster_id_source(42))
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
            Arc::new(KickModeration::new(
                crate::endpoints_test_support::unreachable_api(),
                crate::ban_ledger_test_support::MemoryBanLedger::shared(),
            )),
            token_source(),
            broadcaster_id_source(42),
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
