use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_storage::TriggerInstanceRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, TriggerInstanceId, UserText,
};

use super::core_trigger_shared::{TRIGGER, TRIGGER_SPEC};

pub struct CoreTriggerDisableRunner {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
}

impl CoreTriggerDisableRunner {
    pub fn new(trigger_instances: Arc<dyn TriggerInstanceRepo>) -> Self {
        Self { trigger_instances }
    }

    async fn run(&self, instance_id: TriggerInstanceId) -> SubActionOutcome {
        match self.trigger_instances.set_enabled(instance_id, false).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(UserText::unlocalized(format!(
                "core.trigger.disable: {e}"
            ))),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreTriggerDisableRunner {
    fn id(&self) -> &str {
        "core.trigger.disable"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Disable Trigger"
    }

    fn summary(&self) -> &str {
        "Prevent a trigger instance from firing actions"
    }

    fn search_text(&self) -> &str {
        "disable trigger block deactivate stop"
    }

    fn icon_name(&self) -> &str {
        "toggle-off"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(TRIGGER_SPEC)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let instance_id = match TRIGGER.read(config, ctx.arg_stack) {
            Ok(instance_id) => instance_id,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = self.run(instance_id).await;
        (timer.finish(outcome), None)
    }
}
