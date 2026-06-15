use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::moderation::YoutubeModeration;

const KIND_ID: &str = "youtube.moderation.ban_user";

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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([("channel_id".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "channel_id",
            label: "Channel ID",
            placeholder: "UC… or %user_id%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("channel_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'channel_id' must be a non-empty string"
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
            .get("channel_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let channel_id = ctx.arg_stack.interpolate(template);

        let outcome = if channel_id.is_empty() {
            SubActionOutcome::Failed("channel_id is empty after interpolation".to_owned())
        } else {
            match self.moderation.ban(&channel_id).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
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
    use futures::future::BoxFuture;
    use tokio::sync::Mutex;

    use super::*;
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::quota_state::QuotaState;

    fn runner() -> BanUserRunner {
        let source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, forge_platform_core::PlatformError>>
                + Send
                + Sync,
        > = Arc::new(|| Box::pin(async { Ok(String::new()) }));
        let moderation = YoutubeModeration::new(
            source,
            LiveChatIdHandle::new(),
            Arc::new(Mutex::new(QuotaState::default())),
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
}
