use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::{Mutex, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{BroadcastStream, WatchStream};

use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState,
    ContentList, ContentListItem, DetailSection, HeaderAction, HealthDelta, HealthMetric,
    HealthStream, HealthValue, ListFooter, LiveViewerSource, QuickAction, QuickActions,
    RateLimiter, SectionIcon, TokenBucketRateLimiter, TrailingToken, ViewerReport,
    ViewerReportStream,
};
use std::collections::BTreeMap;

use forge_events::EventPublisher;
use forge_storage::CredentialsRepo;
use forge_types::{SubActionStep, Variant};

use crate::TWITCH_BROADCASTER_SCOPES;
use crate::chat::{ChatConnectionState, TwitchChat, TwitchChatHandle};
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{
    HelixHttpTransport, HelixMethod, HelixRequest, HelixTokenRefresher, HelixTokenSource,
    HelixTransport,
};
use crate::subscriptions::{SubStatus, SubscriptionTracker};

/// Poll cadence for the `GET /helix/streams` viewer-count metric. Viewer counts
/// do not need sub-minute freshness, and a 60s cadence keeps this bundle-local
/// poll a negligible fraction of the Helix budget regardless of how busy the
/// sub-action/chat-send transports (which draw on their own shared bucket) are.
const VIEWER_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Twitch's documented Helix budget (per client-id): 800 requests / 60s. This
/// bundle owns a dedicated bucket sized to match it purely to bound the viewer
/// poll's own request rate - it is NOT the same bucket instance the sub-action
/// and chat-send transports draw on (those are wired up in the app crate at
/// boot), so it cannot report true cross-transport budget usage. See the
/// "API Calls" health metric below for the consequence.
const HELIX_BUDGET_CAPACITY: u32 = 800;
const HELIX_BUDGET_WINDOW: Duration = Duration::from_secs(60);

/// Latest outcome of the periodic viewer-count poll. `Unknown` before the
/// first poll completes (drives the `-` placeholder); `Offline` when Helix
/// reports no active stream for the broadcaster; `Live` carries the last
/// observed `viewer_count`.
#[derive(Debug, Clone, Copy, Default)]
enum ViewerPollState {
    #[default]
    Unknown,
    Offline,
    Live(u64),
}

impl ViewerPollState {
    fn as_report(self) -> ViewerReport {
        match self {
            ViewerPollState::Live(count) => ViewerReport::Live { count },
            ViewerPollState::Unknown | ViewerPollState::Offline => ViewerReport::Absent,
        }
    }
}

/// Bridges the bundle's viewer poll into the runtime live-viewer aggregate.
/// Holds only a `watch` receiver, so it does not keep the bundle alive: once the
/// bundle's sender drops the report stream ends, dropping the aggregate slot.
struct TwitchViewerSource {
    reports: watch::Receiver<ViewerReport>,
}

impl LiveViewerSource for TwitchViewerSource {
    fn viewer_reports(&self) -> ViewerReportStream {
        Box::pin(WatchStream::new(self.reports.clone()))
    }
}

/// Inputs required to (re)build a chat session for the bundle's broadcaster.
pub struct ChatSessionConfig {
    pub client_id: String,
    pub broadcaster_id: String,
    pub user_id: String,
}

pub struct TwitchIntegrationBundle {
    id: BuiltinId,
    login: Option<String>,
    state_rx: watch::Receiver<ChatConnectionState>,
    health_tx: broadcast::Sender<HealthDelta>,
    tracker: SubscriptionTracker,
    config: ChatSessionConfig,
    bus: Arc<dyn EventPublisher>,
    creds: Arc<dyn CredentialsRepo>,
    // Parked here so the &self-async control verbs can take() the consume-on-shutdown
    // handle without racing a concurrent disconnect/reconnect.
    handle: Mutex<Option<TwitchChatHandle>>,
    viewer_state: std::sync::RwLock<ViewerPollState>,
    viewer_report_tx: watch::Sender<ViewerReport>,
}

