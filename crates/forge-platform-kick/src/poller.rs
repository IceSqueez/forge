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
