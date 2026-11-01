use forge_platform_core::{PollError, PollStatusBoard};
use forge_types::{MessageCode, MessageRef};

use crate::api::DonatelloAccount;
use crate::error::DonatelloError;
use crate::messages;

pub use forge_platform_core::PollPhase;

pub type PollStatus = forge_platform_core::PollStatus<DonatelloAccount, PollFailure>;
pub(crate) type StatusBoard = PollStatusBoard<DonatelloAccount, PollFailure>;

struct FailureFacts {
    message: MessageCode,
    label: &'static str,
    banner: Option<MessageCode>,
}

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

    const fn facts(self) -> FailureFacts {
        match self {
            Self::MissingToken => FailureFacts {
                message: messages::FAILURE_MISSING_TOKEN,
                label: "No token",
                banner: Some(messages::LABEL_BANNER_TOKEN_MISSING),
            },
            Self::TokenRejected => FailureFacts {
                message: messages::FAILURE_TOKEN_REJECTED,
                label: "Token rejected",
                banner: Some(messages::LABEL_BANNER_TOKEN_REJECTED),
            },
            Self::TokenUnreadable => FailureFacts {
                message: messages::FAILURE_TOKEN_UNREADABLE,
                label: "Token unreadable",
                banner: Some(messages::LABEL_BANNER_TOKEN_UNREADABLE),
            },
            Self::ProfileIncomplete => FailureFacts {
                message: messages::FAILURE_PROFILE_INCOMPLETE,
                label: "Profile incomplete",
                banner: Some(messages::LABEL_BANNER_PROFILE_INCOMPLETE),
            },
            Self::RateLimited => FailureFacts {
                message: messages::FAILURE_RATE_LIMITED,
                label: "Rate limited",
                banner: None,
            },
            Self::Network => FailureFacts {
                message: messages::FAILURE_NETWORK,
                label: "Network error",
                banner: None,
            },
            Self::ServerError => FailureFacts {
                message: messages::FAILURE_SERVER_ERROR,
                label: "Server error",
                banner: None,
            },
            Self::MalformedResponse => FailureFacts {
                message: messages::FAILURE_MALFORMED_RESPONSE,
                label: "Unreadable response",
                banner: None,
            },
            Self::InvalidDonation => FailureFacts {
                message: messages::FAILURE_INVALID_DONATION,
                label: "Unreadable donation",
                banner: None,
            },
            Self::Local => FailureFacts {
                message: messages::FAILURE_LOCAL,
                label: "Local error",
                banner: None,
            },
        }
    }

    pub fn message(self) -> MessageRef {
        MessageRef::new(self.facts().message)
    }

    pub fn label(self) -> &'static str {
        self.facts().label
    }

    pub fn action_banner(self) -> Option<MessageCode> {
        self.facts().banner
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
