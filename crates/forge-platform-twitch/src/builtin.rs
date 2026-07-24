use std::sync::Arc;
use std::time::{Duration, SystemTime};

use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio::sync::{Mutex, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{BroadcastStream, WatchStream};

use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState,
    DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthStream, HealthValue, HeroBadge,
    HeroBadgeTone, LiveViewerSource, QuickAction, QuickActions, RateLimiter, SectionIcon,
    SubscriptionRow, SubscriptionStatus, TokenBucketRateLimiter, ViewerReport, ViewerReportStream,
};
use std::collections::BTreeMap;

use forge_events::EventPublisher;
use forge_storage::CredentialsRepo;
use forge_types::{SubActionStep, Variant};

use crate::TWITCH_BROADCASTER_SCOPES;
use crate::chat::{ChatConnectionState, TwitchChat, TwitchChatHandle};
use crate::credentials;
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{
    HelixHttpTransport, HelixMethod, HelixRequest, HelixTokenRefresher, HelixTokenSource,
    HelixTransport,
};
use crate::sub_actions::identity::{BroadcasterTier, resolve_broadcaster_tier};
use crate::subscriptions::{SubStatus, SubscriptionTracker};

const VIEWER_POLL_INTERVAL: Duration = Duration::from_secs(60);

/// Dedicated bucket sized to Twitch's 800/60s Helix budget for this poll only -
/// NOT the shared bucket the sub-action/chat-send transports draw on (wired up
/// separately in the app crate), so it cannot report true cross-transport usage.
const HELIX_BUDGET_CAPACITY: u32 = 800;
const HELIX_BUDGET_WINDOW: Duration = Duration::from_secs(60);

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

/// Holds only a `watch` receiver, so it does not keep the bundle alive.
struct TwitchViewerSource {
    reports: watch::Receiver<ViewerReport>,
}

