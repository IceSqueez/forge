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

#[derive(Clone, Copy)]
struct StepCopy {
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    search_text: &'static str,
    icon_name: &'static str,
}

#[derive(Clone, Copy)]
enum StateChange {
    Set(bool),
    Flip,
}

const ENABLE_COPY: StepCopy = StepCopy {
    id: "core.trigger.enable",
    label: "Enable Trigger",
    summary: "Allow a trigger instance to fire actions",
    search_text: "enable trigger allow activate",
    icon_name: "toggle-on",
};

const DISABLE_COPY: StepCopy = StepCopy {
    id: "core.trigger.disable",
    label: "Disable Trigger",
    summary: "Prevent a trigger instance from firing actions",
    search_text: "disable trigger block deactivate stop",
    icon_name: "toggle-off",
};

const TOGGLE_COPY: StepCopy = StepCopy {
    id: "core.trigger.toggle",
    label: "Toggle Trigger",
    summary: "Flip a trigger instance between enabled and disabled",
    search_text: "toggle trigger flip switch enable disable",
    icon_name: "switch",
};

pub struct CoreTriggerStateRunner {
    trigger_instances: Arc<dyn TriggerInstanceRepo>,
    change: StateChange,
    copy: StepCopy,
}

impl CoreTriggerStateRunner {
    pub fn enable(trigger_instances: Arc<dyn TriggerInstanceRepo>) -> Self {
        Self {
            trigger_instances,
            change: StateChange::Set(true),
            copy: ENABLE_COPY,
        }
    }

    pub fn disable(trigger_instances: Arc<dyn TriggerInstanceRepo>) -> Self {
        Self {
            trigger_instances,
            change: StateChange::Set(false),
            copy: DISABLE_COPY,
        }
    }

    pub fn toggle(trigger_instances: Arc<dyn TriggerInstanceRepo>) -> Self {
        Self {
            trigger_instances,
            change: StateChange::Flip,
            copy: TOGGLE_COPY,
        }
    }

    async fn run(&self, instance_id: TriggerInstanceId) -> SubActionOutcome {
        let found = match self.change {
            StateChange::Set(enabled) => {
                self.trigger_instances
                    .set_enabled(instance_id, enabled)
                    .await
            }
            StateChange::Flip => self
                .trigger_instances
                .toggle_enabled(instance_id)
                .await
                .map(|state| state.is_some()),
        };
        match found {
            Ok(true) => SubActionOutcome::Success,
            Ok(false) => SubActionOutcome::Failed(
                core_messages::unknown_trigger_instance(instance_id).into(),
            ),
            Err(e) => SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
        }
    }
}

#[async_trait]
impl SubActionRunner for CoreTriggerStateRunner {
    fn id(&self) -> &str {
        self.copy.id
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        self.copy.label
    }

    fn summary(&self) -> &str {
        self.copy.summary
    }

    fn search_text(&self) -> &str {
        self.copy.search_text
    }

    fn icon_name(&self) -> &str {
        self.copy.icon_name
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
