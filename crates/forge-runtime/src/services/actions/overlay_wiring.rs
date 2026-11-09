use forge_overlay::{
    EventWiringTrigger, OverlayConfig, OverlayKindDescriptor, accepts_event_wiring, content_fields,
    suggested_content,
};
use forge_registry::{TriggerKindDescriptor, declared_variables};
use forge_storage::{OverlayDefinition, OverlayId};
use forge_types::{
    Action, ActionId, DEFAULT_QUEUE_ID, ExecutionMode, Queue, QueueId, SubActionStep,
    TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};

use crate::sub_action_runners::{OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WiringTarget {
    NewAction {
        name: String,
        queue_id: Option<QueueId>,
    },
    ExistingAction(ActionId),
}

pub struct OverlayWiringRequest<'a> {
    pub overlay: &'a OverlayDefinition,
    pub kind: &'a dyn OverlayKindDescriptor,
    pub trigger: &'a dyn TriggerKindDescriptor,
    pub target: WiringTarget,
    pub content: OverlayConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlannedAction {
    Create { action: Action, queue: Queue },
    Extend { action: Action },
}

impl PlannedAction {
    pub fn action(&self) -> &Action {
        match self {
            Self::Create { action, .. } | Self::Extend { action } => action,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlannedTrigger {
    Create(TriggerInstance),
    Reuse(TriggerInstance),
}

impl PlannedTrigger {
    pub fn instance(&self) -> &TriggerInstance {
        match self {
            Self::Create(instance) | Self::Reuse(instance) => instance,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayWiringPlan {
    overlay_id: OverlayId,
    trigger_kind_id: String,
    action: PlannedAction,
    step: Option<SubActionStep>,
    trigger: PlannedTrigger,
}

impl OverlayWiringPlan {
    pub fn overlay_id(&self) -> &OverlayId {
        &self.overlay_id
    }

    pub fn trigger_kind_id(&self) -> &str {
        &self.trigger_kind_id
    }

    pub fn action(&self) -> &PlannedAction {
        &self.action
    }

    pub fn added_step(&self) -> Option<&SubActionStep> {
        self.step.as_ref()
    }

    pub fn trigger(&self) -> &PlannedTrigger {
        &self.trigger
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OverlayWiringPreview {
    Ready(Box<OverlayWiringPlan>),
    AlreadyWired { action_id: ActionId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayWiringRefusal {
    OverlayKindNotEligible { overlay_kind_id: String },
    TargetMissing { trigger_kind_id: String },
    ActionNameEmpty,
    ActionMissing { action_id: ActionId },
    QueueMissing { queue_id: QueueId },
    PlanOutdated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayWiringRecords {
    pub action_id: ActionId,
    pub action_created: bool,
    pub trigger_instance_id: TriggerInstanceId,
    pub trigger_created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayWiringOutcome {
    AlreadyWired { action_id: ActionId },
    Wired(OverlayWiringRecords),
}

pub fn suggest_overlay_content(
    kind: &dyn OverlayKindDescriptor,
    trigger: &dyn TriggerKindDescriptor,
) -> OverlayConfig {
    let variables = declared_variables(trigger).unwrap_or_default();
    suggested_content(
        kind,
        &EventWiringTrigger {
            kind_id: trigger.id(),
            label: trigger.label(),
            variables: &variables,
        },
    )
}

pub(crate) fn ensure_eligible(
    overlay: &OverlayDefinition,
    kind: &dyn OverlayKindDescriptor,
) -> Result<(), OverlayWiringRefusal> {
    if accepts_event_wiring(kind) {
        Ok(())
    } else {
        Err(OverlayWiringRefusal::OverlayKindNotEligible {
            overlay_kind_id: overlay.kind_id.clone(),
        })
    }
}

pub(crate) fn new_action_name(name: &str) -> Result<String, OverlayWiringRefusal> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        Err(OverlayWiringRefusal::ActionNameEmpty)
    } else {
        Ok(trimmed.to_owned())
    }
}

pub(crate) fn chosen_queue_id(queue_id: Option<QueueId>) -> QueueId {
    queue_id.unwrap_or(DEFAULT_QUEUE_ID)
}

pub(crate) fn send_step(
    overlay: &OverlayDefinition,
    kind: &dyn OverlayKindDescriptor,
    content: OverlayConfig,
) -> SubActionStep {
    let accepted: Vec<String> = content_fields(kind)
        .iter()
        .map(|field| field.key().to_owned())
        .collect();
    let mut config: OverlayConfig = content
        .into_iter()
        .filter(|(key, _)| accepted.contains(key))
        .collect();
    config.insert(
        OVERLAY_TARGET_KEY.to_owned(),
        Variant::String(overlay.id.as_str().to_owned()),
    );
    SubActionStep {
        kind_id: OVERLAY_SEND_KIND_ID.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

pub(crate) fn new_action(name: String, queue_id: QueueId, step: SubActionStep) -> Action {
    Action {
        id: ActionId::new(),
        name,
        group: None,
        queue_id,
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![step],
    }
}

pub(crate) fn extended(mut action: Action, step: SubActionStep) -> Action {
    action.sub_actions.push(step);
    action
}

pub(crate) fn new_trigger(
    overlay: &OverlayDefinition,
    trigger: &dyn TriggerKindDescriptor,
) -> TriggerInstance {
    TriggerInstance::new_user_defined(
        trigger.id(),
        format!("{} for {}", trigger.label(), overlay.display_name),
        TriggerConfig::new(),
    )
}

pub(crate) fn assemble(
    overlay: &OverlayDefinition,
    trigger_kind_id: &str,
    action: PlannedAction,
    step: Option<SubActionStep>,
    trigger: PlannedTrigger,
) -> OverlayWiringPlan {
    OverlayWiringPlan {
        overlay_id: overlay.id.clone(),
        trigger_kind_id: trigger_kind_id.to_owned(),
        action,
        step,
        trigger,
    }
}
