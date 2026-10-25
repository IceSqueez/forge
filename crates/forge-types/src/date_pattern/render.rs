use jiff::Timestamp;
use jiff::tz::TimeZone;
use time::{OffsetDateTime, UtcOffset};

use super::names::{self, NamesLanguage};
use super::syntax::{DateField, Padding, PatternPiece};

const MINIMAL_DIGITS: usize = 1;
const TWO_DIGITS: usize = 2;
const FOUR_DIGITS: usize = 4;
const NANOSECOND_DIGITS: usize = 9;
const YEARS_PER_CENTURY: i32 = 100;
const HOURS_PER_HALF_DAY: u8 = 12;
const SECONDS_PER_MINUTE: i32 = 60;
const SECONDS_PER_HOUR: i32 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PatternZone {
    #[default]
    Local,
    Utc,
}

struct CivilFields {
    year: i32,
    month: u8,
    day: u8,
    days_from_monday: u8,
    hour: u8,
    minute: u8,
    second: u8,
    nanosecond: u32,
    offset_seconds: i32,
}

impl CivilFields {
    fn utc(value: OffsetDateTime) -> Self {
        let utc = value.to_offset(UtcOffset::UTC);
        Self {
            year: utc.year(),
            month: u8::from(utc.month()),
            day: utc.day(),
            days_from_monday: utc.weekday().number_days_from_monday(),
            hour: utc.hour(),
            minute: utc.minute(),
            second: utc.second(),
            nanosecond: utc.nanosecond(),
            offset_seconds: 0,
        }
    }

    fn local(value: OffsetDateTime) -> Self {
        let timestamp = i32::try_from(value.nanosecond())
            .ok()
            .and_then(|nanos| Timestamp::new(value.unix_timestamp(), nanos).ok());
        let Some(timestamp) = timestamp else {
            return Self::utc(value);
        };
        let zoned = timestamp.to_zoned(TimeZone::system());
        Self {
            year: i32::from(zoned.year()),
            month: zoned.month().unsigned_abs(),
            day: zoned.day().unsigned_abs(),
            days_from_monday: zoned.weekday().to_monday_zero_offset().unsigned_abs(),
            hour: zoned.hour().unsigned_abs(),
            minute: zoned.minute().unsigned_abs(),
            second: zoned.second().unsigned_abs(),
            nanosecond: zoned.subsec_nanosecond().unsigned_abs(),
            offset_seconds: zoned.offset().seconds(),
        }
    }
}

pub(super) fn render(
    pieces: &[PatternPiece],
    value: OffsetDateTime,
    zone: PatternZone,
    language: NamesLanguage,
) -> String {
    let fields = match zone {
        PatternZone::Local => CivilFields::local(value),
        PatternZone::Utc => CivilFields::utc(value),
    };
    let mut out = String::new();
    for piece in pieces {
        match piece {
            PatternPiece::Literal(text) => out.push_str(text),
            PatternPiece::Field(field) => out.push_str(&render_field(*field, &fields, language)),
        }
    }
    out
}

fn render_field(field: DateField, fields: &CivilFields, language: NamesLanguage) -> String {
    match field {
        DateField::Year => signed_padded(fields.year, MINIMAL_DIGITS),
        DateField::YearFourDigits => signed_padded(fields.year, FOUR_DIGITS),
        DateField::YearTwoDigits => {
            signed_padded(fields.year.rem_euclid(YEARS_PER_CENTURY), TWO_DIGITS)
        }
        DateField::MonthNumber(padding) => padded(fields.month, padding),
        DateField::MonthName(form, width) => {
            names::month_name(language, form, width, fields.month).to_owned()
        }
        DateField::Day(padding) => padded(fields.day, padding),
        DateField::WeekdayName(form, width) => {
            names::weekday_name(language, form, width, fields.days_from_monday).to_owned()
        }
        DateField::Hour24(padding) => padded(fields.hour, padding),
        DateField::Hour12(padding) => {
            let on_clock = fields.hour % HOURS_PER_HALF_DAY;
            let shown = if on_clock == 0 {
                HOURS_PER_HALF_DAY
            } else {
                on_clock
            };
            padded(shown, padding)
        }
        DateField::DayPeriod => {
            names::day_period(language, fields.hour >= HOURS_PER_HALF_DAY).to_owned()
        }
        DateField::Minute(padding) => padded(fields.minute, padding),
        DateField::Second(padding) => padded(fields.second, padding),
        DateField::Fraction(digits) => {
            let all = format!("{:0width$}", fields.nanosecond, width = NANOSECOND_DIGITS);
            all.chars().take(digits).collect()
        }
        DateField::OffsetHoursMinutes => offset_hours_minutes(fields.offset_seconds),
    }
}

fn padded(value: u8, padding: Padding) -> String {
    match padding {
        Padding::Minimal => value.to_string(),
        Padding::TwoDigits => format!("{value:0TWO_DIGITS$}"),
    }
}

fn signed_padded(value: i32, width: usize) -> String {
    let sign = if value < 0 { "-" } else { "" };
    format!("{sign}{:0width$}", value.unsigned_abs())
}

