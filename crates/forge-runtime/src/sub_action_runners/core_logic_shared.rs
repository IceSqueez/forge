use forge_registry::{ChainSignal, ControlSignal, RunContext};
use forge_types::{SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry, Variant};
use time::OffsetDateTime;

/// Decodes an inline sub-chain stored under `key` as a serialized step array. A
/// missing or malformed value yields an empty chain, so an unset branch is a no-op
/// rather than an error.
pub(super) fn decode_chain(config: &SubActionConfig, key: &str) -> Vec<SubActionStep> {
    decode_steps(config.get(key))
}

/// Walks the canonical stored chain form - `Variant::Array` of per-step
/// `Variant::Object`s - into a step list. Anything that is not an array, and any
/// element lacking a `kind_id`, is dropped so a missing or partially-authored
/// branch degrades to a no-op instead of failing the action.
pub(super) fn decode_steps(value: Option<&Variant>) -> Vec<SubActionStep> {
    let Some(steps) = value.and_then(Variant::as_array) else {
        return Vec::new();
    };
    steps
        .iter()
        .filter_map(|step| {
            let obj = step.as_object()?;
            let kind_id = obj.get("kind_id").and_then(Variant::as_str)?.to_owned();
            let config = match obj.get("config") {
                Some(Variant::Object(map)) => map.clone(),
                _ => SubActionConfig::new(),
            };
            let enabled = obj
                .get("enabled")
                .and_then(Variant::as_bool)
                .unwrap_or(true);
            let continue_on_error = obj
                .get("continue_on_error")
                .and_then(Variant::as_bool)
                .unwrap_or(false);
            let condition = obj
                .get("condition")
                .and_then(Variant::as_str)
                .map(str::to_owned);
            let label = obj
                .get("label")
                .and_then(Variant::as_str)
                .map(str::to_owned);
            Some(SubActionStep {
                kind_id,
                config,
                enabled,
                continue_on_error,
                condition,
                label,
            })
        })
        .collect()
}

/// Re-raises a child chain's terminal signal into the enclosing chain unchanged.
/// `Break`/`Continue`/`Stop` are re-armed on the control cell for an outer loop or
/// the action-root to act on, `Error` becomes a failed step, and
/// `Completed`/`Aborted` finish the step (cancellation is observed through the
/// shared cancel signal at the next boundary).
pub(super) fn propagate(signal: ChainSignal, ctx: &RunContext<'_>) -> SubActionOutcome {
    match signal {
        ChainSignal::Completed | ChainSignal::Aborted => SubActionOutcome::Success,
        ChainSignal::Break => {
            ctx.control.set(ControlSignal::Break);
            SubActionOutcome::Success
        }
        ChainSignal::Continue => {
            ctx.control.set(ControlSignal::Continue);
            SubActionOutcome::Success
        }
        ChainSignal::Stop(mark) => {
            ctx.control.set(ControlSignal::Stop(mark));
            SubActionOutcome::Success
        }
        ChainSignal::Error(msg) => SubActionOutcome::Failed(msg),
    }
}

/// Lifts a nested child chain's telemetry into flat rows for the enclosing chain
/// to splice after this composite step. Each row gains a leading `parent_index.arm`
/// path segment (segments joined by `/`) and its `index` is set to
/// `SubActionTelemetry::NESTED`, so surfaces keyed by top-level position skip it
/// while the failure trail stays legible. A row already lifted from a deeper body
/// keeps its trail; a fresh branch-chain row folds its local index and kind id into
/// the trail's final segment.
pub(super) fn retag(
    children: Vec<SubActionTelemetry>,
    parent_index: usize,
    arm: &str,
) -> Vec<SubActionTelemetry> {
    let prefix = format!("{parent_index}.{arm}");
    children
        .into_iter()
        .map(|mut child| {
            let trail = if child.is_nested() {
                child.kind
            } else {
                format!("{}.{}", child.index, child.kind)
            };
            child.kind = format!("{prefix}/{trail}");
            child.index = SubActionTelemetry::NESTED;
            child
        })
        .collect()
}

pub(super) fn telemetry(
    ctx: &RunContext<'_>,
    kind: &str,
    started_at: OffsetDateTime,
    outcome: SubActionOutcome,
) -> SubActionTelemetry {
    let duration_ms = (OffsetDateTime::now_utc() - started_at)
        .whole_milliseconds()
        .max(0) as u64;
    SubActionTelemetry {
        args_in: ::std::collections::BTreeMap::new(),
        produced: ::std::collections::BTreeMap::new(),
        index: ctx.index,
        kind: kind.to_owned(),
        started_at,
        duration_ms,
        outcome,
    }
}
