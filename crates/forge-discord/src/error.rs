use thiserror::Error;

#[derive(Debug, Error)]
pub enum DiscordError {
    #[error("transport error: {0}")]
    Connect(String),

    #[error("rate limited; retry after {retry_after_secs:.2}s")]
    RateLimited { retry_after_secs: f64 },

    #[error("webhook not found: {name:?}")]
    WebhookNotFound { name: String },

    #[error("validation failed: {0}")]
    Validation(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("credential error: {0}")]
    Credential(String),

    #[error("HTTP {status}: {body}")]
    BadResponse { status: u16, body: String },
}

impl DiscordError {
    pub(crate) fn reason_token(&self) -> &'static str {
        match self {
            DiscordError::Connect(_) => "network",
            DiscordError::RateLimited { .. } => "rate_limited",
            DiscordError::WebhookNotFound { .. } => "webhook_not_found",
            DiscordError::Validation(_) => "validation_failed",
            DiscordError::Serde(_) => "serialization_error",
            DiscordError::Credential(_) => "credential_error",
            DiscordError::BadResponse { .. } => "http_status",
        }
    }

    pub(crate) fn status_code(&self) -> Option<u16> {
        match self {
            DiscordError::BadResponse { status, .. } => Some(*status),
            _ => None,
        }
    }

    pub(crate) fn detail(&self) -> String {
        match self {
            DiscordError::Connect(msg) => msg.clone(),
            DiscordError::RateLimited { retry_after_secs } => {
                format!("retry after {retry_after_secs:.2}s")
            }
            DiscordError::WebhookNotFound { name } => format!("webhook {name:?} not configured"),
            DiscordError::Validation(msg) => msg.clone(),
            DiscordError::Serde(err) => err.to_string(),
            DiscordError::Credential(msg) => msg.clone(),
            DiscordError::BadResponse { body, .. } => body.clone(),
        }
    }
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
    fn webhook_not_found_display_contains_name() {
        let e = DiscordError::WebhookNotFound {
            name: "alerts".to_owned(),
        };
        assert!(e.to_string().contains("alerts"));
    }

    #[test]
    fn reason_token_and_status_code_map_per_variant() {
        let serde_variant: DiscordError = serde_json::from_str::<serde_json::Value>("{bad}")
            .unwrap_err()
            .into();
        let cases: [(DiscordError, &str, Option<u16>); 7] = [
            (DiscordError::Connect("x".to_owned()), "network", None),
            (
                DiscordError::RateLimited {
                    retry_after_secs: 1.0,
                },
                "rate_limited",
                None,
            ),
            (
                DiscordError::WebhookNotFound {
                    name: "a".to_owned(),
                },
                "webhook_not_found",
                None,
            ),
            (
                DiscordError::Validation("v".to_owned()),
                "validation_failed",
                None,
            ),
            (serde_variant, "serialization_error", None),
            (
                DiscordError::Credential("c".to_owned()),
                "credential_error",
                None,
            ),
            (
                DiscordError::BadResponse {
                    status: 503,
                    body: "b".to_owned(),
                },
                "http_status",
                Some(503),
            ),
        ];
        for (err, token, status) in cases {
            assert_eq!(err.reason_token(), token);
            assert_eq!(err.status_code(), status);
        }
    }
}
