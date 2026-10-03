use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MICROS_PER_MAJOR_UNIT: u64 = 1_000_000;

const MICRO_DIGITS: u32 = 6;
const SHOWN_FRACTION_DIGITS: usize = 2;
const CURRENCY_CODE_LEN: usize = 3;
const DECIMAL_SEPARATOR: char = '.';
const DECIMAL_BASE: u64 = 10;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MoneyError {
    #[error("currency code must be three ASCII letters: {0:?}")]
    InvalidCurrency(String),
    #[error("amount is not a non-negative decimal number: {0:?}")]
    InvalidAmount(String),
    #[error("amount has more fractional digits than micros can hold: {0:?}")]
    TooPrecise(String),
    #[error("currency exponent {0} is finer than micros")]
    UnsupportedExponent(u32),
    #[error("amount does not fit the supported range")]
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CurrencyCode(String);

impl CurrencyCode {
    pub fn parse(raw: &str) -> Result<Self, MoneyError> {
        let trimmed = raw.trim();
        if trimmed.len() == CURRENCY_CODE_LEN && trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
            Ok(Self(trimmed.to_ascii_uppercase()))
        } else {
            Err(MoneyError::InvalidCurrency(raw.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CurrencyCode {
    type Error = MoneyError;

    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(&raw)
    }
}

impl From<CurrencyCode> for String {
    fn from(code: CurrencyCode) -> Self {
        code.0
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MoneyAmount {
    micros: u64,
    currency: CurrencyCode,
}

impl MoneyAmount {
    pub const fn from_micros(micros: u64, currency: CurrencyCode) -> Self {
        Self { micros, currency }
    }

    pub fn from_minor_units(
        minor_units: u64,
        exponent: u32,
        currency: CurrencyCode,
    ) -> Result<Self, MoneyError> {
        let scale = MICRO_DIGITS
            .checked_sub(exponent)
            .ok_or(MoneyError::UnsupportedExponent(exponent))?;
        let micros = minor_units
            .checked_mul(DECIMAL_BASE.pow(scale))
            .ok_or(MoneyError::Overflow)?;
        Ok(Self { micros, currency })
    }

    pub fn parse_decimal(text: &str, currency: CurrencyCode) -> Result<Self, MoneyError> {
        let trimmed = text.trim();
        let (whole, fraction) = trimmed
            .split_once(DECIMAL_SEPARATOR)
            .map_or((trimmed, None), |(whole, fraction)| (whole, Some(fraction)));
        let all_digits = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
        if !all_digits(whole) || fraction.is_some_and(|part| !all_digits(part)) {
            return Err(MoneyError::InvalidAmount(text.to_owned()));
        }
        let fraction = fraction.unwrap_or_default();
        if fraction.len() > MICRO_DIGITS as usize {
            return Err(MoneyError::TooPrecise(text.to_owned()));
        }
        let whole_units: u64 = whole.parse().map_err(|_| MoneyError::Overflow)?;
        let padded = format!("{fraction:0<width$}", width = MICRO_DIGITS as usize);
        let fraction_micros: u64 = padded
            .parse()
            .map_err(|_| MoneyError::InvalidAmount(text.to_owned()))?;
        let micros = whole_units
            .checked_mul(MICROS_PER_MAJOR_UNIT)
            .and_then(|scaled| scaled.checked_add(fraction_micros))
            .ok_or(MoneyError::Overflow)?;
        Ok(Self { micros, currency })
    }

    pub const fn micros(&self) -> u64 {
        self.micros
    }

    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }

    pub fn major_units(&self) -> f64 {
        self.micros as f64 / MICROS_PER_MAJOR_UNIT as f64
    }

    pub fn formatted(&self) -> String {
        format!("{} {}", self.major_units_text(), self.currency)
    }

    fn major_units_text(&self) -> String {
        let whole = self.micros / MICROS_PER_MAJOR_UNIT;
        let fraction = self.micros % MICROS_PER_MAJOR_UNIT;
        if fraction == 0 {
            return whole.to_string();
        }
        let digits = format!("{fraction:0>width$}", width = MICRO_DIGITS as usize);
        let significant = digits.trim_end_matches('0');
        format!("{whole}.{significant:0<SHOWN_FRACTION_DIGITS$}")
    }
}

impl fmt::Display for MoneyAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.formatted())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn uah() -> CurrencyCode {
        CurrencyCode::parse("UAH").unwrap()
    }

    fn micros_of(text: &str) -> Result<u64, MoneyError> {
        MoneyAmount::parse_decimal(text, uah()).map(|amount| amount.micros())
    }

    #[test]
    fn currency_code_accepts_three_ascii_letters_uppercased_and_trimmed() {
        for (raw, canonical) in [
            ("UAH", "UAH"),
            ("uah", "UAH"),
            ("eUr", "EUR"),
            (" usd ", "USD"),
        ] {
            assert_eq!(
                CurrencyCode::parse(raw).map(|code| code.as_str().to_owned()),
                Ok(canonical.to_owned()),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn currency_code_rejects_anything_but_three_ascii_letters_echoing_the_input() {
        for raw in ["", "UA", "UAHH", "UAH1", "12A", "U A", "ÄBC", "грн"] {
            assert_eq!(
                CurrencyCode::parse(raw),
                Err(MoneyError::InvalidCurrency(raw.to_owned())),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn currency_code_deserialization_applies_the_same_validation_as_parse() {
        assert_eq!(
            serde_json::from_str::<CurrencyCode>("\"uah\"")
                .map(|code| code.to_string())
                .ok(),
            Some("UAH".to_owned())
        );
        assert!(serde_json::from_str::<CurrencyCode>("\"UAH1\"").is_err());
    }

    #[test]
    fn parse_decimal_converts_whole_and_fractional_amounts_to_micros() {
        for (text, micros) in [
            ("100", 100_000_000),
            ("100.5", 100_500_000),
            ("0.000001", 1),
            ("0", 0),
            ("007.25", 7_250_000),
            (" 12 ", 12_000_000),
            ("18446744073709.551615", u64::MAX),
        ] {
            assert_eq!(micros_of(text), Ok(micros), "text {text:?}");
        }
    }

    #[test]
    fn parse_decimal_rejects_malformed_numbers_echoing_the_input() {
        for text in [
            "", " ", ".5", "5.", "-1", "+1", "1,5", "1.2.3", "1 000", "abc", "1e3", "١٢",
        ] {
            assert_eq!(
                micros_of(text),
                Err(MoneyError::InvalidAmount(text.to_owned())),
                "text {text:?}"
            );
        }
    }

    #[test]
    fn parse_decimal_accepts_six_fraction_digits_and_rejects_seven() {
        assert_eq!(micros_of("1.123456"), Ok(1_123_456));
        assert_eq!(
            micros_of("1.1234567"),
            Err(MoneyError::TooPrecise("1.1234567".to_owned()))
        );
    }

    #[test]
    fn parse_decimal_reports_overflow_past_the_largest_representable_amount() {
        for text in [
            "18446744073709.551616",
            "18446744073710",
            "99999999999999999999999",
        ] {
            assert_eq!(micros_of(text), Err(MoneyError::Overflow), "text {text:?}");
        }
    }

    #[test]
    fn from_minor_units_scales_by_the_currency_exponent() {
        for (minor, exponent, micros) in [(100, 0, 100_000_000), (1250, 2, 12_500_000), (5, 6, 5)] {
            assert_eq!(
                MoneyAmount::from_minor_units(minor, exponent, uah()).map(|amount| amount.micros()),
                Ok(micros),
                "minor {minor} exponent {exponent}"
            );
        }
    }

    #[test]
    fn from_minor_units_rejects_an_exponent_finer_than_micros() {
        assert_eq!(
            MoneyAmount::from_minor_units(1, 7, uah()),
            Err(MoneyError::UnsupportedExponent(7))
        );
    }

    #[test]
    fn from_minor_units_reports_overflow_when_scaling_exceeds_u64() {
        assert_eq!(
            MoneyAmount::from_minor_units(u64::MAX, 2, uah()),
            Err(MoneyError::Overflow)
        );
    }

    #[test]
    fn formatted_drops_an_integral_fraction_and_shows_at_least_two_fraction_digits() {
        for (micros, shown) in [
            (100_000_000, "100 UAH"),
            (0, "0 UAH"),
            (12_500_000, "12.50 UAH"),
            (12_050_000, "12.05 UAH"),
            (125_000, "0.125 UAH"),
            (1, "0.000001 UAH"),
            (1_123_456, "1.123456 UAH"),
        ] {
            assert_eq!(
                MoneyAmount::from_micros(micros, uah()).formatted(),
                shown,
                "micros {micros}"
            );
        }
    }

    #[test]
    fn major_units_divides_micros_into_whole_currency_units() {
        assert_eq!(
            MoneyAmount::from_micros(12_500_000, uah()).major_units(),
            12.5
        );
    }
}
