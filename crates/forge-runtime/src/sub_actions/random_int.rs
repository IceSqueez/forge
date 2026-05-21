use forge_events::{Event, EventSource};
use forge_storage::{DataProvider, GlobalsRepo};
use forge_types::{
    ArgStack, EventId, SubActionOutcome, SubActionSpec, SubActionTelemetry, Variant,
};
use rand::RngExt;
use time::OffsetDateTime;

use crate::EventBus;

pub(super) async fn run(
    spec: &SubActionSpec,
    _arg_stack: &ArgStack,
    index: usize,
    parent_event_id: EventId,
    bus: &EventBus,
    dp: &dyn DataProvider,
) -> SubActionTelemetry {
    let started_at = OffsetDateTime::now_utc();

    let SubActionSpec::RandomInt {
        min,
        max,
        target_var,
    } = spec
    else {
        unreachable!()
    };

    let outcome = if min > max {
        SubActionOutcome::Failed(format!("min ({min}) must be <= max ({max})"))
    } else {
        let value = rand::rng().random_range(*min..=*max);
        match GlobalsRepo::set(dp, target_var, Variant::Int(value), false).await {
            Ok(()) => {
                bus.publish(Event::caused_by(
                    EventSource::Core,
                    "global.set",
                    serde_json::json!({
                        "key": target_var,
                        "source": "random_int",
                        "new_value": value,
                    }),
                    parent_event_id,
                ));
                SubActionOutcome::Success
            }
            Err(e) => SubActionOutcome::Failed(format!("global write failed: {e}")),
        }
    };

    let duration_ms = (OffsetDateTime::now_utc() - started_at)
        .whole_milliseconds()
        .max(0) as u64;

    SubActionTelemetry {
        index,
        kind: "RandomInt".to_string(),
        started_at,
        duration_ms,
        outcome,
    }
}
