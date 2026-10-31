use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigReason, ConfigSpec, DurationBounds, DurationField, RunContext, StepTimer,
    SubActionCategory, SubActionRunner, TypedField, ValueBounds,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::display_name;
use super::fields::CHANNEL_ID;
use super::step_failure;
use crate::moderation::YoutubeModeration;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::MODERATION_TIMEOUT_USER;
const MIN_TIMEOUT_SECONDS: i64 = 1;
const MAX_TIMEOUT_SECONDS: i64 = 86_400;
const DEFAULT_TIMEOUT_SECONDS: i64 = 300;

const DURATION_SECONDS: DurationField = DurationField {
    key: config_keys::DURATION_SECONDS,
    label: "Duration",
    default: DEFAULT_TIMEOUT_SECONDS,
    bounds: DurationBounds::in_seconds(MIN_TIMEOUT_SECONDS, MAX_TIMEOUT_SECONDS),
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&CHANNEL_ID, &DURATION_SECONDS]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, u32), ConfigProblem> {
    let seconds = u32::try_from(DURATION_SECONDS.read(config, args)?.as_secs()).map_err(|_| {
        ConfigProblem::field(
            config_keys::DURATION_SECONDS,
            ConfigReason::OutOfRange(ValueBounds::between(
                MIN_TIMEOUT_SECONDS,
                MAX_TIMEOUT_SECONDS,
            )),
        )
    })?;
    Ok((CHANNEL_ID.read(config, args)?, seconds))
}

pub(crate) struct TimeoutUserRunner {
    moderation: Arc<YoutubeModeration>,
}

impl TimeoutUserRunner {
    pub(crate) fn new(moderation: Arc<YoutubeModeration>) -> Self {
        Self { moderation }
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
        "Temporarily bans a user from the active YouTube live chat."
    }

    fn search_text(&self) -> &str {
        "youtube moderation timeout temporary mute silence user channel"
    }

