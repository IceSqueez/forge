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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use time::Duration;

    /// A string no rendering of a disclosed run may ever contain.
    const SENTINEL: &str = "nova_the_viewer";

    fn instant(offset_secs: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).expect("valid timestamp")
            + Duration::seconds(offset_secs)
    }

    fn map(pairs: &[(&str, Variant)]) -> BTreeMap<String, Variant> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    fn rendered(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn step(index: usize, kind: &str, outcome: SubActionOutcome) -> SubActionTelemetry {
        SubActionTelemetry {
            index,
            kind: kind.to_owned(),
            started_at: instant(5),
            duration_ms: 42,
            outcome,
            args_in: rendered(&[("message", SENTINEL)]),
            produced: rendered(&[("reply", SENTINEL)]),
        }
    }

    #[test]
    fn variant_disclosure_reports_a_size_for_every_content_bearing_variant() {
        let at = instant(0);
        for (variant, expected) in [
            (Variant::Int(-7), DisclosedValue::Int(-7)),
            (Variant::Float(1.5), DisclosedValue::Float(1.5)),
            (Variant::Bool(true), DisclosedValue::Bool(true)),
            (Variant::Datetime(at), DisclosedValue::Datetime(at)),
            (
                Variant::String("Привіт".to_owned()),
                DisclosedValue::Text { chars: 6 },
            ),
            (
                Variant::String(String::new()),
                DisclosedValue::Text { chars: 0 },
            ),
            (
                Variant::Array(vec![Variant::Int(1), Variant::Int(2)]),
                DisclosedValue::Array { items: 2 },
            ),
            (
                Variant::Array(Vec::new()),
                DisclosedValue::Array { items: 0 },
            ),
            (
                Variant::Object(map(&[("a", Variant::Bool(false))])),
                DisclosedValue::Object { entries: 1 },
            ),
        ] {
            assert_eq!(
                DisclosedValue::from(&variant),
                expected,
                "variant {variant:?}"
            );
        }
    }

    /// Why: a container is reduced to a count and never walked, so a string buried at any depth
    /// has no path into a rendering. If `Array`/`Object` ever gained recursive arms this breaks.
    #[test]
    fn a_string_nested_inside_a_container_is_unreachable_from_any_rendering() {
        let deep = Variant::Array(vec![Variant::Object(map(&[(
            "inner",
            Variant::Array(vec![Variant::String(SENTINEL.to_owned())]),
        )]))]);

        let disclosed = DisclosedValue::from(&deep);

        assert_eq!(disclosed.to_string(), "array(1)");
        assert!(
            !format!("{disclosed:?}").contains(SENTINEL),
            "the debug rendering leaked a nested string: {disclosed:?}",
        );
    }

    #[test]
    fn text_display_reports_the_character_count_behind_the_shared_stamp() {
        for (value, chars) in [("", 0), ("abc", 3), ("Привіт", 6), ("🙂🙂", 2)] {
            assert_eq!(
                DisclosedValue::text(value).to_string(),
                format!("{STAMP} len={chars}>"),
                "value {value:?}",
            );
        }
    }

    #[test]
    fn outcome_disclosure_reduces_every_failure_reason_to_a_character_count() {
        for (outcome, expected) in [
            (ExecutionOutcome::Success, DisclosedOutcome::Success),
            (ExecutionOutcome::Cancelled, DisclosedOutcome::Cancelled),
            (
                ExecutionOutcome::Failed("Привіт".to_owned()),
                DisclosedOutcome::Failed { reason_chars: 6 },
            ),
        ] {
            assert_eq!(DisclosedOutcome::from(&outcome), expected);
        }
    }

    #[test]
    fn step_outcome_disclosure_reduces_failed_and_skipped_reasons_to_character_counts() {
        for (outcome, expected) in [
            (SubActionOutcome::Success, DisclosedStepOutcome::Success),
            (
                SubActionOutcome::Failed("Привіт".to_owned()),
                DisclosedStepOutcome::Failed { reason_chars: 6 },
            ),
            (
                SubActionOutcome::Skipped("ab".to_owned()),
                DisclosedStepOutcome::Skipped { reason_chars: 2 },
            ),
        ] {
            assert_eq!(DisclosedStepOutcome::from(&outcome), expected);
        }
    }

    #[test]
    fn a_nested_step_discloses_no_position_while_a_top_level_one_keeps_its_index() {
        for (index, expected) in [
            (0usize, Some(0usize)),
            (7, Some(7)),
            (SubActionTelemetry::NESTED, None),
        ] {
            let disclosed =
                DisclosedStep::from(&step(index, "core.log", SubActionOutcome::Success));
            assert_eq!(disclosed.index, expected, "telemetry index {index}");
        }
    }

    #[test]
    fn trigger_disclosure_keeps_the_event_id_and_kind_but_sizes_a_quick_action_label() {
        let event_id = EventId::new();
        let triggered = DisclosedTrigger::from(&ExecutionMetadata::Trigger {
            event_id,
            trigger_kind: Some("twitch.chat".to_owned()),
        });
        assert_eq!(
            triggered,
            DisclosedTrigger::Event {
                event_id,
                trigger_kind: Some("twitch.chat".to_owned()),
            },
        );

        let quick = DisclosedTrigger::from(&ExecutionMetadata::QuickAction {
            builtin_id: "twitch".to_owned(),
            label: SENTINEL.to_owned(),
        });
        assert_eq!(
            quick,
            DisclosedTrigger::QuickAction {
                builtin_id: "twitch".to_owned(),
                label_chars: SENTINEL.chars().count(),
            },
        );
    }

    fn context_with_sentinels(action_id: ActionId, event_id: EventId) -> ExecutionContext {
        ExecutionContext {
            action_id,
            metadata: ExecutionMetadata::Trigger {
                event_id,
                trigger_kind: Some("twitch.chat".to_owned()),
            },
            arg_stack_snapshot: map(&[
                ("user", Variant::String(SENTINEL.to_owned())),
                (
                    "payload",
                    Variant::Object(map(&[("nick", Variant::String(SENTINEL.to_owned()))])),
                ),
            ]),
            started_at: instant(0),
            completed_at: Some(instant(3)),
            telemetry: vec![
                step(0, "twitch.send_chat", SubActionOutcome::Success),
                step(
                    SubActionTelemetry::NESTED,
                    "core.branch/0/core.log",
                    SubActionOutcome::Failed(SENTINEL.to_owned()),
                ),
            ],
            outcome: ExecutionOutcome::Failed(SENTINEL.to_owned()),
        }
    }

    #[test]
    fn run_disclosure_keeps_the_structure_a_reproduction_needs() {
        let action_id = ActionId::new();
        let event_id = EventId::new();

        let run = DisclosedRun::from(&context_with_sentinels(action_id, event_id));

        assert_eq!(run.action_id, action_id);
        assert_eq!(
            run.trigger,
            DisclosedTrigger::Event {
                event_id,
                trigger_kind: Some("twitch.chat".to_owned()),
            },
        );
        assert_eq!(run.started_at, instant(0));
        assert_eq!(run.completed_at, Some(instant(3)));
        assert_eq!(
            run.steps
                .iter()
                .map(|s| s.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["twitch.send_chat", "core.branch/0/core.log"],
        );
        assert_eq!(
            run.steps.iter().map(|s| s.duration_ms).collect::<Vec<_>>(),
            vec![42, 42],
        );
        assert_eq!(
            run.arguments.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["payload", "user"],
        );
    }

    /// Why: R5 - the bundle section is written straight to a file the reporter may attach to a
    /// public issue, so no arm of the projection may carry a value, a reason or a label.
    #[test]
    fn run_disclosure_carries_no_value_content_from_any_field() {
        let run = DisclosedRun::from(&context_with_sentinels(ActionId::new(), EventId::new()));

        assert!(
            !format!("{run:?}").contains(SENTINEL),
            "a value reached the disclosed run: {run:?}",
        );
    }
}
