use jiff::civil;
use jiff::tz::TimeZone;
use thiserror::Error;
use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};

use super::names::{self, NameForm, NameWidth, NamesLanguage};
use super::render::PatternZone;
use super::syntax::{DateField, Padding, PatternPiece};

const MONTHS_IN_YEAR: u8 = 12;
const DAYS_IN_WEEK: u8 = 7;
const HOURS_PER_HALF_DAY: u8 = 12;
const SECONDS_PER_MINUTE: i32 = 60;
const SECONDS_PER_HOUR: i32 = 3600;
const NANOSECOND_DIGITS: usize = 9;
const DECIMAL_BASE: u32 = 10;
const CENTURY_OF_TWO_DIGIT_YEARS: i32 = 2000;
const ONE_DIGIT: usize = 1;
const TWO_DIGITS: usize = 2;
const FOUR_DIGITS: usize = 4;
const MAX_YEAR_DIGITS: usize = 6;
const OFFSET_SEPARATOR: char = ':';
const UTC_DESIGNATORS: [char; 2] = ['Z', 'z'];
const NAME_FORMS: [NameForm; 2] = [NameForm::Format, NameForm::StandAlone];
const NAME_WIDTHS: [NameWidth; 2] = [NameWidth::Wide, NameWidth::Abbreviated];

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DateTextError {
    #[error("expected {expected} at position {position}")]
    Mismatch { expected: String, position: usize },
    #[error("unexpected text at position {position} after the end of the pattern")]
    TrailingText { position: usize },
    #[error("the pattern must contain a year, a month and a day")]
    MissingDate,
    #[error("a 12-hour field needs a day period ('a') in the same pattern")]
    HourWithoutPeriod,
    #[error("the text names a date or time that does not exist")]
    OutOfRange,
}

#[derive(Default)]
struct ReadFields {
    year: Option<i32>,
    month: Option<u8>,
    day: Option<u8>,
    hour24: Option<u8>,
    hour12: Option<u8>,
    after_noon: Option<bool>,
    minute: u8,
    second: u8,
    nanosecond: u32,
    offset: Option<UtcOffset>,
}

struct Cursor<'a> {
    text: &'a str,
    byte: usize,
}

impl<'a> Cursor<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.byte..]
    }

    fn position(&self) -> usize {
        self.text[..self.byte].chars().count()
    }

    fn mismatch(&self, expected: impl Into<String>) -> DateTextError {
        DateTextError::Mismatch {
            expected: expected.into(),
            position: self.position(),
        }
    }

    fn digits(&mut self, min: usize, max: usize, expected: &str) -> Result<u32, DateTextError> {
        let run: String = self
            .rest()
            .chars()
            .take(max)
            .take_while(char::is_ascii_digit)
            .collect();
        if run.len() < min {
            return Err(self.mismatch(expected));
        }
        let value = run.parse::<u32>().map_err(|_| self.mismatch(expected))?;
        self.byte += run.len();
        Ok(value)
    }

    fn padded(&mut self, padding: Padding, expected: &str) -> Result<u8, DateTextError> {
        let min = match padding {
            Padding::Minimal => ONE_DIGIT,
            Padding::TwoDigits => TWO_DIGITS,
        };
        let value = self.digits(min, TWO_DIGITS, expected)?;
        u8::try_from(value).map_err(|_| DateTextError::OutOfRange)
    }

    fn longest_name<T: Copy>(
        &mut self,
        candidates: &[(T, &str)],
        expected: &str,
    ) -> Result<T, DateTextError> {
        let best = candidates
            .iter()
            .filter(|(_, name)| !name.is_empty())
            .filter_map(|(value, name)| {
                prefix_len_ignoring_case(self.rest(), name).map(|len| (*value, len))
            })
            .max_by_key(|(_, len)| *len);
        match best {
            Some((value, len)) => {
                self.byte += len;
                Ok(value)
            }
            None => Err(self.mismatch(expected)),
        }
    }

    fn literal(&mut self, literal: &str) -> Result<(), DateTextError> {
        match prefix_len_ignoring_case(self.rest(), literal) {
            Some(len) => {
                self.byte += len;
                Ok(())
            }
            None => Err(self.mismatch(format!("'{literal}'"))),
        }
    }

    fn offset(&mut self) -> Result<UtcOffset, DateTextError> {
        const EXPECTED: &str = "a UTC offset such as +02:00";
        let Some(sign) = self.rest().chars().next() else {
            return Err(self.mismatch(EXPECTED));
        };
        if UTC_DESIGNATORS.contains(&sign) {
            self.byte += sign.len_utf8();
            return Ok(UtcOffset::UTC);
        }
        let negative = match sign {
            '+' => false,
            '-' => true,
            _ => return Err(self.mismatch(EXPECTED)),
        };
        self.byte += sign.len_utf8();
        let hours = self.digits(TWO_DIGITS, TWO_DIGITS, EXPECTED)?;
        if !self.rest().starts_with(OFFSET_SEPARATOR) {
            return Err(self.mismatch(EXPECTED));
        }
        self.byte += OFFSET_SEPARATOR.len_utf8();
        let minutes = self.digits(TWO_DIGITS, TWO_DIGITS, EXPECTED)?;
        let magnitude = i32::try_from(hours)
            .ok()
            .zip(i32::try_from(minutes).ok())
            .map(|(h, m)| h * SECONDS_PER_HOUR + m * SECONDS_PER_MINUTE)
            .ok_or(DateTextError::OutOfRange)?;
        let seconds = if negative { -magnitude } else { magnitude };
        UtcOffset::from_whole_seconds(seconds).map_err(|_| DateTextError::OutOfRange)
    }
}

