use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};

use super::approve_automod_message::{
    automod_config_fields, automod_default_config, execute_automod_runner, validate_automod_config,
};
use super::identity::SelfIdentity;
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.automod.deny_message";

pub struct DenyAutomodMessageRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl DenyAutomodMessageRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for DenyAutomodMessageRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Deny AutoMod Message"
    }

    fn summary(&self) -> &str {
        "Denies a message held by AutoMod so it is removed from the queue."
    }

    fn search_text(&self) -> &str {
        "twitch automod deny reject remove message held moderation"
    }

    fn icon_name(&self) -> &str {
        "x"
    }

    fn default_config(&self) -> SubActionConfig {
        automod_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        automod_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_automod_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        execute_automod_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "DENY",
            config,
            ctx,
        )
        .await
    }
}
