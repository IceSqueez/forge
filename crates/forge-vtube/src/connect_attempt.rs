use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{oneshot, watch};

use forge_events::{Event, EventPublisher, TypedEvent};
use forge_storage::CredentialsRepo;

use crate::client::{VTubeClient, VTubeConfig};
use crate::supervisor::{HANDSHAKE_REPLY_TIMEOUT, TOKEN_APPROVAL_TIMEOUT};
use crate::vocabulary::{VTubeConnectionChanged, VTubeDisconnectReason};

const DIAL_ALLOWANCE: Duration = Duration::from_secs(10);
const ATTEMPT_DEADLINE: Duration = DIAL_ALLOWANCE
    .saturating_add(TOKEN_APPROVAL_TIMEOUT)
    .saturating_add(HANDSHAKE_REPLY_TIMEOUT)
    .saturating_add(HANDSHAKE_REPLY_TIMEOUT);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VTubeAttemptPhase {
    Connecting,
    AwaitingApproval,
    Connected,
    Failed(VTubeDisconnectReason),
    TimedOut,
}

impl VTubeAttemptPhase {
    fn is_settled(self) -> bool {
        !matches!(self, Self::Connecting | Self::AwaitingApproval)
    }

    fn of(change: &VTubeConnectionChanged) -> Option<Self> {
        if change.is_connected {
            return Some(Self::Connected);
        }
        change.reason.map(|reason| match reason {
            VTubeDisconnectReason::AwaitingApproval => Self::AwaitingApproval,
            reason => Self::Failed(reason),
        })
    }
}

struct AttemptPhaseFeed {
    downstream: Arc<dyn EventPublisher>,
    phases: watch::Sender<VTubeAttemptPhase>,
}

impl EventPublisher for AttemptPhaseFeed {
    fn publish(&self, event: Event) {
        if let Some(next) = VTubeConnectionChanged::from_event(&event)
            .as_ref()
            .and_then(VTubeAttemptPhase::of)
        {
            self.phases.send_if_modified(|phase| {
                let accepted = *phase != VTubeAttemptPhase::TimedOut;
                if accepted {
                    *phase = next;
                }
                accepted
            });
        }
        self.downstream.publish(event);
    }
}

pub struct VTubeConnectAttempt {
    client: Arc<VTubeClient>,
    phases: watch::Receiver<VTubeAttemptPhase>,
    claim: oneshot::Sender<()>,
}

impl VTubeConnectAttempt {
    pub fn begin(
        cfg: VTubeConfig,
        publisher: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Self {
        let (phase_tx, mut phases) = watch::channel(VTubeAttemptPhase::Connecting);
        phases.mark_changed();
        let expiry = phase_tx.clone();
        let feed = AttemptPhaseFeed {
            downstream: publisher,
            phases: phase_tx,
        };
        let client = Arc::new(VTubeClient::connect_with_auto_reconnect(
            cfg,
            Arc::new(feed),
            creds,
            false,
        ));
        let (claim, claimed) = oneshot::channel();
        tokio::spawn(guard_attempt(Arc::clone(&client), expiry, claimed));
        Self {
            client,
            phases,
            claim,
        }
    }

    pub async fn next_phase(&mut self) -> VTubeAttemptPhase {
        match self.phases.changed().await {
            Ok(()) => *self.phases.borrow_and_update(),
            Err(_) => VTubeAttemptPhase::Failed(VTubeDisconnectReason::SocketClosed),
        }
    }

    pub fn into_client(self) -> Arc<VTubeClient> {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the guard ends on its own once the attempt has expired"
        )]
        let _ = self.claim.send(());
        self.client
    }
}

async fn guard_attempt(
    client: Arc<VTubeClient>,
    phases: watch::Sender<VTubeAttemptPhase>,
    mut claimed: oneshot::Receiver<()>,
) {
    tokio::select! {
        claim = &mut claimed => {
            if claim.is_ok() {
                return;
            }
        }
        () = tokio::time::sleep(ATTEMPT_DEADLINE) => {
            let expired = phases.send_if_modified(|phase| {
                let unsettled = !phase.is_settled();
                if unsettled {
                    *phase = VTubeAttemptPhase::TimedOut;
                }
                unsettled
            });
            if !expired && claimed.await.is_ok() {
                return;
            }
        }
    }
    client.shutdown().await;
}

#[cfg(test)]
mod tests {
    use forge_events::EventSource;
    use forge_platform_core::ConnectionState;
    use tokio::task::JoinHandle;

    use super::*;
    use crate::client::tests::{
        FakeVts, MockCreds, MockPublisher, POLL_STEP, stored_token_creds, wait_for,
    };

