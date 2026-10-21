use forge_types::NamesLanguage;

use crate::region::Region;

pub const FOLLOW_REGION_CODE: &str = "region";

pub trait FormatOption: Copy + PartialEq + 'static {
    const ALL: &'static [Self];

    fn code(self) -> &'static str;

    fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|option| option.code() == code)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateFormatChoice {
    DayMonthYearDot,
    DayMonthYearSlash,
    MonthDayYearSlash,
    YearMonthDay,
    NamedMonth,
}

impl DateFormatChoice {
    pub fn pattern(self, names: NamesLanguage) -> &'static str {
        match self {
            Self::DayMonthYearDot => "dd.MM.yyyy",
            Self::DayMonthYearSlash => "dd/MM/yyyy",
            Self::MonthDayYearSlash => "MM/dd/yyyy",
            Self::YearMonthDay => "yyyy-MM-dd",
            Self::NamedMonth => match names {
                NamesLanguage::En => "MMM d, yyyy",
                NamesLanguage::Uk => "d MMM yyyy",
            },
        }
    }
}

impl FormatOption for DateFormatChoice {
    const ALL: &'static [Self] = &[
        Self::DayMonthYearDot,
        Self::DayMonthYearSlash,
        Self::MonthDayYearSlash,
        Self::YearMonthDay,
        Self::NamedMonth,
    ];

    fn code(self) -> &'static str {
        match self {
            Self::DayMonthYearDot => "dmy_dot",
            Self::DayMonthYearSlash => "dmy_slash",
            Self::MonthDayYearSlash => "mdy_slash",
            Self::YearMonthDay => "ymd",
            Self::NamedMonth => "named_month",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClockStyle {
    TwentyFourHour,
    TwelveHour,
}

impl ClockStyle {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::TwentyFourHour => "fmt_clock_24_hour",
            Self::TwelveHour => "fmt_clock_12_hour",
        }
    }
}

impl FormatOption for ClockStyle {
    const ALL: &'static [Self] = &[Self::TwentyFourHour, Self::TwelveHour];

