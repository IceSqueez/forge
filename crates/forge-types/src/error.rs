use thiserror::Error;

use crate::message::MessageCode;
use crate::variant::VariantKind;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TypesError {
    #[error("type mismatch: expected {expected}, got {actual}")]
    TypeMismatch {
        expected: VariantKind,
        actual: VariantKind,
    },

    #[error("invalid float: {reason}")]
    InvalidFloat { reason: &'static str },

    #[error("JSON conversion failed: {0}")]
    JsonConversion(String),

    #[error("null is not a supported Variant value")]
    NullNotSupported,

    #[error("unknown type tag: {0}")]
    UnknownTypeTag(String),

    #[error("currency code must be three ASCII letters: {0:?}")]
    InvalidCurrency(String),

    #[error("amount is not a non-negative decimal number: {0:?}")]
    InvalidAmount(String),

    #[error("amount has more fractional digits than micros can hold: {0:?}")]
    TooPrecise(String),

    #[error("currency exponent {0} is finer than micros")]
    UnsupportedExponent(u32),

    #[error("amount does not fit the supported range")]
    MoneyOverflow,

    #[error("unknown permission rung: {0}")]
    UnknownPermissionRung(String),

    #[error("the date pattern is empty")]
    EmptyDatePattern,

    #[error("quoted text starting at position {position} is never closed with '")]
    UnterminatedQuote { position: usize },

    #[error(
        "'{letter}' at position {position} is not a date pattern letter; put literal text in single quotes"
    )]
    UnsupportedLetter { letter: char, position: usize },

    #[error("'{spelling}' at position {position} is not a supported length for '{letter}'")]
    UnsupportedLength {
        letter: char,
        spelling: String,
        position: usize,
    },

    #[error("'{spelling}' at position {position} is not supported; use '{replacement}'")]
    DotNetSpelling {
        spelling: String,
        replacement: &'static str,
        position: usize,
    },

    #[error("expected {expected} at position {position}")]
    DateTextMismatch { expected: String, position: usize },

    #[error("unexpected text at position {position} after the end of the pattern")]
    TrailingText { position: usize },

    #[error("the pattern must contain a year, a month and a day")]
    MissingDate,

    #[error("a 12-hour field needs a day period ('a') in the same pattern")]
    HourWithoutPeriod,

    #[error("the text names a date or time that does not exist")]
    DateOutOfRange,

    #[error("the weekday does not match the date")]
    WeekdayMismatch,

    #[error("message code must be `namespace.name` of lowercase snake_case segments: {0:?}")]
    MalformedMessageCode(String),

    #[error("catalog namespace is not a lowercase snake_case segment: {0:?}")]
    MalformedNamespace(&'static str),

    #[error("catalog code is malformed: {0}")]
    MalformedCode(MessageCode),

    #[error("catalog {namespace:?} lists a code from another namespace: {code}")]
    ForeignCode {
        namespace: &'static str,
        code: MessageCode,
    },

    #[error("catalog lists a code twice: {0}")]
    DuplicateCode(MessageCode),
}
