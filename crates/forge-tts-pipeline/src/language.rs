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
