use std::cell::RefCell;

use jiff::tz::{AmbiguousOffset, Offset, TimeZone};
use jiff::{Timestamp, civil};
use time::{Duration, OffsetDateTime, PrimitiveDateTime, UtcOffset};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallTime {
    Exact(OffsetDateTime),
    Repeated {
        earlier: OffsetDateTime,
        later: OffsetDateTime,
    },
    Skipped {
        first_valid: OffsetDateTime,
        last_before: OffsetDateTime,
    },
}

#[derive(Debug, Clone)]
pub struct DisplayZone(TimeZone);

impl DisplayZone {
    pub fn system() -> Self {
        Self(TimeZone::system())
    }

    pub fn named(name: &str) -> Option<Self> {
        TimeZone::get(name).ok().map(Self)
    }

    pub fn fixed(offset: UtcOffset) -> Self {
        Offset::from_seconds(offset.whole_seconds())
            .map(|offset| Self(TimeZone::fixed(offset)))
            .unwrap_or_else(|_| Self(TimeZone::UTC))
    }

    pub fn offset_at(&self, instant: OffsetDateTime) -> UtcOffset {
        Timestamp::from_second(instant.unix_timestamp())
            .ok()
            .and_then(|timestamp| utc_offset(self.0.to_offset(timestamp)))
            .unwrap_or(UtcOffset::UTC)
    }

    pub fn wall_clock(&self, instant: OffsetDateTime) -> OffsetDateTime {
        instant.to_offset(self.offset_at(instant))
    }

    pub fn resolve(&self, wall: PrimitiveDateTime) -> Option<WallTime> {
        let civil_time = civil::DateTime::new(
            i16::try_from(wall.year()).ok()?,
            i8::try_from(u8::from(wall.month())).ok()?,
            i8::try_from(wall.day()).ok()?,
            i8::try_from(wall.hour()).ok()?,
            i8::try_from(wall.minute()).ok()?,
            i8::try_from(wall.second()).ok()?,
            0,
        )
        .ok()?;
        let at = |offset: Offset| Some(wall.assume_offset(utc_offset(offset)?));
        match self.0.to_ambiguous_timestamp(civil_time).offset() {
            AmbiguousOffset::Unambiguous { offset } => at(offset).map(WallTime::Exact),
            AmbiguousOffset::Fold { before, after } => Some(WallTime::Repeated {
                earlier: at(before)?,
                later: at(after)?,
            }),
            AmbiguousOffset::Gap { before, after } => {
                let ahead_of_gap = at(after)?;
                let transition = Timestamp::from_second(ahead_of_gap.unix_timestamp())
                    .ok()
                    .and_then(|from| self.0.following(from).next())?;
                let first_valid =
                    OffsetDateTime::from_unix_timestamp(transition.timestamp().as_second())
                        .ok()?
                        .to_offset(utc_offset(after)?);
                let last_before = first_valid
                    .checked_sub(Duration::MINUTE)?
                    .to_offset(utc_offset(before)?);
                Some(WallTime::Skipped {
                    first_valid,
                    last_before,
                })
            }
        }
    }
}

fn utc_offset(offset: Offset) -> Option<UtcOffset> {
    UtcOffset::from_whole_seconds(offset.seconds()).ok()
}

thread_local! {
    static ACTIVE_ZONE: RefCell<DisplayZone> = RefCell::new(DisplayZone::system());
}

pub fn install_display_zone(zone: DisplayZone) {
    ACTIVE_ZONE.with(|cell| *cell.borrow_mut() = zone);
}

pub fn display_zone() -> DisplayZone {
    ACTIVE_ZONE.with(|cell| cell.borrow().clone())
}
