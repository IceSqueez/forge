#![expect(clippy::unwrap_used, reason = "test fixtures assert directly")]
mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{
    Handshake, Recorder, Reply, SCENE, Seen, bind_loopback, completes_every_handshake, connect_to,
    connect_with_timing, connected, fast_retries, finishes_promptly, spawn_fake_obs,
    standard_reply, the_first_two_handshakes_complete,
};
use forge_obs::{ObsClient, ObsError, ObsSink, ObsTiming};
use forge_platform_core::{BuiltinControl, BuiltinStatus, ConnectionState};
use forge_test_support::settle_until;

const BEYOND_ANY_GUARD: Duration = Duration::from_secs(100 * 24 * 3600);
const SHORT_CONNECT_TIMEOUT: Duration = Duration::from_millis(50);
const PROBE_PASSWORD: &str = "obs-auto-reconnect-secret-4k2j";

fn endless_retry_pause() -> ObsTiming {
    ObsTiming {
        reconnect_base_delay: BEYOND_ANY_GUARD,
        reconnect_max_delay: BEYOND_ANY_GUARD,
        ..ObsTiming::default()
    }
}

struct RefusingObs {
    port: u16,
    attempts: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for RefusingObs {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl RefusingObs {
    async fn spawn() -> Self {
        let (listener, port) = bind_loopback().await;
        let attempts = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&attempts);
        let server = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                counter.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });
        Self {
            port,
            attempts,
            server,
        }
    }

    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }

    async fn attempts_excluding_the_probe(&self) -> usize {
        common::every_earlier_dial_was_accepted(self.port).await;
        self.attempts() - 1
    }

    async fn connect(&self, password: Option<&str>) -> Arc<ObsClient> {
        self.connect_with_timing(password, fast_retries()).await
    }

    async fn connect_with_timing(
        &self,
        password: Option<&str>,
        timing: ObsTiming,
    ) -> Arc<ObsClient> {
        let client = ObsClient::connect_with_timing(
            &format!("127.0.0.1:{}", self.port),
            password,
            Arc::new(Recorder::default()),
            timing,
        )
        .await
        .unwrap();
        Arc::new(client)
    }
}

async fn disconnected(client: &ObsClient) {
    settle_until("the client settled disconnected", || {
        client.connection_state() == ConnectionState::Disconnected
    })
    .await;
}

async fn parked_after_one_refusal(fake: &RefusingObs, password: Option<&str>) -> Arc<ObsClient> {
    park(fake, fake.connect(password).await).await
}

async fn park(fake: &RefusingObs, client: Arc<ObsClient>) -> Arc<ObsClient> {
    client.set_auto_reconnect(false);
    disconnected(&client).await;
    assert_eq!(
        fake.attempts(),
        1,
        "precondition: one attempt before parking"
    );
    client
}

fn failure_code(client: &ObsClient) -> Option<String> {
    client
        .connect_failure()
        .map(|failure| failure.code().name().to_owned())
}

#[tokio::test]
async fn a_refused_first_connect_with_auto_reconnect_off_is_attempted_exactly_once() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, None).await;

    client.disconnect().await.unwrap();

    assert_eq!(fake.attempts_excluding_the_probe().await, 1);
}

#[tokio::test]
async fn turning_auto_reconnect_off_while_retrying_settles_disconnected_without_waiting_out_the_pause()
 {
    let fake = RefusingObs::spawn().await;
    let client = fake.connect_with_timing(None, endless_retry_pause()).await;
    settle_until("the first attempt failed", || {
        failure_code(&client).is_some()
    })
    .await;

    client.set_auto_reconnect(false);

    disconnected(&client).await;
}

#[tokio::test]
async fn turning_auto_reconnect_back_on_resumes_a_parked_supervisor_without_a_pause() {
    let fake = RefusingObs::spawn().await;
    let pausing = fake.connect_with_timing(None, endless_retry_pause()).await;
    let client = park(&fake, pausing).await;

    client.set_auto_reconnect(true);

    settle_until("the resumed supervisor dialled again", || {
        fake.attempts() == 2
    })
    .await;
}

#[tokio::test]
async fn a_manual_reconnect_with_auto_reconnect_off_makes_exactly_one_attempt() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, None).await;

    client.reconnect().await.unwrap();
    settle_until("the manual attempt failed and parked", || {
        fake.attempts() == 2 && client.connection_state() == ConnectionState::Disconnected
    })
    .await;
    client.disconnect().await.unwrap();

    assert_eq!(fake.attempts_excluding_the_probe().await, 2);
}

#[tokio::test]
async fn disconnecting_ends_a_parked_supervisor_so_turning_auto_reconnect_on_dials_nothing() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, None).await;

    finishes_promptly(client.disconnect())
        .await
        .expect("disconnect waited on the parked supervisor")
        .unwrap();
    client.set_auto_reconnect(true);

    assert_eq!(fake.attempts_excluding_the_probe().await, 1);
}

