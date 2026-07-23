use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_platform_core::{DedupSet, PlatformError};
use futures::future::BoxFuture;
use tokio::sync::mpsc;
use tracing::warn;

use crate::channel::{ChannelSnapshot, KickChannel};
use crate::payload_fields::{reward as reward_fields, stream as stream_fields};
use crate::rewards::{KickRewards, RedemptionRecord};

const CHANNEL_POLL_INTERVAL: Duration = Duration::from_secs(30);
const REDEMPTION_POLL_INTERVAL: Duration = Duration::from_secs(12);

const LIVESTREAM_STATUS_KIND: &str = "kick.livestream.status.updated";
const LIVESTREAM_METADATA_KIND: &str = "kick.livestream.metadata.updated";
const REWARD_REDEEMED_KIND: &str = "kick.channel.reward.redemption.updated";

type TokenSource = Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

pub fn spawn_kick_poller(
    channel: Arc<KickChannel>,
    rewards: Arc<KickRewards>,
    token_source: TokenSource,
    event_tx: mpsc::Sender<Event>,
) {
    tokio::spawn(run_loop(channel, rewards, token_source, event_tx));
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
        (stream_fields::IS_LIVE): snapshot.is_live,
        (stream_fields::STREAM_TITLE): snapshot.stream_title,
        (stream_fields::CATEGORY): {
            (stream_fields::CATEGORY_ID): snapshot.category_id,
            (stream_fields::CATEGORY_NAME): snapshot.category_name,
        },
    })
}

fn metadata_payload(snapshot: &ChannelSnapshot) -> serde_json::Value {
    serde_json::json!({
        (stream_fields::STREAM_TITLE): snapshot.stream_title,
        (stream_fields::CATEGORY): {
            (stream_fields::CATEGORY_ID): snapshot.category_id,
            (stream_fields::CATEGORY_NAME): snapshot.category_name,
        },
    })
}

fn redemption_payload(record: &RedemptionRecord) -> serde_json::Value {
    serde_json::json!({
        (reward_fields::ID): record.id,
        (reward_fields::REWARD): {
            (reward_fields::ID): record.reward_id,
            (reward_fields::REWARD_TITLE): record.reward_title,
        },
        (reward_fields::REDEEMER): {
            (reward_fields::REDEEMER_USER_ID): record.redeemer_user_id,
            (reward_fields::REDEEMER_USERNAME): record.redeemer_username,
        },
        (reward_fields::USER_INPUT): record.user_input,
    })
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
    seen: &mut DedupSet,
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

    let emit_allowed = *seeded;
    for record in &records {
        if seen.try_insert(record.id.clone()) && emit_allowed {
            emit(event_tx, REWARD_REDEEMED_KIND, redemption_payload(record)).await?;
        }
    }
    seen.retain_present(records.iter().map(|r| r.id.as_str()));
    *seeded = true;
    Ok(())
}

async fn run_loop(
    channel: Arc<KickChannel>,
    rewards: Arc<KickRewards>,
    token_source: TokenSource,
    event_tx: mpsc::Sender<Event>,
) {
    let mut channel_interval = tokio::time::interval(CHANNEL_POLL_INTERVAL);
    let mut redemption_interval = tokio::time::interval(REDEMPTION_POLL_INTERVAL);
    let mut last_snapshot: Option<ChannelSnapshot> = None;
    let mut seen = DedupSet::unbounded();
    let mut redemptions_seeded = false;

    loop {
        tokio::select! {
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
        let mut seen = DedupSet::unbounded();
        let mut seeded = false;

        poll_redemptions(&rewards, &ok_token(), &tx, &mut seen, &mut seeded)
            .await
            .unwrap();

        assert!(rx.try_recv().is_err(), "the seeding poll must emit nothing");
        assert!(seeded, "a successful first poll flips the seeded flag");
        assert!(seen.contains("rd_1"));
        assert!(seen.contains("rd_2"));
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
        let mut seen = DedupSet::unbounded();
        seen.try_insert("rd_1".to_owned());
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
        let mut seen = DedupSet::unbounded();
        let mut seeded = true;

        let result = poll_redemptions(&rewards, &err_token(), &tx, &mut seen, &mut seeded).await;

        assert!(
            result.is_ok(),
            "a token error is non-fatal for the poll loop"
        );
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn poll_redemptions_never_re_emits_still_pending_id_and_prunes_resolved() {
        fn pending(ids: &[&str]) -> serde_json::Value {
            let data: Vec<serde_json::Value> = ids
                .iter()
                .map(|id| serde_json::json!({ "id": id, "reward": { "id": "rw_1", "title": "Hydrate" } }))
                .collect();
            serde_json::json!({ "data": data })
        }

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pending(&["a", "b"])))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pending(&["a", "b", "c"])))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/channels/rewards/redemptions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pending(&["a"])))
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let rewards = rewards_on(&server);
        let (tx, mut rx) = mpsc::channel(8);
        let mut seen = DedupSet::unbounded();
        let mut seeded = false;

        poll_redemptions(&rewards, &ok_token(), &tx, &mut seen, &mut seeded)
            .await
            .unwrap();
        assert!(rx.try_recv().is_err(), "the seeding poll must emit nothing");

        poll_redemptions(&rewards, &ok_token(), &tx, &mut seen, &mut seeded)
            .await
            .unwrap();
        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind, REWARD_REDEEMED_KIND);
        assert_eq!(event.payload["id"], "c");
        assert!(
            rx.try_recv().is_err(),
            "still-pending a and b must never be re-emitted"
        );

        poll_redemptions(&rewards, &ok_token(), &tx, &mut seen, &mut seeded)
            .await
            .unwrap();
        assert!(
            rx.try_recv().is_err(),
            "a is still pending and already seen, so nothing emits"
        );
        assert!(seen.contains("a"));
        assert!(
            !seen.contains("b") && !seen.contains("c"),
            "resolved ids b and c must be pruned, leaving only the live pending id"
        );
    }
}
