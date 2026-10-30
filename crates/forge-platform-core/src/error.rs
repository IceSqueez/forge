use forge_net::RateLimitError;
use thiserror::Error;

use crate::endpoints::EndpointRefusal;

pub const HTTP_UNAUTHORIZED: u16 = forge_net::status::UNAUTHORIZED;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceAuthEnd {
    Cancelled,
    Denied,
    Expired,
}

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("network failure: {reason}")]
    Network { reason: String },

    #[error("HTTP {status} ({})", forge_net::status_class(*status))]
    Http { status: u16, body: String },

    #[error("{reason}")]
    Refused { reason: String },

    #[error("authentication failure: {reason}")]
    Auth { reason: String },

    #[error("device authorization ended: {0:?}")]
    DeviceAuthEnded(DeviceAuthEnd),

    #[error("loopback port {port} is already in use; another forge instance may be running")]
    LoopbackPortInUse { port: u16 },

    #[error("re-authentication required for platform '{platform}'")]
    ReauthRequired { platform: String },

    #[error("rate limited by platform; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u32 },

    #[error("client-side rate limit exhausted; no budget remaining")]
    RateLimitExhausted,

    #[error("daily API quota exhausted")]
    QuotaExhausted,

    #[error("feature '{feature}' is not supported by this platform")]
    Unsupported { feature: String },

    #[error("malformed response: {reason}")]
    MalformedResponse { reason: String },

    #[error("payload deserialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("local I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("endpoint override {variable} refused: {reason}")]
    EndpointOverrideRefused {
        variable: &'static str,
        reason: EndpointRefusal,
    },
}

impl From<RateLimitError> for PlatformError {
    fn from(error: RateLimitError) -> Self {
        match error {
            RateLimitError::Exhausted => Self::RateLimitExhausted,
        }
    }
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

    #[test]
    fn http_error_display_names_status_and_class_and_never_the_body() {
        for (status, rendered) in [
            (401, "HTTP 401 (client error)"),
            (503, "HTTP 503 (server error)"),
            (0, "HTTP 0 (unexpected status)"),
        ] {
            let e = PlatformError::Http {
                status,
                body: "token=abc123".into(),
            };
            assert_eq!(e.to_string(), rendered);
        }
    }

    #[test]
    fn refused_error_display_is_its_reason_without_a_status_prefix() {
        let e = PlatformError::Refused {
            reason: "ids list is empty".into(),
        };
        assert_eq!(e.to_string(), "ids list is empty");
    }
}