impl TwitchIntegrationBundle {
    pub fn new(
        login: Option<String>,
        config: ChatSessionConfig,
        bus: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
        tracker: SubscriptionTracker,
        handle: TwitchChatHandle,
    ) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(16);
        let (viewer_report_tx, _) = watch::channel(ViewerReport::Absent);
        let state_rx = handle.state_receiver();
        let viewer_transport = Self::build_viewer_transport(&config, &bus, &creds);
        let bundle = Arc::new(Self {
            id: BuiltinId::new("twitch"),
            login,
            state_rx,
            health_tx,
            tracker,
            config,
            bus,
            creds,
            handle: Mutex::new(Some(handle)),
            viewer_state: std::sync::RwLock::new(ViewerPollState::default()),
            viewer_report_tx,
        });
        Self::spawn_health_bridge(&bundle);
        Self::spawn_viewer_poll(&bundle, viewer_transport);
        bundle
    }

    /// Builds a Helix transport dedicated to the viewer-count poll, backed by
    /// its own rate-limit bucket (see `HELIX_BUDGET_CAPACITY`/`_WINDOW`) and
    /// the same proactive-refresh token source `refresh_token()` uses.
    fn build_viewer_transport(
        config: &ChatSessionConfig,
        bus: &Arc<dyn EventPublisher>,
        creds: &Arc<dyn CredentialsRepo>,
    ) -> Arc<dyn HelixTransport> {
        let manager = Arc::new(TwitchCredentialsManager::new(
            Arc::clone(creds),
            config.client_id.clone(),
        ));
        let rate_limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
            HELIX_BUDGET_CAPACITY,
            HELIX_BUDGET_WINDOW,
        ));
        Arc::new(
            HelixHttpTransport::new(
                rate_limiter,
                Arc::clone(bus),
                config.client_id.clone(),
                Arc::clone(&manager) as Arc<dyn HelixTokenSource>,
            )
            .with_refresher(Arc::clone(&manager) as Arc<dyn HelixTokenRefresher>),
        )
    }

    /// Bridges chat-connection and EventSub-subscription state changes onto
    /// `health_tx` so `BuiltinHealth::stream()` (subscribed by the UI) is fed
    /// live deltas instead of only ever answering the one-shot `metrics()`
    /// snapshot taken at detail-screen construction. Ends on its own once the
    /// underlying chat session's `watch::Sender` drops (session shutdown).
    fn spawn_health_bridge(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        let mut state_rx = bundle.state_rx.clone();
        tokio::spawn(async move {
            while state_rx.changed().await.is_ok() {
                let chat_delta = HealthDelta {
                    index: 0,
                    new_value: bundle.chat_health_value(),
                };
                let _ = bundle.health_tx.send(chat_delta);

                let eventsub_delta = HealthDelta {
                    index: 1,
                    new_value: bundle.eventsub_health_value(),
                };
                let _ = bundle.health_tx.send(eventsub_delta);
            }
        });
    }

    /// Periodically polls `GET /helix/streams` for the broadcaster's current
    /// viewer count and bridges the result onto `health_tx` (index 2, matching
    /// the "Viewers" row in `metrics()`). Runs for the bundle's lifetime -
    /// independent of the chat/IRC connection state, since viewer count is a
    /// property of the stream, not the chat session. A failed poll (network,
    /// rate limit, expired credentials) is skipped silently and retried on the
    /// next tick; the last known value is kept on screen rather than flashing
    /// back to the placeholder.
    fn spawn_viewer_poll(bundle: &Arc<Self>, transport: Arc<dyn HelixTransport>) {
        let bundle = Arc::clone(bundle);
        let broadcaster_id = bundle.config.broadcaster_id.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(VIEWER_POLL_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let request = HelixRequest::new(HelixMethod::Get, "/helix/streams")
                    .query("user_id", broadcaster_id.clone());
                let Ok(body) = transport.execute(request).await else {
                    continue;
                };
                let new_state = match extract_viewer_count(&body) {
                    Some(count) => ViewerPollState::Live(count),
                    None => ViewerPollState::Offline,
                };
                if let Ok(mut guard) = bundle.viewer_state.write() {
                    *guard = new_state;
                }
                let _ = bundle.viewer_report_tx.send(new_state.as_report());
                let delta = HealthDelta {
                    index: 2,
                    new_value: bundle.viewers_health_value(),
                };
                let _ = bundle.health_tx.send(delta);
            }
        });
    }

    pub(crate) fn spawn_chat(&self, token: forge_types::OAuthToken) -> TwitchChatHandle {
        TwitchChat::new(
            token,
            self.config.client_id.clone(),
            self.config.broadcaster_id.clone(),
            self.config.user_id.clone(),
            Arc::clone(&self.bus),
            self.tracker.clone(),
        )
        .start()
    }

    pub(crate) fn credentials(&self) -> &Arc<dyn CredentialsRepo> {
        &self.creds
    }

    pub(crate) fn config(&self) -> &ChatSessionConfig {
        &self.config
    }

    pub(crate) fn handle_slot(&self) -> &Mutex<Option<TwitchChatHandle>> {
        &self.handle
    }

    pub fn viewer_source(&self) -> Box<dyn LiveViewerSource> {
        Box::new(TwitchViewerSource {
            reports: self.viewer_report_tx.subscribe(),
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        login: Option<String>,
        state_rx: watch::Receiver<ChatConnectionState>,
        tracker: SubscriptionTracker,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(16);
        let (viewer_report_tx, _) = watch::channel(ViewerReport::Absent);
        Arc::new(Self {
            id: BuiltinId::new("twitch"),
            login,
            state_rx,
            health_tx,
            tracker,
            config: ChatSessionConfig {
                client_id: "test-client".to_owned(),
                broadcaster_id: "1".to_owned(),
                user_id: "1".to_owned(),
            },
            bus: Arc::new(crate::event_channel::PlatformEventChannel::new()),
            creds,
            handle: Mutex::new(None),
            viewer_state: std::sync::RwLock::new(ViewerPollState::default()),
            viewer_report_tx,
        })
    }

    fn chat_connection_state(&self) -> ChatConnectionState {
        *self.state_rx.borrow()
    }

    fn is_chat_connected(&self) -> bool {
        self.chat_connection_state() == ChatConnectionState::Connected
    }

    fn chat_label(&self) -> String {
        let state = self.chat_connection_state();
        if state == ChatConnectionState::Connected
            && let Some(login) = &self.login
        {
            return format!("Joined #{login}");
        }
        state.to_connection_state().label().to_owned()
    }

    fn active_sub_count(&self) -> usize {
        let records = self.tracker.read().unwrap_or_else(|p| p.into_inner());
        records
            .iter()
            .filter(|r| matches!(r.status, SubStatus::Active))
            .count()
    }

    fn chat_health_value(&self) -> HealthValue {
        HealthValue::Status {
            label: self.chat_label(),
            active: self.is_chat_connected(),
            detail: self.login.as_ref().map(|l| format!("#{l}")),
        }
    }

    fn eventsub_health_value(&self) -> HealthValue {
        HealthValue::Text {
            primary: format!("{} subs", self.active_sub_count()),
            secondary: Some("WebSocket".to_owned()),
        }
    }

    fn viewers_health_value(&self) -> HealthValue {
        let state = *self.viewer_state.read().unwrap_or_else(|p| p.into_inner());
        match state {
            ViewerPollState::Unknown => HealthValue::Text {
                primary: "\u{2014}".to_owned(),
                secondary: None,
            },
            ViewerPollState::Offline => HealthValue::Text {
                primary: "0".to_owned(),
                secondary: Some("offline".to_owned()),
            },
            ViewerPollState::Live(count) => HealthValue::Text {
                primary: count.to_string(),
                secondary: Some("live".to_owned()),
            },
        }
    }
}

/// Pulls `data[0].viewer_count` out of a `GET /helix/streams` response body.
/// An empty `data` array means the broadcaster is not currently live.
fn extract_viewer_count(body: &serde_json::Value) -> Option<u64> {
    body.get("data")?
        .as_array()?
        .first()?
        .get("viewer_count")?
        .as_u64()
}

impl BuiltinStatus for TwitchIntegrationBundle {
    fn id(&self) -> &BuiltinId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "Twitch"
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        self.chat_connection_state().to_connection_state()
    }

    fn uptime(&self) -> Option<Duration> {
        None
    }

    fn endpoint(&self) -> Option<&str> {
        Some("Connected via device code")
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![HeaderAction::RefreshToken, HeaderAction::Disconnect]
    }
}