    const HANG_GUARD: Duration = Duration::from_secs(30);
    const REFUSED_ENDPOINT: &str = "ws://127.0.0.1:0";

    fn change(is_connected: bool, reason: Option<VTubeDisconnectReason>) -> Event {
        VTubeConnectionChanged {
            is_connected,
            endpoint: "ws://localhost:8001".to_owned(),
            reason,
            detail: None,
        }
        .into_event(EventSource::VTube, None)
    }

    fn feed_at(
        phase: VTubeAttemptPhase,
    ) -> (
        AttemptPhaseFeed,
        watch::Receiver<VTubeAttemptPhase>,
        Arc<MockPublisher>,
    ) {
        let (phases, receiver) = watch::channel(phase);
        let bus = MockPublisher::new();
        let feed = AttemptPhaseFeed {
            downstream: bus.publisher(),
            phases,
        };
        (feed, receiver, bus)
    }

    async fn supervised_client() -> Arc<VTubeClient> {
        let client = Arc::new(VTubeClient::new_for_test("ws://forge.invalid"));
        let stop = client.shutdown.lock().await.clone();
        let supervisor = tokio::spawn(async move { stop.notified().await });
        *client.supervisor.lock().unwrap() = Some(supervisor);
        client
    }

    fn is_torn_down(client: &VTubeClient) -> bool {
        client.supervisor.lock().unwrap().is_none()
    }

    fn guarded(
        client: &Arc<VTubeClient>,
        phase: VTubeAttemptPhase,
    ) -> (
        JoinHandle<()>,
        watch::Receiver<VTubeAttemptPhase>,
        oneshot::Sender<()>,
    ) {
        let (phases, receiver) = watch::channel(phase);
        let (claim, claimed) = oneshot::channel();
        let guard = tokio::spawn(guard_attempt(Arc::clone(client), phases, claimed));
        (guard, receiver, claim)
    }

    fn begin(endpoint: &str, creds: &Arc<MockCreds>) -> VTubeConnectAttempt {
        VTubeConnectAttempt::begin(
            VTubeConfig {
                endpoint: endpoint.to_owned(),
            },
            MockPublisher::new().publisher(),
            creds.creds(),
        )
    }

    async fn settled(attempt: &mut VTubeConnectAttempt) -> VTubeAttemptPhase {
        let settle = async {
            loop {
                let phase = attempt.next_phase().await;
                if phase.is_settled() {
                    return phase;
                }
            }
        };
        tokio::time::timeout(HANG_GUARD, settle)
            .await
            .expect("the attempt never settled")
    }

    async fn reach_phase(attempt: &mut VTubeConnectAttempt, wanted: VTubeAttemptPhase) {
        let reach = async { while attempt.next_phase().await != wanted {} };
        tokio::time::timeout(HANG_GUARD, reach)
            .await
            .expect("the attempt never reached the phase");
    }

    #[test]
    fn a_connection_change_moves_the_attempt_to_the_phase_it_names() {
        for (is_connected, reason, expected) in [
            (true, None, VTubeAttemptPhase::Connected),
            (
                true,
                Some(VTubeDisconnectReason::AuthDenied),
                VTubeAttemptPhase::Connected,
            ),
            (
                false,
                Some(VTubeDisconnectReason::AwaitingApproval),
                VTubeAttemptPhase::AwaitingApproval,
            ),
            (
                false,
                Some(VTubeDisconnectReason::AuthDenied),
                VTubeAttemptPhase::Failed(VTubeDisconnectReason::AuthDenied),
            ),
        ] {
            let (feed, phases, _bus) = feed_at(VTubeAttemptPhase::Connecting);

            feed.publish(change(is_connected, reason));

            assert_eq!(*phases.borrow(), expected, "{is_connected} {reason:?}");
        }
    }

    #[test]
    fn an_event_naming_no_phase_leaves_the_attempt_alone_and_still_reaches_the_bus() {
        let (feed, phases, bus) = feed_at(VTubeAttemptPhase::Connecting);
        let foreign = [
            Event::new(
                EventSource::Twitch,
                "twitch.chat.message",
                serde_json::json!({}),
            ),
            Event::new(
                EventSource::VTube,
                "vtube.model.loaded",
                serde_json::json!({}),
            ),
            change(false, None),
        ];
        let published = foreign.len();

        for event in foreign {
            feed.publish(event);
        }

        assert!(!phases.has_changed().unwrap());
        assert_eq!(bus.events.lock().unwrap().len(), published);
    }

