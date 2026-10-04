use std::sync::Arc;
use std::time::Duration;

use forge_events::{Event, EventSource};
use forge_platform_core::{
    DedupSet, LiveViewerSource, PlatformError, ViewerReport, ViewerReportStream,
};
use futures::future::BoxFuture;
use tokio::sync::{mpsc, watch};
use tokio::task::AbortHandle;
use tokio_stream::wrappers::WatchStream;
use tracing::warn;

use crate::channel::{ChannelSnapshot, KickChannel};
use crate::payload_fields::{reward as reward_fields, stream as stream_fields};
use crate::rewards::{KickRewards, RedemptionRecord};

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
    let _ = viewer_tx.send(report);

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

    const TOKEN_SENTINEL: &str = "kick_token_sentinel";

    fn ok_token() -> TokenSource {
        Arc::new(|| {
            Box::pin(async { Ok::<_, PlatformError>(TOKEN_SENTINEL.to_owned()) })
                as BoxFuture<'static, _>
        })
    }

    fn failing_token(make: fn() -> PlatformError) -> TokenSource {
        Arc::new(move || Box::pin(async move { Err::<String, _>(make()) }) as BoxFuture<'static, _>)
    }

    fn auth_rejected() -> PlatformError {
        PlatformError::Auth {
            reason: "not authorized".to_owned(),
        }
    }

    fn reauth_required() -> PlatformError {
        PlatformError::ReauthRequired {
            platform: "kick".to_owned(),
        }
    }

    fn storage_unreadable() -> PlatformError {
        PlatformError::Io(std::io::Error::other("credential store unreadable"))
    }

    fn network_down() -> PlatformError {
        PlatformError::Network {
            reason: "error sending request".to_owned(),
        }
    }

    fn err_token() -> TokenSource {
        failing_token(auth_rejected)
    }

    fn viewer_sender() -> watch::Sender<ViewerReport> {
        watch::channel(ViewerReport::Absent).0
    }

    fn live_viewer_sender() -> watch::Sender<ViewerReport> {
        watch::channel(ViewerReport::Live { count: 5 }).0
    }

    fn tracker() -> AuthTracker {
        AuthTracker::new(watch::channel(PollerAuth::Authorized).0)
    }

    fn tracker_lost(endpoints: &[PollEndpoint]) -> AuthTracker {
        let mut tracker = AuthTracker::new(watch::channel(PollerAuth::AuthRequired).0);
        for endpoint in endpoints {
            *tracker.flag(*endpoint) = true;
        }
        tracker
    }

    fn published(tracker: &AuthTracker) -> PollerAuth {
        *tracker.tx.borrow()
    }

    const AUTH_LOST_MESSAGE: &str = "kick poller authorization lost, sign in to Kick again";

    const CHANNELS_ROUTE: &str = "/channels";
    const REDEMPTIONS_ROUTE: &str = "/channels/rewards/redemptions";

    async fn mount_status(server: &MockServer, route: &'static str, status: reqwest::StatusCode) {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(status.as_u16()))
            .mount(server)
            .await;
    }

    async fn mount_live_channel(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path(CHANNELS_ROUTE))
            .respond_with(ResponseTemplate::new(200).set_body_json(channel_body(true, "T", 1, "C")))
            .mount(server)
            .await;
    }

    async fn mount_no_redemptions(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path(REDEMPTIONS_ROUTE))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": [] })),
            )
            .mount(server)
            .await;
    }

    fn snapshot(
        is_live: bool,
        title: &str,
        category_id: u64,
        category_name: &str,
    ) -> ChannelSnapshot {
        ChannelSnapshot {
            broadcaster_user_id: 0,
            slug: String::new(),
            is_live,
            stream_title: title.to_owned(),
            category_id,
            category_name: category_name.to_owned(),
            viewer_count: 0,
            started_at: String::new(),
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
        KickChannel::new(
            &forge_platform_core::PlatformEndpoints::default(),
            Arc::new(GrantLimiter),
        )
        .with_api_base(server.uri())
    }

    fn rewards_on(server: &MockServer) -> KickRewards {
        KickRewards::new(
            &forge_platform_core::PlatformEndpoints::default(),
            Arc::new(GrantLimiter),
        )
        .with_api_base(server.uri())
    }

    fn channel_body(
        is_live: bool,
        title: &str,
        category_id: u64,
        category_name: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "data": [{
                "stream_title": title,
                "category": { "id": category_id, "name": category_name },
                "stream": { "is_live": is_live }
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

        poll_channel(
            &channel,
            &ok_token(),
            &tx,
            &viewer_sender(),
            &mut tracker(),
            &mut last,
        )
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

        poll_channel(
            &channel,
            &ok_token(),
            &tx,
            &viewer_sender(),
            &mut tracker(),
            &mut last,
        )
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

        poll_channel(
            &channel,
            &ok_token(),
            &tx,
            &viewer_sender(),
            &mut tracker(),
            &mut last,
        )
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

        poll_channel(
            &channel,
            &ok_token(),
            &tx,
            &viewer_sender(),
            &mut tracker(),
            &mut last,
        )
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

    fn auth_loss_cases() -> [(&'static str, TokenSource, Option<reqwest::StatusCode>); 4] {
        [
            ("token rejected", failing_token(auth_rejected), None),
            ("re-auth required", failing_token(reauth_required), None),
            (
                "API 401",
                ok_token(),
                Some(reqwest::StatusCode::UNAUTHORIZED),
            ),
            ("API 403", ok_token(), Some(reqwest::StatusCode::FORBIDDEN)),
        ]
    }

    #[tokio::test]
    async fn poll_channel_auth_loss_flags_auth_required_and_clears_the_viewer_report() {
        for (case, token_source, api_status) in auth_loss_cases() {
            let server = MockServer::start().await;
            if let Some(status) = api_status {
                mount_status(&server, CHANNELS_ROUTE, status).await;
            }
            let channel = channel_on(&server);
            let (tx, mut rx) = mpsc::channel(4);
            let viewer_tx = live_viewer_sender();
            let mut auth = tracker();
            let mut last = None;

            let result = poll_channel(
                &channel,
                &token_source,
                &tx,
                &viewer_tx,
                &mut auth,
                &mut last,
            )
            .await;

            assert!(result.is_ok(), "{case}: auth loss must not stop the loop");
            assert_eq!(published(&auth), PollerAuth::AuthRequired, "{case}");
            assert_eq!(*viewer_tx.borrow(), ViewerReport::Absent, "{case}");
            assert!(last.is_none(), "{case}: no snapshot without a channel read");
            assert!(rx.try_recv().is_err(), "{case}: nothing emits");
        }
    }

    #[tokio::test]
    async fn poll_redemptions_auth_loss_flags_auth_required_and_emits_nothing() {
        for (case, token_source, api_status) in auth_loss_cases() {
            let server = MockServer::start().await;
            if let Some(status) = api_status {
                mount_status(&server, REDEMPTIONS_ROUTE, status).await;
            }
            let rewards = rewards_on(&server);
            let (tx, mut rx) = mpsc::channel(4);
            let mut auth = tracker();
            let mut seen = DedupSet::unbounded();
            let mut seeded = false;

            let result = poll_redemptions(
                &rewards,
                &token_source,
                &tx,
                &mut auth,
                &mut seen,
                &mut seeded,
            )
            .await;

            assert!(result.is_ok(), "{case}: auth loss must not stop the loop");
            assert_eq!(published(&auth), PollerAuth::AuthRequired, "{case}");
            assert!(!seeded, "{case}: a failed poll must not count as the seed");
            assert!(rx.try_recv().is_err(), "{case}: nothing emits");
        }
    }

    #[tokio::test]
    async fn a_successful_poll_after_auth_loss_restores_authorized() {
        let server = MockServer::start().await;
        mount_live_channel(&server).await;
        mount_no_redemptions(&server).await;
        let (tx, _rx) = mpsc::channel(4);

        let mut channel_auth = tracker_lost(&[PollEndpoint::Channel]);
        poll_channel(
            &channel_on(&server),
            &ok_token(),
            &tx,
            &viewer_sender(),
            &mut channel_auth,
            &mut None,
        )
        .await
        .unwrap();

        let mut redemption_auth = tracker_lost(&[PollEndpoint::Redemptions]);
        poll_redemptions(
            &rewards_on(&server),
            &ok_token(),
            &tx,
            &mut redemption_auth,
            &mut DedupSet::unbounded(),
            &mut false,
        )
        .await
        .unwrap();

        assert_eq!(published(&channel_auth), PollerAuth::Authorized);
        assert_eq!(published(&redemption_auth), PollerAuth::Authorized);
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

        poll_redemptions(
            &rewards,
            &ok_token(),
            &tx,
            &mut tracker(),
            &mut seen,
            &mut seeded,
        )
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

        poll_redemptions(
            &rewards,
            &ok_token(),
            &tx,
            &mut tracker(),
            &mut seen,
            &mut seeded,
        )
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

        poll_redemptions(
            &rewards,
            &ok_token(),
            &tx,
            &mut tracker(),
            &mut seen,
            &mut seeded,
        )
        .await
        .unwrap();
        assert!(rx.try_recv().is_err(), "the seeding poll must emit nothing");

        poll_redemptions(
            &rewards,
            &ok_token(),
            &tx,
            &mut tracker(),
            &mut seen,
            &mut seeded,
        )
        .await
        .unwrap();
        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind, REWARD_REDEEMED_KIND);
        assert_eq!(event.payload["id"], "c");
        assert!(
            rx.try_recv().is_err(),
            "still-pending a and b must never be re-emitted"
        );

        poll_redemptions(
            &rewards,
            &ok_token(),
            &tx,
            &mut tracker(),
            &mut seen,
            &mut seeded,
        )
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

    const BODY_SENTINEL: &str = "kick_poll_body_sentinel";

    #[test]
    fn poll_failure_drops_the_response_body_and_keeps_the_diagnostic() {
        let cases: [(PlatformError, &str); 4] = [
            (
                PlatformError::Http {
                    status: 500,
                    body: format!("<html>{BODY_SENTINEL}</html>"),
                },
                "HTTP 500",
            ),
            (
                PlatformError::Network {
                    reason: "error sending request".to_owned(),
                },
                "error sending request",
            ),
            (
                PlatformError::Auth {
                    reason: "channel token rejected (401)".to_owned(),
                },
                "channel token rejected (401)",
            ),
            (
                PlatformError::RateLimited {
                    retry_after_secs: 30,
                },
                "30",
            ),
        ];

        for (error, must_keep) in cases {
            let rendered = poll_failure(&error);
            assert!(
                !rendered.contains(BODY_SENTINEL),
                "response body leaked into {rendered:?}"
            );
            assert!(
                rendered.contains(must_keep),
                "lost diagnostic {must_keep:?} in {rendered:?}"
            );
        }
    }

    #[test]
    fn a_failing_channel_poll_logs_no_byte_of_the_response_body() {
        let (result, lines) = crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/channels"))
                .respond_with(
                    ResponseTemplate::new(500).set_body_string(format!("<html>{BODY_SENTINEL}")),
                )
                .mount(&server)
                .await;

            let channel = channel_on(&server);
            let (tx, _rx) = mpsc::channel(4);
            let mut last = None;
            poll_channel(
                &channel,
                &ok_token(),
                &tx,
                &viewer_sender(),
                &mut tracker(),
                &mut last,
            )
            .await
        });

        assert!(result.is_ok(), "a failed poll must not stop the loop");
        let forge_lines = crate::log_capture::forge_lines(&lines);
        assert!(
            forge_lines
                .iter()
                .any(|line| line.message() == "kick channel poll failed"),
            "the failure must still be reported: {forge_lines:?}"
        );
        for line in forge_lines {
            assert!(
                !line.mentions(BODY_SENTINEL),
                "response body reached a {} line: {:?}",
                line.level,
                line.fields
            );
        }
    }

    fn auth_lost_warnings(lines: &[crate::log_capture::CapturedLine]) -> usize {
        crate::log_capture::forge_lines(lines)
            .iter()
            .filter(|line| {
                line.level == tracing::Level::WARN && line.message() == AUTH_LOST_MESSAGE
            })
            .count()
    }

    #[test]
    fn each_endpoint_losing_authorization_warns_once_across_repeated_failing_polls() {
        let ((auth, uri), lines) =
            crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
                let server = MockServer::start().await;
                mount_status(&server, CHANNELS_ROUTE, reqwest::StatusCode::UNAUTHORIZED).await;
                mount_status(
                    &server,
                    REDEMPTIONS_ROUTE,
                    reqwest::StatusCode::UNAUTHORIZED,
                )
                .await;
                let channel = channel_on(&server);
                let rewards = rewards_on(&server);
                let (tx, _rx) = mpsc::channel(4);
                let viewer_tx = viewer_sender();
                let mut auth = tracker();
                let mut last = None;
                let mut seen = DedupSet::unbounded();
                let mut seeded = false;
                for _ in 0..3 {
                    poll_channel(&channel, &ok_token(), &tx, &viewer_tx, &mut auth, &mut last)
                        .await
                        .unwrap();
                    poll_redemptions(
                        &rewards,
                        &ok_token(),
                        &tx,
                        &mut auth,
                        &mut seen,
                        &mut seeded,
                    )
                    .await
                    .unwrap();
                }
                (published(&auth), server.uri())
            });

        assert_eq!(auth, PollerAuth::AuthRequired);
        let mut endpoints = auth_lost_endpoints(&lines);
        endpoints.sort();
        assert_eq!(endpoints, vec!["channel", "redemptions"], "{lines:?}");
        for line in crate::log_capture::forge_lines(&lines) {
            assert!(
                !line.mentions(TOKEN_SENTINEL) && !line.mentions(&uri),
                "token or URL reached a {} line: {:?}",
                line.level,
                line.fields
            );
        }
    }

    fn auth_lost_endpoints(lines: &[crate::log_capture::CapturedLine]) -> Vec<String> {
        crate::log_capture::forge_lines(lines)
            .iter()
            .filter(|line| {
                line.level == tracing::Level::WARN && line.message() == AUTH_LOST_MESSAGE
            })
            .filter_map(|line| line.field("endpoint").map(str::to_owned))
            .collect()
    }

    async fn run_cycles(
        channel_server: &MockServer,
        redemptions_server: &MockServer,
        auth: &mut AuthTracker,
        cycles: usize,
    ) {
        let (tx, _rx) = mpsc::channel(4);
        let viewer_tx = viewer_sender();
        let mut last = None;
        let mut seen = DedupSet::unbounded();
        let mut seeded = false;
        for _ in 0..cycles {
            poll_channel(
                &channel_on(channel_server),
                &ok_token(),
                &tx,
                &viewer_tx,
                auth,
                &mut last,
            )
            .await
            .unwrap();
            poll_redemptions(
                &rewards_on(redemptions_server),
                &ok_token(),
                &tx,
                auth,
                &mut seen,
                &mut seeded,
            )
            .await
            .unwrap();
        }
    }

    #[test]
    fn one_healthy_endpoint_does_not_mask_the_other_losing_authorization() {
        let ((states, tracker_state), lines) =
            crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
                let healthy = MockServer::start().await;
                mount_live_channel(&healthy).await;
                let rejected = MockServer::start().await;
                mount_status(&rejected, REDEMPTIONS_ROUTE, reqwest::StatusCode::FORBIDDEN).await;
                let mut auth = tracker();
                let mut observed = Vec::new();
                for _ in 0..3 {
                    run_cycles(&healthy, &rejected, &mut auth, 1).await;
                    observed.push(published(&auth));
                }
                (observed, auth.redemptions_lost && !auth.channel_lost)
            });

        assert_eq!(states, vec![PollerAuth::AuthRequired; 3]);
        assert!(tracker_state);
        assert_eq!(
            auth_lost_endpoints(&lines),
            vec!["redemptions"],
            "{lines:?}"
        );
    }

    #[test]
    fn auth_stays_required_until_every_lost_endpoint_recovers() {
        let (states, _) = crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
            let healthy = MockServer::start().await;
            mount_live_channel(&healthy).await;
            mount_no_redemptions(&healthy).await;
            let rejected = MockServer::start().await;
            mount_status(&rejected, CHANNELS_ROUTE, reqwest::StatusCode::UNAUTHORIZED).await;
            mount_status(
                &rejected,
                REDEMPTIONS_ROUTE,
                reqwest::StatusCode::UNAUTHORIZED,
            )
            .await;
            let channel_back = MockServer::start().await;
            mount_live_channel(&channel_back).await;
            mount_status(
                &channel_back,
                REDEMPTIONS_ROUTE,
                reqwest::StatusCode::UNAUTHORIZED,
            )
            .await;

            let mut auth = tracker();
            let mut observed = Vec::new();
            for (channel_server, redemptions_server) in [
                (&rejected, &rejected),
                (&channel_back, &channel_back),
                (&healthy, &healthy),
            ] {
                run_cycles(channel_server, redemptions_server, &mut auth, 1).await;
                observed.push(published(&auth));
            }
            observed
        });

        assert_eq!(
            states,
            vec![
                PollerAuth::AuthRequired,
                PollerAuth::AuthRequired,
                PollerAuth::Authorized
            ]
        );
    }

    #[test]
    fn auth_loss_after_recovery_warns_again() {
        let (_, lines) = crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
            let rejected = MockServer::start().await;
            mount_status(&rejected, CHANNELS_ROUTE, reqwest::StatusCode::UNAUTHORIZED).await;
            let healthy = MockServer::start().await;
            mount_live_channel(&healthy).await;
            let (tx, _rx) = mpsc::channel(4);
            let viewer_tx = viewer_sender();
            let mut auth = tracker();
            for server in [&rejected, &healthy, &rejected] {
                poll_channel(
                    &channel_on(server),
                    &ok_token(),
                    &tx,
                    &viewer_tx,
                    &mut auth,
                    &mut None,
                )
                .await
                .unwrap();
            }
        });

        assert_eq!(auth_lost_warnings(&lines), 2, "{lines:?}");
    }

    #[test]
    fn recovering_from_auth_required_logs_no_warning() {
        let (auth, lines) = crate::log_capture::capture_blocking(tracing::Level::TRACE, async {
            let server = MockServer::start().await;
            mount_live_channel(&server).await;
            let (tx, _rx) = mpsc::channel(4);
            let mut auth = tracker_lost(&[PollEndpoint::Channel]);
            poll_channel(
                &channel_on(&server),
                &ok_token(),
                &tx,
                &viewer_sender(),
                &mut auth,
                &mut None,
            )
            .await
            .unwrap();
            published(&auth)
        });

        assert_eq!(auth, PollerAuth::Authorized);
        assert!(
            crate::log_capture::forge_lines(&lines)
                .iter()
                .all(|line| line.level != tracing::Level::WARN),
            "{lines:?}"
        );
    }

    #[test]
    fn non_auth_token_failure_warns_without_flagging_auth_or_clearing_viewers() {
        for (case, make) in [
            ("storage", storage_unreadable as fn() -> PlatformError),
            ("network", network_down),
        ] {
            let ((auth, viewers), lines) =
                crate::log_capture::capture_blocking(tracing::Level::TRACE, async move {
                    let server = MockServer::start().await;
                    let (tx, _rx) = mpsc::channel(4);
                    let viewer_tx = live_viewer_sender();
                    let mut auth = tracker();
                    poll_channel(
                        &channel_on(&server),
                        &failing_token(make),
                        &tx,
                        &viewer_tx,
                        &mut auth,
                        &mut None,
                    )
                    .await
                    .unwrap();
                    (published(&auth), *viewer_tx.borrow())
                });

            assert_eq!(auth, PollerAuth::Authorized, "{case}");
            assert_eq!(viewers, ViewerReport::Live { count: 5 }, "{case}");
            assert_eq!(auth_lost_warnings(&lines), 0, "{case}");
            assert!(
                crate::log_capture::forge_lines(&lines).iter().any(|line| {
                    line.level == tracing::Level::WARN && line.message() == "kick token unavailable"
                }),
                "{case}: {lines:?}"
            );
        }
    }

    #[tokio::test]
    async fn stopping_the_poller_ends_its_task_and_closes_both_of_its_outputs() {
        let (event_tx, mut event_rx) = mpsc::channel(8);
        let (source, poller) = spawn_kick_poller(
            Arc::new(KickChannel::new(
                &forge_platform_core::PlatformEndpoints::default(),
                Arc::new(GrantLimiter),
            )),
            Arc::new(KickRewards::new(
                &forge_platform_core::PlatformEndpoints::default(),
                Arc::new(GrantLimiter),
            )),
            err_token(),
            event_tx,
        );
        let mut reports = source.subscribe();
        assert!(
            tokio::time::timeout(StdDuration::from_millis(50), event_rx.recv())
                .await
                .is_err(),
            "a running poller keeps its event channel open"
        );

        poller.stop();

        let closed = tokio::time::timeout(StdDuration::from_secs(2), async {
            while reports.changed().await.is_ok() {}
            event_rx.recv().await
        })
        .await;
        assert!(
            matches!(closed, Ok(None)),
            "the poller task outlived stop()"
        );
    }
}
