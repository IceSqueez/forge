use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueBounds {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

impl ValueBounds {
    pub fn between(min: impl TryInto<i64>, max: impl TryInto<i64>) -> Self {
        Self {
            min: Some(saturating_bound(min)),
            max: Some(saturating_bound(max)),
        }
    }

    pub fn at_least(min: impl TryInto<i64>) -> Self {
        Self {
            min: Some(saturating_bound(min)),
            max: None,
        }
    }

    pub fn at_most(max: impl TryInto<i64>) -> Self {
        Self {
            min: None,
            max: Some(saturating_bound(max)),
        }
    }
}

fn saturating_bound(value: impl TryInto<i64>) -> i64 {
    value.try_into().unwrap_or(i64::MAX)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigReason {
    Required,
    OutOfRange(ValueBounds),
    LengthOutOfRange(ValueBounds),
    NotANumber,
    InvalidChoice(Vec<String>),
    InvalidFormat(String),
    Custom(String),
}

impl ConfigReason {
    pub fn choices<I, S>(choices: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::InvalidChoice(choices.into_iter().map(Into::into).collect())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigProblem {
    pub field: Option<String>,
    pub reason: ConfigReason,
}

impl ConfigProblem {
    pub fn field(field: impl Into<String>, reason: ConfigReason) -> Self {
        Self {
            field: Some(field.into()),
            reason,
        }
    }

    pub fn whole(reason: ConfigReason) -> Self {
        Self {
            field: None,
            reason,
        }
    }
}

impl fmt::Display for ValueBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.min, self.max) {
            (Some(min), Some(max)) => write!(f, "between {min} and {max}"),
            (Some(min), None) => write!(f, "at least {min}"),
            (None, Some(max)) => write!(f, "at most {max}"),
            (None, None) => f.write_str("within its allowed range"),
        }
    }
}

impl fmt::Display for ConfigReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Required => f.write_str("is required"),
            Self::OutOfRange(bounds) => write!(f, "must be {bounds}"),
            Self::LengthOutOfRange(bounds) => write!(f, "must be {bounds} characters long"),
            Self::NotANumber => f.write_str("must be a number"),
            Self::InvalidChoice(choices) => write!(f, "must be one of: {}", choices.join(", ")),
            Self::InvalidFormat(detail) => write!(f, "is invalid: {detail}"),
            Self::Custom(text) => f.write_str(text),
        }
    }
}

impl fmt::Display for ConfigProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.field, &self.reason) {
            (Some(field), ConfigReason::Custom(text)) => write!(f, "'{field}': {text}"),
            (Some(field), reason) => write!(f, "'{field}' {reason}"),
            (None, ConfigReason::Custom(text)) => f.write_str(text),
            (None, reason) => write!(f, "value {reason}"),
        }
    }
}
