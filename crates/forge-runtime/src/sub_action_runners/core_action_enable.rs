use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_storage::ActionRepo;
use forge_types::{
    ActionId, ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, UserText,
};

use super::core_action_shared::{ACTION, ACTION_SPEC};

pub struct CoreActionEnableRunner {
    actions: Arc<dyn ActionRepo>,
}

impl CoreActionEnableRunner {
    pub fn new(actions: Arc<dyn ActionRepo>) -> Self {
        Self { actions }
    }

    async fn run(&self, action_id: ActionId) -> SubActionOutcome {
        match self.actions.set_enabled(action_id, true).await {
            Ok(true) => SubActionOutcome::Success,
            Ok(false) => SubActionOutcome::Failed(UserText::unlocalized(format!(
                "core.action.enable: unknown action '{action_id}'"
            ))),
            Err(e) => {
                SubActionOutcome::Failed(UserText::unlocalized(format!("core.action.enable: {e}")))
            }
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreActionEnableRunner {
    fn id(&self) -> &str {
        "core.action.enable"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Enable Action"
    }

    fn summary(&self) -> &str {
        "Allow an action to run when triggered"
    }

    fn search_text(&self) -> &str {
        "enable action allow activate"
    }

    fn icon_name(&self) -> &str {
        "toggle-on"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ACTION_SPEC)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let action_id = match ACTION.read(config, ctx.arg_stack) {
            Ok(action_id) => action_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self.run(action_id).await;
        (timer.finish(outcome), None)
    }
}
