use std::time::Duration;

use tokio::time::Instant;

use super::*;
use crate::chat::chat_poller::offline_wait::OfflineWait;

const OFFLINE: Duration = ChatPollTimings::DEFAULT.offline_cadence;
const FOLLOW_UP: Duration = ChatPollTimings::DEFAULT.wake_follow_up_interval;
const WOKEN_AFTER: Duration = Duration::from_secs(10);

fn idle_poller() -> YouTubeChatPoller {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    YouTubeChatPoller::new(
        crate::endpoints_test_support::unreachable_data_api(),
        token_source(),
        tx,
        "UCtest".to_owned(),
        LiveChatIdHandle::new(),
        ActiveBroadcastIdHandle::new(),
        crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
    )
}

struct Pacing {
    poller: YouTubeChatPoller,
    wake: BroadcastLookupWake,
    listener: LookupWakeListener,
    offline: OfflineWait,
    cancel: CancellationToken,
}

impl Pacing {
    fn new() -> Self {
        let wake = BroadcastLookupWake::new();
        Self {
            poller: idle_poller(),
            listener: wake.listen(),
            wake,
            offline: OfflineWait::default(),
            cancel: CancellationToken::new(),
        }
    }

    async fn timed_wait(&mut self) -> (Duration, bool) {
        let started = Instant::now();
        let flow = self
            .poller
            .wait_offline(&mut self.offline, &mut self.listener, &self.cancel)
            .await;
        (started.elapsed(), flow.is_continue())
    }

    async fn woken_wait(&mut self) -> (Duration, bool) {
        let wake = self.wake.clone();
        let (waited, ()) = tokio::join!(self.timed_wait(), async move {
            tokio::time::sleep(WOKEN_AFTER).await;
            wake.wake();
        });
        waited
    }
}

#[tokio::test(start_paused = true)]
async fn a_wake_cuts_the_offline_wait_then_three_follow_ups_precede_the_offline_cadence() {
    let mut pacing = Pacing::new();

    let mut waits = vec![pacing.woken_wait().await];
    for _ in 0..4 {
        waits.push(pacing.timed_wait().await);
    }

    assert_eq!(
        waits,
        [
            (WOKEN_AFTER, true),
            (FOLLOW_UP, true),
            (FOLLOW_UP, true),
            (FOLLOW_UP, true),
            (OFFLINE, true),
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn going_live_drops_the_follow_ups_left_from_a_wake() {
    let mut pacing = Pacing::new();
    pacing.woken_wait().await;

    pacing.offline.settle();

    assert_eq!(pacing.timed_wait().await, (OFFLINE, true));
}

#[tokio::test(start_paused = true)]
async fn cancelling_during_the_offline_wait_stops_at_once() {
    let mut pacing = Pacing::new();
    pacing.cancel.cancel();

    assert_eq!(pacing.timed_wait().await, (Duration::ZERO, false));
}
