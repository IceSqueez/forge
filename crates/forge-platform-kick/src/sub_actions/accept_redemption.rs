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

use crate::rewards::KickRewards;

const KIND_ID: &str = "kick.reward.redemption_accept";
const MAX_BATCH: usize = 25;

pub struct AcceptRedemptionRunner {
    client: Arc<KickRewards>,
    token_source: Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>,
}

impl AcceptRedemptionRunner {
    pub fn new(
        client: Arc<KickRewards>,
        token_source: Arc<
            dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            client,
            token_source,
        }
    }
}

fn parse_ids(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

#[async_trait]
impl SubActionRunner for AcceptRedemptionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Accept Reward Redemption(s)"
    }

    fn summary(&self) -> &str {
        "Accepts one or more pending Kick reward redemptions. Requires channel:rewards:write scope."
    }

    fn search_text(&self) -> &str {
        "kick channel reward redemption accept approve fulfill points"
    }

    fn icon_name(&self) -> &str {
        "check"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::from([(
            "redemption_ids".to_owned(),
            Variant::String("%redemption_id%".to_owned()),
        )])
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![FormField::Text {
            key: "redemption_ids",
            label: "Redemption ID(s)",
            placeholder: "%redemption_id% or id1,id2,id3 (max 25)",
        }]
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        let raw = config
            .get("redemption_ids")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if raw.trim().is_empty() {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'redemption_ids' must not be empty"
            )));
        }
        let ids = parse_ids(raw);
        if ids.len() > MAX_BATCH {
            return Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: config contains {} ids, maximum is {MAX_BATCH}",
                ids.len()
            )));
        }
        Ok(())
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let raw = config
            .get("redemption_ids")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let interpolated = ctx.arg_stack.interpolate(raw);
        let ids = parse_ids(&interpolated);

        if ids.is_empty() {
            let outcome =
                SubActionOutcome::Failed("redemption_ids is empty after interpolation".to_owned());
            return (
                SubActionTelemetry {
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome,
                    index: ctx.index,
                },
                None,
            );
        }

        let outcome = match (self.token_source)().await {
            Err(e) => SubActionOutcome::Failed(format!("token error: {e}")),
            Ok(token) => match self.client.accept_redemptions(&ids, &token).await {
                Ok(()) => SubActionOutcome::Success,
                Err(e) => SubActionOutcome::Failed(e.to_string()),
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
