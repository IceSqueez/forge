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
    #[error("the weekday does not match the date")]
    WeekdayMismatch,
}

#[derive(Default)]
struct ReadFields {
    year: Option<i32>,
    month: Option<u8>,
    day: Option<u8>,
    days_from_monday: Option<u8>,
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
            fields.days_from_monday =
                Some(cursor.longest_name(&weekday_candidates(language), "a weekday name")?);
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
    if fields
        .days_from_monday
        .is_some_and(|named| named != date.weekday().number_days_from_monday())
    {
        return Err(DateTextError::WeekdayMismatch);
    }
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

#[cfg(test)]
mod tests {
    use time::{Date, Month, OffsetDateTime, UtcOffset};

    use crate::date_pattern::{DatePattern, DateTextError, NamesLanguage, PatternZone};

    fn at(year: i32, month: Month, day: u8, hms: (u8, u8, u8), nanos: u32) -> OffsetDateTime {
        Date::from_calendar_date(year, month, day)
            .unwrap()
            .with_hms_nano(hms.0, hms.1, hms.2, nanos)
            .unwrap()
            .assume_offset(UtcOffset::UTC)
    }

    fn day(year: i32, month: Month, day_of_month: u8) -> OffsetDateTime {
        at(year, month, day_of_month, (0, 0, 0), 0)
    }

    fn read_in(
        pattern: &str,
        text: &str,
        language: NamesLanguage,
    ) -> Result<OffsetDateTime, DateTextError> {
        DatePattern::parse(pattern)
            .unwrap()
            .parse_text(text, PatternZone::Utc, language)
    }

    fn read(pattern: &str, text: &str) -> Result<OffsetDateTime, DateTextError> {
        read_in(pattern, text, NamesLanguage::En)
    }

    #[test]
    fn numeric_fields_read_padded_and_minimal_spellings() {
        for (pattern, text, expected) in [
            (
                "yyyy-MM-dd HH:mm:ss",
                "2026-03-07 04:05:09",
                at(2026, Month::March, 7, (4, 5, 9), 0),
            ),
            (
                "y-M-d H:m:s",
                "2026-3-7 4:5:9",
                at(2026, Month::March, 7, (4, 5, 9), 0),
            ),
            (
                "y-M-d H:m:s",
                "2026-12-31 23:59:58",
                at(2026, Month::December, 31, (23, 59, 58), 0),
            ),
            ("y.M.d", "7.1.2", day(7, Month::January, 2)),
            ("y.M.d", "9999.1.2", day(9999, Month::January, 2)),
            ("dd.MM.yyyy", "07.03.2026", day(2026, Month::March, 7)),
        ] {
            assert_eq!(read(pattern, text), Ok(expected), "{pattern:?} on {text:?}");
        }
    }

    #[test]
    fn padded_fields_reject_a_single_digit() {
        for (pattern, text, position) in [
            ("yyyy-MM-dd", "2026-3-07", 5),
            ("yyyy-MM-dd", "2026-03-7", 8),
            ("yyyy-MM-dd HH", "2026-03-07 4", 11),
            ("yyyy", "202", 0),
        ] {
            assert!(
                matches!(read(pattern, text), Err(DateTextError::Mismatch { position: p, .. }) if p == position),
                "{pattern:?} on {text:?}: {:?}",
                read(pattern, text)
            );
        }
    }

    #[test]
    fn two_digit_year_lands_in_the_two_thousands() {
        for (text, year) in [("00", 2000), ("26", 2026), ("99", 2099)] {
            assert_eq!(
                read("dd.MM.yy", &format!("01.01.{text}")),
                Ok(day(year, Month::January, 1)),
                "yy {text}"
            );
        }
    }

    #[test]
    fn fraction_reads_exactly_its_digit_count_as_leading_nanosecond_digits() {
        for (fraction, text, nanos) in [
            ("S", "1", 100_000_000),
            ("SSS", "123", 123_000_000),
            ("SSS", "005", 5_000_000),
            ("SSSSSS", "123456", 123_456_000),
            ("SSSSSSSSS", "000000001", 1),
            ("SSSSSSSSS", "999999999", 999_999_999),
        ] {
            let pattern = format!("yyyy-MM-dd HH:mm:ss.{fraction}");
            assert_eq!(
                read(&pattern, &format!("2026-03-07 04:05:09.{text}")),
                Ok(at(2026, Month::March, 7, (4, 5, 9), nanos)),
                "{fraction} on {text}"
            );
        }
    }

    #[test]
    fn fraction_with_fewer_digits_than_letters_is_a_mismatch() {
        assert!(matches!(
            read("yyyy-MM-dd.SSS", "2026-03-07.12"),
            Err(DateTextError::Mismatch { position: 11, .. })
        ));
    }

