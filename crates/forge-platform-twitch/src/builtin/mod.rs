use std::sync::Arc;
use std::time::{Duration, SystemTime};

use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{BroadcastStream, WatchStream};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use forge_net::Gateway;
use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ChatPlatform, ConnectionState,
    DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthStream, HealthValue, HeroBadge,
    HeroBadgeTone, LiveViewerSource, SectionIcon, SubscriptionRow, SubscriptionStatus,
    ViewerReport, ViewerReportStream,
};
use forge_types::IntegrationId;

use forge_events::EventPublisher;
use forge_storage::CredentialsRepo;

use crate::TWITCH_BROADCASTER_SCOPES;
use crate::account::identity::{BroadcasterTier, resolve_broadcaster_tier};
use crate::chat::ChatConnectionState;
use crate::chat_platform::TwitchPlatform;
use crate::config::ChatSessionConfig;
use crate::creator_goals;
use crate::credentials;
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{
    HelixHttpTransport, HelixMethod, HelixRequest, HelixTokenRefresher, HelixTokenSource,
    HelixTransport,
};
use crate::lifecycle::TwitchLifecycle;
use crate::subscriptions::{SubStatus, SubscriptionTracker};

mod quick_actions;

const API_CALLS_METRIC_INDEX: u8 = 3;
const VIEWER_POLL_INTERVAL: Duration = Duration::from_secs(60);

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

struct TwitchViewerSource {
    reports: watch::Receiver<ViewerReport>,
}

impl LiveViewerSource for TwitchViewerSource {
    fn viewer_reports(&self) -> ViewerReportStream {
        Box::pin(WatchStream::new(self.reports.clone()))
    }
}

pub struct TwitchIntegrationBundle {
    id: IntegrationId,
    login: Option<String>,
    platform: Arc<TwitchPlatform>,
    state_rx: watch::Receiver<ChatConnectionState>,
    health_tx: broadcast::Sender<HealthDelta>,
    tracker: SubscriptionTracker,
    config: ChatSessionConfig,
    bus: Arc<dyn EventPublisher>,
    creds: Arc<dyn CredentialsRepo>,
    credentials_manager: Arc<TwitchCredentialsManager>,
    viewer_state: std::sync::RwLock<ViewerPollState>,
    viewer_report_tx: watch::Sender<ViewerReport>,
    transport: Arc<dyn HelixTransport>,
    helix: Gateway,
    tier: std::sync::RwLock<BroadcasterTier>,
    token_expires_at: std::sync::RwLock<Option<SystemTime>>,
    connected_at: std::sync::RwLock<Option<OffsetDateTime>>,
    lifecycle: TwitchLifecycle,
    retired: CancellationToken,
}

