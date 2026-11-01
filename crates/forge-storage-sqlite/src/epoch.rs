use jiff::tz::TimeZone;
use time::OffsetDateTime;

use crate::error::SqliteStorageError;

pub(crate) const NANOS_PER_MILLI: i128 = 1_000_000;
pub(crate) const TELEMETRY_AVERAGE_WINDOW: u32 = 100;

pub(crate) fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    i64::try_from(dt.unix_timestamp_nanos() / NANOS_PER_MILLI).unwrap_or(i64::MAX)
}

pub(crate) fn epoch_ms_now() -> i64 {
    to_epoch_ms(OffsetDateTime::now_utc())
}

pub(crate) fn start_of_system_day_secs(now: OffsetDateTime) -> i64 {
    start_of_day_secs(now, &TimeZone::system())
}

fn start_of_day_secs(now: OffsetDateTime, zone: &TimeZone) -> i64 {
    jiff::Timestamp::from_second(now.unix_timestamp())
        .and_then(|instant| instant.to_zoned(zone.clone()).start_of_day())
        .map(|midnight| midnight.timestamp().as_second())
        .unwrap_or_else(|_| now.replace_time(time::Time::MIDNIGHT).unix_timestamp())
}

pub(crate) fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * NANOS_PER_MILLI)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}

#[cfg(test)]
mod tests {
    use jiff::tz::{Offset, TimeZone};
    use time::macros::datetime;

    use super::start_of_day_secs;

    const KYIV: &str = "EET-2EEST,M3.5.0/3,M10.5.0/4";
    const HAVANA: &str = "CST5CDT,M3.2.0/0,M11.1.0/1";

    fn posix(rule: &str) -> TimeZone {
        TimeZone::posix(rule).unwrap()
    }

    #[test]
    fn the_day_starts_at_midnight_in_the_given_zone_for_the_given_instant() {
        for (now, zone, expected) in [
            (
                datetime!(2026-03-04 12:00 UTC),
                TimeZone::UTC,
                datetime!(2026-03-04 00:00 UTC),
            ),
            (
                datetime!(2026-03-04 12:00 UTC),
                TimeZone::fixed(Offset::constant(-5)),
                datetime!(2026-03-04 05:00 UTC),
            ),
            (
                datetime!(2026-03-04 03:00 UTC),
                TimeZone::fixed(Offset::constant(-5)),
                datetime!(2026-03-03 05:00 UTC),
            ),
            (
                datetime!(2026-03-04 23:00 UTC),
                TimeZone::fixed(Offset::constant(9)),
                datetime!(2026-03-04 15:00 UTC),
            ),
            (
                datetime!(2026-07-01 22:00 UTC),
                posix(KYIV),
                datetime!(2026-07-01 21:00 UTC),
            ),
            (
                datetime!(2026-01-15 22:30 UTC),
                posix(KYIV),
                datetime!(2026-01-15 22:00 UTC),
            ),
        ] {
            assert_eq!(
                start_of_day_secs(now, &zone),
                expected.unix_timestamp(),
                "{now} in {zone:?}"
            );
        }
    }

    #[test]
    fn a_day_whose_midnight_is_skipped_starts_at_its_first_real_instant() {
        let after_the_jump = datetime!(2026-03-08 12:00 UTC);

        assert_eq!(
            start_of_day_secs(after_the_jump, &posix(HAVANA)),
            datetime!(2026-03-08 05:00 UTC).unix_timestamp()
        );
    }
}
