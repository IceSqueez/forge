use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_events::EventPublisher;
use forge_platform_core::{
    ConnectionAnnouncer, ConnectionState, ConnectionTransition, HealthDelta,
};
use time::OffsetDateTime;
use tokio::sync::broadcast;

use crate::error::MonobankError;
use crate::integration::MONOBANK_INTEGRATION;
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
    link: ConnectionAnnouncer,
    running_pollers: AtomicUsize,
}

impl StatusBoard {
    pub(crate) fn new() -> Self {
        let (health_tx, _) = broadcast::channel(HEALTH_CHANNEL_CAPACITY);
        Self {
            status: Mutex::new(PollStatus::initial()),
            health_tx,
            link: ConnectionAnnouncer::new(MONOBANK_INTEGRATION.id.as_str()),
            running_pollers: AtomicUsize::new(0),
        }
    }

    pub(crate) fn attach_publisher(&self, publisher: Arc<dyn EventPublisher>) {
        self.link.attach(publisher);
    }

    pub(crate) fn poller_started(self: &Arc<Self>) -> RunningPoller {
        self.running_pollers.fetch_add(1, Ordering::SeqCst);
        RunningPoller(Arc::clone(self))
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
        self.update_expected(|status| status.enter(phase));
    }

    pub(crate) fn record_jar(&self, jar: MonobankJar) {
        self.update_expected(|status| status.jar = Some(jar));
    }