    #[test]
    fn offset_reads_signed_hours_minutes_and_the_utc_designator() {
        let pattern = "yyyy-MM-dd'T'HH:mmxxx";
        for (text, utc_hm, offset_seconds) in [
            ("2026-03-07T12:00+02:00", (10, 0), 7_200),
            ("2026-03-07T12:00-03:30", (15, 30), -12_600),
            ("2026-03-07T12:00+00:00", (12, 0), 0),
            ("2026-03-07T12:00Z", (12, 0), 0),
            ("2026-03-07T12:00z", (12, 0), 0),
        ] {
            let parsed = read(pattern, text).unwrap();
            assert_eq!(
                parsed,
                at(2026, Month::March, 7, (utc_hm.0, utc_hm.1, 0), 0),
                "{text}"
            );
            assert_eq!(parsed.offset().whole_seconds(), offset_seconds, "{text}");
        }
    }

    #[test]
    fn offset_without_sign_or_colon_is_a_mismatch() {
        for (text, position) in [
            ("2026-03-07 02:00", 11),
            ("2026-03-07 +0200", 14),
            ("2026-03-07 +2:00", 12),
            ("2026-03-07 ", 11),
        ] {
            assert!(
                matches!(read("yyyy-MM-dd xxx", text), Err(DateTextError::Mismatch { position: p, .. }) if p == position),
                "{text:?}: {:?}",
                read("yyyy-MM-dd xxx", text)
            );
        }
    }

    #[test]
    fn offset_in_the_text_wins_over_the_requested_zone() {
        let pattern = DatePattern::parse("yyyy-MM-dd HH:mm xxx").unwrap();
        for zone in [PatternZone::Utc, PatternZone::Local] {
            let parsed = pattern
                .parse_text("2026-03-07 12:00 +05:30", zone, NamesLanguage::En)
                .unwrap();
            assert_eq!(parsed, at(2026, Month::March, 7, (6, 30, 0), 0), "{zone:?}");
            assert_eq!(parsed.offset().whole_seconds(), 19_800, "{zone:?}");
        }
    }

    #[test]
    fn text_without_an_offset_in_the_utc_zone_is_read_as_utc() {
        let parsed = read("yyyy-MM-dd HH:mm", "2026-03-07 12:00").unwrap();
        assert_eq!(parsed.offset(), UtcOffset::UTC);
    }

