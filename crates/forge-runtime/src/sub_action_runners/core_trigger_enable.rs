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

pub struct CoreTriggerEnableRunner {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
}

impl CoreTriggerEnableRunner {
    pub fn new(trigger_instances: Arc<dyn TriggerInstanceRepo>) -> Self {
        Self { trigger_instances }
    }

    async fn run(&self, instance_id: TriggerInstanceId) -> SubActionOutcome {
        match self.trigger_instances.set_enabled(instance_id, true).await {
            Ok(()) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreTriggerEnableRunner {
    fn id(&self) -> &str {
        "core.trigger.enable"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Enable Trigger"
    }

    fn summary(&self) -> &str {
        "Allow a trigger instance to fire actions"
    }

    fn search_text(&self) -> &str {
        "enable trigger allow activate"
    }

    fn icon_name(&self) -> &str {
        "toggle-on"
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
