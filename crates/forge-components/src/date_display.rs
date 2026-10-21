use std::cell::Cell;

use forge_types::{NamesLanguage, PatternZone, datetime_to_text, shared_date_pattern};
use time::OffsetDateTime;

use crate::display_formats::{ClockStyle, DisplayFormatChoices, DisplayFormats};
use crate::locale::{CatalogLanguage, active_locale, relative_within_week, tr_lookup};
use crate::region::Region;

const DATE_TIME_SEPARATOR: &str = ", ";
const MONTH_YEAR_PATTERN: &str = "LLLL yyyy";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateShape {
    Date,
    Time,
    TimeWithSeconds,
    TimeWithMillis,
    DateTime,
    MonthYear,
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

thread_local! {
    static ACTIVE_FORMATS: Cell<DisplayFormats> = Cell::new(
        DisplayFormatChoices::following(Region::for_language(CatalogLanguage::En)).resolve(),
    );
}

pub fn install_display_formats(formats: DisplayFormats) {
    ACTIVE_FORMATS.with(|cell| cell.set(formats));
}

pub fn active_display_formats() -> DisplayFormats {
    ACTIVE_FORMATS.with(Cell::get)
}

pub fn ui_names_language() -> NamesLanguage {
    NamesLanguage::from_code(active_locale()).unwrap_or_default()
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

pub fn fmt_display_relative(value: Option<OffsetDateTime>) -> String {
    date_display().relative(value, OffsetDateTime::now_utc())
}
