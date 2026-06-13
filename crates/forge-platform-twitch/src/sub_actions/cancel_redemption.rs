use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fulfill_redemption::{
    execute_redemption_runner, redemption_config_fields, redemption_default_config,
    validate_redemption_config,
};
use super::identity::SelfIdentity;
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.channel_points.cancel_redemption";

pub struct CancelRedemptionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl CancelRedemptionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for CancelRedemptionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Cancel Channel Point Redemption"
    }

    fn summary(&self) -> &str {
        "Cancels a channel point redemption and refunds the viewer's points."
    }

    fn search_text(&self) -> &str {
        "twitch channel points redemption cancel reject refund"
    }

    fn icon_name(&self) -> &str {
        "x"
    }

    fn default_config(&self) -> SubActionConfig {
        redemption_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        redemption_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_redemption_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        // "CANCELED" — American spelling, single L, as documented by Twitch API.
        execute_redemption_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "CANCELED",
            config,
            ctx,
        )
        .await
    }
}
