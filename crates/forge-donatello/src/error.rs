use forge_net::{GatewayFailure, InvalidRequest, ResponseDecodeError, canonical_status_reason};
use forge_platform_core::PlatformError;
use thiserror::Error;

use crate::integration::DONATELLO_INTEGRATION;

#[derive(Debug, Error)]
pub enum DonatelloError {
    #[error("no Donatello token is stored")]
    MissingToken,

    #[error("the Donatello token contains characters that cannot be sent in a request header")]
    MalformedToken,

    #[error("Donatello rejected the token")]
    Unauthorized,

    #[error("the Donatello profile setup is incomplete")]
    ProfileIncomplete,

    #[error("rate limited by Donatello; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u32 },

    #[error("client-side rate limit exhausted; no request budget remaining")]
    RateLimitExhausted,

    #[error("Donatello answered HTTP {status}")]
    Http { status: u16 },

    #[error("network failure: {reason}")]
    Network { reason: String },

    #[error("malformed Donatello response: {reason}")]
    MalformedResponse { reason: String },

    #[error("donation {donation_id} could not be read: {reason}")]
    InvalidDonation { donation_id: String, reason: String },

    #[error("credential storage failure: {reason}")]
    Credential { reason: String },

    #[error("HTTP client could not be built: {reason}")]
    ClientInit { reason: String },

    #[error("no async runtime is available to run the Donatello poller")]
    NoRuntime,
}

impl DonatelloError {
    pub fn requires_user_action(&self) -> bool {
        matches!(
            self,
            Self::MissingToken
                | Self::MalformedToken
                | Self::Unauthorized
                | Self::ProfileIncomplete
        )
    }

    pub(crate) fn from_decode(error: ResponseDecodeError) -> Self {
        Self::MalformedResponse {
            reason: format!(
                "{:?} error at line {} column {}",
                error.category, error.line, error.column
            ),
        }
    }
}

impl From<GatewayFailure> for DonatelloError {
    fn from(failure: GatewayFailure) -> Self {
        match failure {
            GatewayFailure::Unauthorized { .. } => Self::Unauthorized,
            GatewayFailure::RateLimited { retry_after, .. } => Self::RateLimited {
                retry_after_secs: u32::try_from(
                    retry_after
                        .as_secs()
                        .saturating_add(u64::from(retry_after.subsec_nanos() > 0)),
                )
                .unwrap_or(u32::MAX),
            },
            GatewayFailure::BudgetExhausted { .. } => Self::RateLimitExhausted,
            GatewayFailure::InvalidRequest(InvalidRequest::InvalidHeader) => Self::MalformedToken,
            other => match other.status() {
                Some(status) => Self::Http { status },
                None => Self::Network {
                    reason: other.to_string(),
                },
            },
        }
    }
}

impl From<DonatelloError> for PlatformError {
    fn from(error: DonatelloError) -> Self {
        match error {
            DonatelloError::Unauthorized => PlatformError::ReauthRequired {
                platform: DONATELLO_INTEGRATION.id.as_str().to_owned(),
            },
            DonatelloError::MissingToken
            | DonatelloError::MalformedToken
            | DonatelloError::ProfileIncomplete => PlatformError::Auth {
                reason: error.to_string(),
            },
            DonatelloError::RateLimited { retry_after_secs } => {
                PlatformError::RateLimited { retry_after_secs }
            }
            DonatelloError::RateLimitExhausted => PlatformError::RateLimitExhausted,
            DonatelloError::Http { status } => PlatformError::Http {
                status,
                body: canonical_status_reason(status)
                    .unwrap_or_default()
                    .to_owned(),
            },
            DonatelloError::Network { reason } => PlatformError::Network { reason },
            DonatelloError::MalformedResponse { .. } | DonatelloError::InvalidDonation { .. } => {
                PlatformError::MalformedResponse {
                    reason: error.to_string(),
                }
            }
            DonatelloError::Credential { .. }
            | DonatelloError::ClientInit { .. }
            | DonatelloError::NoRuntime => {
                PlatformError::Io(std::io::Error::other(error.to_string()))
            }
        }
    }
}
