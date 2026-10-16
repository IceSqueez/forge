use forge_registry::{ChainSignal, ControlSignal, RunContext, decode_steps};
use forge_types::{SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry};

pub(super) fn decode_chain(config: &SubActionConfig, key: &str) -> Vec<SubActionStep> {
    decode_steps(config.get(key))
}

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