#[tokio::test]
async fn a_dropped_handshake_is_recorded_as_a_failure_that_never_carries_the_password() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, Some(PROBE_PASSWORD)).await;

    let failure = client.connect_failure().expect("no failure was recorded");

    assert!(
        !format!("{failure:?}").contains(PROBE_PASSWORD),
        "the recorded failure leaked the password"
    );
}

#[tokio::test]
async fn a_connect_to_a_closed_port_records_connect_refused() {
    let (listener, port) = bind_loopback().await;
    drop(listener);
    let client = ObsClient::connect_with_timing(
        &format!("127.0.0.1:{port}"),
        None,
        Arc::new(Recorder::default()),
        fast_retries(),
    )
    .await
    .unwrap();
    client.set_auto_reconnect(false);
    disconnected(&client).await;

    assert_eq!(failure_code(&client).as_deref(), Some("connect_refused"));
}

#[tokio::test]
async fn disconnecting_clears_the_recorded_connect_failure() {
    let fake = RefusingObs::spawn().await;
    let client = parked_after_one_refusal(&fake, None).await;
    assert!(
        failure_code(&client).is_some(),
        "precondition: a failure is recorded"
    );

    client.disconnect().await.unwrap();

    assert_eq!(failure_code(&client), None);
}

#[tokio::test]
async fn a_manual_reconnect_clears_the_previous_failure_while_the_new_attempt_is_pending() {
    let (listener, port) = bind_loopback().await;
    let hold = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let accepted = Arc::new(AtomicUsize::new(0));
    let server = {
        let hold = Arc::clone(&hold);
        let accepted = Arc::clone(&accepted);
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                accepted.fetch_add(1, Ordering::SeqCst);
                if hold.load(Ordering::SeqCst) {
                    held.push(stream);
                }
            }
        })
    };
    let client = ObsClient::connect(
        &format!("127.0.0.1:{port}"),
        None,
        Arc::new(Recorder::default()),
    )
    .await
    .unwrap();
    client.set_auto_reconnect(false);
    disconnected(&client).await;
    assert!(
        failure_code(&client).is_some(),
        "precondition: a failure is recorded"
    );
    hold.store(true, Ordering::SeqCst);

    client.reconnect().await.unwrap();
    settle_until("the second attempt reached the server", || {
        accepted.load(Ordering::SeqCst) == 2
    })
    .await;

    assert_eq!(failure_code(&client), None);
    server.abort();
}

#[tokio::test]
async fn a_successful_reconnect_clears_the_connection_lost_failure() {
    let fake = spawn_fake_obs(
        the_first_two_handshakes_complete,
        Arc::new(|seen: Seen<'_>| match (seen.conn, seen.request_type) {
            (0, "SetCurrentProgramScene") => Reply::DropSocket,
            (_, other) => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;
    let lost = client.set_scene(SCENE).await;
    assert!(matches!(lost, Err(ObsError::Disconnected)), "got {lost:?}");
    settle_until("the lost session was recorded", || {
        failure_code(&client).as_deref() == Some("connection_lost")
    })
    .await;

    settle_until("the second session connected", || {
        fake.log.accepted() == 2 && client.connection_state() == ConnectionState::Connected
    })
    .await;

    assert_eq!(failure_code(&client), None);
}

#[tokio::test]
async fn a_stalled_upgrade_records_unreachable_at_the_connect_timeout() {
    let fake = spawn_fake_obs(
        |_| Handshake::StallUpgrade,
        Arc::new(|seen: Seen<'_>| standard_reply(seen.request_type)),
    )
    .await;
    let recorder = Recorder::default();
    let timing = ObsTiming {
        connect_timeout: SHORT_CONNECT_TIMEOUT,
        ..fast_retries()
    };
    let client = connect_with_timing(&fake, &recorder, timing).await;
    client.set_auto_reconnect(false);

    settle_until("the stalled upgrade gave up", || {
        failure_code(&client).is_some()
    })
    .await;

    assert_eq!(failure_code(&client).as_deref(), Some("unreachable"));
}

#[tokio::test]
async fn a_live_session_lost_with_auto_reconnect_off_shows_why_and_is_not_redialled() {
    let fake = spawn_fake_obs(
        completes_every_handshake,
        Arc::new(|seen: Seen<'_>| match seen.request_type {
            "SetCurrentProgramScene" => Reply::DropSocket,
            other => standard_reply(other),
        }),
    )
    .await;
    let recorder = Recorder::default();
    let client = connect_to(&fake, &recorder).await;
    connected(&client).await;
    client.set_auto_reconnect(false);

    let lost = client.set_scene(SCENE).await;
    assert!(matches!(lost, Err(ObsError::Disconnected)), "got {lost:?}");
    disconnected(&client).await;
    let shown_failure = failure_code(&client);
    client.disconnect().await.unwrap();

    assert_eq!(
        fake.dials_excluding_the_probe().await,
        1,
        "the lost session was redialled"
    );
    assert_eq!(shown_failure.as_deref(), Some("connection_lost"));
}
