use jiff::Timestamp;
use jiff::tz::TimeZone;
use time::{OffsetDateTime, UtcOffset};

pub trait BackupClock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
    fn offset_at(&self, instant: OffsetDateTime) -> UtcOffset;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemBackupClock;

impl BackupClock for SystemBackupClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }

    fn offset_at(&self, instant: OffsetDateTime) -> UtcOffset {
        Timestamp::from_second(instant.unix_timestamp())
            .ok()
            .and_then(|timestamp| {
                let seconds = TimeZone::system().to_offset(timestamp).seconds();
                UtcOffset::from_whole_seconds(seconds).ok()
            })
            .unwrap_or(UtcOffset::UTC)
    }
}
