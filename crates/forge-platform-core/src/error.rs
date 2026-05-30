use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("network failure: {reason}")]
    Network { reason: String },

    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    #[error("authentication failure: {reason}")]
    Auth { reason: String },

    /// Refresh token rejected by the platform; the UI must prompt re-authentication.
    #[error("re-authentication required for platform '{platform}'")]
    ReauthRequired { platform: String },

    #[error("rate limited by platform; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u32 },

    #[error("client-side rate limit exhausted; no budget remaining")]
    RateLimitExhausted,

    /// Daily API quota for this platform has been exhausted; no further calls will succeed
    /// until midnight reset. Callers should switch to long-interval mode or suspend polling.
    #[error("daily API quota exhausted; next reset at platform midnight")]
    QuotaExhausted,

    #[error("feature '{feature}' is not supported by this platform")]
    Unsupported { feature: String },

    #[error("payload deserialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("local I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn network_displays_non_empty() {
        let e = PlatformError::Network {
            reason: "connection refused".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn http_displays_non_empty() {
        let e = PlatformError::Http {
            status: 429,
            body: "too many requests".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn auth_displays_non_empty() {
        let e = PlatformError::Auth {
            reason: "invalid credentials".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn reauth_required_carries_platform_name() {
        let e = PlatformError::ReauthRequired {
            platform: "twitch".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("twitch"), "expected platform name in: {msg}");
        assert!(!msg.is_empty());
    }

    #[test]
    fn rate_limited_displays_non_empty() {
        let e = PlatformError::RateLimited {
            retry_after_secs: 30,
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn rate_limit_exhausted_displays_non_empty() {
        let e = PlatformError::RateLimitExhausted;
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn quota_exhausted_displays_non_empty() {
        let e = PlatformError::QuotaExhausted;
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn unsupported_displays_non_empty() {
        let e = PlatformError::Unsupported {
            feature: "polls".into(),
        };
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn serialization_from_serde_error() {
        let result = serde_json::from_str::<serde_json::Value>("{invalid}");
        assert!(result.is_err());
        if let Err(serde_err) = result {
            let e: PlatformError = serde_err.into();
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn io_from_std_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let e: PlatformError = io_err.into();
        assert!(!e.to_string().is_empty());
    }

    #[test]
    fn platform_error_satisfies_std_error_trait() {
        fn accepts_error<E: Error>(_: &E) {}
        let e = PlatformError::RateLimitExhausted;
        accepts_error(&e);
    }
}
