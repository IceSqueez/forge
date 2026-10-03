use std::fmt;

use reqwest::header::HeaderValue;

use crate::error::DonatelloError;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DonatelloToken(String);

impl DonatelloToken {
    pub(crate) fn parse(raw: &str) -> Result<Self, DonatelloError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(DonatelloError::MissingToken);
        }
        let token = Self(trimmed.to_owned());
        token.header_value()?;
        Ok(token)
    }

    pub(crate) fn header_value(&self) -> Result<HeaderValue, DonatelloError> {
        let mut value =
            HeaderValue::from_str(&self.0).map_err(|_| DonatelloError::MalformedToken)?;
        value.set_sensitive(true);
        Ok(value)
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for DonatelloToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DonatelloToken(***)")
    }
}
