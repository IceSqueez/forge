use time::{Duration, OffsetDateTime};

pub(crate) fn next_due(
    last_success: Option<OffsetDateTime>,
    interval: Duration,
    now: OffsetDateTime,
) -> OffsetDateTime {
    match last_success {
        Some(last) if last <= now => last + interval,
        _ => now,
    }
}
