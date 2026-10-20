const MONTHS_IN_YEAR: usize = 12;
const DAYS_IN_WEEK: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NamesLanguage {
    #[default]
    En,
    Uk,
}

impl NamesLanguage {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "en" => Some(Self::En),
            "uk" => Some(Self::Uk),
            _ => None,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Uk => "uk",
        }
    }

    fn names(self) -> &'static CalendarNames {
        match self {
            Self::En => &EN,
            Self::Uk => &UK,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameForm {
    Format,
    StandAlone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NameWidth {
    Abbreviated,
    Wide,
}

struct NameSet<const N: usize> {
    format_abbreviated: [&'static str; N],
    format_wide: [&'static str; N],
    stand_alone_abbreviated: [&'static str; N],
    stand_alone_wide: [&'static str; N],
}

impl<const N: usize> NameSet<N> {
    fn pick(&self, form: NameForm, width: NameWidth, index: usize) -> &'static str {
        let table = match (form, width) {
            (NameForm::Format, NameWidth::Abbreviated) => &self.format_abbreviated,
            (NameForm::Format, NameWidth::Wide) => &self.format_wide,
            (NameForm::StandAlone, NameWidth::Abbreviated) => &self.stand_alone_abbreviated,
            (NameForm::StandAlone, NameWidth::Wide) => &self.stand_alone_wide,
        };
        table.get(index).copied().unwrap_or_default()
    }
}

struct CalendarNames {
    months: NameSet<MONTHS_IN_YEAR>,
    weekdays_from_monday: NameSet<DAYS_IN_WEEK>,
    before_noon: &'static str,
    after_noon: &'static str,
}

pub(super) fn month_name(
    language: NamesLanguage,
    form: NameForm,
    width: NameWidth,
    month_from_one: u8,
) -> &'static str {
    let index = usize::from(month_from_one).saturating_sub(1);
    language.names().months.pick(form, width, index)
}

pub(super) fn weekday_name(
    language: NamesLanguage,
    form: NameForm,
    width: NameWidth,
    days_from_monday: u8,
) -> &'static str {
    language
        .names()
        .weekdays_from_monday
        .pick(form, width, usize::from(days_from_monday))
}

pub(super) fn day_period(language: NamesLanguage, is_after_noon: bool) -> &'static str {
    let names = language.names();
    if is_after_noon {
        names.after_noon
    } else {
        names.before_noon
    }
}

const EN_MONTHS_ABBREVIATED: [&str; MONTHS_IN_YEAR] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const EN_MONTHS_WIDE: [&str; MONTHS_IN_YEAR] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const EN_WEEKDAYS_ABBREVIATED: [&str; DAYS_IN_WEEK] =
    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const EN_WEEKDAYS_WIDE: [&str; DAYS_IN_WEEK] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

static EN: CalendarNames = CalendarNames {
    months: NameSet {
        format_abbreviated: EN_MONTHS_ABBREVIATED,
        format_wide: EN_MONTHS_WIDE,
        stand_alone_abbreviated: EN_MONTHS_ABBREVIATED,
        stand_alone_wide: EN_MONTHS_WIDE,
    },
    weekdays_from_monday: NameSet {
        format_abbreviated: EN_WEEKDAYS_ABBREVIATED,
        format_wide: EN_WEEKDAYS_WIDE,
        stand_alone_abbreviated: EN_WEEKDAYS_ABBREVIATED,
        stand_alone_wide: EN_WEEKDAYS_WIDE,
    },
    before_noon: "AM",
    after_noon: "PM",
};

const UK_MONTHS_ABBREVIATED: [&str; MONTHS_IN_YEAR] = [
    "січ.",
    "лют.",
    "бер.",
    "квіт.",
    "трав.",
    "черв.",
    "лип.",
    "серп.",
    "вер.",
    "жовт.",
    "лист.",
    "груд.",
];
const UK_WEEKDAYS_ABBREVIATED: [&str; DAYS_IN_WEEK] = ["пн", "вт", "ср", "чт", "пт", "сб", "нд"];
const UK_WEEKDAYS_WIDE: [&str; DAYS_IN_WEEK] = [
    "понеділок",
    "вівторок",
    "середа",
    "четвер",
    "пʼятниця",
    "субота",
    "неділя",
];

static UK: CalendarNames = CalendarNames {
    months: NameSet {
        format_abbreviated: UK_MONTHS_ABBREVIATED,
        format_wide: [
            "січня",
            "лютого",
            "березня",
            "квітня",
            "травня",
            "червня",
            "липня",
            "серпня",
            "вересня",
            "жовтня",
            "листопада",
            "грудня",
        ],
        stand_alone_abbreviated: UK_MONTHS_ABBREVIATED,
        stand_alone_wide: [
            "січень",
            "лютий",
            "березень",
            "квітень",
            "травень",
            "червень",
            "липень",
            "серпень",
            "вересень",
            "жовтень",
            "листопад",
            "грудень",
        ],
    },
    weekdays_from_monday: NameSet {
        format_abbreviated: UK_WEEKDAYS_ABBREVIATED,
        format_wide: UK_WEEKDAYS_WIDE,
        stand_alone_abbreviated: UK_WEEKDAYS_ABBREVIATED,
        stand_alone_wide: UK_WEEKDAYS_WIDE,
    },
    before_noon: "дп",
    after_noon: "пп",
};

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use time::{Date, Month, OffsetDateTime, UtcOffset};

    use super::NamesLanguage;
    use crate::date_pattern::{DatePattern, PatternZone};

    fn noon(year: i32, month: Month, day: u8) -> OffsetDateTime {
        Date::from_calendar_date(year, month, day)
            .unwrap()
            .with_hms(12, 0, 0)
            .unwrap()
            .assume_offset(UtcOffset::UTC)
    }

    fn render(pattern: &str, value: OffsetDateTime, language: NamesLanguage) -> String {
        DatePattern::parse(pattern)
            .unwrap()
            .format(value, PatternZone::Utc, language)
    }

    #[test]
    fn every_month_renders_in_each_width_and_form() {
        let pattern = "MMM|MMMM|LLL|LLLL";
        for (month, en, uk) in [
            (
                Month::January,
                "Jan|January|Jan|January",
                "січ.|січня|січ.|січень",
            ),
            (
                Month::February,
                "Feb|February|Feb|February",
                "лют.|лютого|лют.|лютий",
            ),
            (
                Month::March,
                "Mar|March|Mar|March",
                "бер.|березня|бер.|березень",
            ),
            (
                Month::April,
                "Apr|April|Apr|April",
                "квіт.|квітня|квіт.|квітень",
            ),
            (Month::May, "May|May|May|May", "трав.|травня|трав.|травень"),
            (
                Month::June,
                "Jun|June|Jun|June",
                "черв.|червня|черв.|червень",
            ),
            (Month::July, "Jul|July|Jul|July", "лип.|липня|лип.|липень"),
            (
                Month::August,
                "Aug|August|Aug|August",
                "серп.|серпня|серп.|серпень",
            ),
            (
                Month::September,
                "Sep|September|Sep|September",
                "вер.|вересня|вер.|вересень",
            ),
            (
                Month::October,
                "Oct|October|Oct|October",
                "жовт.|жовтня|жовт.|жовтень",
            ),
            (
                Month::November,
                "Nov|November|Nov|November",
                "лист.|листопада|лист.|листопад",
            ),
            (
                Month::December,
                "Dec|December|Dec|December",
                "груд.|грудня|груд.|грудень",
            ),
        ] {
            let value = noon(2026, month, 15);
            assert_eq!(render(pattern, value, NamesLanguage::En), en, "{month}");
            assert_eq!(render(pattern, value, NamesLanguage::Uk), uk, "{month}");
        }
    }

    #[test]
    fn every_weekday_renders_in_each_width_and_form() {
        let pattern = "EEE|EEEE|ccc|cccc";
        for (day, en, uk) in [
            (5, "Mon|Monday|Mon|Monday", "пн|понеділок|пн|понеділок"),
            (6, "Tue|Tuesday|Tue|Tuesday", "вт|вівторок|вт|вівторок"),
            (7, "Wed|Wednesday|Wed|Wednesday", "ср|середа|ср|середа"),
            (8, "Thu|Thursday|Thu|Thursday", "чт|четвер|чт|четвер"),
            (9, "Fri|Friday|Fri|Friday", "пт|пʼятниця|пт|пʼятниця"),
            (10, "Sat|Saturday|Sat|Saturday", "сб|субота|сб|субота"),
            (11, "Sun|Sunday|Sun|Sunday", "нд|неділя|нд|неділя"),
        ] {
            let value = noon(2026, Month::January, day);
            assert_eq!(render(pattern, value, NamesLanguage::En), en, "Jan {day}");
            assert_eq!(render(pattern, value, NamesLanguage::Uk), uk, "Jan {day}");
        }
    }

    #[test]
    fn ukrainian_month_is_genitive_after_a_day_and_nominative_stand_alone() {
        let value = noon(2026, Month::October, 20);
        assert_eq!(
            render("d MMMM | LLLL yyyy", value, NamesLanguage::Uk),
            "20 жовтня | жовтень 2026"
        );
    }

    #[test]
    fn day_period_switches_at_noon_in_each_language() {
        for (hour, minute, en, uk) in [
            (0, 0, "AM", "дп"),
            (11, 59, "AM", "дп"),
            (12, 0, "PM", "пп"),
            (23, 59, "PM", "пп"),
        ] {
            let value = Date::from_calendar_date(2026, Month::March, 7)
                .unwrap()
                .with_hms(hour, minute, 0)
                .unwrap()
                .assume_offset(UtcOffset::UTC);
            assert_eq!(render("a", value, NamesLanguage::En), en, "{hour}:{minute}");
            assert_eq!(render("a", value, NamesLanguage::Uk), uk, "{hour}:{minute}");
        }
    }

    #[test]
    fn language_codes_round_trip_and_reject_non_canonical_codes() {
        for language in [NamesLanguage::En, NamesLanguage::Uk] {
            assert_eq!(NamesLanguage::from_code(language.code()), Some(language));
        }
        for bad in ["EN", "Uk", "uk-UA", "en_US", "", " en", "ua"] {
            assert_eq!(NamesLanguage::from_code(bad), None, "code {bad:?}");
        }
    }
}
