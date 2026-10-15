use time::OffsetDateTime;

use crate::error::SqliteStorageError;

pub(crate) const NANOS_PER_MILLI: i128 = 1_000_000;

pub(crate) fn to_epoch_ms(dt: OffsetDateTime) -> i64 {
    (dt.unix_timestamp_nanos() / NANOS_PER_MILLI) as i64
}

pub(crate) fn epoch_ms_now() -> i64 {
    to_epoch_ms(OffsetDateTime::now_utc())
}

pub(crate) fn from_epoch_ms(ms: i64) -> Result<OffsetDateTime, SqliteStorageError> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * NANOS_PER_MILLI)
        .map_err(|e| SqliteStorageError::Decode(format!("invalid epoch ms {ms}: {e}")))
}
