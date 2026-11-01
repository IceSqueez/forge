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

pub(crate) fn start_of_local_day_secs(now: OffsetDateTime) -> i64 {
    jiff::Zoned::now()
        .start_of_day()
        .map(|midnight| midnight.timestamp().as_second())
        .unwrap_or_else(|_| now.replace_time(time::Time::MIDNIGHT).unix_timestamp())
}

pub(crate) fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * NANOS_PER_MILLI)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}
