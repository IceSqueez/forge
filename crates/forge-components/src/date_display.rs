use std::cell::Cell;
use std::sync::{PoisonError, RwLock};

use forge_types::{NamesLanguage, PatternZone, datetime_to_text, shared_date_pattern};
use time::{Date, OffsetDateTime, Time};

use crate::display_formats::{ClockStyle, DisplayFormatChoices, DisplayFormats};
use crate::locale::{CatalogLanguage, active_language, relative_within_week, tr_lookup};
use crate::region::Region;

const DATE_TIME_SEPARATOR: &str = ", ";
const MONTH_YEAR_PATTERN: &str = "LLLL yyyy";
const WEEKDAY_SHORT_PATTERN: &str = "ccc";
const DAY_PERIOD_PATTERN: &str = "a";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateShape {
    Date,
    Time,
    TimeWithSeconds,
    TimeWithMillis,
    DateTime,
    MonthYear,
    WeekdayShort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateDisplay {
    formats: DisplayFormats,
    names: NamesLanguage,
    zone: PatternZone,
}

impl DateDisplay {
    pub fn new(formats: DisplayFormats, names: NamesLanguage, zone: PatternZone) -> Self {
        Self {
            formats,
            names,
            zone,
        }
    }

    pub fn format(&self, shape: DateShape, value: OffsetDateTime) -> String {
        match shape {
            DateShape::Date => self.render(self.formats.date_format.pattern(self.names), value),
            DateShape::Time | DateShape::TimeWithSeconds | DateShape::TimeWithMillis => {
                self.render(time_pattern(self.formats.clock, shape), value)
            }
            DateShape::DateTime => format!(
                "{}{DATE_TIME_SEPARATOR}{}",
                self.format(DateShape::Date, value),
                self.format(DateShape::Time, value)
            ),
            DateShape::MonthYear => self.render(MONTH_YEAR_PATTERN, value),
            DateShape::WeekdayShort => self.render(WEEKDAY_SHORT_PATTERN, value),
        }
    }

    pub fn relative(&self, value: Option<OffsetDateTime>, now: OffsetDateTime) -> String {
        let Some(value) = value else {
            return tr_lookup("fmt_relative_never", None);
        };
        relative_within_week(now - value).unwrap_or_else(|| self.format(DateShape::Date, value))
    }

    fn render(&self, pattern: &str, value: OffsetDateTime) -> String {
        match shared_date_pattern(pattern).as_ref() {
            Ok(parsed) => parsed.format(value, self.zone, self.names),
            Err(_) => datetime_to_text(&value),
        }
    }
}

fn time_pattern(clock: ClockStyle, shape: DateShape) -> &'static str {
    match (clock, shape) {
        (ClockStyle::TwentyFourHour, DateShape::TimeWithSeconds) => "HH:mm:ss",
        (ClockStyle::TwentyFourHour, DateShape::TimeWithMillis) => "HH:mm:ss.SSS",
        (ClockStyle::TwentyFourHour, _) => "HH:mm",
        (ClockStyle::TwelveHour, DateShape::TimeWithSeconds) => "h:mm:ss a",
        (ClockStyle::TwelveHour, DateShape::TimeWithMillis) => "h:mm:ss.SSS a",
        (ClockStyle::TwelveHour, _) => "h:mm a",
    }
}

static PROCESS_FORMATS: RwLock<Option<DisplayFormats>> = RwLock::new(None);

thread_local! {
    static THREAD_FORMATS: Cell<Option<DisplayFormats>> = const { Cell::new(None) };
}

pub fn install_process_display_formats(formats: DisplayFormats) {
    *PROCESS_FORMATS
        .write()
        .unwrap_or_else(PoisonError::into_inner) = Some(formats);
}

pub fn install_display_formats(formats: DisplayFormats) {
    THREAD_FORMATS.set(Some(formats));
}

pub fn active_display_formats() -> DisplayFormats {
    THREAD_FORMATS
        .get()
        .or_else(|| {
            *PROCESS_FORMATS
                .read()
                .unwrap_or_else(PoisonError::into_inner)
        })
        .unwrap_or_else(|| {
            DisplayFormatChoices::following(Region::for_language(CatalogLanguage::En)).resolve()
        })
}

pub fn ui_names_language() -> NamesLanguage {
    active_language().names_language()
}

pub fn date_display() -> DateDisplay {
    DateDisplay::new(
        active_display_formats(),
        ui_names_language(),
        PatternZone::Local,
    )
}

pub fn fmt_display(shape: DateShape, value: OffsetDateTime) -> String {
    date_display().format(shape, value)
}

pub(crate) fn fmt_calendar_date(shape: DateShape, date: Date) -> String {
    DateDisplay::new(
        active_display_formats(),
        ui_names_language(),
        PatternZone::Utc,
    )
    .format(shape, date.midnight().assume_utc())
}

pub fn fmt_display_relative(value: Option<OffsetDateTime>) -> String {
    date_display().relative(value, OffsetDateTime::now_utc())
}

