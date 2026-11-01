use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use forge_events::EventPublisher;
use forge_types::{MessageCode, MessageRef, RecoverMutex};
use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::builtin::{HEALTH_CHANNEL_CAPACITY, HealthDelta, HealthMetric, HealthStream};
use crate::chat::{ConnectionState, ConnectionTransition};
use crate::connection_announcer::ConnectionAnnouncer;

#[derive(Debug, Clone)]
pub struct PollPhaseCodes {
    pub paused: MessageCode,
    pub awaiting_token: MessageCode,
    pub starting: MessageCode,
    pub polling: MessageCode,
    pub retrying: MessageCode,
    pub action_required: MessageCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollerPhase {
    Paused,
    AwaitingToken,
    Starting,
    Polling,
    Retrying,
    ActionRequired,
}

impl PollerPhase {
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

    pub fn message(self, codes: &PollPhaseCodes) -> MessageRef {
        MessageRef::new(match self {
            Self::Paused => codes.paused.clone(),
            Self::AwaitingToken => codes.awaiting_token.clone(),
            Self::Starting => codes.starting.clone(),
            Self::Polling => codes.polling.clone(),
            Self::Retrying => codes.retrying.clone(),
            Self::ActionRequired => codes.action_required.clone(),
        })
    }
}

pub trait PollError {
    type Failure;