fn prefix_len_ignoring_case(text: &str, prefix: &str) -> Option<usize> {
    let mut text_chars = text.char_indices();
    let mut consumed = 0;
    for expected in prefix.chars() {
        let (index, actual) = text_chars.next()?;
        if !expected.to_lowercase().eq(actual.to_lowercase()) {
            return None;
        }
        consumed = index + actual.len_utf8();
    }
    Some(consumed)
}

fn accepted_languages(language: NamesLanguage) -> Vec<NamesLanguage> {
    if language == NamesLanguage::En {
        vec![NamesLanguage::En]
    } else {
        vec![language, NamesLanguage::En]
    }
}

fn month_candidates(language: NamesLanguage) -> Vec<(u8, &'static str)> {
    let mut out = Vec::new();
    for language in accepted_languages(language) {
        for form in NAME_FORMS {
            for width in NAME_WIDTHS {
                for month in 1..=MONTHS_IN_YEAR {
                    out.push((month, names::month_name(language, form, width, month)));
                }
            }
        }
    }
    out
}

fn weekday_candidates(language: NamesLanguage) -> Vec<(u8, &'static str)> {
    let mut out = Vec::new();
    for language in accepted_languages(language) {
        for form in NAME_FORMS {
            for width in NAME_WIDTHS {
                for day in 0..DAYS_IN_WEEK {
                    out.push((day, names::weekday_name(language, form, width, day)));
                }
            }
        }
    }
    out
}

fn day_period_candidates(language: NamesLanguage) -> Vec<(bool, &'static str)> {
    accepted_languages(language)
        .into_iter()
        .flat_map(|language| {
            [false, true].map(|after_noon| (after_noon, names::day_period(language, after_noon)))
        })
        .collect()
}

pub(super) fn read(
    pieces: &[PatternPiece],
    text: &str,
    zone: PatternZone,
    language: NamesLanguage,
) -> Result<OffsetDateTime, DateTextError> {
    let mut cursor = Cursor { text, byte: 0 };
    let mut fields = ReadFields::default();
    for piece in pieces {
        match piece {
            PatternPiece::Literal(literal) => cursor.literal(literal)?,
            PatternPiece::Field(field) => read_field(*field, &mut cursor, &mut fields, language)?,
        }
    }
    if !cursor.rest().is_empty() {
        return Err(DateTextError::TrailingText {
            position: cursor.position(),
        });
    }
    assemble(&fields, zone)
}

