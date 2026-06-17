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

const KIND_ID: &str = "kick.moderation.timeout";

pub struct TimeoutUserRunner {
    client: Arc<KickModeration>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_user_id: u64,
}

impl TimeoutUserRunner {
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
        "Temporarily bans a user from the Kick channel. Duration 1–10080 minutes. Requires moderation:ban scope."
    }

    fn search_text(&self) -> &str {
        "kick timeout user moderation temporary ban"
    }

    fn icon_name(&self) -> &str {
        "timeout"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([
            ("user_id".to_owned(), Variant::String(String::new())),
            ("duration_minutes".to_owned(), Variant::Int(10)),
        ])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![
            FormField::Text {
                key: "user_id",
                label: "Target User ID",
                placeholder: "%user_id%",
            },
            FormField::Integer {
                key: "duration_minutes",
                label: "Duration (minutes)",
                min: 1,
                max: 10080,
            },
        ]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("user_id") {
            Some(Variant::String(s)) if !s.is_empty() => {}
            _ => {
                return Err(RegistryError::UnknownKindId(format!(
                    "{KIND_ID}: 'user_id' must be a non-empty string"
                )));
            }
        }

        match config.get("duration_minutes") {
            Some(Variant::Int(n)) if (1..=10080).contains(n) => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'duration_minutes' must be an integer 1–10080"
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

        let user_template = config
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved_uid = ctx.arg_stack.interpolate(user_template);

        let duration_minutes = config
            .get("duration_minutes")
            .and_then(|v| {
                if let Variant::Int(n) = v {
                    Some(*n)
                } else {
                    None
                }
            })
            .unwrap_or(10);

        let outcome = match resolved_uid.parse::<u64>() {
            Err(_) => SubActionOutcome::Failed(format!(
                "user_id '{resolved_uid}' is not a valid numeric id"
            )),
            Ok(target_id) => {
                let duration_u32 = duration_minutes.clamp(1, 10080) as u32;
                match (self.token_source)().await {
                    Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                    Ok(token) => match self
                        .client
                        .timeout(target_id, self.broadcaster_user_id, duration_u32, &token)
                        .await
                    {
                        Ok(()) => SubActionOutcome::Success,
                        Err(e) => SubActionOutcome::Failed(e.to_string()),
                    },
                }
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
