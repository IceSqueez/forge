use std::fmt;

use reqwest::header::HeaderValue;

use crate::error::MonobankError;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct MonobankToken(String);

impl MonobankToken {
    pub(crate) fn parse(raw: &str) -> Result<Self, MonobankError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(MonobankError::MissingToken);
        }
        let token = Self(trimmed.to_owned());
        token.header_value()?;
        Ok(token)
    }

    pub(crate) fn header_value(&self) -> Result<HeaderValue, MonobankError> {
        let mut value =
            HeaderValue::from_str(&self.0).map_err(|_| MonobankError::MalformedToken)?;
        value.set_sensitive(true);
        Ok(value)
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for MonobankToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MonobankToken(***)")
    }
}
