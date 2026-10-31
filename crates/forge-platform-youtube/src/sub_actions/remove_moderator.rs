use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fields::CHANNEL_ID;
use super::step_failure;
use crate::moderation::YoutubeModeration;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::MODERATION_REMOVE_MODERATOR;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&CHANNEL_ID]);

pub(crate) struct RemoveModeratorRunner {
    moderation: Arc<YoutubeModeration>,
}

impl RemoveModeratorRunner {
    pub(crate) fn new(moderation: Arc<YoutubeModeration>) -> Self {
        Self { moderation }
    }
}

#[async_trait]
impl SubActionRunner for RemoveModeratorRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Remove Moderator"
    }

    fn summary(&self) -> &str {
        "Revokes a user's moderator status in the active YouTube live chat."
    }

    fn search_text(&self) -> &str {
        "youtube moderation remove revoke moderator mod demote user channel"
    }

    fn icon_name(&self) -> &str {
        "shield-off"
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

        let outcome = step_failure::outcome(&self.moderation.remove_moderator(&channel_id).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use futures::future::BoxFuture;

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn runner() -> RemoveModeratorRunner {
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
        RemoveModeratorRunner::new(Arc::new(moderation))
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
}