    fn failure(&self) -> Self::Failure;
    fn failure_label(&self) -> &'static str;
    fn needs_user_action(&self) -> bool;
    fn is_missing_token(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollStatus<S, F> {
    pub phase: PollerPhase,
    pub subject: Option<S>,
    pub poll_interval: Duration,
    pub last_poll_at: Option<OffsetDateTime>,
    pub last_donation_at: Option<OffsetDateTime>,
    pub last_error: Option<F>,
    pub last_error_at: Option<OffsetDateTime>,
    polling_since: Option<Instant>,
}

impl<S, F> PollStatus<S, F> {
    pub const fn initial(poll_interval: Duration) -> Self {
        Self {
            phase: PollerPhase::Paused,
            subject: None,
            poll_interval,
            last_poll_at: None,
            last_donation_at: None,
            last_error: None,
            last_error_at: None,
            polling_since: None,
        }
    }

    pub fn uptime(&self) -> Option<Duration> {
        self.polling_since.map(|since| since.elapsed())
    }

    fn enter(&mut self, phase: PollerPhase) {
        match phase {
            PollerPhase::Polling => {
                self.polling_since.get_or_insert_with(Instant::now);
            }
            PollerPhase::Retrying | PollerPhase::Starting => {}
            PollerPhase::Paused | PollerPhase::AwaitingToken | PollerPhase::ActionRequired => {
                self.polling_since = None;
            }
        }
        self.phase = phase;
    }
}

pub type PollMetrics<S, F> = fn(&PollStatus<S, F>) -> [HealthMetric; 4];

pub struct PollStatusBoard<S: Clone + PartialEq, F: Copy + PartialEq> {
    status: Mutex<PollStatus<S, F>>,
    health_tx: broadcast::Sender<HealthDelta>,
    link: ConnectionAnnouncer,
    running_pollers: AtomicUsize,
    metrics: PollMetrics<S, F>,
}

impl<S: Clone + PartialEq, F: Copy + PartialEq> PollStatusBoard<S, F> {
    pub fn new(platform_id: &str, poll_interval: Duration, metrics: PollMetrics<S, F>) -> Self {
        let (health_tx, _) = broadcast::channel(HEALTH_CHANNEL_CAPACITY);
        Self {
            status: Mutex::new(PollStatus::initial(poll_interval)),
            health_tx,
            link: ConnectionAnnouncer::new(platform_id),
            running_pollers: AtomicUsize::new(0),
            metrics,
        }
    }

    pub fn attach_publisher(&self, publisher: Arc<dyn EventPublisher>) {
        self.link.attach(publisher);
    }

    pub fn poller_started(self: &Arc<Self>) -> RunningPoller<S, F> {
        self.running_pollers.fetch_add(1, Ordering::SeqCst);
        RunningPoller(Arc::clone(self))
    }

    pub fn snapshot(&self) -> PollStatus<S, F> {
        self.status.lock_recover().clone()
    }

    pub fn current_metrics(&self) -> [HealthMetric; 4] {
        (self.metrics)(&self.snapshot())
    }

    pub fn health_stream(&self) -> HealthStream {
        Box::pin(BroadcastStream::new(self.health_tx.subscribe()).filter_map(Result::ok))
    }

    pub fn enter(&self, phase: PollerPhase) {
        self.update_expected(|status| status.enter(phase));
    }

    pub fn set_poll_interval(&self, poll_interval: Duration) {
        self.update_expected(|status| status.poll_interval = poll_interval);
    }

    pub fn record_subject(&self, subject: S) {
        self.update_expected(|status| status.subject = Some(subject));
    }

    pub fn record_poll(&self, newest_donation: Option<OffsetDateTime>) {
        self.update_expected(|status| {
            status.enter(PollerPhase::Polling);
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

    pub fn record_failure<E: PollError<Failure = F>>(&self, error: &E) {
        let phase = if !error.needs_user_action() {
            PollerPhase::Retrying
        } else if error.is_missing_token() {
            PollerPhase::AwaitingToken
        } else {
            PollerPhase::ActionRequired
        };
        self.update(ConnectionTransition::Unexpected, |status| {
            status.enter(phase);
            status.last_error = Some(error.failure());
            status.last_error_at = Some(OffsetDateTime::now_utc());
        });
    }

    pub fn record_rejected_donation<E: PollError<Failure = F>>(&self, error: &E) {
        self.update_expected(|status| {
            status.last_error = Some(error.failure());
            status.last_error_at = Some(OffsetDateTime::now_utc());
        });
    }

    fn update_expected(&self, change: impl FnOnce(&mut PollStatus<S, F>)) {
        self.update(ConnectionTransition::Expected, change);
    }

    fn update(&self, transition: ConnectionTransition, change: impl FnOnce(&mut PollStatus<S, F>)) {
        let (snapshot, changed) = {
            let mut status = self.status.lock_recover();
            let before = status.clone();
            change(&mut status);
            (status.clone(), *status != before)
        };
        if changed {
            for metric in (self.metrics)(&snapshot) {
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = self.health_tx.send(HealthDelta {
                    metric: metric.id,
                    new_value: metric.value,
                });
            }
        }
        self.link.announce(snapshot.phase.connection(), transition);
    }
}

pub struct RunningPoller<S: Clone + PartialEq, F: Copy + PartialEq>(Arc<PollStatusBoard<S, F>>);

impl<S: Clone + PartialEq, F: Copy + PartialEq> Drop for RunningPoller<S, F> {
    fn drop(&mut self) {
        if self.0.running_pollers.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.update_expected(|status| {
                if status.phase.connection() != ConnectionState::Disconnected {
                    status.enter(PollerPhase::Paused);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    use crate::builtin::{HealthMetricId, HealthText, HealthValue};
    use forge_events::test_support::CapturingPublisher;

    const POLL_INTERVAL: Duration = Duration::from_secs(30);

    type Board = PollStatusBoard<&'static str, Failure>;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Failure {
        MissingToken,
        Rejected,
        Network,
    }

    impl PollError for Failure {
        type Failure = Self;

        fn failure(&self) -> Self {
            *self
        }

        fn failure_label(&self) -> &'static str {
            "test failure"
        }

        fn needs_user_action(&self) -> bool {
            !matches!(self, Self::Network)
        }

        fn is_missing_token(&self) -> bool {
            matches!(self, Self::MissingToken)
        }
    }

    fn fixed_metrics(_: &PollStatus<&'static str, Failure>) -> [HealthMetric; 4] {
        std::array::from_fn(|_| HealthMetric {
            id: HealthMetricId::VIEWERS,
            value: HealthValue::Text {
                primary: HealthText::Span(POLL_INTERVAL),
                secondary: None,
            },
        })
    }

    fn board() -> Board {
        PollStatusBoard::new("test", POLL_INTERVAL, fixed_metrics)
    }

    fn announcing_board() -> (Arc<Board>, Arc<CapturingPublisher>) {
        let recorder = Arc::new(CapturingPublisher::default());
        let board = Arc::new(board());
        board.attach_publisher(Arc::clone(&recorder) as Arc<dyn EventPublisher>);
        (board, recorder)
    }

    fn announcements(recorder: &CapturingPublisher) -> Vec<(ConnectionState, bool)> {
        recorder
            .events()
            .iter()
            .map(|event| {
                (
                    serde_json::from_value(event.payload["state"].clone()).unwrap(),
                    event.payload["expected"].as_bool().unwrap(),
                )
            })
            .collect()
    }

    fn last(recorder: &CapturingPublisher) -> Option<(ConnectionState, bool)> {
        announcements(recorder).last().copied()
    }

    #[test]
    fn poll_failures_after_polling_announce_an_unexpected_loss() {
        for (error, state) in [
            (Failure::Network, ConnectionState::Reconnecting),
            (Failure::Rejected, ConnectionState::Disconnected),
            (Failure::MissingToken, ConnectionState::Disconnected),
        ] {
            let (board, recorder) = announcing_board();
            board.record_poll(None);
            board.record_failure(&error);
            assert_eq!(last(&recorder), Some((state, false)), "{error:?}");
        }
    }

    #[test]
    fn failures_pick_the_phase_from_what_they_need_from_the_user() {
        for (error, phase) in [
            (Failure::Network, PollerPhase::Retrying),
            (Failure::Rejected, PollerPhase::ActionRequired),
            (Failure::MissingToken, PollerPhase::AwaitingToken),
        ] {
            let board = board();
            board.record_failure(&error);

            let status = board.snapshot();

            assert_eq!((status.phase, status.last_error), (phase, Some(error)));
        }
    }

    #[test]
    fn polling_pausing_and_restarting_announce_expected_transitions() {
        let (board, recorder) = announcing_board();
        board.record_poll(None);
        board.enter(PollerPhase::Paused);
        board.enter(PollerPhase::Starting);
        assert_eq!(
            announcements(&recorder),
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
        board.record_failure(&Failure::Network);
        board.record_poll(None);
        assert_eq!(last(&recorder), Some((ConnectionState::Connected, true)));
    }

    #[test]
    fn repeated_failures_announce_the_loss_once() {
        let (board, recorder) = announcing_board();
        board.record_poll(None);
        board.record_failure(&Failure::Network);
        board.record_failure(&Failure::Network);
        assert_eq!(
            announcements(&recorder),
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
        board.record_rejected_donation(&Failure::Network);
        assert_eq!(
            announcements(&recorder),
            [(ConnectionState::Connected, true)]
        );
        assert_eq!(board.snapshot().phase, PollerPhase::Polling);
    }

    #[test]
    fn last_poller_exit_announces_an_expected_disconnect() {
        let (board, recorder) = announcing_board();
        let running = board.poller_started();
        board.record_failure(&Failure::Network);
        drop(running);
        assert_eq!(last(&recorder), Some((ConnectionState::Disconnected, true)));
    }

    #[test]
    fn retired_poller_exiting_while_a_newer_one_runs_announces_nothing() {
        let (board, recorder) = announcing_board();
        let retired = board.poller_started();
        board.record_poll(None);
        let _current = board.poller_started();
        drop(retired);
        assert_eq!(
            announcements(&recorder),
            [(ConnectionState::Connected, true)]
        );
    }

    #[test]
    fn restarted_poller_does_not_announce_connected_before_it_polls() {
        let (board, recorder) = announcing_board();
        let first = board.poller_started();
        board.record_poll(None);
        drop(first);
        let _restarted = board.poller_started();
        board.set_poll_interval(POLL_INTERVAL);
        assert_eq!(last(&recorder), Some((ConnectionState::Disconnected, true)));
    }

    fn phase_after_last_poller_exit(reach: impl FnOnce(&Board)) -> PollerPhase {
        let (board, _recorder) = announcing_board();
        let running = board.poller_started();
        reach(&board);
        drop(running);
        board.snapshot().phase
    }

    #[test]
    fn last_poller_exit_parks_an_active_phase_as_paused() {
        let reach_starting = |board: &Board| board.enter(PollerPhase::Starting);
        let reach_polling = |board: &Board| board.record_poll(None);
        let reach_retrying = |board: &Board| board.record_failure(&Failure::Network);
        assert_eq!(
            [
                phase_after_last_poller_exit(reach_starting),
                phase_after_last_poller_exit(reach_polling),
                phase_after_last_poller_exit(reach_retrying),
            ],
            [PollerPhase::Paused; 3]
        );
    }

    #[test]
    fn last_poller_exit_keeps_a_phase_that_waits_for_the_user() {
        let reach_awaiting = |board: &Board| board.record_failure(&Failure::MissingToken);
        let reach_action = |board: &Board| board.record_failure(&Failure::Rejected);
        assert_eq!(
            [
                phase_after_last_poller_exit(reach_awaiting),
                phase_after_last_poller_exit(reach_action),
            ],
            [PollerPhase::AwaitingToken, PollerPhase::ActionRequired]
        );
    }
}
