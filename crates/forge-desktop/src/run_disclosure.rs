use std::collections::BTreeMap;
use std::fmt;

use forge_types::{
    ActionId, EventId, ExecutionContext, ExecutionMetadata, ExecutionOutcome, IntegrationId, STAMP,
    SubActionOutcome, SubActionTelemetry, Variant, datetime_to_text,
};
use time::OffsetDateTime;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DisclosedValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Datetime(OffsetDateTime),
    Text { chars: usize },
    Array { items: usize },
    Object { entries: usize },
}

impl DisclosedValue {
    pub(crate) fn text(value: &str) -> Self {
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
            Self::Datetime(v) => f.write_str(&datetime_to_text(v)),
            Self::Text { chars } => write!(f, "{STAMP} len={chars}>"),
            Self::Array { items } => write!(f, "array({items})"),
            Self::Object { entries } => write!(f, "object({entries})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum DisclosedTrigger {
    Event {
        event_id: EventId,
        trigger_kind: Option<String>,
    },
    QuickAction {
        builtin_id: String,
        label_chars: usize,
    },
    Scheduled {
        event_id: EventId,
        scheduled_run_id: i64,
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
            ExecutionMetadata::Scheduled {
                event_id,
                scheduled_run_id,
                ..
            } => Self::Scheduled {
                event_id: *event_id,
                scheduled_run_id: *scheduled_run_id,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DisclosedOutcome {
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
pub(crate) enum DisclosedStepOutcome {
    Success,
    Failed { reason_chars: usize },
    Skipped { reason_chars: usize },
    IntegrationDisabled(IntegrationId),
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
            SubActionOutcome::IntegrationDisabled(integration) => {
                Self::IntegrationDisabled(integration.clone())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DisclosedStep {
    pub(crate) index: Option<usize>,
    pub(crate) kind: String,
    pub(crate) started_at: OffsetDateTime,
    pub(crate) duration_ms: u64,
    pub(crate) outcome: DisclosedStepOutcome,
    pub(crate) args_in: BTreeMap<String, DisclosedValue>,
    pub(crate) produced: BTreeMap<String, DisclosedValue>,
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
pub(crate) struct DisclosedRun {
    pub(crate) action_id: ActionId,
    pub(crate) trigger: DisclosedTrigger,
    pub(crate) started_at: OffsetDateTime,
    pub(crate) completed_at: Option<OffsetDateTime>,
    pub(crate) outcome: DisclosedOutcome,
    pub(crate) arguments: BTreeMap<String, DisclosedValue>,
    pub(crate) steps: Vec<DisclosedStep>,
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
