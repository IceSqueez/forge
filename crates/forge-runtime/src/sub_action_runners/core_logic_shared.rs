use forge_registry::{ChainSignal, ControlSignal, RunContext};
use forge_types::{SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry, Variant};
use time::OffsetDateTime;

/// Decodes an inline sub-chain stored under `key` as a serialized step array. A
/// missing or malformed value yields an empty chain, so an unset branch is a no-op
/// rather than an error.
pub(super) fn decode_chain(config: &SubActionConfig, key: &str) -> Vec<SubActionStep> {
    decode_steps(config.get(key))
}

/// Decodes a `Variant` (as authored into config) into a step list, reusing the
/// `Variant` <-> JSON bridge so an embedded chain round-trips through storage.
pub(super) fn decode_steps(value: Option<&Variant>) -> Vec<SubActionStep> {
    value
        .map(Variant::to_json)
        .and_then(|json| serde_json::from_value::<Vec<SubActionStep>>(json).ok())
        .unwrap_or_default()
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
        index: ctx.index,
        kind: kind.to_owned(),
        started_at,
        duration_ms,
        outcome,
    }
}
