use forge_net::is_valid_header_value;
use forge_types::Secret;

use crate::error::DonatelloError;

pub(crate) fn parse_token(raw: &str) -> Result<Secret, DonatelloError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DonatelloError::MissingToken);
    }
    if !is_valid_header_value(trimmed) {
        return Err(DonatelloError::MalformedToken);
    }
    Ok(Secret::new(trimmed))
}

#[cfg(test)]
mod tests {
    use super::parse_token;
    use crate::error::DonatelloError;
    use forge_types::Secret;

    const RAW: &str = "dn-secret-token-value";

    #[test]
    fn parse_trims_surrounding_whitespace() {
        let token = parse_token(&format!("  {RAW}\n")).ok();
        assert_eq!(token.as_ref().map(Secret::expose), Some(RAW));
    }

    #[test]
    fn parse_rejects_blank_input_as_missing() {
        for raw in ["", "   ", "\n\t"] {
            assert!(
                matches!(parse_token(raw), Err(DonatelloError::MissingToken)),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn parse_rejects_text_that_cannot_travel_in_a_header() {
        for raw in ["tok\nen", "tok\u{7f}en", "tok\0en"] {
            assert!(
                matches!(parse_token(raw), Err(DonatelloError::MalformedToken)),
                "raw {raw:?}"
            );
        }
    }

    #[test]
    fn debug_never_shows_the_token() {
        let token = parse_token(RAW).ok();
        assert!(!format!("{token:?}").contains(RAW));
    }

    #[test]
    fn parse_keeps_inner_tabs_and_non_ascii_text_that_a_header_can_carry() {
        for raw in ["tok\ten", "токен-é"] {
            let token = parse_token(raw).ok();
            assert_eq!(token.as_ref().map(Secret::expose), Some(raw), "raw {raw:?}");
        }
    }
}