impl LiveViewerSource for TwitchViewerSource {
    fn viewer_reports(&self) -> ViewerReportStream {
        Box::pin(WatchStream::new(self.reports.clone()))
    }
}

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
    // Mutex lets &self-async control verbs take() the handle without racing a concurrent disconnect/reconnect.
    handle: Mutex<Option<TwitchChatHandle>>,
    viewer_state: std::sync::RwLock<ViewerPollState>,
    viewer_report_tx: watch::Sender<ViewerReport>,
    transport: Arc<dyn HelixTransport>,
    tier: std::sync::RwLock<BroadcasterTier>,
    token_expires_at: std::sync::RwLock<Option<SystemTime>>,
    connected_at: std::sync::RwLock<Option<OffsetDateTime>>,
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
        let transport = Self::build_helix_transport(&config, &bus, &creds);
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
            transport: Arc::clone(&transport),
            tier: std::sync::RwLock::new(BroadcasterTier::default()),
            token_expires_at: std::sync::RwLock::new(None),
            connected_at: std::sync::RwLock::new(None),
        });
        Self::spawn_health_bridge(&bundle);
        Self::spawn_viewer_poll(&bundle, transport);
        Self::spawn_identity_refresh(&bundle);
        bundle
    }

    fn build_helix_transport(
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

    fn spawn_health_bridge(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        let mut state_rx = bundle.state_rx.clone();
        tokio::spawn(async move {
            while state_rx.changed().await.is_ok() {
                let state = *state_rx.borrow();
                bundle.on_chat_state_changed(state);

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

    /// Fires an identity refresh only on the transition into `Connected`, not on every
    /// broadcast while already connected.
    fn on_chat_state_changed(self: &Arc<Self>, state: ChatConnectionState) {
        if state == ChatConnectionState::Connected {
            let already_connected = self
                .connected_at
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .is_some();
            if let Ok(mut guard) = self.connected_at.write() {
                guard.get_or_insert_with(OffsetDateTime::now_utc);
            }
            if !already_connected {
                Self::spawn_identity_refresh(self);
            }
        } else if let Ok(mut guard) = self.connected_at.write() {
            *guard = None;
        }
    }

    fn spawn_identity_refresh(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        tokio::spawn(async move {
            bundle.refresh_identity().await;
        });
    }

    /// Missing/unloadable credentials leave the previously cached tier and expiry in
    /// place rather than resetting them to unlocked/unknown.
    async fn refresh_identity(&self) {
        let Ok(Some(stored)) = credentials::load(self.creds.as_ref()).await else {
            return;
        };
        if let Ok(mut guard) = self.token_expires_at.write() {
            *guard = stored.expires_at;
        }
        if let Ok(tier) = resolve_broadcaster_tier(self.transport.as_ref(), &stored.user_id).await
            && let Ok(mut guard) = self.tier.write()
        {
            *guard = tier;
        }
    }

    pub fn tier(&self) -> BroadcasterTier {
        *self.tier.read().unwrap_or_else(|p| p.into_inner())
    }

    /// A failed poll is skipped silently and retried next tick; last known value stays on screen.
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
        tier: BroadcasterTier,
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
            transport: Arc::new(crate::sub_actions::test_support::MockTransport::returning(
                Ok(serde_json::Value::Null),
            )),
            tier: std::sync::RwLock::new(tier),
            token_expires_at: std::sync::RwLock::new(None),
            connected_at: std::sync::RwLock::new(None),
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
        if state == ChatConnectionState::Connected {
            return "Joined".to_owned();
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
                primary: "-".to_owned(),
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

/// Empty `data` array means the broadcaster is not currently live.
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

    fn hero_name(&self) -> Option<&str> {
        self.login.as_deref()
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        self.chat_connection_state().to_connection_state()
    }

    fn uptime(&self) -> Option<Duration> {
        let at = (*self.connected_at.read().unwrap_or_else(|p| p.into_inner()))?;
        let elapsed = OffsetDateTime::now_utc() - at;
        if elapsed.is_positive() {
            Some(elapsed.unsigned_abs())
        } else {
            None
        }
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

    fn token_expiry(&self) -> Option<SystemTime> {
        *self
            .token_expires_at
            .read()
            .unwrap_or_else(|p| p.into_inner())
    }

    fn name_badges(&self) -> Vec<HeroBadge> {
        let mut badges = vec![HeroBadge {
            label: format!("user_id {}", self.config.user_id),
            tone: HeroBadgeTone::Neutral,
            monospace: true,
        }];
        let tier = self.tier();
        if tier != BroadcasterTier::Standard {
            badges.push(HeroBadge {
                label: tier.label().to_owned(),
                tone: HeroBadgeTone::Positive,
                monospace: false,
            });
        }
        badges
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
                value: HealthValue::Text {
                    primary: "-".to_owned(),
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
        let scopes_list = DetailSection::ScopesList {
            title: "OAuth scopes".to_owned(),
            icon: SectionIcon::new("key"),
            scopes: TWITCH_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            footer: None,
        };

        let records = self.tracker.read().unwrap_or_else(|p| p.into_inner());
        let dropped_kind = records
            .iter()
            .find(|r| matches!(r.status, SubStatus::Failed(_)))
            .map(|r| r.kind.clone());

        let sub_items: Vec<SubscriptionRow> = records
            .iter()
            .map(|r| {
                let (status, error_label) = match &r.status {
                    SubStatus::Active => (SubscriptionStatus::Active, None),
                    SubStatus::Pending => (SubscriptionStatus::Degraded, None),
                    SubStatus::Failed(_) => {
                        (SubscriptionStatus::Error, Some("retry pending".to_owned()))
                    }
                };
                SubscriptionRow {
                    name: r.kind.clone(),
                    status,
                    version: Some(format!("v{}", r.version)),
                    event_count: None,
                    error_label,
                }
            })
            .collect();

        let eventsub_list = DetailSection::SubscriptionList {
            title: "EventSub subscriptions".to_owned(),
            icon: SectionIcon::new("rss"),
            items: sub_items,
            footer: None,
            banner: dropped_kind.map(|kind| format!("{kind} subscription dropped")),
        };

        vec![DetailSection::TwoColumn {
            left: Box::new(scopes_list),
            right: Box::new(eventsub_list),
        }]
    }
}

/// Only Affiliate and Partner broadcasters may run commercials; the other quick actions
/// carry no tier restriction.
const COMMERCIAL_LOCKED_REASON: &str = "Requires Twitch Affiliate or Partner";

impl QuickActions for TwitchIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.is_chat_connected();
        let commercial_locked = self.tier() == BroadcasterTier::Standard;
        vec![
            QuickAction {
                label: "Send chat message".to_owned(),
                icon: SectionIcon::new("send"),
                enabled: connected,
                locked_reason: None,
                group: Some("Chat".to_owned()),
                destructive: false,
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
                locked_reason: None,
                group: Some("Raids & ads".to_owned()),
                destructive: false,
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
                enabled: connected && !commercial_locked,
                locked_reason: commercial_locked.then(|| COMMERCIAL_LOCKED_REASON.to_owned()),
                group: Some("Raids & ads".to_owned()),
                destructive: false,
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
                locked_reason: None,
                group: Some("Stream info".to_owned()),
                destructive: false,
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
        make_bundle_full(state, tracker, BroadcasterTier::Standard)
    }

    fn make_bundle_with_tier(
        state: ChatConnectionState,
        tier: BroadcasterTier,
    ) -> Arc<TwitchIntegrationBundle> {
        make_bundle_full(state, SubscriptionTracker::default(), tier)
    }

    fn make_bundle_full(
        state: ChatConnectionState,
        tracker: SubscriptionTracker,
        tier: BroadcasterTier,
    ) -> Arc<TwitchIntegrationBundle> {
        let (tx, rx) = watch::channel(state);
        let _ = tx;
        TwitchIntegrationBundle::for_test(
            Some("streamer".to_owned()),
            rx,
            tracker,
            Arc::new(NullCreds),
            tier,
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
    fn chat_irc_metric_active_when_connected() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Status {
            active,
            label,
            detail,
        } = &metrics[0].value
        else {
            panic!("expected Status variant");
        };
        assert!(*active);
        assert_eq!(label, "Joined");
        assert_eq!(detail.as_deref(), Some("#streamer"));
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
    fn hero_name_is_the_login_not_the_display_name() {
        let b = make_bundle(ChatConnectionState::Connected);
        let status: &dyn BuiltinStatus = b.as_ref();
        assert_eq!(status.hero_name(), Some("streamer"));
        assert_ne!(status.hero_name(), Some(status.display_name()));
    }

    #[test]
    fn name_badges_pin_user_id_and_gate_tier_above_standard() {
        for (tier, tier_badge_shown) in [
            (BroadcasterTier::Standard, false),
            (BroadcasterTier::Partner, true),
        ] {
            let b = make_bundle_with_tier(ChatConnectionState::Connected, tier);
            let status: &dyn BuiltinStatus = b.as_ref();
            let badges = status.name_badges();
            assert_eq!(badges[0].label, "user_id 1");
            assert!(badges[0].monospace);
            let has_tier_badge = badges.iter().any(|badge| badge.label == tier.label());
            assert_eq!(has_tier_badge, tier_badge_shown, "tier {tier:?}");
        }
    }

    #[test]
    fn sections_omit_the_broadcaster_info_card() {
        let b = make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Partner);
        let content: &dyn BuiltinContent = b.as_ref();
        assert!(
            !content
                .sections()
                .iter()
                .any(|s| matches!(s, DetailSection::InfoCard { .. })),
            "broadcaster InfoCard must not resurface"
        );
    }

    #[test]
    fn sections_expose_two_column_scopes_and_subscriptions() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        let DetailSection::TwoColumn { left, right } = &sections[0] else {
            panic!("expected TwoColumn as the first section");
        };
        let DetailSection::ScopesList { title, scopes, .. } = left.as_ref() else {
            panic!("expected ScopesList on the left");
        };
        assert_eq!(title, "OAuth scopes");
        assert!(
            scopes
                .iter()
                .map(String::as_str)
                .eq(TWITCH_BROADCASTER_SCOPES.iter().copied())
        );
        let DetailSection::SubscriptionList { title, .. } = right.as_ref() else {
            panic!("expected SubscriptionList on the right");
        };
        assert_eq!(title, "EventSub subscriptions");
    }

    fn subscription_list(
        bundle: &TwitchIntegrationBundle,
    ) -> (Vec<SubscriptionRow>, Option<String>) {
        let content: &dyn BuiltinContent = bundle;
        let sections = content.sections();
        let DetailSection::TwoColumn { right, .. } = sections.into_iter().next().unwrap() else {
            panic!("expected TwoColumn as the first section");
        };
        let DetailSection::SubscriptionList { items, banner, .. } = *right else {
            panic!("expected SubscriptionList on the right");
        };
        (items, banner)
    }

    #[test]
    fn subscription_rows_map_tracker_status_and_banner_flags_dropped() {
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
        let (items, banner) = subscription_list(&b);

        for (row, name, expected_status, expected_error) in [
            (
                &items[0],
                "channel.chat.message",
                SubscriptionStatus::Active,
                None,
            ),
            (
                &items[1],
                "channel.subscribe",
                SubscriptionStatus::Degraded,
                None,
            ),
            (
                &items[2],
                "channel.cheer",
                SubscriptionStatus::Error,
                Some("retry pending"),
            ),
        ] {
            assert_eq!(row.name, name);
            assert_eq!(row.status, expected_status);
            assert_eq!(row.error_label.as_deref(), expected_error);
        }
        assert_eq!(
            banner.as_deref(),
            Some("channel.cheer subscription dropped")
        );
    }

    #[test]
    fn subscription_list_empty_without_records_and_carries_no_banner() {
        let b = make_bundle(ChatConnectionState::Connected);
        let (items, banner) = subscription_list(&b);
        assert!(items.is_empty());
        assert!(banner.is_none());
    }

    #[test]
    fn commercial_action_locks_for_standard_tier_and_unlocks_for_affiliate() {
        let standard =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Standard);
        let affiliate =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);

        let gated_under_standard: Vec<String> = standard
            .actions()
            .into_iter()
            .filter(|a| a.locked_reason.is_some())
            .map(|a| a.subaction_template.kind_id)
            .collect();
        assert_eq!(
            gated_under_standard,
            vec!["twitch.channel.run_ad".to_owned()]
        );

        for kind in &gated_under_standard {
            let unlocked = affiliate
                .actions()
                .into_iter()
                .find(|a| &a.subaction_template.kind_id == kind)
                .unwrap();
            assert!(
                unlocked.locked_reason.is_none(),
                "{kind} must unlock at affiliate tier"
            );
        }

        for bundle in [&standard, &affiliate] {
            let open = bundle
                .actions()
                .into_iter()
                .find(|a| a.subaction_template.kind_id == "twitch.chat.send_message")
                .unwrap();
            assert!(open.locked_reason.is_none());
        }
    }

    #[test]
    fn quick_actions_enabled_when_chat_connected() {
        let b = make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let actions = b.actions();
        assert!(actions.iter().all(|a| a.enabled));
    }

    #[test]
    fn quick_actions_disabled_when_chat_disconnected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let actions = b.actions();
        assert!(actions.iter().all(|a| !a.enabled));
    }

    #[test]
    fn bundle_coerces_to_dyn_builtin_control() {
        fn accepts(_: Arc<dyn forge_platform_core::BuiltinControl>) {}
        let b = make_bundle(ChatConnectionState::Connected);
        accepts(b);
    }

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
