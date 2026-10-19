use forge_types::{Action, Queue, SubActionStep, TriggerInstance};

use crate::pack::documents::{ActionDocument, QueueDocument, StepDocument, TriggerDocument};

pub(crate) fn action_of(document: &ActionDocument) -> Action {
    Action {
        id: document.id,
        name: document.name.clone(),
        group: document.group.clone(),
        queue_id: document.queue_id,
        enabled: document.enabled,
        concurrent: document.concurrent,
        bypass_pause: document.bypass_pause,
        execution_mode: document.execution_mode,
        description: document.description.clone(),
        sub_actions: document.steps.iter().map(step_of).collect(),
    }
}

fn step_of(document: &StepDocument) -> SubActionStep {
    SubActionStep {
        kind_id: document.kind_id.clone(),
        config: document.config.clone(),
        enabled: document.enabled,
        continue_on_error: document.continue_on_error,
        condition: document.condition.clone(),
        label: document.label.clone(),
    }
}

pub(crate) fn trigger_of(document: &TriggerDocument) -> TriggerInstance {
    TriggerInstance {
        id: document.id,
        kind_id: document.kind_id.clone(),
        name: document.name.clone(),
        overrides: document.overrides.clone(),
        enabled: document.enabled,
        user_defined: document.user_defined,
        platform_scope: document.platform_scope.clone(),
        cooldown_secs: document.cooldown_secs,
        cooldown_global: document.cooldown_global,
        permission_rung: document.permission_rung,
    }
}

pub(crate) fn queue_of(document: &QueueDocument) -> Queue {
    Queue {
        id: document.id,
        name: document.name.clone(),
        description: document.description.clone(),
        concurrency: document.concurrency,
    }
}
