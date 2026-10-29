use forge_registry::{ChainSignal, ControlSignal, RunContext, decode_chain};
use forge_types::{SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry, Variant};

pub(super) fn chain_at(
    config: &SubActionConfig,
    key: &str,
    ctx: &RunContext<'_>,
) -> Vec<SubActionStep> {
    runnable_steps(config.get(key), key, ctx)
}

pub(super) fn runnable_steps(
    chain: Option<&Variant>,
    arm: &str,
    ctx: &RunContext<'_>,
) -> Vec<SubActionStep> {
    let decoded = decode_chain(chain);
    for undecodable in decoded.undecodable() {
        tracing::warn!(
            event_id = %ctx.parent_event_id,
            step_index = ctx.index,
            arm,
            position = undecodable.position,
            reason = ?undecodable.reason,
            "nested step cannot be read; it is skipped and kept as stored"
        );
    }
    decoded.into_steps()
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