    #[test]
    fn a_timed_out_attempt_ignores_every_later_connection_change_but_forwards_it() {
        let (feed, phases, bus) = feed_at(VTubeAttemptPhase::TimedOut);

        feed.publish(change(true, None));
        feed.publish(change(false, Some(VTubeDisconnectReason::SocketClosed)));

        assert_eq!(*phases.borrow(), VTubeAttemptPhase::TimedOut);
        assert_eq!(bus.events.lock().unwrap().len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn an_unsettled_attempt_times_out_exactly_at_the_deadline_and_tears_down() {
        for phase in [
            VTubeAttemptPhase::Connecting,
            VTubeAttemptPhase::AwaitingApproval,
        ] {
            let client = supervised_client().await;
            let (guard, phases, _claim) = guarded(&client, phase);

            tokio::time::sleep(ATTEMPT_DEADLINE - POLL_STEP).await;
            assert_eq!(*phases.borrow(), phase);
            assert!(!is_torn_down(&client));

            guard.await.unwrap();
            assert_eq!(*phases.borrow(), VTubeAttemptPhase::TimedOut);
            assert!(is_torn_down(&client));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_connected_attempt_outlives_the_deadline_and_survives_its_claim() {
        let client = supervised_client().await;
        let (guard, phases, claim) = guarded(&client, VTubeAttemptPhase::Connected);

        tokio::time::sleep(ATTEMPT_DEADLINE + ATTEMPT_DEADLINE).await;
        assert_eq!(*phases.borrow(), VTubeAttemptPhase::Connected);
        assert!(!is_torn_down(&client));

        claim.send(()).unwrap();
        guard.await.unwrap();
        assert!(!is_torn_down(&client));
    }

    #[tokio::test(start_paused = true)]
    async fn a_settled_attempt_dropped_after_the_deadline_tears_down_and_keeps_its_phase() {
        for phase in [
            VTubeAttemptPhase::Connected,
            VTubeAttemptPhase::Failed(VTubeDisconnectReason::AuthDenied),
        ] {
            let client = supervised_client().await;
            let (guard, phases, claim) = guarded(&client, phase);
            tokio::time::sleep(ATTEMPT_DEADLINE + ATTEMPT_DEADLINE).await;

            drop(claim);
            guard.await.unwrap();

            assert_eq!(*phases.borrow(), phase);
            assert!(is_torn_down(&client));
        }
    }

    #[tokio::test]
    async fn a_refused_connection_fails_the_attempt_as_connect_failed() {
        let mut attempt = begin(REFUSED_ENDPOINT, &MockCreds::new());

        assert_eq!(
            settled(&mut attempt).await,
            VTubeAttemptPhase::Failed(VTubeDisconnectReason::ConnectFailed)
        );
    }

    #[tokio::test]
    async fn a_failed_attempt_stays_down_until_auto_reconnect_is_turned_on() {
        let mut vts = FakeVts::bind().await;
        let mut attempt = begin(&vts.endpoint, &stored_token_creds());
        let first = vts
            .next_conn(HANG_GUARD)
            .await
            .expect("forge never dialed the fake VTube Studio");

        first.tx.close();

        assert!(matches!(
            settled(&mut attempt).await,
            VTubeAttemptPhase::Failed(_)
        ));
        assert!(!attempt.client.auto_reconnect_enabled());
        assert_eq!(
            attempt.client.connection_state(),
            ConnectionState::Disconnected
        );

        attempt.client.set_auto_reconnect(true);
        assert!(vts.next_conn(HANG_GUARD).await.is_some());
    }

    #[tokio::test]
    async fn dropping_an_unclaimed_attempt_disconnects_its_client() {
        let vts = FakeVts::bind().await;
        let mut attempt = begin(&vts.endpoint, &MockCreds::new());
        reach_phase(&mut attempt, VTubeAttemptPhase::AwaitingApproval).await;
        let client = Arc::clone(&attempt.client);

        drop(attempt);

        assert!(
            wait_for(|| client.connection_state() == ConnectionState::Disconnected).await,
            "the abandoned attempt must stop its client"
        );
    }

    #[tokio::test]
    async fn a_claimed_client_keeps_answering_after_the_attempt_is_gone() {
        let mut vts = FakeVts::bind().await;
        let mut attempt = begin(&vts.endpoint, &stored_token_creds());
        let conn = vts.logged_in_conn().await;
        assert_eq!(settled(&mut attempt).await, VTubeAttemptPhase::Connected);
        let (_peer, _ignored) = conn.answer_all_except(|_| false);

        let client = attempt.into_client();

        let reply = client
            .send_json_request("StatisticsRequest", serde_json::json!({}))
            .await;
        assert!(reply.is_ok(), "{reply:?}");
        assert_eq!(client.connection_state(), ConnectionState::Connected);
    }
}