fn read_field(
    field: DateField,
    cursor: &mut Cursor<'_>,
    fields: &mut ReadFields,
    language: NamesLanguage,
) -> Result<(), DateTextError> {
    const YEAR: &str = "a year";
    const MONTH: &str = "a month";
    const DAY: &str = "a day";
    const HOUR: &str = "an hour";
    const MINUTE: &str = "minutes";
    const SECOND: &str = "seconds";
    match field {
        DateField::Year => {
            fields.year = Some(year(cursor.digits(ONE_DIGIT, MAX_YEAR_DIGITS, YEAR)?)?)
        }
        DateField::YearFourDigits => {
            fields.year = Some(year(cursor.digits(FOUR_DIGITS, FOUR_DIGITS, YEAR)?)?);
        }
        DateField::YearTwoDigits => {
            let short = year(cursor.digits(TWO_DIGITS, TWO_DIGITS, YEAR)?)?;
            fields.year = Some(CENTURY_OF_TWO_DIGIT_YEARS + short);
        }
        DateField::MonthNumber(padding) => fields.month = Some(cursor.padded(padding, MONTH)?),
        DateField::MonthName(..) => {
            fields.month = Some(cursor.longest_name(&month_candidates(language), "a month name")?);
        }
        DateField::Day(padding) => fields.day = Some(cursor.padded(padding, DAY)?),
        DateField::WeekdayName(..) => {
            cursor.longest_name(&weekday_candidates(language), "a weekday name")?;
        }
        DateField::Hour24(padding) => fields.hour24 = Some(cursor.padded(padding, HOUR)?),
        DateField::Hour12(padding) => fields.hour12 = Some(cursor.padded(padding, HOUR)?),
        DateField::DayPeriod => {
            fields.after_noon =
                Some(cursor.longest_name(&day_period_candidates(language), "a day period")?);
        }
        DateField::Minute(padding) => fields.minute = cursor.padded(padding, MINUTE)?,
        DateField::Second(padding) => fields.second = cursor.padded(padding, SECOND)?,
        DateField::Fraction(digits) => {
            let value = cursor.digits(digits, digits, "fractional seconds")?;
            let scale = u32::try_from(NANOSECOND_DIGITS.saturating_sub(digits))
                .map(|exponent| DECIMAL_BASE.pow(exponent))
                .map_err(|_| DateTextError::OutOfRange)?;
            fields.nanosecond = value * scale;
        }
        DateField::OffsetHoursMinutes => fields.offset = Some(cursor.offset()?),
    }
    Ok(())
}

fn year(digits: u32) -> Result<i32, DateTextError> {
    i32::try_from(digits).map_err(|_| DateTextError::OutOfRange)
}

fn hour_of_day(fields: &ReadFields) -> Result<u8, DateTextError> {
    if let Some(hour) = fields.hour24 {
        return Ok(hour);
    }
    let Some(on_clock) = fields.hour12 else {
        return Ok(0);
    };
    let after_noon = fields.after_noon.ok_or(DateTextError::HourWithoutPeriod)?;
    if on_clock == 0 || on_clock > HOURS_PER_HALF_DAY {
        return Err(DateTextError::OutOfRange);
    }
    let morning_hour = on_clock % HOURS_PER_HALF_DAY;
    Ok(if after_noon {
        morning_hour + HOURS_PER_HALF_DAY
    } else {
        morning_hour
    })
}

fn assemble(fields: &ReadFields, zone: PatternZone) -> Result<OffsetDateTime, DateTextError> {
    let (Some(year), Some(month), Some(day)) = (fields.year, fields.month, fields.day) else {
        return Err(DateTextError::MissingDate);
    };
    let hour = hour_of_day(fields)?;
    let month_of_year = Month::try_from(month).map_err(|_| DateTextError::OutOfRange)?;
    let date = Date::from_calendar_date(year, month_of_year, day)
        .map_err(|_| DateTextError::OutOfRange)?;
    let time = Time::from_hms_nano(hour, fields.minute, fields.second, fields.nanosecond)
        .map_err(|_| DateTextError::OutOfRange)?;
    let civil_time = PrimitiveDateTime::new(date, time);
    match (fields.offset, zone) {
        (Some(offset), _) => Ok(civil_time.assume_offset(offset)),
        (None, PatternZone::Utc) => Ok(civil_time.assume_utc()),
        (None, PatternZone::Local) => in_system_zone(civil_time),
    }
}

fn in_system_zone(civil_time: PrimitiveDateTime) -> Result<OffsetDateTime, DateTextError> {
    let narrow = |value: u8| i8::try_from(value).map_err(|_| DateTextError::OutOfRange);
    let year = i16::try_from(civil_time.year()).map_err(|_| DateTextError::OutOfRange)?;
    let nanosecond =
        i32::try_from(civil_time.nanosecond()).map_err(|_| DateTextError::OutOfRange)?;
    let zoned = civil::DateTime::new(
        year,
        narrow(u8::from(civil_time.month()))?,
        narrow(civil_time.day())?,
        narrow(civil_time.hour())?,
        narrow(civil_time.minute())?,
        narrow(civil_time.second())?,
        nanosecond,
    )
    .and_then(|civil| civil.to_zoned(TimeZone::system()))
    .map_err(|_| DateTextError::OutOfRange)?;
    let offset = UtcOffset::from_whole_seconds(zoned.offset().seconds())
        .map_err(|_| DateTextError::OutOfRange)?;
    OffsetDateTime::from_unix_timestamp_nanos(zoned.timestamp().as_nanosecond())
        .map(|instant| instant.to_offset(offset))
        .map_err(|_| DateTextError::OutOfRange)
}
