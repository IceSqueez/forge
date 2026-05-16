use serde::{Deserialize, Serialize};
use std::fmt;

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
                f.write_str(concat!(stringify!($name), "(<redacted>)"))
            }
        }
    };
}

define_redacted_token!(
    /// An OAuth access token. `Debug` output is redacted; use `expose()` for the raw value.
    OAuthToken
);

define_redacted_token!(
    /// An OAuth refresh token. `Debug` output is redacted; use `expose()` for the raw value.
    RefreshToken
);

define_redacted_token!(
    /// An API key for a third-party service. `Debug` output is redacted; use `expose()` for the raw value.
    ApiKey
);

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn oauth_token_debug_is_redacted() {
        let tok = OAuthToken::new("super_secret_value");
        let dbg = format!("{tok:?}");
        assert!(!dbg.contains("super_secret_value"));
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn refresh_token_debug_is_redacted() {
        let tok = RefreshToken::new("another_secret");
        let dbg = format!("{tok:?}");
        assert!(!dbg.contains("another_secret"));
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn api_key_debug_is_redacted() {
        let key = ApiKey::new("sk-live-abc123");
        let dbg = format!("{key:?}");
        assert!(!dbg.contains("sk-live-abc123"));
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn expose_returns_raw_value() {
        let tok = OAuthToken::new("raw_token_value");
        assert_eq!(tok.expose(), "raw_token_value");
    }

    #[test]
    fn token_serde_roundtrip() {
        let tok = OAuthToken::new("roundtrip_value");
        let json = serde_json::to_string(&tok).unwrap();
        let back: OAuthToken = serde_json::from_str(&json).unwrap();
        assert_eq!(back.expose(), tok.expose());
    }
}
