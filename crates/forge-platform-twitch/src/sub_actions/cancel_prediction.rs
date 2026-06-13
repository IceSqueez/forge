use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};

use super::identity::SelfIdentity;
use super::lock_prediction::{
    execute_prediction_runner, prediction_config_fields, prediction_default_config,
    validate_prediction_config,
};
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.prediction.cancel";

pub struct CancelPredictionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl CancelPredictionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for CancelPredictionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Cancel Prediction"
    }

    fn summary(&self) -> &str {
        "Cancels an active prediction and refunds all channel points to voters."
    }

    fn search_text(&self) -> &str {
        "twitch prediction cancel refund abort"
    }

    fn icon_name(&self) -> &str {
        "x"
    }

    fn default_config(&self) -> SubActionConfig {
        prediction_default_config()
    }

    fn config_fields(&self) -> Vec<FormField> {
        prediction_config_fields()
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_prediction_config(KIND_ID, config)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        // "CANCELED" — American spelling, single L, as required by the Twitch API.
        execute_prediction_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "CANCELED",
            None,
            config,
            ctx,
        )
        .await
    }
}
