use std::sync::Arc;

use async_trait::async_trait;
use forge_platform_core::PlatformError;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, DurationBounds, DurationField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use futures::future::BoxFuture;

use super::ban_reason::REASON;
use super::display_name;
use super::fields::USER_ID;
use super::step_failure;
use crate::api::moderation::KickModeration;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::MODERATION_TIMEOUT;
const MIN_TIMEOUT_MINUTES: i64 = 1;
const MAX_TIMEOUT_MINUTES: i64 = 10080;
const DEFAULT_TIMEOUT_MINUTES: i64 = 10;
const SECONDS_PER_MINUTE: u64 = 60;

const DURATION_MINUTES: DurationField = DurationField {
    key: config_keys::DURATION_MINUTES,
    label: "Duration",
    default: DEFAULT_TIMEOUT_MINUTES,
    bounds: DurationBounds::in_minutes(MIN_TIMEOUT_MINUTES, MAX_TIMEOUT_MINUTES),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&USER_ID, &DURATION_MINUTES, &REASON]);

struct TimeoutRequest {
    target_id: u64,
    minutes: u32,
    reason: Option<String>,
}

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<TimeoutRequest, ConfigProblem> {
    let minutes =
        u32::try_from(DURATION_MINUTES.read(config, args)?.as_secs() / SECONDS_PER_MINUTE)
            .map_err(|_| {
                ConfigProblem::field(
                    config_keys::DURATION_MINUTES,
                    ConfigReason::OutOfRange(ValueBounds::between(
                        MIN_TIMEOUT_MINUTES,
                        MAX_TIMEOUT_MINUTES,
                    )),
                )
            })?;
    Ok(TimeoutRequest {
        target_id: USER_ID.read(config, args)?,
        minutes,
        reason: REASON.read(config, args)?,
    })
}

pub(crate) struct TimeoutUserRunner {
    client: Arc<KickModeration>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_id_source:
        Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync>,
}

impl TimeoutUserRunner {
    pub(crate) fn new(
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
        "Temporarily bans a user from the Kick channel. Duration 1-10080 minutes. Requires moderation:ban scope."
    }

    fn search_text(&self) -> &str {
        "kick timeout user moderation temporary ban"
    }

