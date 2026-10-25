use forge_net::{GatewayFailure, InvalidRequest, ResponseDecodeError, canonical_status_reason};
use forge_platform_core::PlatformError;
use thiserror::Error;

use crate::api::whole_seconds;
use crate::config::STATEMENT_CALL_INTERVAL;
use crate::integration::MONOBANK_INTEGRATION;

#[derive(Debug, Error)]
pub enum MonobankError {
    #[error("no monobank token is stored")]
    MissingToken,

    #[error("the monobank token contains characters that cannot be sent in a request header")]
    MalformedToken,

    #[error("monobank rejected the token")]
    Unauthorized,

    #[error("no monobank jar is selected")]
    MissingJar,

    #[error("the selected monobank jar is not among the jars of this token")]
    JarNotFound,

    #[error("the monobank jar identifier is not valid")]
    InvalidJarId,

    #[error("monobank allows one call per minute; next call in {retry_after_secs}s")]
    CoolingDown { retry_after_secs: u32 },

    #[error("rate limited by monobank; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u32 },

    #[error("monobank answered HTTP {status}")]
    Http { status: u16 },

    #[error("network failure: {reason}")]
    Network { reason: String },

    #[error("malformed monobank response: {reason}")]
    MalformedResponse { reason: String },

    #[error("transaction {donation_id} could not be read: {reason}")]
    InvalidDonation { donation_id: String, reason: String },

    #[error("credential storage failure: {reason}")]
    Credential { reason: String },

    #[error("HTTP client could not be built: {reason}")]
    ClientInit { reason: String },

    #[error("no async runtime is available to run the monobank poller")]
    NoRuntime,
}

impl MonobankError {
    pub fn requires_user_action(&self) -> bool {
        matches!(
            self,
            Self::MissingToken
                | Self::MalformedToken
                | Self::Unauthorized
                | Self::MissingJar
                | Self::JarNotFound
                | Self::InvalidJarId
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

impl From<GatewayFailure> for MonobankError {
    fn from(failure: GatewayFailure) -> Self {
        match failure {
            GatewayFailure::Unauthorized { .. } | GatewayFailure::Forbidden { .. } => {
                Self::Unauthorized
            }
            GatewayFailure::RateLimited { retry_after, .. } => Self::RateLimited {
                retry_after_secs: whole_seconds(retry_after.max(STATEMENT_CALL_INTERVAL)),
            },
            GatewayFailure::BudgetExhausted { retry_after } => Self::CoolingDown {
                retry_after_secs: whole_seconds(retry_after.unwrap_or(STATEMENT_CALL_INTERVAL)),
            },
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

impl From<MonobankError> for PlatformError {
    fn from(error: MonobankError) -> Self {
        match error {
            MonobankError::Unauthorized => PlatformError::ReauthRequired {
                platform: MONOBANK_INTEGRATION.id.as_str().to_owned(),
            },
            MonobankError::MissingToken
            | MonobankError::MalformedToken
            | MonobankError::MissingJar
            | MonobankError::JarNotFound
            | MonobankError::InvalidJarId => PlatformError::Auth {
                reason: error.to_string(),
            },
            MonobankError::CoolingDown { retry_after_secs }
            | MonobankError::RateLimited { retry_after_secs } => {
                PlatformError::RateLimited { retry_after_secs }
            }
            MonobankError::Http { status } => PlatformError::Http {
                status,
                body: canonical_status_reason(status)
                    .unwrap_or_default()
                    .to_owned(),
            },
            MonobankError::Network { reason } => PlatformError::Network { reason },
            MonobankError::MalformedResponse { .. } | MonobankError::InvalidDonation { .. } => {
                PlatformError::MalformedResponse {
                    reason: error.to_string(),
                }
            }
            MonobankError::Credential { .. }
            | MonobankError::ClientInit { .. }
            | MonobankError::NoRuntime => {
                PlatformError::Io(std::io::Error::other(error.to_string()))
            }
        }
    }
}
