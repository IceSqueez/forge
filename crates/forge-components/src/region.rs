use unic_langid::LanguageIdentifier;

use crate::display_formats::{ClockStyle, DateFormatChoice, FirstWeekday};
use crate::locale::CatalogLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Region {
    Ukraine,
    UnitedStates,
    UnitedKingdom,
    Canada,
    Poland,
    Germany,
    France,
    Spain,
    Italy,
    Czechia,
    Kazakhstan,
    Australia,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegionDefaults {
    pub date_format: DateFormatChoice,
    pub clock: ClockStyle,
    pub first_day: FirstWeekday,
}

struct RegionEntry {
    code: &'static str,
    name_key: &'static str,
    defaults: RegionDefaults,
}

const fn entry(
    code: &'static str,
    name_key: &'static str,
    date_format: DateFormatChoice,
    clock: ClockStyle,
    first_day: FirstWeekday,
) -> RegionEntry {
    RegionEntry {
        code,
        name_key,
        defaults: RegionDefaults {
            date_format,
            clock,
            first_day,
        },
    }
}

const REGION_COUNT: usize = 12;
const LOCALE_ENCODING_MARK: char = '.';
const LOCALE_MODIFIER_MARK: char = '@';
const POSIX_SUBTAG_SEPARATOR: char = '_';
const BCP47_SUBTAG_SEPARATOR: &str = "-";

impl Region {
    pub const ALL: [Region; REGION_COUNT] = [
        Self::Ukraine,
        Self::UnitedStates,
        Self::UnitedKingdom,
        Self::Canada,
        Self::Poland,
        Self::Germany,
        Self::France,
        Self::Spain,
        Self::Italy,
        Self::Czechia,
        Self::Kazakhstan,
        Self::Australia,
    ];

    const fn entry(self) -> RegionEntry {
        match self {
            Self::Ukraine => entry(
                "UA",
                "region_name_ua",
                DateFormatChoice::DayMonthYearDot,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::UnitedStates => entry(
                "US",
                "region_name_us",
                DateFormatChoice::MonthDayYearSlash,
                ClockStyle::TwelveHour,
                FirstWeekday::Sunday,
            ),
            Self::UnitedKingdom => entry(
                "GB",
                "region_name_gb",
                DateFormatChoice::DayMonthYearSlash,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::Canada => entry(
                "CA",
                "region_name_ca",
                DateFormatChoice::YearMonthDay,
                ClockStyle::TwelveHour,
                FirstWeekday::Sunday,
            ),
            Self::Poland => entry(
                "PL",
                "region_name_pl",
                DateFormatChoice::DayMonthYearDot,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::Germany => entry(
                "DE",
                "region_name_de",
                DateFormatChoice::DayMonthYearDot,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::France => entry(
                "FR",
                "region_name_fr",
                DateFormatChoice::DayMonthYearSlash,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::Spain => entry(
                "ES",
                "region_name_es",
                DateFormatChoice::DayMonthYearSlash,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::Italy => entry(
                "IT",
                "region_name_it",
                DateFormatChoice::DayMonthYearSlash,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::Czechia => entry(
                "CZ",
                "region_name_cz",
                DateFormatChoice::DayMonthYearDot,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::Kazakhstan => entry(
                "KZ",
                "region_name_kz",
                DateFormatChoice::DayMonthYearDot,
                ClockStyle::TwentyFourHour,
                FirstWeekday::Monday,
            ),
            Self::Australia => entry(
                "AU",
                "region_name_au",
                DateFormatChoice::DayMonthYearSlash,
                ClockStyle::TwelveHour,
                FirstWeekday::Monday,
            ),
        }
    }

    pub fn code(self) -> &'static str {
        self.entry().code
    }

    pub fn name_key(self) -> &'static str {
        self.entry().name_key
    }

    pub fn defaults(self) -> RegionDefaults {
        self.entry().defaults
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|region| region.code().eq_ignore_ascii_case(code))
    }

    pub fn for_language(language: CatalogLanguage) -> Self {
        match language {
            CatalogLanguage::En => Self::UnitedStates,
            CatalogLanguage::Uk => Self::Ukraine,
        }
    }

    pub fn from_locale_tag(tag: &str) -> Option<Self> {
        let bcp47: String = tag
            .split([LOCALE_ENCODING_MARK, LOCALE_MODIFIER_MARK])
            .next()
            .unwrap_or_default()
            .replace(POSIX_SUBTAG_SEPARATOR, BCP47_SUBTAG_SEPARATOR);
        let identifier: LanguageIdentifier = bcp47.parse().ok()?;
        Self::from_code(identifier.region?.as_str())
    }
}
