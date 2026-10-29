use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, FormField, RegistryError, RunContext, SubActionCategory, SubActionConfigExt,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use futures::future::BoxFuture;
use time::OffsetDateTime;

use super::{ban_reason, display_name};
use crate::api::moderation::KickModeration;
use crate::vocabulary::sub_action_ids;
use crate::vocabulary::variables::references;

const KIND_ID: &str = sub_action_ids::MODERATION_BAN;

pub struct BanUserRunner {
    client: Arc<KickModeration>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_id_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync>,
}

impl BanUserRunner {
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
impl SubActionRunner for BanUserRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Ban User"
    }

    fn summary(&self) -> &str {
        "Permanently bans a user from the Kick channel. Requires moderation:ban scope."
    }

    fn search_text(&self) -> &str {
        "kick ban user moderation permanent"
    }

    fn icon_name(&self) -> &str {
        "ban"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("user_id".to_owned(), Variant::String(String::new())),
            ban_reason::default_entry(),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "user_id",
                label: "Target User ID",
                placeholder: references::USER_ID,
            },
            ban_reason::form_field(),
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("user_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::invalid_field(
                    "user_id",
                    ConfigReason::Required,
                ));
            }
        }
        ban_reason::validate(config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let template = config.str("user_id").unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(template);
        let reason = ban_reason::resolve(config, ctx);
        let display_name = display_name::resolve(config, ctx);

        let outcome = match resolved.parse::<u64>() {
            Err(_) => {
                SubActionOutcome::Failed(format!("user_id '{resolved}' is not a valid numeric id"))
            }
            Ok(target_id) => match (self.token_source)().await {
                Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                Ok(token) => match (self.broadcaster_id_source)().await {
                    Err(e) => SubActionOutcome::Failed(format!("broadcaster id error: {e}")),
                    Ok(broadcaster_user_id) => match self
                        .client
                        .ban(
                            target_id,
                            broadcaster_user_id,
                            display_name.as_deref(),
                            reason.as_deref(),
                            &token,
                        )
                        .await
                    {
                        Ok(()) => SubActionOutcome::Success,
                        Err(e) => SubActionOutcome::Failed(e.to_string()),
                    },
                },
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
mod tests {
    use super::*;
    use crate::ban_ledger_test_support::MemoryBanLedger;
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

    fn runner_on(server: &MockServer) -> BanUserRunner {
        let client = KickModeration::new(
            crate::endpoints_test_support::api_at(&server.uri()),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        );
        BanUserRunner::new(Arc::new(client), token_source(), broadcaster_id_source(42))
    }

    fn config(user_id: &str) -> SubActionConfig {
        BTreeMap::from([("user_id".to_owned(), Variant::String(user_id.to_owned()))])
    }

    #[tokio::test]
    async fn execute_bans_interpolated_numeric_id_and_reports_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/moderation/bans"))
            .respond_with(ResponseTemplate::new(200))
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
        let runner = BanUserRunner::new(
            Arc::new(KickModeration::new(
                crate::endpoints_test_support::unreachable_api(),
                crate::ban_ledger_test_support::MemoryBanLedger::shared(),
            )),
            token_source(),
            broadcaster_id_source(42),
        );

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("non-empty", config("777"), true),
            (
                "hidden display name",
                with_display_name(config("777"), "Alice"),
                true,
            ),
            (
                "reason at limit",
                with_reason(config("777"), &"a".repeat(100)),
                true,
            ),
            (
                "reason over limit",
                with_reason(config("777"), &"a".repeat(101)),
                false,
            ),
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

    fn with_reason(mut config: SubActionConfig, reason: &str) -> SubActionConfig {
        config.insert("reason".to_owned(), Variant::String(reason.to_owned()));
        config
    }

    #[tokio::test]
    async fn execute_posts_the_interpolated_reason_only_when_it_is_not_blank() {
        for (template, expected) in [("%why%", Some("spam links")), ("  ", None)] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/moderation/bans"))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;
            let stack =
                ArgStack::new().set("why".to_owned(), Variant::String("spam links".to_owned()));

            let (telemetry, _) = runner_on(&server)
                .execute(&with_reason(config("777"), template), &make_ctx(&stack))
                .await;

            assert_eq!(telemetry.outcome, SubActionOutcome::Success);
            let reqs = server.received_requests().await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
            assert_eq!(
                body.get("reason").and_then(serde_json::Value::as_str),
                expected,
                "template {template:?}"
            );
        }
    }

    fn with_display_name(mut config: SubActionConfig, name: &str) -> SubActionConfig {
        config.insert("display_name".to_owned(), Variant::String(name.to_owned()));
        config
    }

    #[tokio::test]
    async fn execute_records_the_interpolated_display_name_in_the_ban_ledger() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/moderation/bans"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let ledger = Arc::new(MemoryBanLedger::default());
        let client = KickModeration::new(
            crate::endpoints_test_support::api_at(&server.uri()),
            Arc::clone(&ledger) as Arc<dyn forge_storage::BanLedgerRepo>,
        );
        let runner =
            BanUserRunner::new(Arc::new(client), token_source(), broadcaster_id_source(42));
        let stack = ArgStack::new()
            .set("uid".to_owned(), Variant::String("777".to_owned()))
            .set("user_name".to_owned(), Variant::String("Alice".to_owned()));

        let (telemetry, _) = runner
            .execute(
                &with_display_name(config("%uid%"), "%user_name%"),
                &make_ctx(&stack),
            )
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let key = forge_storage::BanLedgerKey {
            platform: forge_types::PlatformId::Kick,
            channel_id: "42".to_owned(),
            viewer_id: "777".to_owned(),
        };
        assert_eq!(
            ledger.stored(&key).map(|entry| entry.viewer_name),
            Some("Alice".to_owned())
        );
    }
}
