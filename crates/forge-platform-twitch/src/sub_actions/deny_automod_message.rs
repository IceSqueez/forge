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
        SubActionCategory::Moderation
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::helix::{HelixMethod, HelixTransport};
    use crate::sub_actions::approve_automod_message::automod_default_config;
    use crate::sub_actions::test_support::{MockCreds, MockTransport, SELF_USER_ID, make_ctx};
    use forge_types::{SubActionOutcome, Variant};

    // Distinct-action contract: deny_message POSTs action "DENY" (the one field that
    // differs from approve). The shared body shape, interpolation, validation and
    // failure paths are covered once in approve_automod_message.rs; this asserts only
    // that this runner flips the action while still carrying self user_id and msg_id.
    #[tokio::test]
    async fn deny_posts_deny_action_with_user_id_and_msg_id() {
        let transport = Arc::new(MockTransport::returning(Ok(serde_json::Value::Null)));
        let runner = DenyAutomodMessageRunner::new(
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            Arc::new(SelfIdentity::new(Arc::new(MockCreds::with_identity()))),
        );
        let stack = ArgStack::new().set(
            "automod.message_id".to_owned(),
            Variant::String("msg42".to_owned()),
        );

        let (telemetry, _) = runner
            .execute(&automod_default_config(), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        let request = transport.request(0);
        assert_eq!(request.method, HelixMethod::Post);
        assert_eq!(
            request.body,
            Some(serde_json::json!({
                "user_id": SELF_USER_ID,
                "msg_id": "msg42",
                "action": "DENY",
            })),
        );
    }
}
