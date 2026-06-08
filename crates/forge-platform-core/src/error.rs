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

    #[test]
    fn reauth_required_carries_platform_name() {
        let e = PlatformError::ReauthRequired {
            platform: "twitch".into(),
        };
        let msg = e.to_string();
        assert!(msg.contains("twitch"), "expected platform name in: {msg}");
    }
}
