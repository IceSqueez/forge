use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_platform_core::PlatformError;
use futures::future::BoxFuture;
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::channel::{ChannelSnapshot, KickChannel};
use crate::rewards::{KickRewards, RedemptionRecord};

const CHANNEL_POLL_INTERVAL: Duration = Duration::from_secs(30);
const REDEMPTION_POLL_INTERVAL: Duration = Duration::from_secs(12);
const SEEN_REDEMPTIONS_CAP: usize = 1000;

const LIVESTREAM_STATUS_KIND: &str = "kick.channel.livestream_status";
const LIVESTREAM_METADATA_KIND: &str = "kick.channel.livestream_metadata";
const REWARD_REDEEMED_KIND: &str = "kick.channel.reward_redeemed";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub struct KickPollerHandle {
    pub close_tx: oneshot::Sender<()>,
}

pub fn spawn_kick_poller(
    channel: Arc<KickChannel>,
    rewards: Arc<KickRewards>,
    token_source: TokenSource,
    event_tx: mpsc::Sender<Event>,
) -> KickPollerHandle {
    let (close_tx, close_rx) = oneshot::channel();
    tokio::spawn(run_loop(channel, rewards, token_source, event_tx, close_rx));
    KickPollerHandle { close_tx }
}

struct ChannelDelta {
    status_changed: bool,
    metadata_changed: bool,
}

fn diff_channel(prev: &ChannelSnapshot, next: &ChannelSnapshot) -> ChannelDelta {
    ChannelDelta {
        status_changed: prev.is_live != next.is_live,
        metadata_changed: prev.stream_title != next.stream_title
            || prev.category_id != next.category_id,
    }
}

fn status_payload(snapshot: &ChannelSnapshot) -> serde_json::Value {
    serde_json::json!({
        "is_live": snapshot.is_live,
        "stream_title": snapshot.stream_title,
        "category": { "id": snapshot.category_id, "name": snapshot.category_name },
    })
}

fn metadata_payload(snapshot: &ChannelSnapshot) -> serde_json::Value {
    serde_json::json!({
        "stream_title": snapshot.stream_title,
        "category": { "id": snapshot.category_id, "name": snapshot.category_name },
    })
}

fn redemption_payload(record: &RedemptionRecord) -> serde_json::Value {
    serde_json::json!({
        "id": record.id,
        "reward": { "id": record.reward_id, "title": record.reward_title },
        "redeemer": { "user_id": record.redeemer_user_id, "username": record.redeemer_username },
        "user_input": record.user_input,
    })
}

struct SeenRedemptions {
    seen: HashSet<String>,
    order: VecDeque<String>,
    cap: usize,
}

impl SeenRedemptions {
    fn new(cap: usize) -> Self {
        Self {
            seen: HashSet::new(),
            order: VecDeque::new(),
            cap,
        }
    }

    fn mark_new(&mut self, id: &str) -> bool {
        if self.seen.contains(id) {
            return false;
        }
        self.seen.insert(id.to_owned());
        self.order.push_back(id.to_owned());
        if self.order.len() > self.cap
            && let Some(evicted) = self.order.pop_front()
        {
            self.seen.remove(&evicted);
        }
        true
    }
}

async fn resolve_token(token_source: &TokenSource) -> Option<String> {
    token_source().await.ok()
}

async fn emit(
    event_tx: &mpsc::Sender<Event>,
    kind: &'static str,
    payload: serde_json::Value,
) -> Result<(), ()> {
    event_tx
        .send(Event::new(EventSource::Kick, kind, payload))
        .await
        .map_err(|_| ())
}

async fn poll_channel(
    channel: &KickChannel,
    token_source: &TokenSource,
    event_tx: &mpsc::Sender<Event>,
    last_snapshot: &mut Option<ChannelSnapshot>,
) -> Result<(), ()> {
    let Some(token) = resolve_token(token_source).await else {
        return Ok(());
    };

    let snapshot = match channel.get_channel(&token).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            warn!(%error, "kick channel poll failed");
            return Ok(());
        }
    };

    if let Some(prev) = last_snapshot.as_ref() {
        let delta = diff_channel(prev, &snapshot);
        if delta.status_changed {
            emit(event_tx, LIVESTREAM_STATUS_KIND, status_payload(&snapshot)).await?;
        }
        if delta.metadata_changed {
            emit(
                event_tx,
                LIVESTREAM_METADATA_KIND,
                metadata_payload(&snapshot),
            )
            .await?;
        }
    }

    *last_snapshot = Some(snapshot);
    Ok(())
}

