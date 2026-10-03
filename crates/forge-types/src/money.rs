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
