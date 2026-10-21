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
