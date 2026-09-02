use std::fmt;
use std::str::FromStr;

use lingua::{IsoCode639_1, LanguageDetectorBuilder};

// lingua normalizes the confidence distribution to sum to 1.0, so 0.65 means the
// winner leads the runner-up by at least ~2x in a two-candidate set.
const MINIMUM_CONFIDENCE: f64 = 0.65;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LanguageCode([u8; 2]);

impl LanguageCode {
    /// Reads the primary subtag only, case-insensitively, and rejects anything that is
    /// not two ASCII letters - an empty locale, `und`, or a raw numeric LCID yields `None`.
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DetectionOutcome {
    Detected {
        language: LanguageCode,
        confidence: f64,
    },
    Inconclusive,
}

pub struct LanguageDetector {
    candidates: Vec<LanguageCode>,
    inner: lingua::LanguageDetector,
}

impl LanguageDetector {
    /// `None` unless at least two distinct candidates are present in the compiled
    /// language set; construction eagerly loads their models, so it belongs off the
    /// per-utterance path.
    pub fn new(candidates: &[LanguageCode]) -> Option<Self> {
        let mut accepted = Vec::with_capacity(candidates.len());
        let mut iso_codes = Vec::with_capacity(candidates.len());

        for candidate in candidates {
            let Ok(iso_code) = IsoCode639_1::from_str(&candidate.to_string()) else {
                continue;
            };
            if iso_codes.contains(&iso_code) {
                continue;
            }
            iso_codes.push(iso_code);
            accepted.push(*candidate);
        }

        if iso_codes.len() < 2 {
            return None;
        }

        let inner = LanguageDetectorBuilder::from_iso_codes_639_1(&iso_codes)
            .with_preloaded_language_models()
            .build();

        Some(Self {
            candidates: accepted,
            inner,
        })
    }

    pub fn detect(&self, text: &str) -> DetectionOutcome {
        if text.trim().is_empty() {
            return DetectionOutcome::Inconclusive;
        }

        let Some((language, confidence)) = self
            .inner
            .compute_language_confidence_values(text)
            .into_iter()
            .next()
        else {
            return DetectionOutcome::Inconclusive;
        };

        if confidence < MINIMUM_CONFIDENCE {
            return DetectionOutcome::Inconclusive;
        }

        match LanguageCode::from_locale(&language.iso_code_639_1().to_string()) {
            Some(language) => DetectionOutcome::Detected {
                language,
                confidence,
            },
            None => DetectionOutcome::Inconclusive,
        }
    }
}

impl fmt::Debug for LanguageDetector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LanguageDetector")
            .field("candidates", &self.candidates)
            .finish()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn code(locale: &str) -> LanguageCode {
        LanguageCode::from_locale(locale).unwrap()
    }

    fn en_uk_ru() -> LanguageDetector {
        LanguageDetector::new(&[code("en"), code("uk"), code("ru")]).unwrap()
    }

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
        // Why: `None` is the "does not match" signal for eligibility. An unreadable locale
        // must never widen into "matches everything", so every shape the engine catalogs
        // actually emit for an unknown language has to land here.
        for locale in [
            "", "und", "0409", "fil-PH", "eng", "e", "e1", "1e", "-", "_US", " en", "en ",
        ] {
            assert!(
                LanguageCode::from_locale(locale).is_none(),
                "locale {locale:?} must not resolve to a language"
            );
        }
    }

    #[test]
    fn new_returns_none_unless_two_distinct_compiled_candidates_survive() {
        for candidates in [
            vec![],
            vec![code("en")],
            vec![code("en"), code("en")],
            vec![code("de"), code("fr")],
            vec![code("en"), code("de")],
        ] {
            assert!(
                LanguageDetector::new(&candidates).is_none(),
                "candidates {candidates:?} leave nothing to discriminate"
            );
        }
    }

    #[test]
    fn detect_reports_the_language_of_a_confident_message() {
        let detector = en_uk_ru();
        for (text, expected) in [
            ("добрий вечір, як ваші справи сьогодні", "uk"),
            ("good evening everyone, how is the stream going", "en"),
            ("добрый вечер, как ваши дела сегодня", "ru"),
        ] {
            let DetectionOutcome::Detected {
                language,
                confidence,
            } = detector.detect(text)
            else {
                panic!("{text:?} must be detected");
            };
            assert_eq!(language.to_string(), expected, "text {text:?}");
            assert!(
                confidence >= MINIMUM_CONFIDENCE,
                "a reported detection must clear the floor, got {confidence}"
            );
        }
    }

    #[test]
    fn detect_returns_inconclusive_for_text_carrying_no_words() {
        let detector = en_uk_ru();
        for text in ["", "   ", "\n\t ", "!!! ???", "🎉🎉🎉"] {
            assert_eq!(
                detector.detect(text),
                DetectionOutcome::Inconclusive,
                "text {text:?} carries no language signal"
            );
        }
    }

    #[test]
    fn detect_returns_inconclusive_for_short_tokens_shared_between_candidates() {
        // Why: these score 0.52-0.61 across the en/uk/ru set. Admitting them would pick a
        // confidently wrong voice, which is the one failure this feature must not have.
        let detector = en_uk_ru();
        for text in ["ок", "да", "го", "круто", "не знаю"] {
            assert_eq!(
                detector.detect(text),
                DetectionOutcome::Inconclusive,
                "ambiguous token {text:?} must not narrow the voice pool"
            );
        }
    }
}
