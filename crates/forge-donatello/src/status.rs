use std::sync::Mutex;
use std::time::{Duration, Instant};

use forge_platform_core::{ConnectionState, HealthDelta};
use time::OffsetDateTime;
use tokio::sync::broadcast;

use crate::api::DonatelloAccount;
use crate::error::DonatelloError;

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
            DonatelloError::MalformedToken | DonatelloError::Unauthorized => Self::TokenRejected,
            DonatelloError::ProfileIncomplete => Self::ProfileIncomplete,
            DonatelloError::RateLimited { .. } | DonatelloError::RateLimitExhausted => {
                Self::RateLimited
            }
            DonatelloError::Http { .. } => Self::ServerError,
            DonatelloError::Network { .. } => Self::Network,
            DonatelloError::MalformedResponse { .. } => Self::MalformedResponse,
            DonatelloError::InvalidDonation { .. } => Self::InvalidDonation,
            DonatelloError::Credential { .. }
            | DonatelloError::ClientInit { .. }
            | DonatelloError::NoRuntime => Self::Local,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MissingToken => "No token",
            Self::TokenRejected => "Token rejected",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollStatus {
    pub phase: PollPhase,
    pub account: Option<DonatelloAccount>,
    pub poll_interval: Duration,
    pub last_poll_at: Option<OffsetDateTime>,
    pub last_donation_at: Option<OffsetDateTime>,
    pub last_error: Option<PollFailure>,
    pub last_error_at: Option<OffsetDateTime>,
    pub(crate) polling_since: Option<Instant>,
}

impl PollStatus {
    fn initial(poll_interval: Duration) -> Self {
        Self {
            phase: PollPhase::Paused,
            account: None,
            poll_interval,
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
    pub(crate) fn new(poll_interval: Duration) -> Self {
        let (health_tx, _) = broadcast::channel(HEALTH_CHANNEL_CAPACITY);
        Self {
            status: Mutex::new(PollStatus::initial(poll_interval)),
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

    pub(crate) fn set_poll_interval(&self, poll_interval: Duration) {
        self.update(|status| status.poll_interval = poll_interval);
    }

    pub(crate) fn record_account(&self, account: DonatelloAccount) {
        self.update(|status| status.account = Some(account));
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

    pub(crate) fn record_failure(&self, error: &DonatelloError) {
        let phase = if error.requires_user_action() {
            if matches!(error, DonatelloError::MissingToken) {
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

    pub(crate) fn record_rejected_donation(&self, error: &DonatelloError) {
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