impl BuiltinHealth for TwitchIntegrationBundle {
    fn metrics(&self) -> [HealthMetric; 4] {
        [
            HealthMetric {
                label: "Chat IRC".to_owned(),
                value: self.chat_health_value(),
            },
            HealthMetric {
                label: "EventSub".to_owned(),
                value: self.eventsub_health_value(),
            },
            HealthMetric {
                label: "Viewers".to_owned(),
                value: self.viewers_health_value(),
            },
            HealthMetric {
                label: "API Calls".to_owned(),
                // Deferred: this bundle's Helix calls (the viewer poll above) run
                // on their own dedicated rate-limit bucket, not the one the
                // sub-action/chat-send transports share (wired up separately at
                // boot). Reporting a Ratio from the local bucket would show a
                // number disconnected from the app's real Helix call volume, so
                // this stays a placeholder until the two share one bucket.
                value: HealthValue::Text {
                    primary: "\u{2014}".to_owned(),
                    secondary: None,
                },
            },
        ]
    }

    fn stream(&self) -> HealthStream {
        let rx = self.health_tx.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
    }
}

impl BuiltinContent for TwitchIntegrationBundle {
    fn sections(&self) -> Vec<DetailSection> {
        let scope_items: Vec<ContentListItem> = TWITCH_BROADCASTER_SCOPES
            .iter()
            .map(|s| ContentListItem {
                icon: SectionIcon::new("check"),
                name: (*s).to_owned(),
                monospace_name: true,
                active: true,
                active_label: None,
                trailing: vec![],
                enabled: true,
            })
            .collect();

        let scopes_count = format!("{}", scope_items.len());
        let scopes_list = ContentList {
            title: "OAuth scopes".to_owned(),
            icon: SectionIcon::new("key"),
            count_label: Some(scopes_count),
            items: scope_items,
            footer: Some(ListFooter {
                cta_label: Some("Request more scopes".to_owned()),
                trailing_label: None,
            }),
        };

        let records = self.tracker.read().unwrap_or_else(|p| p.into_inner());
        let active_count = records
            .iter()
            .filter(|r| matches!(r.status, SubStatus::Active))
            .count();

        let sub_items: Vec<ContentListItem> = records
            .iter()
            .map(|r| {
                let (active, status_label) = match &r.status {
                    SubStatus::Active => (true, "active".to_owned()),
                    SubStatus::Pending => (false, "pending".to_owned()),
                    SubStatus::Failed(_) => (false, "failed".to_owned()),
                };
                ContentListItem {
                    icon: SectionIcon::new("circle"),
                    name: r.kind.clone(),
                    monospace_name: true,
                    active,
                    active_label: None,
                    trailing: vec![
                        TrailingToken::Label(format!("v{}", r.version)),
                        TrailingToken::Label(status_label),
                    ],
                    enabled: true,
                }
            })
            .collect();

        let eventsub_list = ContentList {
            title: "EventSub subscriptions".to_owned(),
            icon: SectionIcon::new("rss"),
            count_label: Some(format!("{active_count} active")),
            items: sub_items,
            footer: Some(ListFooter {
                cta_label: Some("Subscribe to event".to_owned()),
                trailing_label: Some("subscribing on session start".to_owned()),
            }),
        };

        vec![DetailSection::TwoColumnLists {
            left: scopes_list,
            right: eventsub_list,
        }]
    }
}

