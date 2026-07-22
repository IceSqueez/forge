use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};

use super::enable_reward::{
    config_fields, default_config, execute_bool_runner, validate_reward_id,
};
use super::identity::SelfIdentity;
use crate::helix::HelixTransport;

const KIND_ID: &str = "twitch.channel_points.disable_reward";

pub struct DisableRewardRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl DisableRewardRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }
}

#[async_trait]
impl SubActionRunner for DisableRewardRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::ChannelPoints
    }

    fn label(&self) -> &str {
        "Disable Channel Point Reward"
    }

    fn summary(&self) -> &str {
        "Disables a custom channel point reward so viewers cannot redeem it."
    }

    fn search_text(&self) -> &str {
        "twitch channel points custom reward disable off redemption"
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
            "is_enabled",
            false,
            config,
            ctx,
        )
        .await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::{HelixMethod, HelixTransport};
    use crate::sub_actions::identity::SelfIdentity;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, SELF_USER_ID, make_ctx};
    use forge_types::{SubActionOutcome, Variant};

    #[tokio::test]
    async fn disable_patches_is_enabled_false() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = DisableRewardRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        let stack = ArgStack::new().set("reward.id".to_owned(), Variant::String("rw1".to_owned()));

        let (telemetry, _) = runner.execute(&default_config(), &make_ctx(&stack)).await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Patch);
        assert_eq!(request.path, "/helix/channel_points/custom_rewards");
        assert!(
            request
                .query
                .contains(&("broadcaster_id".to_owned(), SELF_USER_ID.to_owned()))
        );
        assert!(request.query.contains(&("id".to_owned(), "rw1".to_owned())));
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "is_enabled": false }))
        );
    }
}
