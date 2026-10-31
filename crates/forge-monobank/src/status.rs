use forge_platform_core::{PollError, PollStatusBoard};
use forge_types::MessageRef;

use crate::error::MonobankError;
use crate::jar::MonobankJar;
use crate::messages;

pub use forge_platform_core::PollPhase;

pub type PollStatus = forge_platform_core::PollStatus<MonobankJar, PollFailure>;
pub(crate) type StatusBoard = PollStatusBoard<MonobankJar, PollFailure>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollFailure {
    MissingToken,
    TokenRejected,
    TokenUnreadable,
    JarNotSelected,
    JarMissing,
    RateLimited,
    Network,
    ServerError,
    MalformedResponse,
    InvalidDonation,
    Local,
}

impl PollFailure {
    pub fn of(error: &MonobankError) -> Self {
        match error {
            MonobankError::MissingToken => Self::MissingToken,
            MonobankError::MalformedToken => Self::TokenUnreadable,
            MonobankError::Unauthorized => Self::TokenRejected,
            MonobankError::MissingJar | MonobankError::InvalidJarId => Self::JarNotSelected,
            MonobankError::JarNotFound => Self::JarMissing,
            MonobankError::CoolingDown { .. } | MonobankError::RateLimited { .. } => {
                Self::RateLimited
            }
            MonobankError::Http { .. } => Self::ServerError,
            MonobankError::Network { .. } => Self::Network,
            MonobankError::MalformedResponse { .. } => Self::MalformedResponse,
            MonobankError::InvalidDonation { .. } => Self::InvalidDonation,
            MonobankError::Credential { .. }
            | MonobankError::ClientInit { .. }
            | MonobankError::NoRuntime => Self::Local,
        }
    }

    pub fn message(self) -> MessageRef {
        MessageRef::new(match self {
            Self::MissingToken => messages::FAILURE_MISSING_TOKEN,
            Self::TokenRejected => messages::FAILURE_TOKEN_REJECTED,
            Self::TokenUnreadable => messages::FAILURE_TOKEN_UNREADABLE,
            Self::JarNotSelected => messages::FAILURE_JAR_NOT_SELECTED,
            Self::JarMissing => messages::FAILURE_JAR_MISSING,
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
            Self::JarNotSelected => "No jar selected",
            Self::JarMissing => "Jar not found",
            Self::RateLimited => "Rate limited",
            Self::Network => "Network error",
            Self::ServerError => "Server error",
            Self::MalformedResponse => "Unreadable response",
            Self::InvalidDonation => "Unreadable donation",
            Self::Local => "Local error",
        }
    }
}

impl PollError for MonobankError {
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
    fn failures_needing_the_user_stop_while_transient_ones_retry() {
        for (error, phase, failure) in [
            (
                MonobankError::MissingToken,
                PollPhase::AwaitingToken,
                PollFailure::MissingToken,
            ),
            (
                MonobankError::MalformedToken,
                PollPhase::ActionRequired,
                PollFailure::TokenUnreadable,
            ),
            (
                MonobankError::Unauthorized,
                PollPhase::ActionRequired,
                PollFailure::TokenRejected,
            ),
            (
                MonobankError::MissingJar,
                PollPhase::ActionRequired,
                PollFailure::JarNotSelected,
            ),
            (
                MonobankError::InvalidJarId,
                PollPhase::ActionRequired,
                PollFailure::JarNotSelected,
            ),
            (
                MonobankError::JarNotFound,
                PollPhase::ActionRequired,
                PollFailure::JarMissing,
            ),
            (
                MonobankError::RateLimited {
                    retry_after_secs: 60,
                },
                PollPhase::Retrying,
                PollFailure::RateLimited,
            ),
            (
                MonobankError::Http { status: 503 },
                PollPhase::Retrying,
                PollFailure::ServerError,
            ),
            (
                MonobankError::Network {
                    reason: String::new(),
                },
                PollPhase::Retrying,
                PollFailure::Network,
            ),
            (
                MonobankError::MalformedResponse {
                    reason: String::new(),
                },
                PollPhase::Retrying,
                PollFailure::MalformedResponse,
            ),
        ] {
            let board = board();
            board.record_failure(&error);
            let status = board.snapshot();
            assert_eq!(
                (status.phase, status.last_error),
                (phase, Some(failure)),
                "{error:?}"
            );
        }
    }

    #[test]
    fn uptime_survives_a_retry_but_resets_when_the_user_must_act() {
        let board = board();
        board.record_poll(None);
        board.record_failure(&MonobankError::Http { status: 503 });
        let retrying = board.snapshot().uptime();
        board.record_failure(&MonobankError::Unauthorized);
        let stopped = board.snapshot().uptime();

        assert!(retrying.is_some());
        assert_eq!(stopped, None);
    }

    #[test]
    fn rejected_donation_records_the_failure_without_leaving_polling() {
        let board = board();
        board.record_poll(None);
        board.record_rejected_donation(&MonobankError::InvalidDonation {
            donation_id: "TX-1".to_owned(),
            reason: String::new(),
        });
        let status = board.snapshot();

        assert_eq!(
            (status.phase, status.last_error),
            (PollPhase::Polling, Some(PollFailure::InvalidDonation))
        );
    }
}
