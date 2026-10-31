use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::display_name;
use super::fields::CHANNEL_ID;
use super::step_failure;
use crate::moderation::YoutubeModeration;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::MODERATION_BAN_USER;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&CHANNEL_ID]);

pub struct BanUserRunner {
    moderation: Arc<YoutubeModeration>,
}

impl BanUserRunner {
    pub fn new(moderation: Arc<YoutubeModeration>) -> Self {
        Self { moderation }
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
        "Permanently bans a user from the active YouTube live chat."
    }

    fn search_text(&self) -> &str {
        "youtube moderation ban block permanent remove user channel"
    }

    fn icon_name(&self) -> &str {
        "ban"
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
        let channel_id = match CHANNEL_ID.read(config, ctx.arg_stack) {
            Ok(channel_id) => channel_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let display_name = display_name::resolve(config, ctx);
        let outcome = step_failure::outcome(
            &self
                .moderation
                .ban(&channel_id, display_name.as_deref())
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

    fn runner() -> BanUserRunner {
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
        BanUserRunner::new(Arc::new(moderation))
    }

    #[test]
    fn validate_config_requires_a_non_empty_channel_id() {
        let runner = runner();
        let cases: Vec<(&str, SubActionConfig, bool)> = vec![
            (
                "non-empty channel id",
                BTreeMap::from([("channel_id".to_owned(), Variant::String("UC1".to_owned()))]),
                true,
            ),
            (
                "empty channel id",
                BTreeMap::from([("channel_id".to_owned(), Variant::String(String::new()))]),
                false,
            ),
            ("missing channel id", BTreeMap::new(), false),
            (
                "non-string channel id",
                BTreeMap::from([("channel_id".to_owned(), Variant::Int(7))]),
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

        let (telemetry, _) = BanUserRunner::new(Arc::new(moderation))
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
