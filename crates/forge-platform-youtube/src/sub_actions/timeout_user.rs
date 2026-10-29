use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigReason, DurationBounds, FormField, RegistryError, RunContext, SubActionCategory,
    SubActionConfigExt, SubActionRunner, ValueBounds,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use super::display_name;
use crate::moderation::YoutubeModeration;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::MODERATION_TIMEOUT_USER;

pub struct TimeoutUserRunner {
    moderation: Arc<YoutubeModeration>,
}

impl TimeoutUserRunner {
    pub fn new(moderation: Arc<YoutubeModeration>) -> Self {
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("channel_id".to_owned(), Variant::String(String::new())),
            ("duration_seconds".to_owned(), Variant::Int(300)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "channel_id",
                label: "Channel ID",
                placeholder: "UC... or %user_id%",
            },
            FormField::Duration {
                key: "duration_seconds",
                label: "Duration",
                bounds: DurationBounds::in_seconds(1, 86_400),
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("channel_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::invalid_field(
                    "channel_id",
                    ConfigReason::Required,
                ));
            }
        }
        match config.get("duration_seconds") {
            Some(Variant::Int(n)) if (1..=86_400).contains(n) => Ok(()),
            _ => Err(RegistryError::invalid_field(
                "duration_seconds",
                ConfigReason::OutOfRange(ValueBounds::between(1, 86400)),
            )),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let template = config.str("channel_id").unwrap_or_default();
        let channel_id = ctx.arg_stack.interpolate(template);
        let display_name = display_name::resolve(config, ctx);
        let duration = config
            .get("duration_seconds")
            .and_then(|v| match v {
                Variant::Int(n) => u32::try_from(*n).ok(),
                _ => None,
            })
            .unwrap_or(300);

        let outcome = if channel_id.is_empty() {
            SubActionOutcome::Failed("channel_id is empty after interpolation".to_owned())
        } else {
            SubActionOutcome::from_result(
                &self
                    .moderation
                    .timeout(&channel_id, duration, display_name.as_deref())
                    .await,
            )
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
    use futures::future::BoxFuture;

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::ban_ledger_test_support::{
        MemoryBanLedger, TEST_BROADCASTER_CHANNEL_ID, broadcaster_source,
    };
    use crate::live_chat_id::LiveChatIdHandle;

    struct NoopPublisher;
    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

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
            ("non-integer", Variant::String("300".to_owned()), false),
            ("missing", Variant::Bool(false), false),
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
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);

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
}