    pub(crate) fn record_poll(&self, newest_donation: Option<OffsetDateTime>) {
        self.update_expected(|status| {
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
        self.update(ConnectionTransition::Unexpected, |status| {
            status.enter(phase);
            status.last_error = Some(PollFailure::of(error));
            status.last_error_at = Some(OffsetDateTime::now_utc());
        });
    }

    pub(crate) fn record_rejected_donation(&self, error: &MonobankError) {
        self.update_expected(|status| {
            status.last_error = Some(PollFailure::of(error));
            status.last_error_at = Some(OffsetDateTime::now_utc());
        });
    }

    fn update_expected(&self, change: impl FnOnce(&mut PollStatus)) {
        self.update(ConnectionTransition::Expected, change);
    }

    fn update(&self, transition: ConnectionTransition, change: impl FnOnce(&mut PollStatus)) {
        let (snapshot, changed) = {
            let mut status = self
                .status
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let before = status.clone();
            change(&mut status);
            (status.clone(), *status != before)
        };
        if changed {
            for delta in crate::health::deltas(&snapshot) {
                let _ = self.health_tx.send(delta);
            }
        }
        self.link.announce(snapshot.phase.connection(), transition);
    }
}

pub(crate) struct RunningPoller(Arc<StatusBoard>);

impl Drop for RunningPoller {
    fn drop(&mut self) {
        if self.0.running_pollers.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.update_expected(|status| {
                if status.phase.connection() != ConnectionState::Disconnected {
                    status.enter(PollPhase::Paused);
                }
            });
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::Arc;

    use forge_events::EventPublisher;
    use forge_platform_core::ConnectionState;

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

    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<forge_events::Event>>);

    impl EventPublisher for Recorder {
        fn publish(&self, event: forge_events::Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    impl Recorder {
        fn announcements(&self) -> Vec<(ConnectionState, bool)> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|event| {
                    (
                        serde_json::from_value(event.payload["state"].clone()).unwrap(),
                        event.payload["expected"].as_bool().unwrap(),
                    )
                })
                .collect()
        }

        fn last(&self) -> Option<(ConnectionState, bool)> {
            self.announcements().last().copied()
        }
    }

    fn announcing_board() -> (Arc<StatusBoard>, Arc<Recorder>) {
        let recorder = Arc::new(Recorder::default());
        let board = Arc::new(StatusBoard::new());
        board.attach_publisher(Arc::clone(&recorder) as Arc<dyn EventPublisher>);
        (board, recorder)
    }

    fn network_error() -> MonobankError {
        MonobankError::Network {
            reason: String::new(),
        }
    }

    #[test]
    fn poll_failures_after_polling_announce_an_unexpected_loss() {
        for (error, state) in [
            (network_error(), ConnectionState::Reconnecting),
            (MonobankError::Unauthorized, ConnectionState::Disconnected),
            (MonobankError::MissingToken, ConnectionState::Disconnected),
        ] {
            let (board, recorder) = announcing_board();
            board.record_poll(None);
            board.record_failure(&error);
            assert_eq!(recorder.last(), Some((state, false)), "{error:?}");
        }
    }

    #[test]
    fn polling_pausing_and_restarting_announce_expected_transitions() {
        let (board, recorder) = announcing_board();
        board.record_poll(None);
        board.enter(PollPhase::Paused);
        board.enter(PollPhase::Starting);
        assert_eq!(
            recorder.announcements(),
            [
                (ConnectionState::Connected, true),
                (ConnectionState::Disconnected, true),
                (ConnectionState::Connecting, true),
            ]
        );
    }

    #[test]
    fn recovering_poll_after_a_failure_announces_connected() {
        let (board, recorder) = announcing_board();
        board.record_poll(None);
        board.record_failure(&network_error());
        board.record_poll(None);
        assert_eq!(recorder.last(), Some((ConnectionState::Connected, true)));
    }

    #[test]
    fn repeated_failures_announce_the_loss_once() {
        let (board, recorder) = announcing_board();
        board.record_poll(None);
        board.record_failure(&network_error());
        board.record_failure(&network_error());
        assert_eq!(
            recorder.announcements(),
            [
                (ConnectionState::Connected, true),
                (ConnectionState::Reconnecting, false),
            ]
        );
    }

    #[test]
    fn rejected_donation_while_polling_announces_nothing_new() {
        let (board, recorder) = announcing_board();
        board.record_poll(None);
        board.record_rejected_donation(&MonobankError::InvalidDonation {
            donation_id: "TX-1".to_owned(),
            reason: String::new(),
        });
        assert_eq!(
            recorder.announcements(),
            [(ConnectionState::Connected, true)]
        );
    }

    #[test]
    fn last_poller_exit_announces_an_expected_disconnect() {
        let (board, recorder) = announcing_board();
        let running = board.poller_started();
        board.record_failure(&network_error());
        drop(running);
        assert_eq!(recorder.last(), Some((ConnectionState::Disconnected, true)));
    }

    #[test]
    fn retired_poller_exiting_while_a_newer_one_runs_announces_nothing() {
        let (board, recorder) = announcing_board();
        let retired = board.poller_started();
        board.record_poll(None);
        let _current = board.poller_started();
        drop(retired);
        assert_eq!(
            recorder.announcements(),
            [(ConnectionState::Connected, true)]
        );
    }

    fn phase_after_last_poller_exit(reach: impl FnOnce(&StatusBoard)) -> PollPhase {
        let (board, _recorder) = announcing_board();
        let running = board.poller_started();
        reach(&board);
        drop(running);
        board.snapshot().phase
    }

    #[test]
    fn last_poller_exit_parks_an_active_phase_as_paused() {
        let reach_starting = |board: &StatusBoard| board.enter(PollPhase::Starting);
        let reach_polling = |board: &StatusBoard| board.record_poll(None);
        let reach_retrying = |board: &StatusBoard| board.record_failure(&network_error());
        assert_eq!(
            [
                phase_after_last_poller_exit(reach_starting),
                phase_after_last_poller_exit(reach_polling),
                phase_after_last_poller_exit(reach_retrying),
            ],
            [PollPhase::Paused; 3]
        );
    }

    #[test]
    fn last_poller_exit_keeps_a_phase_that_waits_for_the_user() {
        let reach_awaiting =
            |board: &StatusBoard| board.record_failure(&MonobankError::MissingToken);
        let reach_action = |board: &StatusBoard| board.record_failure(&MonobankError::Unauthorized);
        assert_eq!(
            [
                phase_after_last_poller_exit(reach_awaiting),
                phase_after_last_poller_exit(reach_action),
            ],
            [PollPhase::AwaitingToken, PollPhase::ActionRequired]
        );
    }
}
