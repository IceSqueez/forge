use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::moderation::YoutubeModeration;

const KIND_ID: &str = "youtube.moderation.timeout_user";

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
                placeholder: "UC… or %user_id%",
            },
            FormField::Integer {
                key: "duration_seconds",
                label: "Duration (seconds)",
                min: 1,
                max: 86_400,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("channel_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'channel_id' must be a non-empty string"
                )));
            }
        }
        match config.get("duration_seconds") {
            Some(Variant::Int(n)) if (1..=86_400).contains(n) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'duration_seconds' must be between 1 and 86400"
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
            match self.moderation.timeout(&channel_id, duration).await {
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

    fn runner() -> TimeoutUserRunner {
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
        // Boundaries and ±1 around each edge of 1..=86400.
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
}
