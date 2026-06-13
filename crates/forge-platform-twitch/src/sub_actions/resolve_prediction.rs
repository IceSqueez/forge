use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry, Variant};

use super::identity::SelfIdentity;
use super::lock_prediction::{
    execute_prediction_runner, prediction_config_fields, prediction_default_config,
    validate_prediction_config,
};
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.prediction.resolve";

pub struct ResolvePredictionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl ResolvePredictionRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for ResolvePredictionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Twitch
    }

    fn label(&self) -> &str {
        "Resolve Prediction"
    }

    fn summary(&self) -> &str {
        "Resolves a prediction by declaring a winning outcome and distributing points to winners."
    }

    fn search_text(&self) -> &str {
        "twitch prediction resolve win outcome declare result"
    }

    fn icon_name(&self) -> &str {
        "trophy"
    }

    fn default_config(&self) -> SubActionConfig {
        let mut cfg = prediction_default_config();
        cfg.insert(
            "winning_outcome_id".to_owned(),
            Variant::String(String::new()),
        );
        cfg
    }

    fn config_fields(&self) -> Vec<FormField> {
        let mut fields = prediction_config_fields();
        fields.push(FormField::Text {
            key: "winning_outcome_id",
            label: "Winning Outcome ID",
            placeholder: "%prediction.outcome.id%",
        });
        fields
    }

    fn validate_config(&self, config: &SubActionConfig) -> Result<(), RegistryError> {
        validate_prediction_config(KIND_ID, config)?;
        // winning_outcome_id is only required for RESOLVED; other status runners omit this field.
        match config.get("winning_outcome_id") {
            Some(Variant::String(s)) if !s.is_empty() => Ok(()),
            _ => Err(RegistryError::UnknownKindId(format!(
                "{KIND_ID}: 'winning_outcome_id' is required"
            ))),
        }
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        // winning_outcome_id is only sent in the body for RESOLVED; lock and cancel pass None.
        execute_prediction_runner(
            &self.transport,
            &self.identity,
            KIND_ID,
            "RESOLVED",
            Some("winning_outcome_id"),
            config,
            ctx,
        )
        .await
    }
}
