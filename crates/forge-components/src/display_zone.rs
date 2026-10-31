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

#[cfg(test)]
pub(crate) fn install_display_zone(zone: DisplayZone) {
    ACTIVE_ZONE.with(|cell| *cell.borrow_mut() = zone);
}

pub fn display_zone() -> DisplayZone {
    ACTIVE_ZONE.with(|cell| cell.borrow().clone())
}

#[cfg(test)]
mod tests {
    use forge_types::datetime_from_text;
    use time::{Date, Month, Time};

    use super::*;

    fn kyiv() -> DisplayZone {
        DisplayZone::named("Europe/Kyiv").unwrap()
    }

    fn new_york() -> DisplayZone {
        DisplayZone::named("America/New_York").unwrap()
    }

    fn wall(month: Month, day: u8, hour: u8, minute: u8) -> PrimitiveDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, month, day).unwrap(),
            Time::from_hms(hour, minute, 0).unwrap(),
        )
    }

    fn at(text: &str) -> OffsetDateTime {
        datetime_from_text(text).unwrap()
    }

    fn with_offsets(resolved: WallTime) -> Vec<(OffsetDateTime, UtcOffset)> {
        match resolved {
            WallTime::Exact(instant) => vec![(instant, instant.offset())],
            WallTime::Repeated { earlier, later } => {
                vec![(earlier, earlier.offset()), (later, later.offset())]
            }
            WallTime::Skipped {
                first_valid,
                last_before,
            } => vec![
                (first_valid, first_valid.offset()),
                (last_before, last_before.offset()),
            ],
        }
    }

    fn expected(texts: &[&str]) -> Vec<(OffsetDateTime, UtcOffset)> {
        texts
            .iter()
            .map(|text| (at(text), at(text).offset()))
            .collect()
    }

    #[test]
    fn ordinary_wall_times_resolve_to_one_instant_including_the_minutes_around_each_transition() {
        for (zone, name, wall_time, instant) in [
            (
                kyiv(),
                "kyiv",
                wall(Month::October, 21, 14, 30),
                "2026-10-21T14:30:00+03:00",
            ),
            (
                kyiv(),
                "kyiv",
                wall(Month::March, 29, 2, 59),
                "2026-03-29T02:59:00+02:00",
            ),
            (
                kyiv(),
                "kyiv",
                wall(Month::March, 29, 4, 0),
                "2026-03-29T04:00:00+03:00",
            ),
            (
                kyiv(),
                "kyiv",
                wall(Month::October, 25, 2, 59),
                "2026-10-25T02:59:00+03:00",
            ),
            (
                kyiv(),
                "kyiv",
                wall(Month::October, 25, 4, 0),
                "2026-10-25T04:00:00+02:00",
            ),
            (
                new_york(),
                "new_york",
                wall(Month::March, 8, 1, 59),
                "2026-03-08T01:59:00-05:00",
            ),
            (
                new_york(),
                "new_york",
                wall(Month::November, 1, 2, 0),
                "2026-11-01T02:00:00-05:00",
            ),
        ] {
            assert_eq!(
                with_offsets(zone.resolve(wall_time).unwrap()),
                expected(&[instant]),
                "{name} {wall_time}"
            );
        }
    }

    #[test]
    fn repeated_autumn_wall_time_resolves_to_both_instants_earlier_first() {
        for (zone, name, wall_time, earlier, later) in [
            (
                kyiv(),
                "kyiv",
                wall(Month::October, 25, 3, 0),
                "2026-10-25T03:00:00+03:00",
                "2026-10-25T03:00:00+02:00",
            ),
            (
                kyiv(),
                "kyiv",
                wall(Month::October, 25, 3, 59),
                "2026-10-25T03:59:00+03:00",
                "2026-10-25T03:59:00+02:00",
            ),
            (
                new_york(),
                "new_york",
                wall(Month::November, 1, 1, 30),
                "2026-11-01T01:30:00-04:00",
                "2026-11-01T01:30:00-05:00",
            ),
        ] {
            assert_eq!(
                with_offsets(zone.resolve(wall_time).unwrap()),
                expected(&[earlier, later]),
                "{name} {wall_time}"
            );
        }
    }

    #[test]
    fn skipped_spring_wall_time_resolves_to_the_first_valid_and_the_last_earlier_minute() {
        for (zone, name, wall_time, first_valid, last_before) in [
            (
                kyiv(),
                "kyiv",
                wall(Month::March, 29, 3, 0),
                "2026-03-29T04:00:00+03:00",
                "2026-03-29T02:59:00+02:00",
            ),
            (
                kyiv(),
                "kyiv",
                wall(Month::March, 29, 3, 59),
                "2026-03-29T04:00:00+03:00",
                "2026-03-29T02:59:00+02:00",
            ),
            (
                new_york(),
                "new_york",
                wall(Month::March, 8, 2, 30),
                "2026-03-08T03:00:00-04:00",
                "2026-03-08T01:59:00-05:00",
            ),
        ] {
            assert_eq!(
                with_offsets(zone.resolve(wall_time).unwrap()),
                expected(&[first_valid, last_before]),
                "{name} {wall_time}"
            );
        }
    }

    #[test]
    fn wall_clock_shows_an_instant_in_the_zone_offset_in_force_at_that_instant() {
        for (zone, name, instant, shown) in [
            (
                kyiv(),
                "kyiv",
                "2026-10-25T00:59:00Z",
                "2026-10-25T03:59:00+03:00",
            ),
            (
                kyiv(),
                "kyiv",
                "2026-10-25T01:00:00Z",
                "2026-10-25T03:00:00+02:00",
            ),
            (
                new_york(),
                "new_york",
                "2026-03-08T07:00:00Z",
                "2026-03-08T03:00:00-04:00",
            ),
            (
                DisplayZone::fixed(UtcOffset::from_hms(5, 30, 0).unwrap()),
                "fixed",
                "2026-10-21T00:00:00Z",
                "2026-10-21T05:30:00+05:30",
            ),
        ] {
            let wall_clock = zone.wall_clock(at(instant));
            assert_eq!(
                (wall_clock, wall_clock.offset()),
                (at(shown), at(shown).offset()),
                "{name} {instant}"
            );
        }
    }

    #[test]
    fn named_rejects_a_zone_name_the_database_does_not_know() {
        for name in ["", "Mars/Olympus_Mons", "europe/kyiv "] {
            assert!(DisplayZone::named(name).is_none(), "{name:?}");
        }
    }

    #[test]
    fn installed_display_zone_replaces_the_system_zone_for_this_thread() {
        install_display_zone(new_york());
        assert_eq!(
            display_zone().offset_at(at("2026-12-01T12:00:00Z")),
            UtcOffset::from_hms(-5, 0, 0).unwrap()
        );
    }
}
