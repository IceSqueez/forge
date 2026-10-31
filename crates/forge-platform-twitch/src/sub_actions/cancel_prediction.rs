use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{ConfigSpec, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};

use super::lock_prediction::{PREDICTION_CONFIG, PredictionUpdate};
use crate::account::identity::SelfIdentity;
use crate::helix::HelixTransport;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::PREDICTION_CANCEL;

pub(crate) struct CancelPredictionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl CancelPredictionRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
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
        SubActionCategory::PollsPredictions
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(PREDICTION_CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        PredictionUpdate {
            kind_id: KIND_ID,
            status: "CANCELED",
            winning_outcome: None,
        }
        .execute(&self.transport, &self.identity, config, ctx)
        .await
    }
}

#[cfg(test)]
mod tests {
    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, make_ctx};

    #[tokio::test]
    async fn cancel_sends_canceled_body_without_winning_outcome_id() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = CancelPredictionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        let stack = ArgStack::new().set(
            "prediction.id".to_owned(),
            Variant::String("pr42".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&PREDICTION_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body,
            Some(serde_json::json!({ "id": "pr42", "status": "CANCELED" })),
            "cancel must send CANCELED (single L) with no winning_outcome_id key"
        );
    }
}
