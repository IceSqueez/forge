use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_net::DedupSet;
use forge_platform_core::{LiveViewerSource, PlatformError, ViewerReport, ViewerReportStream};
use futures::future::BoxFuture;
use tokio::sync::{mpsc, watch};
use tokio::task::AbortHandle;
use tokio_stream::wrappers::WatchStream;
use tracing::warn;

use crate::api::channel::{ChannelSnapshot, KickChannel};
use crate::api::rewards::{KickRewards, RedemptionRecord};
use crate::payload_fields::{reward as reward_fields, stream as stream_fields};

const CHANNEL_POLL_INTERVAL: Duration = Duration::from_secs(30);
const REDEMPTION_POLL_INTERVAL: Duration = Duration::from_secs(12);

const LIVESTREAM_STATUS_KIND: &str = "kick.livestream.status.updated";
const LIVESTREAM_METADATA_KIND: &str = "kick.livestream.metadata.updated";
const REWARD_REDEEMED_KIND: &str = "kick.channel.reward.redemption.updated";

pub(crate) type TokenSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollerAuth {
    Authorized,
    AuthRequired,
}

pub struct KickViewerSource {
    reports: watch::Receiver<ViewerReport>,
    auth: watch::Receiver<PollerAuth>,
}

impl LiveViewerSource for KickViewerSource {
    fn viewer_reports(&self) -> ViewerReportStream {
        Box::pin(WatchStream::new(self.reports.clone()))
    }
}

impl KickViewerSource {
    pub fn subscribe(&self) -> watch::Receiver<ViewerReport> {
        self.reports.clone()
    }

    pub fn subscribe_auth(&self) -> watch::Receiver<PollerAuth> {
        self.auth.clone()
    }
}

pub struct KickPollerHandle(AbortHandle);

impl KickPollerHandle {
    pub fn stop(&self) {
        self.0.abort();
    }
}

pub fn spawn_kick_poller(
    channel: Arc<KickChannel>,
    rewards: Arc<KickRewards>,
    token_source: TokenSource,
    event_tx: mpsc::Sender<Event>,
) -> (KickViewerSource, KickPollerHandle) {
    let (viewer_tx, viewer_rx) = watch::channel(ViewerReport::Absent);
    let (auth_tx, auth_rx) = watch::channel(PollerAuth::Authorized);
    let task = tokio::spawn(run_loop(
        channel,
        rewards,
        token_source,
        event_tx,
        viewer_tx,
        auth_tx,
    ));
    (
        KickViewerSource {
            reports: viewer_rx,
            auth: auth_rx,
        },
        KickPollerHandle(task.abort_handle()),
    )
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

fn poll_failure(error: &PlatformError) -> String {
    match error {
        PlatformError::Http { status, .. } => format!("HTTP {status}"),
        PlatformError::Network { reason } => reason.clone(),
        other => other.to_string(),
    }
}

fn is_auth_loss(error: &PlatformError) -> bool {
    matches!(
        error,
        PlatformError::ReauthRequired { .. } | PlatformError::Auth { .. }
    )
}

#[derive(Debug, Clone, Copy)]
enum PollEndpoint {
    Channel,
    Redemptions,
}

impl PollEndpoint {
    fn name(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Redemptions => "redemptions",
        }
    }
}

struct AuthTracker {
    tx: watch::Sender<PollerAuth>,
    channel_lost: bool,
    redemptions_lost: bool,
}

impl AuthTracker {
    fn new(tx: watch::Sender<PollerAuth>) -> Self {
        Self {
            tx,
            channel_lost: false,
            redemptions_lost: false,
        }
    }

    fn flag(&mut self, endpoint: PollEndpoint) -> &mut bool {
        match endpoint {
            PollEndpoint::Channel => &mut self.channel_lost,
            PollEndpoint::Redemptions => &mut self.redemptions_lost,
        }
    }

