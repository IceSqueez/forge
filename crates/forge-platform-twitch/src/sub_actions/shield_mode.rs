use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

async fn set_shield(
    transport: &dyn HelixTransport,
    identity: &SelfIdentity,
    is_active: bool,
) -> SubActionOutcome {
    let self_id = match identity.user_id().await {
        Ok(id) => id,
        Err(e) => return SubActionOutcome::Failed(e.to_string()),
    };
    let request = HelixRequest::new(HelixMethod::Post, "/helix/moderation/shield_mode")
        .query("broadcaster_id", self_id.clone())
        .query("moderator_id", self_id)
        .body(serde_json::json!({ "is_active": is_active }));
    match transport.execute(request).await {
        Ok(_) => SubActionOutcome::Success,
        Err(e) => SubActionOutcome::Failed(e.to_string()),
    }
}

// ─── Shield Mode On ──────────────────────────────────────────────────────────

const ON_KIND_ID: &str = "twitch.moderation.shield_mode_on";

pub struct ShieldModeOnRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ShieldModeOnRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for ShieldModeOnRunner {
    fn id(&self) -> &str {
        ON_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Enable Shield Mode"
    }

    fn summary(&self) -> &str {
        "Activates Shield Mode on the broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation shield mode on enable activate protect"
    }

    fn icon_name(&self) -> &str {
        "shield"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<forge_registry::FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = set_shield(self.transport.as_ref(), &self.identity, true).await;

        (
            SubActionTelemetry {
                kind: ON_KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

// ─── Shield Mode Off ─────────────────────────────────────────────────────────

const OFF_KIND_ID: &str = "twitch.moderation.shield_mode_off";

pub struct ShieldModeOffRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ShieldModeOffRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for ShieldModeOffRunner {
    fn id(&self) -> &str {
        OFF_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Disable Shield Mode"
    }

    fn summary(&self) -> &str {
        "Deactivates Shield Mode on the broadcaster's channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation shield mode off disable deactivate"
    }

    fn icon_name(&self) -> &str {
        "shield-off"
    }

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<forge_registry::FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = set_shield(self.transport.as_ref(), &self.identity, false).await;

        (
            SubActionTelemetry {
                kind: OFF_KIND_ID.to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}