    fn code(self) -> &'static str {
        match self {
            Self::TwentyFourHour => "24h",
            Self::TwelveHour => "12h",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FirstWeekday {
    Monday,
    Sunday,
    Saturday,
}

impl FirstWeekday {
    pub fn days_from_monday(self) -> u8 {
        match self {
            Self::Monday => 0,
            Self::Saturday => 5,
            Self::Sunday => 6,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Monday => "fmt_weekday_monday",
            Self::Sunday => "fmt_weekday_sunday",
            Self::Saturday => "fmt_weekday_saturday",
        }
    }
}

impl FormatOption for FirstWeekday {
    const ALL: &'static [Self] = &[Self::Monday, Self::Sunday, Self::Saturday];

    fn code(self) -> &'static str {
        match self {
            Self::Monday => "mon",
            Self::Sunday => "sun",
            Self::Saturday => "sat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatChoice<T> {
    RegionDefault,
    Fixed(T),
}

impl<T: FormatOption> FormatChoice<T> {
    pub fn from_stored(code: Option<&str>) -> Self {
        code.and_then(T::from_code)
            .map_or(Self::RegionDefault, Self::Fixed)
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::RegionDefault => FOLLOW_REGION_CODE,
            Self::Fixed(option) => option.code(),
        }
    }

    pub fn resolve(self, region_default: T) -> T {
        match self {
            Self::RegionDefault => region_default,
            Self::Fixed(option) => option,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayFormats {
    pub date_format: DateFormatChoice,
    pub clock: ClockStyle,
    pub first_day: FirstWeekday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayFormatChoices {
    pub region: Region,
    pub date_format: FormatChoice<DateFormatChoice>,
    pub clock: FormatChoice<ClockStyle>,
    pub first_day: FormatChoice<FirstWeekday>,
}

impl DisplayFormatChoices {
    pub fn following(region: Region) -> Self {
        Self {
            region,
            date_format: FormatChoice::RegionDefault,
            clock: FormatChoice::RegionDefault,
            first_day: FormatChoice::RegionDefault,
        }
    }

    pub fn from_stored(
        region: Option<&str>,
        date_format: Option<&str>,
        clock: Option<&str>,
        first_day: Option<&str>,
        fallback_region: Region,
    ) -> Self {
        Self {
            region: region
                .and_then(Region::from_code)
                .unwrap_or(fallback_region),
            date_format: FormatChoice::from_stored(date_format),
            clock: FormatChoice::from_stored(clock),
            first_day: FormatChoice::from_stored(first_day),
        }
    }

    pub fn resolve(&self) -> DisplayFormats {
        let defaults = self.region.defaults();
        DisplayFormats {
            date_format: self.date_format.resolve(defaults.date_format),
            clock: self.clock.resolve(defaults.clock),
            first_day: self.first_day.resolve(defaults.first_day),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored_codes<T: FormatOption>() -> Vec<&'static str> {
        T::ALL.iter().map(|option| option.code()).collect()
    }

    fn codes_not_round_tripping<T: FormatOption>() -> Vec<&'static str> {
        std::iter::once(FormatChoice::<T>::RegionDefault)
            .chain(T::ALL.iter().copied().map(FormatChoice::Fixed))
            .filter(|choice| FormatChoice::from_stored(Some(choice.code())) != *choice)
            .map(FormatChoice::code)
            .collect()
    }

    #[test]
    fn persisted_option_codes_match_the_stored_setting_vocabulary() {
        assert_eq!(
            (
                stored_codes::<DateFormatChoice>(),
                stored_codes::<ClockStyle>(),
                stored_codes::<FirstWeekday>(),
            ),
            (
                vec!["dmy_dot", "dmy_slash", "mdy_slash", "ymd", "named_month"],
                vec!["24h", "12h"],
                vec!["mon", "sun", "sat"],
            )
        );
    }

    #[test]
    fn every_choice_including_region_default_round_trips_through_its_stored_code() {
        assert_eq!(
            [
                codes_not_round_tripping::<DateFormatChoice>(),
                codes_not_round_tripping::<ClockStyle>(),
                codes_not_round_tripping::<FirstWeekday>(),
            ],
            [Vec::<&str>::new(), Vec::new(), Vec::new()]
        );
    }

    #[test]
    fn an_absent_unknown_or_region_code_follows_the_region() {
        for stored in [
            None,
            Some(FOLLOW_REGION_CODE),
            Some(""),
            Some("bogus"),
            Some("DMY_DOT"),
            Some(" ymd"),
            Some("24H"),
        ] {
            assert_eq!(
                FormatChoice::<DateFormatChoice>::from_stored(stored),
                FormatChoice::RegionDefault,
                "{stored:?}"
            );
            assert_eq!(
                FormatChoice::<ClockStyle>::from_stored(stored),
                FormatChoice::RegionDefault,
                "{stored:?}"
            );
            assert_eq!(
                FormatChoice::<FirstWeekday>::from_stored(stored),
                FormatChoice::RegionDefault,
                "{stored:?}"
            );
        }
    }

    #[test]
    fn from_stored_falls_back_to_the_given_region_when_the_region_is_absent_or_unknown() {
        for (stored, expected) in [
            (None, Region::Ukraine),
            (Some("ZZ"), Region::Ukraine),
            (Some(""), Region::Ukraine),
            (Some("gb"), Region::UnitedKingdom),
            (Some("US"), Region::UnitedStates),
        ] {
            let choices =
                DisplayFormatChoices::from_stored(stored, None, None, None, Region::Ukraine);
            assert_eq!(choices.region, expected, "{stored:?}");
        }
    }

    #[test]
    fn region_default_rows_follow_a_region_change_while_manual_rows_stay() {
        let mut choices = DisplayFormatChoices::from_stored(
            Some("UA"),
            Some("ymd"),
            Some(FOLLOW_REGION_CODE),
            None,
            Region::UnitedStates,
        );
        let before = choices.resolve();

        choices.region = Region::UnitedStates;
        let after = choices.resolve();

        assert_eq!(
            (before, after),
            (
                DisplayFormats {
                    date_format: DateFormatChoice::YearMonthDay,
                    clock: ClockStyle::TwentyFourHour,
                    first_day: FirstWeekday::Monday,
                },
                DisplayFormats {
                    date_format: DateFormatChoice::YearMonthDay,
                    clock: ClockStyle::TwelveHour,
                    first_day: FirstWeekday::Sunday,
                },
            )
        );
    }

    #[test]
    fn manual_rows_override_every_region_preset() {
        let choices = DisplayFormatChoices::from_stored(
            Some("US"),
            Some("dmy_dot"),
            Some("24h"),
            Some("sat"),
            Region::Ukraine,
        );

        assert_eq!(
            choices.resolve(),
            DisplayFormats {
                date_format: DateFormatChoice::DayMonthYearDot,
                clock: ClockStyle::TwentyFourHour,
                first_day: FirstWeekday::Saturday,
            }
        );
    }
}
