use std::fmt;

use forge_net::{Secret, is_valid_header_value};

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
        if !is_valid_header_value(&token.0) {
            return Err(MonobankError::MalformedToken);
        }
        Ok(token)
    }

    pub(crate) fn secret(&self) -> Secret {
        Secret::new(self.0.clone())
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

#[cfg(test)]
mod tests {
    use super::MonobankToken;
    use crate::error::MonobankError;

    const RAW: &str = "uMono-secret-token-value";

    #[test]
    fn parse_trims_surrounding_whitespace() {
        let token = MonobankToken::parse(&format!("  {RAW}\n")).ok();
        assert_eq!(token.as_ref().map(MonobankToken::expose), Some(RAW));
    }

    #[test]
    fn parse_rejects_blank_input_as_missing() {
        for raw in ["", "   ", "\n\t"] {
            assert!(
                matches!(MonobankToken::parse(raw), Err(MonobankError::MissingToken)),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn parse_rejects_text_that_cannot_travel_in_a_header() {
        for raw in ["tok\nen", "tok\u{7f}en", "tok\0en"] {
            assert!(
                matches!(
                    MonobankToken::parse(raw),
                    Err(MonobankError::MalformedToken)
                ),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn debug_never_shows_the_token() {
        let token = MonobankToken::parse(RAW).ok();
        assert!(!format!("{token:?}").contains(RAW));
    }

    #[test]
    fn parse_keeps_inner_tabs_and_non_ascii_text_that_a_header_can_carry() {
        for raw in ["tok\ten", "токен-é"] {
            let token = MonobankToken::parse(raw).ok();
            assert_eq!(
                token.as_ref().map(MonobankToken::expose),
                Some(raw),
                "raw {raw:?}"
            );
        }
    }
}