impl QuickActions for TwitchIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.is_chat_connected();
        vec![
            QuickAction {
                label: "Send chat message".to_owned(),
                icon: SectionIcon::new("send"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "twitch.chat.send_message".to_owned(),
                    config: BTreeMap::from([
                        ("message".to_owned(), Variant::String(String::new())),
                        ("target".to_owned(), Variant::String("twitch".to_owned())),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Run shoutout".to_owned(),
                icon: SectionIcon::new("flag"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "twitch.channel.send_shoutout".to_owned(),
                    config: BTreeMap::from([(
                        "to_broadcaster_login".to_owned(),
                        Variant::String(String::new()),
                    )]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Run commercial".to_owned(),
                icon: SectionIcon::new("clock"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "twitch.channel.run_ad".to_owned(),
                    config: BTreeMap::from([(
                        "duration_seconds".to_owned(),
                        Variant::String("90".to_owned()),
                    )]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Update title".to_owned(),
                icon: SectionIcon::new("edit"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "twitch.channel.update_title".to_owned(),
                    config: BTreeMap::from([("title".to_owned(), Variant::String(String::new()))]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use async_trait::async_trait;
    use forge_platform_core::{BuiltinContent, BuiltinHealth, BuiltinStatus};
    use forge_storage::{CredentialId, StorageError};
    use time::OffsetDateTime;
    use tokio::sync::watch;

    use super::*;
    use crate::subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};

    struct NullCreds;

    #[async_trait]
    impl CredentialsRepo for NullCreds {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(None)
        }
        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(false)
        }
        async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
            Ok(Vec::new())
        }
        async fn last_refresh(
            &self,
            _: &CredentialId,
        ) -> Result<Option<OffsetDateTime>, StorageError> {
            Ok(None)
        }
        async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
            Ok(())
        }
    }

    fn make_bundle(state: ChatConnectionState) -> Arc<TwitchIntegrationBundle> {
        make_bundle_with_tracker(state, SubscriptionTracker::default())
    }

    fn make_bundle_with_tracker(
        state: ChatConnectionState,
        tracker: SubscriptionTracker,
    ) -> Arc<TwitchIntegrationBundle> {
        let (tx, rx) = watch::channel(state);
        let _ = tx;
        TwitchIntegrationBundle::for_test(
            Some("streamer".to_owned()),
            rx,
            tracker,
            Arc::new(NullCreds),
        )
    }

    #[test]
    fn status_connection_maps_chat_state() {
        let cases = [
            (ChatConnectionState::Connected, ConnectionState::Connected),
            (ChatConnectionState::Connecting, ConnectionState::Connecting),
            (
                ChatConnectionState::Reconnecting { attempt: 1 },
                ConnectionState::Reconnecting,
            ),
            (
                ChatConnectionState::Disconnected,
                ConnectionState::Disconnected,
            ),
        ];
        for (chat_state, expected) in cases {
            let b = make_bundle(chat_state);
            let status: &dyn BuiltinStatus = b.as_ref();
            assert_eq!(status.connection(), expected, "failed for {chat_state:?}");
        }
    }

    #[test]
    fn status_header_actions_contain_refresh_and_disconnect() {
        let b = make_bundle(ChatConnectionState::Connected);
        let status: &dyn BuiltinStatus = b.as_ref();
        let actions = status.header_actions();
        assert!(actions.contains(&HeaderAction::RefreshToken));
        assert!(actions.contains(&HeaderAction::Disconnect));
    }

    #[test]
    fn health_metrics_returns_four_with_correct_labels() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        assert_eq!(metrics.len(), 4);
        assert_eq!(metrics[0].label, "Chat IRC");
        assert_eq!(metrics[1].label, "EventSub");
        assert_eq!(metrics[2].label, "Viewers");
        assert_eq!(metrics[3].label, "API Calls");
    }

    #[test]
    fn chat_irc_metric_active_when_connected() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Status { active, label, .. } = &metrics[0].value else {
            panic!("expected Status variant");
        };
        assert!(*active);
        assert_eq!(label, "Joined #streamer");
    }

    #[test]
    fn chat_irc_metric_inactive_when_disconnected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Status { active, label, .. } = &metrics[0].value else {
            panic!("expected Status variant");
        };
        assert!(!*active);
        assert_eq!(label, "Disconnected");
    }

    #[test]
    fn eventsub_metric_shows_active_count_from_tracker() {
        let tracker = SubscriptionTracker::default();
        {
            let mut records = tracker.write().unwrap();
            records.push(SubscriptionRecord {
                kind: "channel.chat.message".to_owned(),
                version: "1".to_owned(),
                status: SubStatus::Active,
                subscription_id: Some("sub-1".to_owned()),
            });
            records.push(SubscriptionRecord {
                kind: "channel.subscribe".to_owned(),
                version: "1".to_owned(),
                status: SubStatus::Pending,
                subscription_id: None,
            });
        }
        let b = make_bundle_with_tracker(ChatConnectionState::Connected, tracker);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Text { primary, .. } = &metrics[1].value else {
            panic!("expected Text variant for EventSub metric");
        };
        assert_eq!(primary, "1 subs");
    }

    #[tokio::test]
    async fn health_stream_is_subscribable() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let items: Vec<_> = health.stream().take(0).collect().await;
        assert!(items.is_empty());
    }

    #[test]
    fn content_sections_returns_one_two_column() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        assert_eq!(sections.len(), 1);
        assert!(matches!(&sections[0], DetailSection::TwoColumnLists { .. }));
    }

    #[test]
    fn content_scopes_section_has_all_broadcaster_scopes() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        let DetailSection::TwoColumnLists { left, .. } = &sections[0] else {
            panic!("expected TwoColumnLists in first section");
        };
        assert_eq!(left.items.len(), TWITCH_BROADCASTER_SCOPES.len());
        for (item, scope) in left.items.iter().zip(TWITCH_BROADCASTER_SCOPES.iter()) {
            assert_eq!(item.name, *scope);
            assert!(item.active);
        }
    }

    #[test]
    fn content_sections_two_column_scopes_and_eventsub() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        assert_eq!(sections.len(), 1);
        let DetailSection::TwoColumnLists { left, right } = &sections[0] else {
            panic!("expected TwoColumnLists");
        };
        assert_eq!(left.title, "OAuth scopes");
        assert_eq!(left.items.len(), TWITCH_BROADCASTER_SCOPES.len());
        assert_eq!(right.title, "EventSub subscriptions");
    }

    #[test]
    fn content_eventsub_empty_tracker_shows_zero_active() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        let DetailSection::TwoColumnLists { right, .. } = &sections[0] else {
            panic!("expected TwoColumnLists");
        };
        assert_eq!(right.count_label.as_deref(), Some("0 active"));
        assert!(right.items.is_empty());
    }

    #[test]
    fn content_eventsub_populated_tracker_renders_items() {
        let tracker = SubscriptionTracker::default();
        {
            let mut records = tracker.write().unwrap();
            records.push(SubscriptionRecord {
                kind: "channel.chat.message".to_owned(),
                version: "1".to_owned(),
                status: SubStatus::Active,
                subscription_id: Some("sub-abc".to_owned()),
            });
            records.push(SubscriptionRecord {
                kind: "channel.subscribe".to_owned(),
                version: "1".to_owned(),
                status: SubStatus::Pending,
                subscription_id: None,
            });
            records.push(SubscriptionRecord {
                kind: "channel.cheer".to_owned(),
                version: "1".to_owned(),
                status: SubStatus::Failed("HTTP 403".to_owned()),
                subscription_id: None,
            });
        }
        let b = make_bundle_with_tracker(ChatConnectionState::Connected, tracker);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        let DetailSection::TwoColumnLists { right, .. } = &sections[0] else {
            panic!("expected TwoColumnLists");
        };
        assert_eq!(right.count_label.as_deref(), Some("1 active"));
        assert_eq!(right.items.len(), 3);

        let active_item = &right.items[0];
        assert_eq!(active_item.name, "channel.chat.message");
        assert!(active_item.active);
        assert!(active_item.monospace_name);
        assert_eq!(
            active_item.trailing,
            vec![
                TrailingToken::Label("v1".to_owned()),
                TrailingToken::Label("active".to_owned()),
            ]
        );

        let pending_item = &right.items[1];
        assert_eq!(pending_item.name, "channel.subscribe");
        assert!(!pending_item.active);
        assert_eq!(
            pending_item.trailing,
            vec![
                TrailingToken::Label("v1".to_owned()),
                TrailingToken::Label("pending".to_owned()),
            ]
        );

        let failed_item = &right.items[2];
        assert_eq!(failed_item.name, "channel.cheer");
        assert!(!failed_item.active);
        assert_eq!(
            failed_item.trailing,
            vec![
                TrailingToken::Label("v1".to_owned()),
                TrailingToken::Label("failed".to_owned()),
            ]
        );
    }

    #[test]
    fn quick_actions_enabled_when_chat_connected() {
        let b = make_bundle(ChatConnectionState::Connected);
        let actions = b.actions();
        assert!(actions.iter().all(|a| a.enabled));
    }

    #[test]
    fn quick_actions_disabled_when_chat_disconnected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let actions = b.actions();
        assert!(actions.iter().all(|a| !a.enabled));
    }

    // Compile-time object-safety guard: the bundle must coerce into
    // `Arc<dyn BuiltinControl>` so the generic UI renderer can dispatch
    // lifecycle verbs through one trait object.
    #[test]
    fn bundle_coerces_to_dyn_builtin_control() {
        fn accepts(_: Arc<dyn forge_platform_core::BuiltinControl>) {}
        let b = make_bundle(ChatConnectionState::Connected);
        accepts(b);
    }

    // Missing-credentials guard: both `refresh_token` and `reconnect` `load()`
    // creds up front. With a store that yields None they must report
    // NotConnected without touching the network (NullCreds never connects).
    #[tokio::test]
    async fn refresh_token_without_stored_credentials_reports_not_connected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let outcome = forge_platform_core::BuiltinControl::refresh_token(b.as_ref()).await;
        assert_eq!(
            outcome,
            Err(forge_platform_core::ControlFailure::NotConnected)
        );
    }

    #[tokio::test]
    async fn reconnect_without_stored_credentials_reports_not_connected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let outcome = forge_platform_core::BuiltinControl::reconnect(b.as_ref()).await;
        assert_eq!(
            outcome,
            Err(forge_platform_core::ControlFailure::NotConnected)
        );
    }

    // Disconnect with an empty handle slot is the reachable no-session branch:
    // it must reject with NotConnected rather than silently succeed.
    #[tokio::test]
    async fn disconnect_with_no_live_session_reports_not_connected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let outcome = forge_platform_core::BuiltinControl::disconnect(b.as_ref()).await;
        assert_eq!(
            outcome,
            Err(forge_platform_core::ControlFailure::NotConnected)
        );
    }
}
