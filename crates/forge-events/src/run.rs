use forge_types::{ActionId, QueueId, SubActionOutcome};
use serde::{Deserialize, Serialize};

use crate::TypedEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    Success,
    Failed,
    Skipped,
}

impl From<&SubActionOutcome> for RunOutcome {
    fn from(outcome: &SubActionOutcome) -> Self {
        match outcome {
            SubActionOutcome::Success => Self::Success,
            SubActionOutcome::Failed(_) | SubActionOutcome::IntegrationDisabled(_) => Self::Failed,
            SubActionOutcome::Skipped(_) => Self::Skipped,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionStarted {
    pub action_id: ActionId,
    pub action_name: String,
    pub sub_action_count: usize,
}

impl TypedEvent for ActionStarted {
    const KIND: &'static str = "action.start";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionDone {
    pub action_id: ActionId,
    pub action_name: String,
    pub outcome: RunOutcome,
    pub total_ms: u64,
    #[serde(default)]
    pub manual_test: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl TypedEvent for ActionDone {
    const KIND: &'static str = "action.done";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSkipped {
    pub action_id: ActionId,
    pub reason: String,
    pub queue_id: QueueId,
}

impl TypedEvent for ActionSkipped {
    const KIND: &'static str = "action.skipped";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickActionDone {
    pub kind: String,
    pub outcome: RunOutcome,
    pub label: String,
    pub builtin_id: String,
}

impl TypedEvent for QuickActionDone {
    const KIND: &'static str = "action.quick.done";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubActionRun {
    pub step_index: usize,
    pub kind: String,
}

impl TypedEvent for SubActionRun {
    const KIND: &'static str = "subaction.run";
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubActionDone {
    pub step_index: usize,
    pub kind: String,
    pub outcome: RunOutcome,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<String>,
}

impl TypedEvent for SubActionDone {
    const KIND: &'static str = "subaction.done";
}