async fn poll_redemptions(
    rewards: &KickRewards,
    token_source: &TokenSource,
    event_tx: &mpsc::Sender<Event>,
    seen: &mut SeenRedemptions,
    seeded: &mut bool,
) -> Result<(), ()> {
    let Some(token) = resolve_token(token_source).await else {
        return Ok(());
    };

    let records = match rewards.list_pending_redemptions(&token).await {
        Ok(records) => records,
        Err(error) => {
            warn!(%error, "kick redemption poll failed");
            return Ok(());
        }
    };

    // The first successful poll seeds the dedupe set without emitting, so a
    // restart does not replay redemptions that were already pending.
    let emit_allowed = *seeded;
    for record in &records {
        let is_new = seen.mark_new(&record.id);
        if is_new && emit_allowed {
            emit(event_tx, REWARD_REDEEMED_KIND, redemption_payload(record)).await?;
        }
    }
    *seeded = true;
    Ok(())
}

async fn run_loop(
    channel: Arc<KickChannel>,
    rewards: Arc<KickRewards>,
    token_source: TokenSource,
    event_tx: mpsc::Sender<Event>,
    mut close_rx: oneshot::Receiver<()>,
) {
    let mut channel_interval = tokio::time::interval(CHANNEL_POLL_INTERVAL);
    let mut redemption_interval = tokio::time::interval(REDEMPTION_POLL_INTERVAL);
    let mut last_snapshot: Option<ChannelSnapshot> = None;
    let mut seen = SeenRedemptions::new(SEEN_REDEMPTIONS_CAP);
    let mut redemptions_seeded = false;

    loop {
        tokio::select! {
            _ = &mut close_rx => break,
            _ = channel_interval.tick() => {
                if poll_channel(&channel, &token_source, &event_tx, &mut last_snapshot)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            _ = redemption_interval.tick() => {
                if poll_redemptions(
                    &rewards,
                    &token_source,
                    &event_tx,
                    &mut seen,
                    &mut redemptions_seeded,
                )
                .await
                .is_err()
                {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use forge_platform_core::{RateLimitOutcome, RateLimiter};
    use std::time::Duration as StdDuration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    struct GrantLimiter;
    #[async_trait::async_trait]
    impl RateLimiter for GrantLimiter {
        async fn acquire(&self, _weight: u32) -> Result<RateLimitOutcome, PlatformError> {
            Ok(RateLimitOutcome::Granted)
        }
        fn remaining(&self) -> u32 {
            120
        }
        async fn observe_remote_throttle(&self, _retry_after: StdDuration) {}
    }

    fn ok_token() -> TokenSource {
        Arc::new(|| {
            Box::pin(async { Ok::<_, PlatformError>("tok".to_owned()) }) as BoxFuture<'static, _>
        })
    }

    fn err_token() -> TokenSource {
        Arc::new(|| {
            Box::pin(async {
                Err::<String, _>(PlatformError::Auth {
                    reason: "not authorized".to_owned(),
                })
            }) as BoxFuture<'static, _>
        })
    }

    fn snapshot(
        is_live: bool,
        title: &str,
        category_id: u64,
        category_name: &str,
    ) -> ChannelSnapshot {
        ChannelSnapshot {
            is_live,
            stream_title: title.to_owned(),
            category_id,
            category_name: category_name.to_owned(),
        }
    }

    fn record(id: &str) -> RedemptionRecord {
        RedemptionRecord {
            id: id.to_owned(),
            reward_id: "rw_1".to_owned(),
            reward_title: "Hydrate".to_owned(),
            redeemer_user_id: 42,
            redeemer_username: "alice".to_owned(),
            user_input: "drink water".to_owned(),
        }
    }

    fn channel_on(server: &MockServer) -> KickChannel {
        KickChannel::new(Arc::new(GrantLimiter)).with_api_base(server.uri())
    }

    fn rewards_on(server: &MockServer) -> KickRewards {
        KickRewards::new(Arc::new(GrantLimiter)).with_api_base(server.uri())
    }

    fn channel_body(
        is_live: bool,
        title: &str,
        category_id: u64,
        category_name: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "data": [{
                "is_live": is_live,
                "stream_title": title,
                "category": { "id": category_id, "name": category_name }
            }]
        })
    }

    // --- SeenRedemptions::mark_new ---

    #[test]
    fn mark_new_is_true_on_first_sighting_and_false_on_repeat() {
        let mut seen = SeenRedemptions::new(8);
        assert!(seen.mark_new("rd_1"), "first sighting is new");
        assert!(
            !seen.mark_new("rd_1"),
            "second sighting of same id is not new"
        );
    }

    #[test]
    fn mark_new_evicts_oldest_when_over_cap_and_keeps_recent() {
        let mut seen = SeenRedemptions::new(2);
        assert!(seen.mark_new("a"));
        assert!(seen.mark_new("b"));
        // Inserting a 3rd distinct id pushes capacity past cap, evicting "a".
        assert!(seen.mark_new("c"));
        // "c" is still resident (cap not yet exceeded by it) -> repeat is not new.
        assert!(!seen.mark_new("c"), "still-resident id must stay deduped");
        // "a" was evicted by inserting "c" -> it is insertable again (treated as new).
        assert!(seen.mark_new("a"), "oldest id should have been evicted");
    }

    // --- diff_channel ---

    #[test]
    fn diff_channel_reports_status_change_only_on_live_flip() {
        let prev = snapshot(false, "Title", 1, "Cat");
        let next = snapshot(true, "Title", 1, "Cat");
        let delta = diff_channel(&prev, &next);
        assert!(delta.status_changed);
        assert!(!delta.metadata_changed);
    }

    #[test]
    fn diff_channel_reports_metadata_change_on_title_change() {
        let prev = snapshot(true, "Old", 1, "Cat");
        let next = snapshot(true, "New", 1, "Cat");
        let delta = diff_channel(&prev, &next);
        assert!(!delta.status_changed);
        assert!(delta.metadata_changed);
    }

    #[test]
    fn diff_channel_reports_metadata_change_on_category_id_change() {
        let prev = snapshot(true, "Title", 1, "Cat");
        let next = snapshot(true, "Title", 2, "Cat");
        let delta = diff_channel(&prev, &next);
        assert!(!delta.status_changed);
        assert!(delta.metadata_changed);
    }

    #[test]
    fn diff_channel_reports_nothing_changed_for_identical_snapshots() {
        let prev = snapshot(true, "Title", 1, "Cat");
        let next = snapshot(true, "Title", 1, "Cat");
        let delta = diff_channel(&prev, &next);
        assert!(!delta.status_changed);
        assert!(!delta.metadata_changed);
    }

    // --- payload builders (poller -> descriptor contract) ---

    #[test]
    fn status_payload_carries_is_live_title_and_nested_category() {
        let payload = status_payload(&snapshot(true, "Stream Title", 77, "Just Chatting"));
        assert_eq!(
            payload,
            serde_json::json!({
                "is_live": true,
                "stream_title": "Stream Title",
                "category": { "id": 77, "name": "Just Chatting" }
            })
        );
    }

    #[test]
    fn metadata_payload_carries_title_and_nested_category_without_is_live() {
        let payload = metadata_payload(&snapshot(true, "Stream Title", 77, "Just Chatting"));
        assert_eq!(
            payload,
            serde_json::json!({
                "stream_title": "Stream Title",
                "category": { "id": 77, "name": "Just Chatting" }
            })
        );
    }

    #[test]
    fn redemption_payload_carries_nested_reward_and_redeemer_blocks() {
        let payload = redemption_payload(&record("rd_9"));
        assert_eq!(
            payload,
            serde_json::json!({
                "id": "rd_9",
                "reward": { "id": "rw_1", "title": "Hydrate" },
                "redeemer": { "user_id": 42, "username": "alice" },
                "user_input": "drink water"
            })
        );
    }

    // --- poll_channel ---

    #[tokio::test]
    async fn poll_channel_first_call_seeds_snapshot_and_emits_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body(true, "T", 1, "C")))
            .mount(&server)
            .await;

        let channel = channel_on(&server);
        let (tx, mut rx) = mpsc::channel(4);
        let mut last = None;

        poll_channel(&channel, &ok_token(), &tx, &mut last)
            .await
            .unwrap();

        assert!(last.is_some(), "first successful poll seeds last_snapshot");
        assert!(rx.try_recv().is_err(), "seeding must not emit any event");
    }

    #[tokio::test]
    async fn poll_channel_unchanged_data_emits_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body(true, "T", 1, "C")))
            .mount(&server)
            .await;

        let channel = channel_on(&server);
        let (tx, mut rx) = mpsc::channel(4);
        let mut last = Some(snapshot(true, "T", 1, "C"));

        poll_channel(&channel, &ok_token(), &tx, &mut last)
            .await
            .unwrap();

        assert!(rx.try_recv().is_err(), "identical data must emit nothing");
    }

    #[tokio::test]
    async fn poll_channel_live_flip_emits_one_status_event_with_payload() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body(true, "T", 1, "C")))
            .mount(&server)
            .await;

        let channel = channel_on(&server);
        let (tx, mut rx) = mpsc::channel(4);
        let mut last = Some(snapshot(false, "T", 1, "C"));

        poll_channel(&channel, &ok_token(), &tx, &mut last)
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind, LIVESTREAM_STATUS_KIND);
        assert_eq!(event.payload["is_live"], true);
        assert!(
            rx.try_recv().is_err(),
            "only one event for a pure status flip"
        );
    }

    #[tokio::test]
    async fn poll_channel_title_change_emits_one_metadata_event() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(channel_body(true, "New", 1, "C")),
            )
            .mount(&server)
            .await;

        let channel = channel_on(&server);
        let (tx, mut rx) = mpsc::channel(4);
        let mut last = Some(snapshot(true, "Old", 1, "C"));

        poll_channel(&channel, &ok_token(), &tx, &mut last)
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind, LIVESTREAM_METADATA_KIND);
        assert_eq!(event.payload["stream_title"], "New");
        assert!(
            rx.try_recv().is_err(),
            "only one event for a pure title change"
        );
    }

    #[tokio::test]
    async fn poll_channel_token_error_skips_http_and_emits_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels"))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body(true, "T", 1, "C")))
            .expect(0)
            .mount(&server)
            .await;

        let channel = channel_on(&server);
        let (tx, mut rx) = mpsc::channel(4);
        let mut last = None;

        let result = poll_channel(&channel, &err_token(), &tx, &mut last).await;

        assert!(
            result.is_ok(),
            "a token error is non-fatal for the poll loop"
        );
        assert!(
            last.is_none(),
            "no snapshot seeded when the token is unavailable"
        );
        assert!(rx.try_recv().is_err());
    }

    // --- poll_redemptions ---

    #[tokio::test]
    async fn poll_redemptions_first_poll_seeds_silently_and_marks_ids() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "rd_1", "reward": { "id": "rw_1", "title": "Hydrate" } },
                    { "id": "rd_2", "reward": { "id": "rw_1", "title": "Hydrate" } }
                ]
            })))
            .mount(&server)
            .await;

        let rewards = rewards_on(&server);
        let (tx, mut rx) = mpsc::channel(4);
        let mut seen = SeenRedemptions::new(8);
        let mut seeded = false;

        poll_redemptions(&rewards, &ok_token(), &tx, &mut seen, &mut seeded)
            .await
            .unwrap();

        assert!(rx.try_recv().is_err(), "the seeding poll must emit nothing");
        assert!(seeded, "a successful first poll flips the seeded flag");
        // The ids were marked: re-marking returns false.
        assert!(!seen.mark_new("rd_1"));
        assert!(!seen.mark_new("rd_2"));
    }

    #[tokio::test]
    async fn poll_redemptions_after_seed_emits_only_for_brand_new_id() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    { "id": "rd_1", "reward": { "id": "rw_1", "title": "Hydrate" },
                      "redeemer": { "user_id": 42, "username": "alice" }, "user_input": "" },
                    { "id": "rd_new", "reward": { "id": "rw_1", "title": "Hydrate" },
                      "redeemer": { "user_id": 7, "username": "bob" }, "user_input": "hi" }
                ]
            })))
            .mount(&server)
            .await;

        let rewards = rewards_on(&server);
        let (tx, mut rx) = mpsc::channel(4);
        let mut seen = SeenRedemptions::new(8);
        seen.mark_new("rd_1"); // previously observed
        let mut seeded = true;

        poll_redemptions(&rewards, &ok_token(), &tx, &mut seen, &mut seeded)
            .await
            .unwrap();

        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind, REWARD_REDEEMED_KIND);
        assert_eq!(event.payload["id"], "rd_new");
        assert!(
            rx.try_recv().is_err(),
            "the already-seen id must not re-emit"
        );
    }

    #[tokio::test]
    async fn poll_redemptions_token_error_skips_and_emits_nothing() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .expect(0)
            .mount(&server)
            .await;

        let rewards = rewards_on(&server);
        let (tx, mut rx) = mpsc::channel(4);
        let mut seen = SeenRedemptions::new(8);
        let mut seeded = true;

        let result = poll_redemptions(&rewards, &err_token(), &tx, &mut seen, &mut seeded).await;

        assert!(
            result.is_ok(),
            "a token error is non-fatal for the poll loop"
        );
        assert!(rx.try_recv().is_err());
    }
}
