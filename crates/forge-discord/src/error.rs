use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscordError {
    #[error("transport error: {0}")]
    Connect(String),

    #[error("rate limited; retry after {retry_after_secs:.2}s")]
    RateLimited { retry_after_secs: f64 },

    #[error("forbidden")]
    Forbidden,

    #[error("webhook not found")]
    NotFound,

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("credential error: {0}")]
    Credential(String),

    #[error("HTTP {status}: {body}")]
    BadResponse { status: u16, body: String },
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rate_limited_display() {
        let e = DiscordError::RateLimited {
            retry_after_secs: 1.5,
        };
        assert!(e.to_string().contains("1.50"));
    }

    #[test]
    fn bad_response_display() {
        let e = DiscordError::BadResponse {
            status: 404,
            body: "unknown webhook".to_owned(),
        };
        let s = e.to_string();
        assert!(s.contains("404"));
        assert!(s.contains("unknown webhook"));
    }

    #[test]
    fn serde_from_converts_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
        let e: DiscordError = json_err.into();
        assert!(matches!(e, DiscordError::Serde(_)));
    }
}
