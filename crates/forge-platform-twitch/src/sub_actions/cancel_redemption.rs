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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::sub_actions::fulfill_redemption::redemption_default_config;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, make_ctx};

    // The ONE behavior cancel_redemption owns: its body status is "CANCELED"
    // (American single-L spelling). Fails if a second L slips in. The shared
    // query/validation/leak path is covered by fulfill_redemption's tests.
    #[tokio::test]
    async fn cancel_sends_status_canceled_single_l() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = CancelRedemptionRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        let stack = ArgStack::new()
            .set(
                "redemption.id".to_owned(),
                Variant::String("rd5".to_owned()),
            )
            .set("reward.id".to_owned(), Variant::String("rw7".to_owned()));

        let (telemetry, _) = runner
            .execute(&redemption_default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body,
            Some(serde_json::json!({ "status": "CANCELED" })),
            "cancel must send CANCELED (single L), not CANCELLED"
        );
    }
}
