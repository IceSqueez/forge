use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::TARGET_LOGIN;
use crate::account::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::MODERATION_REMOVE_VIP;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TARGET_LOGIN]);

pub struct RemoveVipRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl RemoveVipRunner {
    pub fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, target_login: &str) -> SubActionOutcome {
        if target_login.is_empty() {
            return SubActionOutcome::Failed(
                "target_user_login is empty after interpolation".to_owned(),
            );
        }
        let broadcaster_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let user_id = match resolve_user_id(self.transport.as_ref(), target_login).await {
            Ok(id) => id,
            Err(e) => return SubActionOutcome::Failed(e.to_string()),
        };
        let request = HelixRequest::new(HelixMethod::Delete, "/helix/channels/vips")
            .query("broadcaster_id", broadcaster_id)
            .query("user_id", user_id);
        SubActionOutcome::from_result(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for RemoveVipRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Remove VIP"
    }

    fn summary(&self) -> &str {
        "Revokes VIP status from a user in the channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation remove vip revoke unvip channel"
    }

    fn icon_name(&self) -> &str {
        "star-minus"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let target_login = match TARGET_LOGIN.read(config, ctx.arg_stack) {
            Ok(login) => login,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        (timer.finish(self.apply(&target_login).await), None)
    }
}
