use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};

use super::enable_reward::{config_fields, default_config, execute_bool_runner, validate_reward_id};
use super::identity::SelfIdentity;
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.channel_points.pause_reward";

pub struct PauseRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl PauseRewardRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for PauseRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Pause Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Pauses a custom channel point reward, queueing new redemptions without fulfilling them."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward pause hold queue redemption"
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
        execute_bool_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "is_paused",
            true,
            config,
            ctx,
        )
        .await
    }
}
