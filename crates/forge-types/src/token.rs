use serde::{Deserialize, Serialize};
use std::fmt;

use crate::redaction::Redacted;

macro_rules! define_redacted_token {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[derive(Clone, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn expose(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), Redacted)
            }
        }
    };
}

define_redacted_token!(OAuthToken);
define_redacted_token!(RefreshToken);

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Why: downstream crates assert on the exact `Name(<redacted>)` spelling; routing the macro
    /// through the shared `Redacted` placeholder must leave it byte-identical.
    #[test]
    fn token_debug_renders_the_type_name_wrapping_the_bare_marker() {
        assert_eq!(
            format!("{:?}", OAuthToken::new("SENTINEL_SECRET")),
            "OAuthToken(<redacted>)"
        );
        assert_eq!(
            format!("{:?}", RefreshToken::new("SENTINEL_SECRET")),
            "RefreshToken(<redacted>)"
        );
    }

    #[test]
    fn token_serde_roundtrip() {
        let tok = OAuthToken::new("roundtrip_value");
        let json = serde_json::to_string(&tok).unwrap();
        let back: OAuthToken = serde_json::from_str(&json).unwrap();
        assert_eq!(back.expose(), tok.expose());
    }
}
