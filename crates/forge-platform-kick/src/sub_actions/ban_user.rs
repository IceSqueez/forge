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

const KIND_ID: &str = "kick.moderation.ban";

pub struct BanUserRunner {
    client: Arc<KickModeration>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
    broadcaster_user_id: u64,
}

impl BanUserRunner {
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
        BTreeMap::from([("user_id".to_owned(), Variant::String(String::new()))])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "user_id",
            label: "Target User ID",
            placeholder: "%user_id%",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        match config.get("user_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'user_id' must be a non-empty string"
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
            .get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let resolved = ctx.arg_stack.interpolate(template);

        let outcome = match resolved.parse::<u64>() {
            Err(_) => {
                SubActionOutcome::Failed(format!("user_id '{resolved}' is not a valid numeric id"))
            }
            Ok(target_id) => match (self.token_source)().await {
                Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
                Ok(token) => match self
                    .client
                    .ban(target_id, self.broadcaster_user_id, &token)
                    .await
                {
                    Ok(()) => SubActionOutcome::Success,
                    Err(e) => SubActionOutcome::Failed(e.to_string()),
                },
            },
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