pub fn fmt_day_period(time_of_day: Time) -> String {
    DateDisplay::new(
        active_display_formats(),
        ui_names_language(),
        PatternZone::Utc,
    )
    .render(
        DAY_PERIOD_PATTERN,
        OffsetDateTime::UNIX_EPOCH.replace_time(time_of_day),
    )
}

#[cfg(test)]
mod tests {
    use time::{Date, Duration, Month, Time, UtcOffset};

    use super::*;
    use crate::display_formats::{DateFormatChoice, FirstWeekday};
    use crate::locale::set_thread_language;
    use crate::test_support::install_catalog;

    fn utc(year: i32, month: Month, day: u8, hour: u8, minute: u8) -> OffsetDateTime {
        OffsetDateTime::new_utc(
            Date::from_calendar_date(year, month, day).unwrap(),
            Time::from_hms(hour, minute, 0).unwrap(),
        )
    }

    fn formats(date_format: DateFormatChoice, clock: ClockStyle) -> DisplayFormats {
        DisplayFormats {
            date_format,
            clock,
            first_day: FirstWeekday::Monday,
        }
    }

    fn in_utc(
        date_format: DateFormatChoice,
        clock: ClockStyle,
        names: NamesLanguage,
    ) -> DateDisplay {
        DateDisplay::new(formats(date_format, clock), names, PatternZone::Utc)
    }

    #[test]
    fn date_shape_renders_each_numeric_format_with_padded_single_digit_day_and_month() {
        let value = utc(2026, Month::March, 5, 12, 0);
        for (date_format, expected) in [
            (DateFormatChoice::DayMonthYearDot, "05.03.2026"),
            (DateFormatChoice::DayMonthYearSlash, "05/03/2026"),
            (DateFormatChoice::MonthDayYearSlash, "03/05/2026"),
            (DateFormatChoice::YearMonthDay, "2026-03-05"),
        ] {
            let display = in_utc(date_format, ClockStyle::TwentyFourHour, NamesLanguage::En);
            assert_eq!(
                display.format(DateShape::Date, value),
                expected,
                "{date_format:?}"
            );
        }
    }

    #[test]
    fn named_month_uses_the_names_language_order_and_month_names() {
        let value = utc(2026, Month::March, 5, 12, 0);
        for (names, expected) in [
            (NamesLanguage::En, "Mar 5, 2026"),
            (NamesLanguage::Uk, "5 бер. 2026"),
        ] {
            let display = in_utc(
                DateFormatChoice::NamedMonth,
                ClockStyle::TwentyFourHour,
                names,
            );
            assert_eq!(
                display.format(DateShape::Date, value),
                expected,
                "{names:?}"
            );
        }
    }

    #[test]
    fn month_year_uses_the_stand_alone_month_name_of_the_names_language() {
        let value = utc(2026, Month::March, 5, 12, 0);
        for (names, expected) in [
            (NamesLanguage::En, "March 2026"),
            (NamesLanguage::Uk, "березень 2026"),
        ] {
            let display = in_utc(
                DateFormatChoice::YearMonthDay,
                ClockStyle::TwentyFourHour,
                names,
            );
            assert_eq!(
                display.format(DateShape::MonthYear, value),
                expected,
                "{names:?}"
            );
        }
    }

    #[test]
    fn time_shape_renders_both_clocks_across_midnight_and_noon_boundaries() {
        for ((hour, minute), twenty_four, twelve) in [
            ((0, 0), "00:00", "12:00 AM"),
            ((0, 59), "00:59", "12:59 AM"),
            ((1, 5), "01:05", "1:05 AM"),
            ((11, 59), "11:59", "11:59 AM"),
            ((12, 0), "12:00", "12:00 PM"),
            ((12, 1), "12:01", "12:01 PM"),
            ((13, 5), "13:05", "1:05 PM"),
            ((23, 59), "23:59", "11:59 PM"),
        ] {
            let value = utc(2026, Month::March, 5, hour, minute);
            let rendered = [ClockStyle::TwentyFourHour, ClockStyle::TwelveHour].map(|clock| {
                in_utc(DateFormatChoice::YearMonthDay, clock, NamesLanguage::En)
                    .format(DateShape::Time, value)
            });
            assert_eq!(rendered, [twenty_four, twelve], "{hour}:{minute}");
        }
    }

    #[test]
    fn twelve_hour_day_period_follows_the_names_language() {
        let morning = utc(2026, Month::March, 5, 0, 30);
        let evening = utc(2026, Month::March, 5, 12, 30);
        let display = in_utc(
            DateFormatChoice::YearMonthDay,
            ClockStyle::TwelveHour,
            NamesLanguage::Uk,
        );

        assert_eq!(
            [morning, evening].map(|value| display.format(DateShape::Time, value)),
            ["12:30 дп", "12:30 пп"]
        );
    }

