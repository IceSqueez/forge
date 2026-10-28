use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LanguageCode([u8; 2]);

impl LanguageCode {
    pub fn from_locale(locale: &str) -> Option<Self> {
        let primary = locale.split(['-', '_']).next()?.as_bytes();
        let [first, second] = primary else {
            return None;
        };
        if !first.is_ascii_alphabetic() || !second.is_ascii_alphabetic() {
            return None;
        }
        Some(Self([
            first.to_ascii_lowercase(),
            second.to_ascii_lowercase(),
        ]))
    }
}

impl fmt::Display for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [first, second] = self.0;
        write!(f, "{}{}", first as char, second as char)
    }
}

impl fmt::Debug for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_locale_keeps_the_primary_subtag_lowercased_across_every_shape_engines_emit() {
        for (locale, expected) in [
            ("uk-UA", "uk"),
            ("en_US", "en"),
            ("en-gb", "en"),
            ("zh-Hans-CN", "zh"),
            ("EN", "en"),
            ("Ru", "ru"),
            ("ja", "ja"),
        ] {
            let parsed = LanguageCode::from_locale(locale)
                .unwrap_or_else(|| panic!("{locale} must yield a language code"));
            assert_eq!(parsed.to_string(), expected, "locale {locale}");
        }
    }

    #[test]
    fn from_locale_rejects_anything_without_a_two_letter_primary_subtag() {
        for locale in [
            "", "und", "0409", "fil-PH", "eng", "e", "e1", "1e", "-", "_US", " en", "en ",
        ] {
            assert!(
                LanguageCode::from_locale(locale).is_none(),
                "locale {locale:?} must not resolve to a language"
            );
        }
    }
}
