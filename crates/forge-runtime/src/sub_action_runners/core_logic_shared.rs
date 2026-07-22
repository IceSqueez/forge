use forge_registry::{ChainSignal, ControlSignal, RunContext};
use forge_types::{SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry, Variant};

/// A missing or malformed value yields an empty chain, so an unset branch is a no-op, not an error.
pub(super) fn decode_chain(config: &SubActionConfig, key: &str) -> Vec<SubActionStep> {
    decode_steps(config.get(key))
}

/// Anything not an array, and any element lacking a `kind_id`, is dropped rather than failing the action.
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

/// `Break`/`Continue`/`Stop` re-arm the control cell; `Error` fails the step; others succeed.
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

/// Prepends a `parent_index.arm` path segment and marks `index` as `NESTED` so top-level surfaces skip the row.
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
