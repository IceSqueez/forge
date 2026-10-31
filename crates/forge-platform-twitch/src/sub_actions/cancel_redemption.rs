use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{ConfigSpec, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};

use super::fulfill_redemption::{REDEMPTION_CONFIG, execute_redemption_runner};
use crate::account::identity::SelfIdentity;
use crate::helix::HelixTransport;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::CHANNEL_POINTS_CANCEL_REDEMPTION;

pub(crate) struct CancelRedemptionRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl CancelRedemptionRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
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
        SubActionCategory::ChannelPoints
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

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(REDEMPTION_CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
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
mod tests {
    use forge_types::{ArgStack, SubActionOutcome, Variant};

    use super::*;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, make_ctx};

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
            .execute(&REDEMPTION_CONFIG.default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            transport.request(0).body,
            Some(serde_json::json!({ "status": "CANCELED" })),
            "cancel must send CANCELED (single L), not CANCELLED"
        );
    }
}
