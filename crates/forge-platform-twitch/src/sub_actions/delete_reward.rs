use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use super::enable_reward::{config_fields, default_config, validate_reward_id};
use super::identity::SelfIdentity;
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};

const KIND_ID: &str = "twitch.channel_points.delete_reward";

pub struct DeleteRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl DeleteRewardRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for DeleteRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Delete Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Permanently deletes a custom channel point reward."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward delete remove redemption"
    }

    fn icon_name(&self) -> &str {
        "star"
    }

    fn default_config(&self) -> SubActionConfig {
        default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_reward_id(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let reward_id_template = config
            .get("reward_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let reward_id = ctx.arg_stack.interpolate(reward_id_template);

        let outcome = if reward_id.is_empty() {
            SubActionOutcome::Failed("reward_id is required".to_owned())
        } else {
            self.apply(&reward_id).await
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

impl DeleteRewardRunner {
    async fn apply(&self, reward_id: &str) -> SubActionOutcome {
        let user_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };

        // DELETE /helix/channel_points/custom_rewards sends both broadcaster_id and
        // the reward id as query params — no request body (204 on success).
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/channel_points/custom_rewards")
            .query("broadcaster_id", user_id)
            .query("id", reward_id.to_owned());

        match self.transport.execute(request).await {
            Ok(_) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(e.to_string()),
        }
    }
}
