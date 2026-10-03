use forge_types::{CurrencyCode, MoneyAmount, MoneyError};

const CENTS_EXPONENT: u32 = 2;
const ISO_NUMERIC_UAH: i64 = 980;
const ISO_NUMERIC_USD: i64 = 840;
const ISO_NUMERIC_EUR: i64 = 978;

const KNOWN_CURRENCIES: [(i64, &str, u32); 3] = [
    (ISO_NUMERIC_UAH, "UAH", CENTS_EXPONENT),
    (ISO_NUMERIC_USD, "USD", CENTS_EXPONENT),
    (ISO_NUMERIC_EUR, "EUR", CENTS_EXPONENT),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnownCurrency {
    pub(crate) code: CurrencyCode,
    exponent: u32,
}

impl KnownCurrency {
    pub(crate) fn amount(&self, minor_units: u64) -> Result<MoneyAmount, MoneyError> {
        MoneyAmount::from_minor_units(minor_units, self.exponent, self.code.clone())
    }
}

pub(crate) fn from_iso_numeric(numeric: i64) -> Option<KnownCurrency> {
    KNOWN_CURRENCIES
        .iter()
        .find(|(known, _, _)| *known == numeric)
        .and_then(|(_, alpha, exponent)| {
            CurrencyCode::parse(alpha).ok().map(|code| KnownCurrency {
                code,
                exponent: *exponent,
            })
        })
}