impl TwitchIntegrationBundle {
    pub fn new(login: Option<String>, platform: Arc<TwitchPlatform>) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
        let (viewer_report_tx, _) = watch::channel(ViewerReport::Absent);
        let config = platform.config().clone();
        let bus = platform.publisher();
        let credentials_manager = Arc::clone(platform.credentials_manager());
        let helix = platform.helix_gateway().clone();
        let transport = Self::build_helix_transport(&config, &credentials_manager, helix.clone());
        let bundle = Arc::new(Self {
            id: crate::TWITCH_INTEGRATION.id,
            login,
            state_rx: platform.session_states(),
            health_tx,
            tracker: platform.tracker().clone(),
            config,
            bus,
            creds: Arc::clone(platform.credentials()),
            credentials_manager,
            viewer_state: std::sync::RwLock::new(ViewerPollState::default()),
            viewer_report_tx,
            transport: Arc::clone(&transport),
            helix,
            tier: std::sync::RwLock::new(BroadcasterTier::default()),
            token_expires_at: std::sync::RwLock::new(None),
            connected_at: std::sync::RwLock::new(None),
            lifecycle: platform.lifecycle().clone(),
            platform,
            retired: CancellationToken::new(),
        });
        Self::spawn_health_bridge(&bundle);
        Self::spawn_token_expiry_bridge(&bundle);
        Self::spawn_viewer_poll(&bundle, transport);
        Self::spawn_identity_refresh(&bundle);
        Self::spawn_lifecycle_seed(&bundle);
        bundle
    }

    fn build_helix_transport(
        config: &ChatSessionConfig,
        manager: &Arc<TwitchCredentialsManager>,
        helix: Gateway,
    ) -> Arc<dyn HelixTransport> {
        Arc::new(
            HelixHttpTransport::new(
                helix,
                config.client_id.clone(),
                Arc::clone(manager) as Arc<dyn HelixTokenSource>,
            )
            .with_refresher(Arc::clone(manager) as Arc<dyn HelixTokenRefresher>),
        )
    }

    fn spawn_health_bridge(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        let mut state_rx = bundle.state_rx.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    () = bundle.retired.cancelled() => break,
                    changed = state_rx.changed() => if changed.is_err() { break },
                }
                let state = *state_rx.borrow();
                bundle.on_chat_state_changed(state);

                let chat_delta = HealthDelta {
                    index: 0,
                    new_value: bundle.chat_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(chat_delta);

                let eventsub_delta = HealthDelta {
                    index: 1,
                    new_value: bundle.eventsub_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(eventsub_delta);

                let api_calls_delta = HealthDelta {
                    index: 3,
                    new_value: bundle.api_calls_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(api_calls_delta);
            }
        });
    }

    fn spawn_token_expiry_bridge(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        let mut expiry_rx = bundle.credentials_manager.subscribe_expiry();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    () = bundle.retired.cancelled() => break,
                    changed = expiry_rx.changed() => if changed.is_err() { break },
                }
                let expiry = *expiry_rx.borrow_and_update();
                if let Ok(mut guard) = bundle.token_expires_at.write() {
                    *guard = expiry;
                }
                let delta = HealthDelta {
                    index: API_CALLS_METRIC_INDEX,
                    new_value: bundle.api_calls_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(delta);
            }
        });
    }

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
                Self::spawn_lifecycle_seed(self);
                Self::spawn_goal_sync(self);
                self.lifecycle.rewards_changed();
            }
        } else {
            self.lifecycle.forget_phases();
            if let Ok(mut guard) = self.connected_at.write() {
                *guard = None;
            }
        }
    }

    fn spawn_identity_refresh(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        tokio::spawn(async move {
            bundle.refresh_identity().await;
        });
    }

    fn spawn_lifecycle_seed(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        tokio::spawn(async move {
            bundle
                .lifecycle
                .seed_from_helix(bundle.transport.as_ref(), &bundle.config.broadcaster_id)
                .await;
            bundle
                .lifecycle
                .seed_shield_from_helix(bundle.transport.as_ref(), &bundle.config.broadcaster_id)
                .await;
        });
    }

    fn spawn_goal_sync(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        tokio::spawn(async move {
            match creator_goals::publish_current_goals(
                bundle.transport.as_ref(),
                bundle.bus.as_ref(),
                &bundle.config.broadcaster_id,
            )
            .await
            {
                Ok(synced) => debug!(synced, "creator goals synced"),
                Err(_) => debug!("creator goal sync failed"),
            }
        });
    }

    pub(crate) async fn refresh_identity(&self) {
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

    fn spawn_viewer_poll(bundle: &Arc<Self>, transport: Arc<dyn HelixTransport>) {
        let bundle = Arc::clone(bundle);
        let broadcaster_id = bundle.config.broadcaster_id.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(VIEWER_POLL_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tokio::select! {
                    biased;
                    () = bundle.retired.cancelled() => break,
                    _ = ticker.tick() => {}
                }

                let api_calls_delta = HealthDelta {
                    index: 3,
                    new_value: bundle.api_calls_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(api_calls_delta);

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
                bundle.viewer_report_tx.send_replace(new_state.as_report());
                let delta = HealthDelta {
                    index: 2,
                    new_value: bundle.viewers_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(delta);
            }
        });
    }

    pub(crate) fn credentials(&self) -> &Arc<dyn CredentialsRepo> {
        &self.creds
    }

    pub(crate) fn credentials_manager(&self) -> &TwitchCredentialsManager {
        &self.credentials_manager
    }

    pub(crate) fn helix(&self) -> &dyn HelixTransport {
        self.transport.as_ref()
    }

    pub(crate) fn broadcaster_id(&self) -> &str {
        &self.config.broadcaster_id
    }

    pub(crate) fn lifecycle(&self) -> &TwitchLifecycle {
        &self.lifecycle
    }

    pub(crate) fn platform(&self) -> &TwitchPlatform {
        &self.platform
    }

    pub async fn shutdown(&self) {
        self.retired.cancel();
        if let Err(error) = self.platform.disconnect().await {
            tracing::warn!(%error, "twitch chat disconnect failed at shutdown");
        }
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
        Self::for_test_with_transport(
            login,
            state_rx,
            tracker,
            creds,
            tier,
            Arc::new(crate::sub_actions::test_support::MockTransport::returning(
                Ok(serde_json::Value::Null),
            )),
            "1",
        )
    }

    #[cfg(test)]
    pub(crate) fn for_test_with_transport(
        login: Option<String>,
        state_rx: watch::Receiver<ChatConnectionState>,
        tracker: SubscriptionTracker,
        creds: Arc<dyn CredentialsRepo>,
        tier: BroadcasterTier,
        transport: Arc<dyn HelixTransport>,
        broadcaster_id: &str,
    ) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(16);
        let (viewer_report_tx, _) = watch::channel(ViewerReport::Absent);
        let credentials_manager = Arc::new(TwitchCredentialsManager::new(
            crate::sub_actions::test_support::unreachable_oauth_client("test-client"),
            Arc::clone(&creds),
        ));
        let config = ChatSessionConfig {
            client_id: "test-client".to_owned(),
            broadcaster_id: broadcaster_id.to_owned(),
            user_id: broadcaster_id.to_owned(),
            endpoints: crate::sub_actions::test_support::unreachable_twitch_endpoints(),
        };
        let helix = crate::sub_actions::test_support::unreachable_helix_gateway();
        let lifecycle = TwitchLifecycle::new();
        let platform = Arc::new(TwitchPlatform::new(
            config.clone(),
            Arc::clone(&creds),
            Arc::clone(&credentials_manager),
            tracker.clone(),
            helix.clone(),
            lifecycle.clone(),
        ));
        Arc::new(Self {
            id: IntegrationId::new("twitch"),
            login,
            platform,
            state_rx,
            health_tx,
            tracker,
            config,
            bus: Arc::new(forge_events::EventChannel::new(
                crate::chat_platform::EVENT_CHANNEL_CAPACITY,
            )),
            creds,
            credentials_manager,
            viewer_state: std::sync::RwLock::new(ViewerPollState::default()),
            viewer_report_tx,
            transport,
            helix,
            tier: std::sync::RwLock::new(tier),
            token_expires_at: std::sync::RwLock::new(None),
            connected_at: std::sync::RwLock::new(None),
            lifecycle,
            retired: CancellationToken::new(),
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
        let count = self.active_sub_count();
        HealthValue::Status {
            label: format!("{count} subs"),
            active: count > 0,
            detail: Some("WebSocket".to_owned()),
        }
    }

    fn viewers_health_value(&self) -> HealthValue {
        let state = *self.viewer_state.read().unwrap_or_else(|p| p.into_inner());
        match state {
            ViewerPollState::Unknown => HealthValue::Text {
                primary: "0".to_owned(),
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

    fn api_calls_health_value(&self) -> HealthValue {
        let usage = self.helix.limiter_usage();
        match (usage.used, usage.capacity) {
            (Some(used), Some(capacity)) => HealthValue::Ratio {
                used: u64::from(used),
                total: u64::from(capacity),
                reset_hint: usage
                    .resets_in
                    .map(|d| format!("resets in {}s", d.as_secs())),
            },
            _ => HealthValue::Text {
                primary: "-".to_owned(),
                secondary: None,
            },
        }
    }
}

fn extract_viewer_count(body: &serde_json::Value) -> Option<u64> {
    body.get("data")?
        .as_array()?
        .first()?
        .get("viewer_count")?
        .as_u64()
}

impl BuiltinStatus for TwitchIntegrationBundle {
    fn id(&self) -> &IntegrationId {
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
        badges.push(HeroBadge {
            label: tier.label().to_owned(),
            tone: if tier == BroadcasterTier::Standard {
                HeroBadgeTone::Neutral
            } else {
                HeroBadgeTone::Positive
            },
            monospace: false,
        });
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
                value: self.api_calls_health_value(),
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
            .find(|r| matches!(r.status, SubStatus::Failed(_) | SubStatus::Retrying(_)))
            .map(|r| r.kind.clone());

        let sub_items: Vec<SubscriptionRow> = records
            .iter()
            .map(|r| {
                let (status, error_label) = match &r.status {
                    SubStatus::Active => (SubscriptionStatus::Active, None),
                    SubStatus::Pending => (SubscriptionStatus::Degraded, None),
                    SubStatus::Retrying(_) => {
                        (SubscriptionStatus::Error, Some("retry pending".to_owned()))
                    }
                    SubStatus::Failed(reason) => (SubscriptionStatus::Error, Some(reason.clone())),
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
