use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_storage::ActionRepo;
use forge_types::{ActionId, ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::core_action_shared::{ACTION, ACTION_SPEC};

pub struct CoreActionToggleRunner {
    actions: Arc<dyn ActionRepo>,
}

impl CoreActionToggleRunner {
    pub fn new(actions: Arc<dyn ActionRepo>) -> Self {
        Self { actions }
    }

    async fn run(&self, action_id: ActionId) -> SubActionOutcome {
        match self.actions.toggle_enabled(action_id).await {
            Ok(Some(_)) => SubActionOutcome::Success,
            Ok(None) => SubActionOutcome::Failed(format!(
                "core.action.toggle: unknown action '{action_id}'"
            )),
            Err(e) => SubActionOutcome::Failed(format!("core.action.toggle: {e}")),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreActionToggleRunner {
    fn id(&self) -> &str {
        "core.action.toggle"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Toggle Action"
    }

    fn summary(&self) -> &str {
        "Flip an action between enabled and disabled"
    }

    fn search_text(&self) -> &str {
        "toggle action flip switch enable disable"
    }

    fn icon_name(&self) -> &str {
        "switch"
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
