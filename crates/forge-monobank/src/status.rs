use forge_platform_core::{PollError, PollStatusBoard};
use forge_types::{MessageCode, MessageRef};

use crate::error::MonobankError;
use crate::jar::MonobankJar;
use crate::messages;

pub use forge_platform_core::PollerPhase;

pub type PollStatus = forge_platform_core::PollStatus<MonobankJar, PollFailure>;
pub(crate) type StatusBoard = PollStatusBoard<MonobankJar, PollFailure>;

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
            Self::JarNotSelected => FailureFacts {
                message: messages::FAILURE_JAR_NOT_SELECTED,
                label: "No jar selected",
                banner: Some(messages::LABEL_BANNER_JAR_NOT_SELECTED),
            },
            Self::JarMissing => FailureFacts {
                message: messages::FAILURE_JAR_MISSING,
                label: "Jar not found",
                banner: Some(messages::LABEL_BANNER_JAR_MISSING),
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
                PollerPhase::AwaitingToken,
                PollFailure::MissingToken,
            ),
            (
                MonobankError::MalformedToken,
                PollerPhase::ActionRequired,
                PollFailure::TokenUnreadable,
            ),
            (
                MonobankError::Unauthorized,
                PollerPhase::ActionRequired,
                PollFailure::TokenRejected,
            ),
            (
                MonobankError::MissingJar,
                PollerPhase::ActionRequired,
                PollFailure::JarNotSelected,
            ),
            (
                MonobankError::InvalidJarId,
                PollerPhase::ActionRequired,
                PollFailure::JarNotSelected,
            ),
            (
                MonobankError::JarNotFound,
                PollerPhase::ActionRequired,
                PollFailure::JarMissing,
            ),
            (
                MonobankError::RateLimited {
                    retry_after_secs: 60,
                },
                PollerPhase::Retrying,
                PollFailure::RateLimited,
            ),
            (
                MonobankError::Http { status: 503 },
                PollerPhase::Retrying,
                PollFailure::ServerError,
            ),
            (
                MonobankError::Network {
                    reason: String::new(),
                },
                PollerPhase::Retrying,
                PollFailure::Network,
            ),
            (
                MonobankError::MalformedResponse {
                    reason: String::new(),
                },
                PollerPhase::Retrying,
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
            (PollerPhase::Polling, Some(PollFailure::InvalidDonation))
        );
    }

    const ALL_FAILURES: [PollFailure; 11] = [
        PollFailure::MissingToken,
        PollFailure::TokenRejected,
        PollFailure::TokenUnreadable,
        PollFailure::JarNotSelected,
        PollFailure::JarMissing,
        PollFailure::RateLimited,
        PollFailure::Network,
        PollFailure::ServerError,
        PollFailure::MalformedResponse,
        PollFailure::InvalidDonation,
        PollFailure::Local,
    ];
    const BANNERED: [PollFailure; 5] = [
        PollFailure::MissingToken,
        PollFailure::TokenRejected,
        PollFailure::TokenUnreadable,
        PollFailure::JarNotSelected,
        PollFailure::JarMissing,
    ];

    fn distinct<T: Ord>(items: impl IntoIterator<Item = T>) -> usize {
        items
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }

    #[test]
    fn every_failure_has_its_own_message_and_label() {
        assert_eq!(
            distinct(ALL_FAILURES.map(|failure| failure.facts().message)),
            ALL_FAILURES.len()
        );
        assert_eq!(
            distinct(ALL_FAILURES.map(PollFailure::label)),
            ALL_FAILURES.len()
        );
        assert!(
            ALL_FAILURES
                .iter()
                .all(|failure| !failure.label().is_empty())
        );
    }

    #[test]
    fn only_failures_the_user_must_fix_carry_an_action_banner() {
        let with_banner: Vec<PollFailure> = ALL_FAILURES
            .into_iter()
            .filter(|failure| failure.facts().banner.is_some())
            .collect();

        assert_eq!(with_banner, BANNERED);
        assert_eq!(
            distinct(
                with_banner
                    .iter()
                    .filter_map(|failure| failure.facts().banner)
            ),
            BANNERED.len()
        );
    }
}