    #[test]
    fn every_month_name_reads_back_in_every_spelling_and_language() {
        for language in [NamesLanguage::En, NamesLanguage::Uk] {
            for spelling in ["MMM", "MMMM", "LLL", "LLLL"] {
                let pattern = DatePattern::parse(&format!("d {spelling} yyyy")).unwrap();
                for month in 1..=12u8 {
                    let value = day(2026, Month::try_from(month).unwrap(), 1);
                    let text = pattern.format(value, PatternZone::Utc, language);
                    assert_eq!(
                        pattern.parse_text(&text, PatternZone::Utc, language),
                        Ok(value),
                        "{language:?} {spelling} {text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_weekday_name_is_consumed_in_every_spelling_and_language() {
        for language in [NamesLanguage::En, NamesLanguage::Uk] {
            for spelling in ["EEE", "EEEE", "ccc", "cccc"] {
                let pattern = DatePattern::parse(&format!("{spelling}, dd.MM.yyyy")).unwrap();
                for day_of_month in 2..=8u8 {
                    let value = day(2026, Month::March, day_of_month);
                    let text = pattern.format(value, PatternZone::Utc, language);
                    assert_eq!(
                        pattern.parse_text(&text, PatternZone::Utc, language),
                        Ok(value),
                        "{language:?} {spelling} {text:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_weekday_that_contradicts_the_date_is_refused_in_every_language() {
        for (text, language) in [
            ("Mon, 03.03.2026", NamesLanguage::En),
            ("Wed, 03.03.2026", NamesLanguage::En),
            ("пн, 03.03.2026", NamesLanguage::Uk),
        ] {
            assert_eq!(
                read_in("EEE, dd.MM.yyyy", text, language),
                Err(DateTextError::WeekdayMismatch),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_weekday_is_compared_only_after_the_date_is_known_to_exist() {
        assert_eq!(
            read("EEE, dd.MM.yyyy", "Mon, 31.02.2026"),
            Err(DateTextError::OutOfRange)
        );
    }

    #[test]
    fn ukrainian_month_field_accepts_every_form_width_and_english() {
        for text in [
            "15 жовтня 2026",
            "15 жовтень 2026",
            "15 жовт. 2026",
            "15 October 2026",
            "15 Oct 2026",
        ] {
            assert_eq!(
                read_in("d MMMM yyyy", text, NamesLanguage::Uk),
                Ok(day(2026, Month::October, 15)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn english_month_field_rejects_ukrainian_names() {
        let result = read_in("d MMMM yyyy", "15 жовтня 2026", NamesLanguage::En);
        assert!(
            matches!(result, Err(DateTextError::Mismatch { position: 3, .. })),
            "{result:?}"
        );
    }

    #[test]
    fn names_take_the_longest_matching_candidate() {
        for (pattern, text, language, expected) in [
            (
                "d MMM yyyy",
                "1 March 2026",
                NamesLanguage::En,
                day(2026, Month::March, 1),
            ),
            (
                "d MMMM yyyy",
                "1 листопада 2026",
                NamesLanguage::Uk,
                day(2026, Month::November, 1),
            ),
            (
                "d MMMM yyyy",
                "1 листопад 2026",
                NamesLanguage::Uk,
                day(2026, Month::November, 1),
            ),
            (
                "d MMMM yyyy",
                "1 червня 2026",
                NamesLanguage::Uk,
                day(2026, Month::June, 1),
            ),
            (
                "EEE dd.MM.yyyy",
                "середа 04.03.2026",
                NamesLanguage::Uk,
                day(2026, Month::March, 4),
            ),
            (
                "EEEE dd.MM.yyyy",
                "пʼятниця 06.03.2026",
                NamesLanguage::Uk,
                day(2026, Month::March, 6),
            ),
        ] {
            assert_eq!(
                read_in(pattern, text, language),
                Ok(expected),
                "{pattern:?} on {text:?}"
            );
        }
    }

    #[test]
    fn names_and_literals_match_case_insensitively() {
        for (pattern, text, language, expected) in [
            (
                "d MMMM yyyy",
                "15 OCTOBER 2026",
                NamesLanguage::En,
                day(2026, Month::October, 15),
            ),
            (
                "d MMMM yyyy",
                "15 ЖОВТНЯ 2026",
                NamesLanguage::Uk,
                day(2026, Month::October, 15),
            ),
            (
                "yyyy-MM-dd h:mm a",
                "2026-03-07 3:00 pm",
                NamesLanguage::En,
                at(2026, Month::March, 7, (15, 0, 0), 0),
            ),
            (
                "yyyy-MM-dd'T'HH:mm",
                "2026-03-07t04:05",
                NamesLanguage::En,
                at(2026, Month::March, 7, (4, 5, 0), 0),
            ),
            (
                "yyyy-MM-dd 'о' HH",
                "2026-03-07 О 04",
                NamesLanguage::Uk,
                at(2026, Month::March, 7, (4, 0, 0), 0),
            ),
        ] {
            assert_eq!(
                read_in(pattern, text, language),
                Ok(expected),
                "{pattern:?} on {text:?}"
            );
        }
    }

    #[test]
    fn twelve_hour_clock_maps_twelve_am_to_midnight_and_twelve_pm_to_noon() {
        for (text, language, hour) in [
            ("12:30 AM", NamesLanguage::En, 0),
            ("1:30 AM", NamesLanguage::En, 1),
            ("11:30 AM", NamesLanguage::En, 11),
            ("12:30 PM", NamesLanguage::En, 12),
            ("1:30 PM", NamesLanguage::En, 13),
            ("11:30 PM", NamesLanguage::En, 23),
            ("12:30 дп", NamesLanguage::Uk, 0),
            ("12:30 пп", NamesLanguage::Uk, 12),
            ("11:30 PM", NamesLanguage::Uk, 23),
        ] {
            assert_eq!(
                read_in("yyyy-MM-dd h:mm a", &format!("2026-03-07 {text}"), language),
                Ok(at(2026, Month::March, 7, (hour, 30, 0), 0)),
                "{text}"
            );
        }
    }

    #[test]
    fn twelve_hour_field_outside_one_to_twelve_is_out_of_range() {
        for text in ["2026-03-07 0:30 AM", "2026-03-07 13:30 PM"] {
            assert_eq!(
                read("yyyy-MM-dd h:mm a", text),
                Err(DateTextError::OutOfRange),
                "{text}"
            );
        }
    }

    #[test]
    fn twelve_hour_field_without_a_day_period_is_rejected() {
        assert_eq!(
            read("yyyy-MM-dd h:mm", "2026-03-07 3:00"),
            Err(DateTextError::HourWithoutPeriod)
        );
    }

    #[test]
    fn missing_time_fields_default_to_midnight() {
        assert_eq!(
            read("dd.MM.yyyy HH", "07.03.2026 05"),
            Ok(at(2026, Month::March, 7, (5, 0, 0), 0))
        );
    }

    #[test]
    fn pattern_without_year_month_and_day_is_missing_a_date() {
        for (pattern, text) in [
            ("HH:mm", "12:30"),
            ("yyyy-MM", "2026-03"),
            ("MM-dd", "03-07"),
            ("yyyy dd", "2026 07"),
        ] {
            assert_eq!(
                read(pattern, text),
                Err(DateTextError::MissingDate),
                "{pattern}"
            );
        }
    }

    #[test]
    fn impossible_dates_and_times_are_out_of_range() {
        for (pattern, text) in [
            ("dd.MM.yyyy", "31.02.2026"),
            ("dd.MM.yyyy", "29.02.2026"),
            ("dd.MM.yyyy", "00.03.2026"),
            ("dd.MM.yyyy", "07.13.2026"),
            ("dd.MM.yyyy", "07.00.2026"),
            ("dd.MM.yyyy HH:mm", "07.03.2026 24:00"),
            ("dd.MM.yyyy HH:mm", "07.03.2026 12:60"),
            ("dd.MM.yyyy HH:mm:ss", "07.03.2026 12:00:60"),
            ("dd.MM.yyyy xxx", "07.03.2026 +26:00"),
            ("d.M.y", "1.1.123456"),
        ] {
            assert_eq!(
                read(pattern, text),
                Err(DateTextError::OutOfRange),
                "{text}"
            );
        }
    }

    #[test]
    fn last_valid_day_of_a_month_is_accepted() {
        for (text, expected) in [
            ("28.02.2026", day(2026, Month::February, 28)),
            ("29.02.2028", day(2028, Month::February, 29)),
            ("31.12.2026", day(2026, Month::December, 31)),
        ] {
            assert_eq!(read("dd.MM.yyyy", text), Ok(expected), "{text}");
        }
    }

    #[test]
    fn text_left_after_the_pattern_is_trailing_text_at_its_char_position() {
        for (pattern, text, language, position) in [
            ("yyyy-MM-dd", "2026-03-07 extra", NamesLanguage::En, 10),
            ("yyyy-MM-dd", "2026-03-07Z", NamesLanguage::En, 10),
            ("d MMMM yyyy", "15 жовтня 2026р", NamesLanguage::Uk, 14),
        ] {
            assert_eq!(
                read_in(pattern, text, language),
                Err(DateTextError::TrailingText { position }),
                "{text:?}"
            );
        }
    }

    #[test]
    fn mismatch_reports_the_char_position_and_what_was_expected() {
        assert_eq!(
            read("yyyy-MM-dd", "2026/03/07"),
            Err(DateTextError::Mismatch {
                expected: "'-'".to_owned(),
                position: 4,
            })
        );
    }

    #[test]
    fn mismatch_position_counts_chars_after_cyrillic_text() {
        assert!(matches!(
            read_in("d MMMM yyyy", "15 жовтня x026", NamesLanguage::Uk),
            Err(DateTextError::Mismatch { position: 10, .. })
        ));
    }

    #[test]
    fn unknown_name_and_empty_text_are_mismatches() {
        for (pattern, text, position) in [
            ("d MMMM yyyy", "1 Smarch 2026", 2),
            ("EEEE dd.MM.yyyy", "Funday 07.03.2026", 0),
            ("yyyy-MM-dd h a", "2026-03-07 3 noon", 13),
            ("yyyy-MM-dd", "", 0),
        ] {
            assert!(
                matches!(read(pattern, text), Err(DateTextError::Mismatch { position: p, .. }) if p == position),
                "{pattern:?} on {text:?}: {:?}",
                read(pattern, text)
            );
        }
    }

    #[test]
    fn format_then_parse_text_round_trips_a_full_utc_instant() {
        let pattern = DatePattern::parse("EEEE, d MMMM yyyy h:mm:ss.SSSSSSSSS a xxx").unwrap();
        for language in [NamesLanguage::En, NamesLanguage::Uk] {
            for value in [
                at(2026, Month::October, 20, (14, 35, 59), 999_999_999),
                at(2000, Month::January, 1, (0, 0, 0), 0),
                at(2028, Month::February, 29, (12, 0, 0), 1),
            ] {
                let text = pattern.format(value, PatternZone::Utc, language);
                assert_eq!(
                    pattern.parse_text(&text, PatternZone::Utc, language),
                    Ok(value),
                    "{language:?} {text:?}"
                );
            }
        }
    }
}