    #[test]
    fn seconds_and_millis_shapes_extend_the_time_in_each_clock() {
        let value = utc(2026, Month::March, 5, 21, 5) + Duration::milliseconds(7_042);
        for (clock, seconds, millis) in [
            (ClockStyle::TwentyFourHour, "21:05:07", "21:05:07.042"),
            (ClockStyle::TwelveHour, "9:05:07 PM", "9:05:07.042 PM"),
        ] {
            let display = in_utc(DateFormatChoice::YearMonthDay, clock, NamesLanguage::En);
            assert_eq!(
                [DateShape::TimeWithSeconds, DateShape::TimeWithMillis]
                    .map(|shape| display.format(shape, value)),
                [seconds, millis],
                "{clock:?}"
            );
        }
    }

    #[test]
    fn date_time_joins_the_chosen_date_and_short_time() {
        let value = utc(2026, Month::October, 21, 23, 5);
        let display = in_utc(
            DateFormatChoice::DayMonthYearDot,
            ClockStyle::TwelveHour,
            NamesLanguage::En,
        );

        assert_eq!(
            display.format(DateShape::DateTime, value),
            "21.10.2026, 11:05 PM"
        );
    }

    #[test]
    fn utc_zone_renders_the_instant_not_the_wall_clock_of_the_stored_offset() {
        let kyiv = UtcOffset::from_hms(3, 0, 0).unwrap();
        let after_kyiv_midnight = utc(2026, Month::March, 15, 23, 30).to_offset(kyiv);
        let display = in_utc(
            DateFormatChoice::DayMonthYearDot,
            ClockStyle::TwentyFourHour,
            NamesLanguage::En,
        );

        assert_eq!(
            display.format(DateShape::DateTime, after_kyiv_midnight),
            "15.03.2026, 23:30"
        );
    }

    #[test]
    fn local_zone_renders_one_instant_identically_whatever_offset_it_carries() {
        let instant = utc(2026, Month::October, 21, 20, 5) + Duration::milliseconds(90);
        let display = DateDisplay::new(
            formats(
                DateFormatChoice::DayMonthYearDot,
                ClockStyle::TwentyFourHour,
            ),
            NamesLanguage::En,
            PatternZone::Local,
        );
        let rendered: Vec<String> = [(0, 0), (3, 0), (-5, 0), (5, 45), (14, 0)]
            .into_iter()
            .map(|(hours, minutes)| UtcOffset::from_hms(hours, minutes, 0).unwrap())
            .map(|offset| display.format(DateShape::TimeWithMillis, instant.to_offset(offset)))
            .collect();

        assert!(
            rendered.windows(2).all(|pair| pair[0] == pair[1]),
            "{rendered:?}"
        );
    }

    #[test]
    fn relative_reads_never_for_a_missing_instant() {
        install_catalog(CatalogLanguage::En);
        let display = in_utc(
            DateFormatChoice::YearMonthDay,
            ClockStyle::TwentyFourHour,
            NamesLanguage::En,
        );

        assert_eq!(
            display.relative(None, utc(2026, Month::March, 5, 12, 0)),
            "never"
        );
    }

    #[test]
    fn relative_switches_to_the_display_date_from_one_week_back() {
        install_catalog(CatalogLanguage::En);
        let now = utc(2026, Month::March, 15, 12, 0);
        let display = in_utc(
            DateFormatChoice::DayMonthYearDot,
            ClockStyle::TwentyFourHour,
            NamesLanguage::En,
        );
        for (ago, expected) in [
            (Duration::seconds(-30), "0s ago"),
            (Duration::ZERO, "0s ago"),
            (Duration::days(7) - Duration::seconds(1), "6d ago"),
            (Duration::days(7), "08.03.2026"),
            (Duration::days(40), "03.02.2026"),
        ] {
            assert_eq!(display.relative(Some(now - ago), now), expected, "{ago}");
        }
    }

    #[test]
    fn installed_formats_apply_live_to_every_following_display_call() {
        let value = utc(2026, Month::March, 15, 12, 0);
        let shape_of = |rendered: String| -> String {
            rendered
                .chars()
                .map(|c| if c.is_ascii_digit() { 'd' } else { c })
                .collect()
        };

        install_display_formats(formats(
            DateFormatChoice::DayMonthYearDot,
            ClockStyle::TwentyFourHour,
        ));
        let dotted = shape_of(fmt_display(DateShape::Date, value));
        install_display_formats(formats(
            DateFormatChoice::YearMonthDay,
            ClockStyle::TwentyFourHour,
        ));
        let iso = shape_of(fmt_display(DateShape::Date, value));

        assert_eq!(
            (dotted, iso),
            ("dd.dd.dddd".to_owned(), "dddd-dd-dd".to_owned())
        );
    }

    #[test]
    fn display_names_follow_the_ui_language() {
        let value = utc(2026, Month::March, 15, 12, 0);
        install_display_formats(formats(
            DateFormatChoice::NamedMonth,
            ClockStyle::TwentyFourHour,
        ));
        let mut rendered = Vec::new();
        for language in [CatalogLanguage::En, CatalogLanguage::Uk] {
            set_thread_language(language);
            rendered.push(fmt_display(DateShape::Date, value));
        }

        assert!(
            rendered[0].starts_with("Mar ") && rendered[1].contains(" бер. "),
            "{rendered:?}"
        );
    }
}