    fn icon_name(&self) -> &str {
        "clock"
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
        let (channel_id, seconds) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let display_name = display_name::resolve(config, ctx);
        let outcome = step_failure::outcome(
            &self
                .moderation
                .timeout(&channel_id, seconds, display_name.as_deref())
                .await,
        );
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use forge_events::test_support::NullPublisher;
    use futures::future::BoxFuture;

    use forge_types::EventId;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::ban_ledger_test_support::{
        MemoryBanLedger, TEST_BROADCASTER_CHANNEL_ID, broadcaster_source,
    };
    use crate::live_chat_id::LiveChatIdHandle;
    use forge_types::SubActionOutcome;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner() -> TimeoutUserRunner {
        let source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
                + Send
                + Sync,
        > = Arc::new(|| Box::pin(async { Ok(String::new()) }));
        let moderation = YoutubeModeration::new(
            crate::endpoints_test_support::unreachable_data_api(),
            source,
            crate::ban_ledger_test_support::broadcaster_source(),
            LiveChatIdHandle::new(),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        );
        TimeoutUserRunner::new(Arc::new(moderation))
    }

    fn config(channel: Variant, duration: Variant) -> SubActionConfig {
        BTreeMap::from([
            ("channel_id".to_owned(), channel),
            ("duration_seconds".to_owned(), duration),
        ])
    }

    #[test]
    fn validate_config_requires_a_non_empty_channel_id() {
        let runner = runner();
        let valid_duration = Variant::Int(300);
        let cases: Vec<(&str, Variant, bool)> = vec![
            ("non-empty", Variant::String("UC1".to_owned()), true),
            ("empty", Variant::String(String::new()), false),
            ("non-string", Variant::Int(7), false),
        ];
        for (label, channel, expect_ok) in cases {
            assert_eq!(
                runner
                    .validate_config(&config(channel, valid_duration.clone()))
                    .is_ok(),
                expect_ok,
                "channel case: {label}"
            );
        }
    }

    #[test]
    fn validate_config_bounds_duration_to_one_through_86400_inclusive() {
        let runner = runner();
        let channel = Variant::String("UC1".to_owned());
        let cases: Vec<(&str, Variant, bool)> = vec![
            ("below min (0)", Variant::Int(0), false),
            ("min (1)", Variant::Int(1), true),
            ("max (86400)", Variant::Int(86_400), true),
            ("above max (86401)", Variant::Int(86_401), false),
            ("negative", Variant::Int(-1), false),
            ("numeric text", Variant::String("300".to_owned()), true),
            ("template", Variant::String("%secs%".to_owned()), true),
            (
                "non-numeric text",
                Variant::String("five".to_owned()),
                false,
            ),
            ("bool", Variant::Bool(false), false),
        ];
        for (label, duration, expect_ok) in cases {
            assert_eq!(
                runner
                    .validate_config(&config(channel.clone(), duration))
                    .is_ok(),
                expect_ok,
                "duration case: {label}"
            );
        }
    }

    fn display_name_config(channel_id: &str, display_name: &str) -> SubActionConfig {
        BTreeMap::from([
            (
                "channel_id".to_owned(),
                Variant::String(channel_id.to_owned()),
            ),
            (
                "display_name".to_owned(),
                Variant::String(display_name.to_owned()),
            ),
            ("duration_seconds".to_owned(), Variant::Int(600)),
        ])
    }

    #[test]
    fn validate_config_accepts_a_hidden_display_name() {
        assert!(
            runner()
                .validate_config(&display_name_config("UC1", "%user_name%"))
                .is_ok()
        );
    }

    #[tokio::test]
    async fn execute_records_the_interpolated_display_name_in_the_ban_ledger() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/liveChat/bans"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "ban-1"})),
            )
            .mount(&server)
            .await;
        let ledger = Arc::new(MemoryBanLedger::default());
        let live_chat = LiveChatIdHandle::new();
        live_chat.set(Some("lc-test".to_owned()));
        let token: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
                + Send
                + Sync,
        > = Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }));
        let moderation = YoutubeModeration::new(
            crate::endpoints_test_support::data_api_at(&server.uri()),
            token,
            broadcaster_source(),
            live_chat,
            Arc::clone(&ledger) as Arc<dyn forge_storage::BanLedgerRepo>,
        );
        let stack = ArgStack::new()
            .set("uid".to_owned(), Variant::String("UC-target".to_owned()))
            .set("user_name".to_owned(), Variant::String("Alice".to_owned()));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);

        let (telemetry, _) = TimeoutUserRunner::new(Arc::new(moderation))
            .execute(&display_name_config("%uid%", "%user_name%"), &ctx)
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let key = crate::ban_ledger::ban_ledger_key(TEST_BROADCASTER_CHANNEL_ID, "UC-target");
        assert_eq!(
            ledger.stored(&key).map(|entry| entry.viewer_name),
            Some("Alice".to_owned())
        );
    }

    #[tokio::test]
    async fn execute_sends_the_declared_default_or_the_numeric_text_duration() {
        for (case, duration, expected) in [
            ("missing key", None, "300"),
            (
                "numeric text",
                Some(Variant::String("120".to_owned())),
                "120",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/liveChat/bans"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "ban-1"})),
                )
                .mount(&server)
                .await;
            let live_chat = LiveChatIdHandle::new();
            live_chat.set(Some("lc-test".to_owned()));
            let token: Arc<
                dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
                    + Send
                    + Sync,
            > = Arc::new(|| Box::pin(async { Ok("tok".to_owned()) }));
            let moderation = YoutubeModeration::new(
                crate::endpoints_test_support::data_api_at(&server.uri()),
                token,
                broadcaster_source(),
                live_chat,
                MemoryBanLedger::shared(),
            );
            let mut step_config =
                BTreeMap::from([("channel_id".to_owned(), Variant::String("UC1".to_owned()))]);
            if let Some(duration) = duration {
                step_config.insert("duration_seconds".to_owned(), duration);
            }
            let stack = ArgStack::new();
            let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);

            let (telemetry, _) = TimeoutUserRunner::new(Arc::new(moderation))
                .execute(&step_config, &ctx)
                .await;

            assert_eq!(telemetry.outcome, SubActionOutcome::Success, "{case}");
            let req = &server.received_requests().await.unwrap()[0];
            let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
            assert_eq!(body["snippet"]["banDurationSeconds"], expected, "{case}");
        }
    }
}
