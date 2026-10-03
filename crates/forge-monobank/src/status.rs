use std::sync::Mutex;
use std::time::{Duration, Instant};

use forge_platform_core::{ConnectionState, HealthDelta};
use time::OffsetDateTime;
use tokio::sync::broadcast;

use crate::error::MonobankError;
use crate::jar::MonobankJar;

const HEALTH_CHANNEL_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollPhase {
    Paused,
    AwaitingToken,
    Starting,
    Polling,
    Retrying,
    ActionRequired,
}

impl PollPhase {
    pub const fn connection(self) -> ConnectionState {
        match self {
            Self::Paused | Self::AwaitingToken | Self::ActionRequired => {
                ConnectionState::Disconnected
            }
            Self::Starting => ConnectionState::Connecting,
            Self::Polling => ConnectionState::Connected,
            Self::Retrying => ConnectionState::Reconnecting,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollFailure {
    MissingToken,
    TokenRejected,
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
            MonobankError::MalformedToken | MonobankError::Unauthorized => Self::TokenRejected,
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

    pub const fn label(self) -> &'static str {
        match self {
            Self::MissingToken => "No token",
            Self::TokenRejected => "Token rejected",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollStatus {
    pub phase: PollPhase,
    pub jar: Option<MonobankJar>,
    pub last_poll_at: Option<OffsetDateTime>,
    pub last_donation_at: Option<OffsetDateTime>,
    pub last_error: Option<PollFailure>,
    pub last_error_at: Option<OffsetDateTime>,
    pub(crate) polling_since: Option<Instant>,
}

impl PollStatus {
    fn initial() -> Self {
        Self {
            phase: PollPhase::Paused,
            jar: None,
            last_poll_at: None,
            last_donation_at: None,
            last_error: None,
            last_error_at: None,
            polling_since: None,
        }
    }

    pub const fn phase_label(&self) -> &'static str {
        match self.phase {
            PollPhase::Paused => "Paused",
            PollPhase::AwaitingToken => "Waiting for a token",
            PollPhase::Starting => "Starting",
            PollPhase::Polling => "Polling",
            PollPhase::Retrying => "Retrying",
            PollPhase::ActionRequired => "Action required",
        }
    }

    pub fn uptime(&self) -> Option<Duration> {
        self.polling_since.map(|since| since.elapsed())
    }

    fn enter(&mut self, phase: PollPhase) {
        match phase {
            PollPhase::Polling => {
                self.polling_since.get_or_insert_with(Instant::now);
            }
            PollPhase::Retrying | PollPhase::Starting => {}
            PollPhase::Paused | PollPhase::AwaitingToken | PollPhase::ActionRequired => {
                self.polling_since = None;
            }
        }
        self.phase = phase;
    }
}

pub(crate) struct StatusBoard {
    status: Mutex<PollStatus>,
    health_tx: broadcast::Sender<HealthDelta>,
}

impl StatusBoard {
    pub(crate) fn new() -> Self {
        let (health_tx, _) = broadcast::channel(HEALTH_CHANNEL_CAPACITY);
        Self {
            status: Mutex::new(PollStatus::initial()),
            health_tx,
        }
    }

    pub(crate) fn snapshot(&self) -> PollStatus {
        self.status
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub(crate) fn subscribe_health(&self) -> broadcast::Receiver<HealthDelta> {
        self.health_tx.subscribe()
    }

    pub(crate) fn enter(&self, phase: PollPhase) {
        self.update(|status| status.enter(phase));
    }

    pub(crate) fn record_jar(&self, jar: MonobankJar) {
        self.update(|status| status.jar = Some(jar));
    }

    pub(crate) fn record_poll(&self, newest_donation: Option<OffsetDateTime>) {
        self.update(|status| {
            status.enter(PollPhase::Polling);
            status.last_poll_at = Some(OffsetDateTime::now_utc());
            if let Some(occurred_at) = newest_donation {
                status.last_donation_at = Some(
                    status
                        .last_donation_at
                        .map_or(occurred_at, |previous| previous.max(occurred_at)),
                );
            }
        });
    }

    pub(crate) fn record_failure(&self, error: &MonobankError) {
        let phase = if error.requires_user_action() {
            if matches!(error, MonobankError::MissingToken) {
                PollPhase::AwaitingToken
            } else {
                PollPhase::ActionRequired
            }
        } else {
            PollPhase::Retrying
        };
        self.update(|status| {
            status.enter(phase);
            status.last_error = Some(PollFailure::of(error));
            status.last_error_at = Some(OffsetDateTime::now_utc());
        });
    }

    pub(crate) fn record_rejected_donation(&self, error: &MonobankError) {
        self.update(|status| {
            status.last_error = Some(PollFailure::of(error));
            status.last_error_at = Some(OffsetDateTime::now_utc());
        });
    }

    fn update(&self, change: impl FnOnce(&mut PollStatus)) {
        let snapshot = {
            let mut status = self
                .status
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let before = status.clone();
            change(&mut status);
            if *status == before {
                return;
            }
            status.clone()
        };
        for delta in crate::health::deltas(&snapshot) {
            let _ = self.health_tx.send(delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PollFailure, PollPhase, StatusBoard};
    use crate::error::MonobankError;

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
                PollFailure::TokenRejected,
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
            let board = StatusBoard::new();
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
        let board = StatusBoard::new();
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
        let board = StatusBoard::new();
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