fn offset_hours_minutes(offset_seconds: i32) -> String {
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let magnitude = offset_seconds.abs();
    let hours = magnitude / SECONDS_PER_HOUR;
    let minutes = magnitude % SECONDS_PER_HOUR / SECONDS_PER_MINUTE;
    format!("{sign}{hours:0TWO_DIGITS$}:{minutes:0TWO_DIGITS$}")
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, OffsetDateTime, UtcOffset};

    use crate::date_pattern::{DatePattern, NamesLanguage, PatternZone};

    fn at(year: i32, month: Month, day: u8, hms: (u8, u8, u8), nanos: u32) -> OffsetDateTime {
        Date::from_calendar_date(year, month, day)
            .unwrap()
            .with_hms_nano(hms.0, hms.1, hms.2, nanos)
            .unwrap()
            .assume_offset(UtcOffset::UTC)
    }

    fn utc_en(pattern: &str, value: OffsetDateTime) -> String {
        DatePattern::parse(pattern)
            .unwrap()
            .format(value, PatternZone::Utc, NamesLanguage::En)
    }

    #[test]
    fn every_spelling_renders_its_field_in_utc_english() {
        let value = at(2026, Month::March, 7, (4, 5, 9), 123_456_789);
        for (pattern, expected) in [
            ("y", "2026"),
            ("yy", "26"),
            ("yyyy", "2026"),
            ("M", "3"),
            ("MM", "03"),
            ("MMM", "Mar"),
            ("MMMM", "March"),
            ("LLL", "Mar"),
            ("LLLL", "March"),
            ("d", "7"),
            ("dd", "07"),
            ("EEE", "Sat"),
            ("EEEE", "Saturday"),
            ("ccc", "Sat"),
            ("cccc", "Saturday"),
            ("H", "4"),
            ("HH", "04"),
            ("h", "4"),
            ("hh", "04"),
            ("a", "AM"),
            ("m", "5"),
            ("mm", "05"),
            ("s", "9"),
            ("ss", "09"),
            ("S", "1"),
            ("SS", "12"),
            ("SSS", "123"),
            ("SSSS", "1234"),
            ("SSSSS", "12345"),
            ("SSSSSS", "123456"),
            ("SSSSSSS", "1234567"),
            ("SSSSSSSS", "12345678"),
            ("SSSSSSSSS", "123456789"),
            ("xxx", "+00:00"),
        ] {
            assert_eq!(utc_en(pattern, value), expected, "pattern {pattern:?}");
        }
    }

    #[test]
    fn two_digit_padding_leaves_two_digit_values_unchanged() {
        let value = at(2026, Month::December, 31, (23, 59, 58), 0);
        assert_eq!(
            utc_en("M/MM d/dd H/HH h/hh m/mm s/ss", value),
            "12/12 31/31 23/23 11/11 59/59 58/58"
        );
    }

    #[test]
    fn twelve_hour_clock_shows_twelve_at_midnight_and_noon() {
        for (hour, minute, expected) in [
            (0, 0, "12:00 AM / 12"),
            (0, 59, "12:59 AM / 12"),
            (1, 0, "1:00 AM / 01"),
            (11, 59, "11:59 AM / 11"),
            (12, 0, "12:00 PM / 12"),
            (12, 59, "12:59 PM / 12"),
            (13, 0, "1:00 PM / 01"),
            (23, 59, "11:59 PM / 11"),
        ] {
            let value = at(2026, Month::March, 7, (hour, minute, 0), 0);
            assert_eq!(utc_en("h:mm a / hh", value), expected, "{hour}:{minute}");
        }
    }

    #[test]
    fn two_digit_year_keeps_leading_zero_and_wraps_the_century() {
        for (year, expected) in [
            (2000, "00"),
            (2009, "09"),
            (2010, "10"),
            (1999, "99"),
            (-1, "99"),
        ] {
            let value = at(year, Month::January, 1, (0, 0, 0), 0);
            assert_eq!(utc_en("yy", value), expected, "year {year}");
        }
    }

    #[test]
    fn negative_and_short_years_carry_a_leading_minus_and_padding() {
        for (year, expected) in [
            (-44, "-44 -0044"),
            (-2026, "-2026 -2026"),
            (0, "0 0000"),
            (7, "7 0007"),
            (9999, "9999 9999"),
        ] {
            let value = at(year, Month::January, 1, (0, 0, 0), 0);
            assert_eq!(utc_en("y yyyy", value), expected, "year {year}");
        }
    }

    #[test]
    fn fraction_truncates_instead_of_rounding() {
        for (nanos, pattern, expected) in [
            (999_999_999, "S", "9"),
            (999_999_999, "SSS", "999"),
            (123_999_999, "SSS", "123"),
            (5_000_000, "SS", "00"),
            (5_000_000, "SSS", "005"),
            (0, "SSSSSSSSS", "000000000"),
            (1, "SSSSSSSSS", "000000001"),
            (1, "SSSSSSSS", "00000000"),
        ] {
            let value = at(2026, Month::March, 7, (4, 5, 9), nanos);
            assert_eq!(utc_en(pattern, value), expected, "{nanos} with {pattern}");
        }
    }

    #[test]
    fn utc_zone_converts_from_the_input_offset() {
        for (offset_seconds, local_hms, expected) in [
            (19_800, (1, 0, 0), "2026-03-07 19:30 +00:00"),
            (-12_600, (22, 45, 0), "2026-03-09 02:15 +00:00"),
            (0, (0, 0, 0), "2026-03-08 00:00 +00:00"),
        ] {
            let value = Date::from_calendar_date(2026, Month::March, 8)
                .unwrap()
                .with_hms(local_hms.0, local_hms.1, local_hms.2)
                .unwrap()
                .assume_offset(UtcOffset::from_whole_seconds(offset_seconds).unwrap());
            assert_eq!(
                utc_en("yyyy-MM-dd HH:mm xxx", value),
                expected,
                "offset {offset_seconds}"
            );
        }
    }

    #[test]
    fn weekday_follows_the_utc_date_not_the_input_date() {
        let value = Date::from_calendar_date(2026, Month::March, 9)
            .unwrap()
            .with_hms(1, 0, 0)
            .unwrap()
            .assume_offset(UtcOffset::from_hms(3, 0, 0).unwrap());
        assert_eq!(utc_en("EEEE d", value), "Sunday 8");
    }
}
