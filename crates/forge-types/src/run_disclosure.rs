//! Length-only projection of a persisted run for the publicly-attachable diagnostic bundle.
//! Unlike `variant_preview`, nothing here can carry a value's content: the bundle section is written
//! straight to the file and never passes through the subscriber's scrubber. Failure reasons are sizes
//! for the same reason - they quote third-party text, and the log corpus carries the readable form.

use std::collections::BTreeMap;
use std::fmt;

use time::OffsetDateTime;

use crate::execution::{
    ExecutionContext, ExecutionMetadata, ExecutionOutcome, SubActionOutcome, SubActionTelemetry,
};
use crate::ids::{ActionId, EventId};
use crate::redaction::STAMP;
use crate::variant::Variant;

/// Every arm that could hold authored text is a size instead, so no rendering of this type discloses a value.
#[derive(Debug, Clone, PartialEq)]
pub enum DisclosedValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Datetime(OffsetDateTime),
    Text { chars: usize },
    Array { items: usize },
    Object { entries: usize },
}

impl DisclosedValue {
    pub fn text(value: &str) -> Self {
        Self::Text {
            chars: value.chars().count(),
        }
    }
}

impl From<&Variant> for DisclosedValue {
    fn from(value: &Variant) -> Self {
        match value {
            Variant::Int(v) => Self::Int(*v),
            Variant::Float(v) => Self::Float(*v),
            Variant::Bool(v) => Self::Bool(*v),
            Variant::Datetime(v) => Self::Datetime(*v),
            Variant::String(v) => Self::text(v),
            Variant::Array(items) => Self::Array { items: items.len() },
            Variant::Object(fields) => Self::Object {
                entries: fields.len(),
            },
        }
    }
}

impl fmt::Display for DisclosedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(v) => write!(f, "{v}"),
            Self::Float(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Datetime(v) => write!(f, "{v}"),
            Self::Text { chars } => write!(f, "{STAMP} len={chars}>"),
            Self::Array { items } => write!(f, "array({items})"),
            Self::Object { entries } => write!(f, "object({entries})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DisclosedTrigger {
    Event {
        event_id: EventId,
        trigger_kind: Option<String>,
    },
    QuickAction {
        builtin_id: String,
        label_chars: usize,
    },
}

impl From<&ExecutionMetadata> for DisclosedTrigger {
    fn from(metadata: &ExecutionMetadata) -> Self {
        match metadata {
            ExecutionMetadata::Trigger {
                event_id,
                trigger_kind,
            } => Self::Event {
                event_id: *event_id,
                trigger_kind: trigger_kind.clone(),
            },
            ExecutionMetadata::QuickAction { builtin_id, label } => Self::QuickAction {
                builtin_id: builtin_id.clone(),
                label_chars: label.chars().count(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisclosedOutcome {
    Success,
    Failed { reason_chars: usize },
    Cancelled,
}

impl From<&ExecutionOutcome> for DisclosedOutcome {
    fn from(outcome: &ExecutionOutcome) -> Self {
        match outcome {
            ExecutionOutcome::Success => Self::Success,
            ExecutionOutcome::Failed(reason) => Self::Failed {
                reason_chars: reason.chars().count(),
            },
            ExecutionOutcome::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisclosedStepOutcome {
    Success,
    Failed { reason_chars: usize },
    Skipped { reason_chars: usize },
}

impl From<&SubActionOutcome> for DisclosedStepOutcome {
    fn from(outcome: &SubActionOutcome) -> Self {
        match outcome {
            SubActionOutcome::Success => Self::Success,
            SubActionOutcome::Failed(reason) => Self::Failed {
                reason_chars: reason.chars().count(),
            },
            SubActionOutcome::Skipped(reason) => Self::Skipped {
                reason_chars: reason.chars().count(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisclosedStep {
    /// `None` for a step lifted out of a branch or loop body, whose `kind` carries the parent path instead.
    pub index: Option<usize>,
    pub kind: String,
    pub started_at: OffsetDateTime,
    pub duration_ms: u64,
    pub outcome: DisclosedStepOutcome,
    /// Sizes are of each argument's persisted rendering, which is not the length of the source value.
    pub args_in: BTreeMap<String, DisclosedValue>,
    pub produced: BTreeMap<String, DisclosedValue>,
}

impl From<&SubActionTelemetry> for DisclosedStep {
    fn from(step: &SubActionTelemetry) -> Self {
        Self {
            index: (!step.is_nested()).then_some(step.index),
            kind: step.kind.clone(),
            started_at: step.started_at,
            duration_ms: step.duration_ms,
            outcome: DisclosedStepOutcome::from(&step.outcome),
            args_in: disclose_rendered(&step.args_in),
            produced: disclose_rendered(&step.produced),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisclosedRun {
    pub action_id: ActionId,
    pub trigger: DisclosedTrigger,
    pub started_at: OffsetDateTime,
    pub completed_at: Option<OffsetDateTime>,
    pub outcome: DisclosedOutcome,
    pub arguments: BTreeMap<String, DisclosedValue>,
    pub steps: Vec<DisclosedStep>,
}

impl From<&ExecutionContext> for DisclosedRun {
    fn from(ctx: &ExecutionContext) -> Self {
        Self {
            action_id: ctx.action_id,
            trigger: DisclosedTrigger::from(&ctx.metadata),
            started_at: ctx.started_at,
            completed_at: ctx.completed_at,
            outcome: DisclosedOutcome::from(&ctx.outcome),
            arguments: disclose_variants(&ctx.arg_stack_snapshot),
            steps: ctx.telemetry.iter().map(DisclosedStep::from).collect(),
        }
    }
}

fn disclose_variants(values: &BTreeMap<String, Variant>) -> BTreeMap<String, DisclosedValue> {
    values
        .iter()
        .map(|(name, value)| (name.clone(), DisclosedValue::from(value)))
        .collect()
}

fn disclose_rendered(values: &BTreeMap<String, String>) -> BTreeMap<String, DisclosedValue> {
    values
        .iter()
        .map(|(name, value)| (name.clone(), DisclosedValue::text(value)))
        .collect()
}