    fn publish(&self) {
        let next = if self.channel_lost || self.redemptions_lost {
            PollerAuth::AuthRequired
        } else {
            PollerAuth::Authorized
        };
        self.tx.send_if_modified(|current| {
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        });
    }

    fn mark_lost(&mut self, endpoint: PollEndpoint) {
        let flag = self.flag(endpoint);
        let newly_lost = !*flag;
        *flag = true;
        if newly_lost {
            warn!(
                endpoint = endpoint.name(),
                "kick poller authorization lost, sign in to Kick again"
            );
        }
        self.publish();
    }

    fn mark_authorized(&mut self, endpoint: PollEndpoint) {
        *self.flag(endpoint) = false;
        self.publish();
    }
}

fn clear_viewer_report(viewer_tx: &watch::Sender<ViewerReport>) {
    viewer_tx.send_if_modified(|current| {
        if matches!(current, ViewerReport::Absent) {
            false
        } else {
            *current = ViewerReport::Absent;
            true
        }
    });
}

fn note_poll_failure(
    error: &PlatformError,
    auth: &mut AuthTracker,
    endpoint: PollEndpoint,
    viewer_tx: Option<&watch::Sender<ViewerReport>>,
    context: &'static str,
) {
    if is_auth_loss(error) {
        auth.mark_lost(endpoint);
        if let Some(viewer_tx) = viewer_tx {
            clear_viewer_report(viewer_tx);
        }
    } else {
        warn!(error = %poll_failure(error), "{context}");
    }
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
    viewer_tx: &watch::Sender<ViewerReport>,
    auth: &mut AuthTracker,
    last_snapshot: &mut Option<ChannelSnapshot>,
) -> Result<(), ()> {
    let token = match token_source().await {
        Ok(token) => token,
        Err(error) => {
            note_poll_failure(
                &error,
                auth,
                PollEndpoint::Channel,
                Some(viewer_tx),
                "kick token unavailable",
            );
            return Ok(());
        }
    };

    let snapshot = match channel.get_channel(&token).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            note_poll_failure(
                &error,
                auth,
                PollEndpoint::Channel,
                Some(viewer_tx),
                "kick channel poll failed",
            );
            return Ok(());
        }
    };
    auth.mark_authorized(PollEndpoint::Channel);

    let report = if snapshot.is_live {
        ViewerReport::Live {
            count: snapshot.viewer_count,
        }
    } else {
        ViewerReport::Absent
    };
    viewer_tx.send_replace(report);

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
    auth: &mut AuthTracker,
    seen: &mut DedupSet,
    seeded: &mut bool,
) -> Result<(), ()> {
    let token = match token_source().await {
        Ok(token) => token,
        Err(error) => {
            note_poll_failure(
                &error,
                auth,
                PollEndpoint::Redemptions,
                None,
                "kick token unavailable",
            );
            return Ok(());
        }
    };

    let records = match rewards.list_pending_redemptions(&token).await {
        Ok(records) => records,
        Err(error) => {
            note_poll_failure(
                &error,
                auth,
                PollEndpoint::Redemptions,
                None,
                "kick redemption poll failed",
            );
            return Ok(());
        }
    };
    auth.mark_authorized(PollEndpoint::Redemptions);

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
    viewer_tx: watch::Sender<ViewerReport>,
    auth_tx: watch::Sender<PollerAuth>,
) {
    let mut auth = AuthTracker::new(auth_tx);
    let mut channel_interval = tokio::time::interval(CHANNEL_POLL_INTERVAL);
    let mut redemption_interval = tokio::time::interval(REDEMPTION_POLL_INTERVAL);
    let mut last_snapshot: Option<ChannelSnapshot> = None;
    let mut seen = DedupSet::unbounded();
    let mut redemptions_seeded = false;

    loop {
        tokio::select! {
            _ = channel_interval.tick() => {
                if poll_channel(
                    &channel,
                    &token_source,
                    &event_tx,
                    &viewer_tx,
                    &mut auth,
                    &mut last_snapshot,
                )
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
                    &mut auth,
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
