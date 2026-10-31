use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use super::fields::TARGET_LOGIN;
use crate::account::identity::{SelfIdentity, resolve_user_id};
use crate::helix::{HelixMethod, HelixRequest, HelixTransport};
use crate::step_failure;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::MODERATION_ADD_VIP;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TARGET_LOGIN]);

pub(crate) struct AddVipRunner {
    transport: Arc<dyn HelixTransport>,
    identity: Arc<SelfIdentity>,
}

impl AddVipRunner {
    pub(crate) fn new(transport: Arc<dyn HelixTransport>, identity: Arc<SelfIdentity>) -> Self {
        Self {
            transport,
            identity,
        }
    }

    async fn apply(&self, target_login: &str) -> SubActionOutcome {
        let broadcaster_id = match self.identity.user_id().await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let user_id = match resolve_user_id(self.transport.as_ref(), target_login).await {
            Ok(id) => id,
            Err(e) => return step_failure::failed(&e),
        };
        let request = HelixRequest::new(HelixMethod::Post, "/helix/channels/vips")
            .query("broadcaster_id", broadcaster_id)
            .query("user_id", user_id);
        step_failure::outcome(&self.transport.execute(request).await)
    }
}

#[async_trait]
impl SubActionRunner for AddVipRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Moderation
    }

    fn label(&self) -> &str {
        "Add VIP"
    }

    fn summary(&self) -> &str {
        "Grants VIP status to a user in the channel."
    }

    fn search_text(&self) -> &str {
        "twitch moderation add vip grant role channel"
    }

    fn icon_name(&self) -> &str {
        "star-plus"
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
