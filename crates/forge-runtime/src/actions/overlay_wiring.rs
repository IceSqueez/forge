use std::fmt;
use std::time::Duration;

use forge_overlay::config::DURATION;
use forge_overlay::{
    EventWiringTrigger, OverlayConfig, OverlayKindDescriptor, accepts_event_wiring,
    effective_overlay_config, suggested_content,
};
use forge_registry::{TriggerKindDescriptor, declared_variables};
use forge_storage::{OverlayDefinition, StorageError};
use forge_types::{
    Action, ActionId, ExecutionMode, PermissionRung, PlatformScope, Queue, QueueId,
    SubActionConfig, SubActionStep, TriggerConfig, TriggerInstance, TriggerInstanceId, Variant,
};

use crate::sub_action_runners::{
    OVERLAY_SEND_KIND_ID, OVERLAY_TARGET_KEY, WAIT_KIND_ID, WAIT_MS_KEY,
};

pub const OVERLAY_ALERT_QUEUE: QueuePlacement = QueuePlacement {
    name: "Overlay Alerts",
    description: "Shows one overlay alert at a time, so a second event waits its turn.",
    concurrency: 1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuePlacement {
    pub name: &'static str,
    pub description: &'static str,
    pub concurrency: u32,
}

impl QueuePlacement {
    pub fn into_queue(self) -> Queue {
        Queue {
            id: QueueId::new(),
            name: self.name.to_owned(),
            description: self.description.to_owned(),
            concurrency: self.concurrency,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WiredQueue {
    Existing(Queue),
    Created(Queue),
}

impl WiredQueue {
    pub fn queue(&self) -> &Queue {
        match self {
            Self::Existing(queue) | Self::Created(queue) => queue,
        }
    }

    pub fn id(&self) -> QueueId {
        self.queue().id
    }

    pub fn created(&self) -> Option<&Queue> {
        match self {
            Self::Created(queue) => Some(queue),
            Self::Existing(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedAction {
    pub id: ActionId,
    pub name: String,
    pub steps: Vec<SubActionStep>,
}

impl PlannedAction {
    pub fn on_queue(&self, queue_id: QueueId) -> Action {
        Action {
            id: self.id,
            name: self.name.clone(),
            group: None,
            queue_id,
            enabled: true,
            concurrent: false,
            bypass_pause: false,
            execution_mode: ExecutionMode::Sequential,
            description: None,
            sub_actions: self.steps.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayWiringPlan {
    pub queue: QueuePlacement,
    pub action: PlannedAction,
    pub trigger: TriggerInstance,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OverlayWiringRefusal {
    #[error("a '{overlay_kind_id}' overlay does not take event wiring")]
    OverlayKindNotEligible { overlay_kind_id: String },
    #[error("'{trigger_kind_id}' declares no variables an overlay could show")]
    TriggerDeclaresNoVariables { trigger_kind_id: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayWiringRecords {
    pub queue: Option<WiredQueue>,
    pub action_id: Option<ActionId>,
    pub trigger_instance_id: Option<TriggerInstanceId>,
    pub linked: bool,
}

impl fmt::Display for OverlayWiringRecords {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.action_id {
            Some(id) => write!(f, "action {id}")?,
            None => f.write_str("no action")?,
        }
        match self.trigger_instance_id {
            Some(id) => write!(f, ", trigger {id}")?,
            None => f.write_str(", no trigger")?,
        }
        f.write_str(if self.linked {
            ", linked"
        } else {
            ", not linked"
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayWiringOutcome {
    AlreadyWired { action_id: ActionId },
    Wired(OverlayWiringRecords),
}

#[derive(Debug, thiserror::Error)]
pub enum OverlayWiringError {
    #[error(transparent)]
    Refused(#[from] OverlayWiringRefusal),
    #[error("nothing was written: {source}")]
    NothingWritten {
        #[source]
        source: StorageError,
    },
    #[error("wiring stopped with {records}: {source}")]
    Incomplete {
        records: OverlayWiringRecords,
        #[source]
        source: StorageError,
    },
}

pub fn plan_overlay_wiring(
    overlay: &OverlayDefinition,
    kind: &dyn OverlayKindDescriptor,
    trigger: &dyn TriggerKindDescriptor,
) -> Result<OverlayWiringPlan, OverlayWiringRefusal> {
    if !accepts_event_wiring(kind) {
        return Err(OverlayWiringRefusal::OverlayKindNotEligible {
            overlay_kind_id: overlay.kind_id.clone(),
        });
    }

    let variables = declared_variables(trigger)
        .filter(|declared| !declared.is_empty())
        .ok_or_else(|| OverlayWiringRefusal::TriggerDeclaresNoVariables {
            trigger_kind_id: trigger.id().to_owned(),
        })?;

    let wiring_trigger = EventWiringTrigger {
        kind_id: trigger.id(),
        label: trigger.label(),
        variables: &variables,
    };

    Ok(OverlayWiringPlan {
        queue: OVERLAY_ALERT_QUEUE,
        action: PlannedAction {
            id: ActionId::new(),
            name: action_name(&overlay.display_name, trigger.label()),
            steps: steps(
                overlay,
                kind,
                &wiring_trigger,
                &effective_overlay_config(kind, &overlay.config),
            ),
        },
        trigger: TriggerInstance {
            id: TriggerInstanceId::new(),
            kind_id: trigger.id().to_owned(),
            name: trigger_name(&overlay.display_name, trigger.label()),
            overrides: TriggerConfig::new(),
            enabled: true,
            user_defined: true,
            platform_scope: PlatformScope::Any,
            cooldown_secs: 0,
            cooldown_global: true,
            permission_rung: PermissionRung::Everyone,
        },
    })
}

fn action_name(overlay_display_name: &str, trigger_label: &str) -> String {
    format!("{overlay_display_name} on {trigger_label}")
}

fn trigger_name(overlay_display_name: &str, trigger_label: &str) -> String {
    format!("{trigger_label} for {overlay_display_name}")
}

fn steps(
    overlay: &OverlayDefinition,
    kind: &dyn OverlayKindDescriptor,
    trigger: &EventWiringTrigger<'_>,
    effective: &OverlayConfig,
) -> Vec<SubActionStep> {
    let mut steps = vec![step(
        OVERLAY_SEND_KIND_ID,
        send_config(overlay, kind, trigger),
    )];
    if let Some(ms) = reveal_wait_ms(effective) {
        steps.push(step(
            WAIT_KIND_ID,
            SubActionConfig::from([(WAIT_MS_KEY.to_owned(), Variant::Int(ms))]),
        ));
    }
    steps
}

fn send_config(
    overlay: &OverlayDefinition,
    kind: &dyn OverlayKindDescriptor,
    trigger: &EventWiringTrigger<'_>,
) -> SubActionConfig {
    let mut config = suggested_content(kind, trigger);
    config.insert(
        OVERLAY_TARGET_KEY.to_owned(),
        Variant::String(overlay.id.as_str().to_owned()),
    );
    config
}

fn step(kind_id: &str, config: SubActionConfig) -> SubActionStep {
    SubActionStep {
        kind_id: kind_id.to_owned(),
        config,
        enabled: true,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

fn reveal_wait_ms(effective: &OverlayConfig) -> Option<i64> {
    let seconds = u64::try_from(effective.get(DURATION).and_then(Variant::as_int)?).ok()?;
    i64::try_from(Duration::from_secs(seconds).as_millis()).ok()
}
