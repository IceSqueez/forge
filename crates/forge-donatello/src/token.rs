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

#[cfg(test)]
mod tests {
    use super::DonatelloToken;
    use crate::error::DonatelloError;

    const RAW: &str = "dn-secret-token-value";

    #[test]
    fn parse_trims_surrounding_whitespace() {
        let token = DonatelloToken::parse(&format!("  {RAW}\n")).ok();
        assert_eq!(token.as_ref().map(DonatelloToken::expose), Some(RAW));
    }

    #[test]
    fn parse_rejects_blank_input_as_missing() {
        for raw in ["", "   ", "\n\t"] {
            assert!(
                matches!(
                    DonatelloToken::parse(raw),
                    Err(DonatelloError::MissingToken)
                ),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn parse_rejects_text_that_cannot_travel_in_a_header() {
        for raw in ["tok\nen", "tok\u{7f}en", "tok\0en"] {
            assert!(
                matches!(
                    DonatelloToken::parse(raw),
                    Err(DonatelloError::MalformedToken)
                ),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn debug_never_shows_the_token() {
        let token = DonatelloToken::parse(RAW).ok();
        assert!(!format!("{token:?}").contains(RAW));
    }

    #[test]
    fn header_value_is_marked_sensitive() {
        let header = DonatelloToken::parse(RAW).and_then(|token| token.header_value());
        assert!(header.is_ok_and(|value| value.is_sensitive()));
    }
}
