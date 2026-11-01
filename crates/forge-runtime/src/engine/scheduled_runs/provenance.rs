use std::time::Duration;

use forge_storage::ScheduledRun;
use forge_types::{ArgStack, Variant};
use time::OffsetDateTime;

pub const SCHEDULED_AT_VARIABLE: &str = "scheduled_at";
pub const SCHEDULED_DUE_AT_VARIABLE: &str = "scheduled_due_at";
pub const SCHEDULED_LATE_SECONDS_VARIABLE: &str = "scheduled_late_seconds";
pub const SCHEDULED_KEY_VARIABLE: &str = "scheduled_key";

pub(super) fn lateness(due_at: OffsetDateTime, now: OffsetDateTime) -> Duration {
    Duration::try_from(now - due_at).unwrap_or(Duration::ZERO)
}

pub(super) fn initial_args(run: &ScheduledRun, late: Duration) -> ArgStack {
    let snapshot = run
        .spec
        .args
        .iter()
        .fold(ArgStack::new(), |stack, (name, value)| {
            stack.set(name.clone(), value.clone())
        });
    snapshot
        .set(
            SCHEDULED_AT_VARIABLE.to_owned(),
            Variant::Datetime(run.spec.scheduled_at),
        )
        .set(
            SCHEDULED_DUE_AT_VARIABLE.to_owned(),
            Variant::Datetime(run.spec.due_at),
        )
        .set(
            SCHEDULED_LATE_SECONDS_VARIABLE.to_owned(),
            Variant::Int(i64::try_from(late.as_secs()).unwrap_or(i64::MAX)),
        )
        .set(
            SCHEDULED_KEY_VARIABLE.to_owned(),
            Variant::String(run.spec.key.clone().unwrap_or_default()),
        )
}