    fn icon_name(&self) -> &str {
        "timeout"
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
        let request = match read_config(config, ctx.arg_stack) {
            Ok(request) => request,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let display_name = display_name::resolve(config, ctx);

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(step_failure::token_failed(&e)),
            Ok(token) => match (self.broadcaster_id_source)().await {
                Err(e) => SubActionOutcome::Failed(step_failure::broadcaster_failed(&e)),
                Ok(broadcaster_user_id) => step_failure::outcome(
                    &self
                        .client
                        .timeout(
                            request.target_id,
                            broadcaster_user_id,
                            request.minutes,
                            display_name.as_deref(),
                            request.reason.as_deref(),
                            &token,
                        )
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
    use crate::ban_ledger_test_support::MemoryBanLedger;
    use crate::sub_actions::test_support::{make_ctx, token_source};

    use forge_types::Variant;
    use std::collections::BTreeMap;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn broadcaster_id_source(
        id: u64,
    ) -> Arc<dyn Fn() -> BoxFuture<'static, Result<u64, PlatformError>> + Send + Sync> {
        Arc::new(move || Box::pin(async move { Ok(id) }))
    }

    fn runner_on(server: &MockServer) -> TimeoutUserRunner {
        let client = KickModeration::new(
            crate::endpoints_test_support::api_at(&server.uri()),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        );
        TimeoutUserRunner::new(Arc::new(client), token_source(), broadcaster_id_source(42))
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
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
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
            Arc::new(KickModeration::new(
                crate::endpoints_test_support::unreachable_api(),
                crate::ban_ledger_test_support::MemoryBanLedger::shared(),
            )),
            token_source(),
            broadcaster_id_source(42),
        );

        let missing_user: SubActionConfig =
            BTreeMap::from([("duration_minutes".to_owned(), Variant::Int(5))]);
        let non_string_user: SubActionConfig = BTreeMap::from([
            ("user_id".to_owned(), Variant::Int(7)),
            ("duration_minutes".to_owned(), Variant::Int(5)),
        ]);

        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            ("valid in-range", config("777", 5), true),
            (
                "hidden display name",
                with_display_name(config("777", 5), "Alice"),
                true,
            ),
            (
                "reason at limit",
                with_reason(config("777", 5), &"a".repeat(100)),
                true,
            ),
            (
                "reason over limit",
                with_reason(config("777", 5), &"a".repeat(101)),
                false,
            ),
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

    fn with_duration(mut config: SubActionConfig, duration: Variant) -> SubActionConfig {
        config.insert("duration_minutes".to_owned(), duration);
        config
    }

    #[tokio::test]
    async fn a_duration_resolving_outside_one_to_10080_minutes_fails_without_request() {
        let stack = ArgStack::new()
            .set("zero".to_owned(), Variant::String("0".to_owned()))
            .set("week_plus".to_owned(), Variant::String("10081".to_owned()));
        let out_of_range = forge_registry::ConfigProblem::field(
            "duration_minutes",
            forge_registry::ConfigReason::OutOfRange(forge_registry::ValueBounds::between(
                1, 10080,
            )),
        );
        for template in ["%zero%", "%week_plus%"] {
            let server = MockServer::start().await;
            let step_config = with_duration(config("777", 5), Variant::String(template.to_owned()));

            let (telemetry, _) = runner_on(&server)
                .execute(&step_config, &make_ctx(&stack))
                .await;

            assert_eq!(
                telemetry.outcome,
                SubActionOutcome::Failed(super::super::test_support::step_problem(
                    "kick.moderation.timeout",
                    &out_of_range
                )),
                "{template}"
            );
            assert!(
                server.received_requests().await.unwrap().is_empty(),
                "{template}"
            );
        }
    }

    #[tokio::test]
    async fn execute_sends_the_declared_default_or_the_resolved_text_duration() {
        let stack = ArgStack::new().set("mins".to_owned(), Variant::String(" 10080 ".to_owned()));
        let mut without_duration = config("777", 5);
        without_duration.remove("duration_minutes");
        for (case, step_config, expected) in [
            ("missing key", without_duration, 10),
            (
                "numeric text",
                with_duration(config("777", 5), Variant::String("15".to_owned())),
                15,
            ),
            (
                "template at the upper bound",
                with_duration(config("777", 5), Variant::String("%mins%".to_owned())),
                10080,
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/moderation/bans"))
                .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
                .mount(&server)
                .await;

            let (telemetry, _) = runner_on(&server)
                .execute(&step_config, &make_ctx(&stack))
                .await;

            assert_eq!(telemetry.outcome, SubActionOutcome::Success, "{case}");
            let reqs = server.received_requests().await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&reqs[0].body).unwrap();
            assert_eq!(body["duration"], expected, "{case}");
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
                .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
                .mount(&server)
                .await;
            let stack =
                ArgStack::new().set("why".to_owned(), Variant::String("spam links".to_owned()));

            let (telemetry, _) = runner_on(&server)
                .execute(&with_reason(config("777", 5), template), &make_ctx(&stack))
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
            .respond_with(ResponseTemplate::new(reqwest::StatusCode::OK))
            .mount(&server)
            .await;
        let ledger = Arc::new(MemoryBanLedger::default());
        let client = KickModeration::new(
            crate::endpoints_test_support::api_at(&server.uri()),
            Arc::clone(&ledger) as Arc<dyn forge_storage::BanLedgerRepo>,
        );
        let runner =
            TimeoutUserRunner::new(Arc::new(client), token_source(), broadcaster_id_source(42));
        let stack = ArgStack::new()
            .set("uid".to_owned(), Variant::String("777".to_owned()))
            .set("user_name".to_owned(), Variant::String("Alice".to_owned()));

        let (telemetry, _) = runner
            .execute(
                &with_display_name(config("%uid%", 5), "%user_name%"),
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
