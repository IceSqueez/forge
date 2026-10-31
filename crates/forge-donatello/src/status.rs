use forge_platform_core::{PollError, PollStatusBoard};
use forge_types::MessageRef;

use crate::api::DonatelloAccount;
use crate::error::DonatelloError;
use crate::messages;

pub use forge_platform_core::PollPhase;

pub type PollStatus = forge_platform_core::PollStatus<DonatelloAccount, PollFailure>;
pub(crate) type StatusBoard = PollStatusBoard<DonatelloAccount, PollFailure>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollFailure {
    MissingToken,
    TokenRejected,
    TokenUnreadable,
    ProfileIncomplete,
    RateLimited,
    Network,
    ServerError,
    MalformedResponse,
    InvalidDonation,
    Local,
}

impl PollFailure {
    pub fn of(error: &DonatelloError) -> Self {
        match error {
            DonatelloError::MissingToken => Self::MissingToken,
            DonatelloError::MalformedToken => Self::TokenUnreadable,
            DonatelloError::Unauthorized => Self::TokenRejected,
            DonatelloError::ProfileIncomplete => Self::ProfileIncomplete,
            DonatelloError::RateLimited { .. } | DonatelloError::RateLimitExhausted => {
                Self::RateLimited
            }
            DonatelloError::Http { .. } => Self::ServerError,
            DonatelloError::Network { .. } => Self::Network,
            DonatelloError::MalformedResponse { .. } => Self::MalformedResponse,
            DonatelloError::InvalidDonation { .. } => Self::InvalidDonation,
            DonatelloError::Credential(_)
            | DonatelloError::CredentialBundle { .. }
            | DonatelloError::ClientInit { .. }
            | DonatelloError::NoRuntime => Self::Local,
        }
    }

    pub fn message(self) -> MessageRef {
        MessageRef::new(match self {
            Self::MissingToken => messages::FAILURE_MISSING_TOKEN,
            Self::TokenRejected => messages::FAILURE_TOKEN_REJECTED,
            Self::TokenUnreadable => messages::FAILURE_TOKEN_UNREADABLE,
            Self::ProfileIncomplete => messages::FAILURE_PROFILE_INCOMPLETE,
            Self::RateLimited => messages::FAILURE_RATE_LIMITED,
            Self::Network => messages::FAILURE_NETWORK,
            Self::ServerError => messages::FAILURE_SERVER_ERROR,
            Self::MalformedResponse => messages::FAILURE_MALFORMED_RESPONSE,
            Self::InvalidDonation => messages::FAILURE_INVALID_DONATION,
            Self::Local => messages::FAILURE_LOCAL,
        })
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MissingToken => "No token",
            Self::TokenRejected => "Token rejected",
            Self::TokenUnreadable => "Token unreadable",
            Self::ProfileIncomplete => "Profile incomplete",
            Self::RateLimited => "Rate limited",
            Self::Network => "Network error",
            Self::ServerError => "Server error",
            Self::MalformedResponse => "Unreadable response",
            Self::InvalidDonation => "Unreadable donation",
            Self::Local => "Local error",
        }
    }
}

impl PollError for DonatelloError {
    type Failure = PollFailure;

    fn failure(&self) -> PollFailure {
        PollFailure::of(self)
    }

    fn failure_label(&self) -> &'static str {
        PollFailure::of(self).label()
    }

    fn needs_user_action(&self) -> bool {
        self.requires_user_action()
    }

    fn is_missing_token(&self) -> bool {
        matches!(self, Self::MissingToken)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn board() -> StatusBoard {
        StatusBoard::new("test", Duration::ZERO, crate::health::metrics)
    }

    #[test]
    fn a_token_that_cannot_be_read_stops_polling_under_its_own_failure() {
        for (error, failure) in [
            (DonatelloError::MalformedToken, PollFailure::TokenUnreadable),
            (DonatelloError::Unauthorized, PollFailure::TokenRejected),
        ] {
            let board = board();
            board.record_failure(&error);

            let status = board.snapshot();

            assert_eq!(
                (status.phase, status.last_error),
                (PollPhase::ActionRequired, Some(failure)),
                "{error:?}"
            );
        }
    }
}
