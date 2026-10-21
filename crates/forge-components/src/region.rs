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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_region_preset_resolves_from_its_code_to_its_formats() {
        use ClockStyle::{TwelveHour, TwentyFourHour};
        use DateFormatChoice::{
            DayMonthYearDot, DayMonthYearSlash, MonthDayYearSlash, YearMonthDay,
        };
        use FirstWeekday::{Monday, Sunday};
        let presets = [
            (
                "UA",
                Region::Ukraine,
                DayMonthYearDot,
                TwentyFourHour,
                Monday,
            ),
            (
                "US",
                Region::UnitedStates,
                MonthDayYearSlash,
                TwelveHour,
                Sunday,
            ),
            (
                "GB",
                Region::UnitedKingdom,
                DayMonthYearSlash,
                TwentyFourHour,
                Monday,
            ),
            ("CA", Region::Canada, YearMonthDay, TwelveHour, Sunday),
            (
                "PL",
                Region::Poland,
                DayMonthYearDot,
                TwentyFourHour,
                Monday,
            ),
            (
                "DE",
                Region::Germany,
                DayMonthYearDot,
                TwentyFourHour,
                Monday,
            ),
            (
                "FR",
                Region::France,
                DayMonthYearSlash,
                TwentyFourHour,
                Monday,
            ),
            (
                "ES",
                Region::Spain,
                DayMonthYearSlash,
                TwentyFourHour,
                Monday,
            ),
            (
                "IT",
                Region::Italy,
                DayMonthYearSlash,
                TwentyFourHour,
                Monday,
            ),
            (
                "CZ",
                Region::Czechia,
                DayMonthYearDot,
                TwentyFourHour,
                Monday,
            ),
            (
                "KZ",
                Region::Kazakhstan,
                DayMonthYearDot,
                TwentyFourHour,
                Monday,
            ),
            (
                "AU",
                Region::Australia,
                DayMonthYearSlash,
                TwelveHour,
                Monday,
            ),
        ];
        assert_eq!(presets.len(), Region::ALL.len());
        for (code, region, date_format, clock, first_day) in presets {
            assert_eq!(Region::from_code(code), Some(region), "{code}");
            assert_eq!(
                region.defaults(),
                RegionDefaults {
                    date_format,
                    clock,
                    first_day,
                },
                "{code}"
            );
        }
    }

    #[test]
    fn from_code_ignores_ascii_case_and_rejects_anything_else() {
        for (code, expected) in [
            ("ua", Some(Region::Ukraine)),
            ("Gb", Some(Region::UnitedKingdom)),
            ("", None),
            ("XX", None),
            ("UKR", None),
            (" UA", None),
            ("UA ", None),
            ("U", None),
        ] {
            assert_eq!(Region::from_code(code), expected, "{code:?}");
        }
    }

    #[test]
    fn from_locale_tag_reads_the_region_of_posix_and_bcp47_tags() {
        for (tag, expected) in [
            ("uk_UA.UTF-8", Some(Region::Ukraine)),
            ("en-US", Some(Region::UnitedStates)),
            ("en_GB", Some(Region::UnitedKingdom)),
            ("de_DE@euro", Some(Region::Germany)),
            ("fr_CA.UTF-8@latin", Some(Region::Canada)),
            ("sr-Latn-CZ", Some(Region::Czechia)),
            ("en_NZ.UTF-8", None),
            ("es-419", None),
            ("uk", None),
            ("C", None),
            ("POSIX", None),
            ("", None),
            ("not a locale", None),
        ] {
            assert_eq!(Region::from_locale_tag(tag), expected, "{tag:?}");
        }
    }
}
