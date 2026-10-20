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
