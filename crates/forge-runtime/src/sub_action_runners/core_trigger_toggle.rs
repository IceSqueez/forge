use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_storage::TriggerInstanceRepo;
use forge_types::{
    ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry, TriggerInstanceId,
};

use super::core_trigger_shared::{TRIGGER, TRIGGER_SPEC};
use crate::core_messages;

pub struct CoreTriggerToggleRunner {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
}

impl CoreTriggerToggleRunner {
    pub fn new(trigger_instances: Arc<dyn TriggerInstanceRepo>) -> Self {
        Self { trigger_instances }
    }

    async fn run(&self, instance_id: TriggerInstanceId) -> SubActionOutcome {
        match self.trigger_instances.toggle_enabled(instance_id).await {
            Ok(Some(_)) => SubActionOutcome::Success,
            Ok(None) => SubActionOutcome::Failed(
                core_messages::unknown_trigger_instance(instance_id).into(),
            ),
            Err(e) => SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreTriggerToggleRunner {
    fn id(&self) -> &str {
        "core.trigger.toggle"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Toggle Trigger"
    }

    fn summary(&self) -> &str {
        "Flip a trigger instance between enabled and disabled"
    }

    fn search_text(&self) -> &str {
        "toggle trigger flip switch enable disable"
    }

    fn icon_name(&self) -> &str {
        "switch"
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
