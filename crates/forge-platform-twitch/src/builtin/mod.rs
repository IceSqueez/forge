use crate::vocabulary::labels;
use forge_types::UserText;
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
    DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthMetricId, HealthStream,
    HealthText, HealthValue, HeroBadge, HeroBadgeTone, LiveViewerSource, SectionIcon,
    SubscriptionRow, SubscriptionStatus, ViewerReport, ViewerReportStream,
};
use forge_types::{IntegrationId, MessageRef, VerbatimText};

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
use crate::subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};
use crate::vocabulary::messages;

mod quick_actions;

const CHAT_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_CHAT);
const EVENTSUB_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_EVENTSUB);
const API_CALLS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_API_CALLS);
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
                    metric: CHAT_METRIC,
                    new_value: bundle.chat_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(chat_delta);

                let eventsub_delta = HealthDelta {
                    metric: EVENTSUB_METRIC,
                    new_value: bundle.eventsub_health_value(),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(eventsub_delta);

                let api_calls_delta = HealthDelta {
                    metric: API_CALLS_METRIC,
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
                *bundle
                    .token_expires_at
                    .write()
                    .unwrap_or_else(|p| p.into_inner()) = expiry;
                let delta = HealthDelta {
                    metric: API_CALLS_METRIC,
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
            self.connected_at
                .write()
                .unwrap_or_else(|p| p.into_inner())
                .get_or_insert_with(OffsetDateTime::now_utc);
            if !already_connected {
                Self::spawn_identity_refresh(self);
                Self::spawn_lifecycle_seed(self);
                Self::spawn_goal_sync(self);
                self.lifecycle.rewards_changed();
            }
        } else {
            self.lifecycle.forget_phases();
            *self.connected_at.write().unwrap_or_else(|p| p.into_inner()) = None;
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
                Err(e) => debug!(error = %e, "creator goal sync failed"),
            }
        });
    }

    pub(crate) async fn refresh_identity(&self) {
        let Ok(Some(stored)) = credentials::load(self.creds.as_ref()).await else {
            return;
        };
        *self
            .token_expires_at
            .write()
            .unwrap_or_else(|p| p.into_inner()) = stored.expires_at;
        if let Ok(tier) = resolve_broadcaster_tier(self.transport.as_ref(), &stored.user_id).await {
            *self.tier.write().unwrap_or_else(|p| p.into_inner()) = tier;
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
                    metric: API_CALLS_METRIC,
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
                *bundle
                    .viewer_state
                    .write()
                    .unwrap_or_else(|p| p.into_inner()) = new_state;
                bundle.viewer_report_tx.send_replace(new_state.as_report());
                let delta = HealthDelta {
                    metric: HealthMetricId::VIEWERS,
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

    fn chat_status(&self) -> HealthText {
        let state = self.chat_connection_state();
        if state == ChatConnectionState::Connected {
            return message_text(messages::health_chat_joined());
        }
        HealthText::Connection(state.to_connection_state())
    }

    fn active_sub_count(&self) -> usize {
        self.tracker.active_count()
    }

    fn chat_health_value(&self) -> HealthValue {
        HealthValue::Status {
            label: self.chat_status(),
            active: self.is_chat_connected(),
            detail: self
                .login
                .as_ref()
                .map(|l| HealthText::Text(VerbatimText::new(format!("#{l}")).into())),
        }
    }

    fn eventsub_health_value(&self) -> HealthValue {
        let count = self.active_sub_count();
        HealthValue::Status {
            label: message_text(messages::health_eventsub_subs(count)),
            active: count > 0,
            detail: Some(HealthText::Text(
                VerbatimText::new(EVENTSUB_TRANSPORT).into(),
            )),
        }
    }

    fn viewers_health_value(&self) -> HealthValue {
        let state = *self.viewer_state.read().unwrap_or_else(|p| p.into_inner());
        match state {
            ViewerPollState::Unknown => HealthValue::Text {
                primary: HealthText::Count(0),
                secondary: None,
            },
            ViewerPollState::Offline => HealthValue::Text {
                primary: HealthText::Count(0),
                secondary: Some(message_text(messages::health_viewers_offline())),
            },
            ViewerPollState::Live(count) => HealthValue::Text {
                primary: HealthText::Count(count),
                secondary: Some(message_text(messages::health_viewers_live())),
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
                    .map(|d| message_text(messages::health_resets_in(d.as_secs()))),
            },
            _ => HealthValue::Text {
                primary: message_text(messages::health_no_usage()),
                secondary: None,
            },
        }
    }
}

const EVENTSUB_TRANSPORT: &str = "WebSocket";

fn message_text(message: MessageRef) -> HealthText {
    HealthText::Text(message.into())
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

    fn hero_name(&self) -> Option<String> {
        self.login.clone()
    }

    fn version(&self) -> Option<String> {
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

    fn endpoint(&self) -> Option<UserText> {
        Some(messages::ENDPOINT_SUMMARY.into())
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
            label: messages::user_id_badge(&self.config.user_id).into(),
            tone: HeroBadgeTone::Neutral,
            monospace: true,
        }];
        let tier = self.tier();
        badges.push(HeroBadge {
            label: tier.label().into(),
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
                id: CHAT_METRIC,
                value: self.chat_health_value(),
            },
            HealthMetric {
                id: EVENTSUB_METRIC,
                value: self.eventsub_health_value(),
            },
            HealthMetric {
                id: HealthMetricId::VIEWERS,
                value: self.viewers_health_value(),
            },
            HealthMetric {
                id: API_CALLS_METRIC,
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
            title: labels::OAUTH_SCOPES.into(),
            icon: SectionIcon::new("key"),
            scopes: TWITCH_BROADCASTER_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            footer: None,
        };

        let records = self.tracker.snapshot();
        let dropped_kind = records
            .iter()
            .find(|r| matches!(r.status, SubStatus::Failed(_) | SubStatus::Retrying(_)))
            .map(SubscriptionRecord::label);

        let sub_items: Vec<SubscriptionRow> = records
            .iter()
            .map(|r| {
                let (status, error_label) = match &r.status {
                    SubStatus::Active => (SubscriptionStatus::Active, None),
                    SubStatus::Pending => (SubscriptionStatus::Degraded, None),
                    SubStatus::Retrying(_) => (
                        SubscriptionStatus::Error,
                        Some(labels::RETRY_PENDING.into()),
                    ),
                    SubStatus::Failed(failure) => (
                        SubscriptionStatus::Error,
                        Some(messages::subscription_failure(failure).into()),
                    ),
                };
                SubscriptionRow {
                    kind: r.kind.clone(),
                    name: r.label(),
                    status,
                    version: Some(format!("v{}", r.version)),
                    event_count: None,
                    error_label,
                }
            })
            .collect();

        let eventsub_list = DetailSection::SubscriptionList {
            title: labels::EVENTSUB_SUBSCRIPTIONS.into(),
            icon: SectionIcon::new("rss"),
            items: sub_items,
            footer: None,
            banner: dropped_kind.map(|kind| {
                MessageRef::new(labels::SUBSCRIPTION_DROPPED)
                    .with_arg("kind", kind)
                    .into()
            }),
        };

        vec![DetailSection::TwoColumn {
            left: Box::new(scopes_list),
            right: Box::new(eventsub_list),
        }]
    }
}

#[cfg(test)]
mod tests {
    use crate::config::HELIX_BUDGET_CAPACITY;
    use forge_storage::test_support::MemoryCredentials;

    use forge_platform_core::{
        BuiltinContent, BuiltinHealth, BuiltinStatus, CollectionId, QuickActionChoiceSource,
        QuickActionFieldKind, QuickActionFieldValue, QuickActionLiveness, QuickActions,
    };

    use std::collections::BTreeMap;

    use tokio::sync::watch;

    use super::*;
    use crate::subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};

    fn make_bundle(state: ChatConnectionState) -> Arc<TwitchIntegrationBundle> {
        make_bundle_with_tracker(state, SubscriptionTracker::default())
    }

    fn make_bundle_with_tracker(
        state: ChatConnectionState,
        tracker: SubscriptionTracker,
    ) -> Arc<TwitchIntegrationBundle> {
        make_bundle_full(state, tracker, BroadcasterTier::Standard)
    }

    pub(super) fn make_bundle_with_tier(
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
            Arc::new(MemoryCredentials::default()),
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

    fn metric_value<'a>(metrics: &'a [HealthMetric], id: &HealthMetricId) -> &'a HealthValue {
        &metrics
            .iter()
            .find(|metric| &metric.id == id)
            .unwrap_or_else(|| panic!("no metric {id}"))
            .value
    }

    #[test]
    fn metrics_declare_unique_catalogued_ids() {
        let b = make_bundle(ChatConnectionState::Connected);

        let ids: std::collections::HashSet<_> = BuiltinHealth::metrics(b.as_ref())
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                *id == HealthMetricId::VIEWERS
                    || crate::vocabulary::messages::TWITCH_MESSAGES
                        .codes
                        .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
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
        } = metric_value(&metrics, &CHAT_METRIC)
        else {
            panic!("expected Status variant");
        };
        assert!(*active);
        assert_eq!(*label, message_text(messages::health_chat_joined()));
        assert_eq!(
            *detail,
            Some(HealthText::Text(VerbatimText::new("#streamer").into()))
        );
    }

    #[test]
    fn chat_irc_metric_inactive_when_disconnected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Status { active, label, .. } = metric_value(&metrics, &CHAT_METRIC) else {
            panic!("expected Status variant");
        };
        assert!(!*active);
        assert_eq!(
            *label,
            HealthText::Connection(ConnectionState::Disconnected)
        );
    }

    #[test]
    fn eventsub_metric_shows_active_count_from_tracker() {
        let tracker = SubscriptionTracker::default();
        tracker.reset(vec![
            SubscriptionRecord {
                kind: "channel.chat.message".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Active,
                subscription_id: Some("sub-1".to_owned()),
            },
            SubscriptionRecord {
                kind: "channel.subscribe".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Pending,
                subscription_id: None,
            },
        ]);
        let b = make_bundle_with_tracker(ChatConnectionState::Connected, tracker);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Status {
            label,
            active,
            detail,
        } = metric_value(&metrics, &EVENTSUB_METRIC)
        else {
            panic!("expected Status variant for EventSub metric");
        };
        assert_eq!(*label, message_text(messages::health_eventsub_subs(1)));
        assert!(*active, "one active subscription must light EventSub green");
        assert_eq!(
            *detail,
            Some(HealthText::Text(
                VerbatimText::new(EVENTSUB_TRANSPORT).into()
            ))
        );
    }

    #[test]
    fn eventsub_metric_inactive_when_no_subscriptions_are_active() {
        let tracker = SubscriptionTracker::default();
        tracker.reset(vec![SubscriptionRecord {
            kind: "channel.subscribe".to_owned(),
            direction: None,
            version: "1".to_owned(),
            status: SubStatus::Pending,
            subscription_id: None,
        }]);
        let b = make_bundle_with_tracker(ChatConnectionState::Connected, tracker);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Status { label, active, .. } = metric_value(&metrics, &EVENTSUB_METRIC)
        else {
            panic!("expected Status variant for EventSub metric");
        };
        assert_eq!(*label, message_text(messages::health_eventsub_subs(0)));
        assert!(
            !*active,
            "no active subscription must leave EventSub inactive"
        );
    }

    #[test]
    fn api_calls_metric_reports_helix_budget_ratio() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Ratio { used, total, .. } = metric_value(&metrics, &API_CALLS_METRIC)
        else {
            panic!("expected Ratio variant for the API Calls metric");
        };
        assert_eq!(*used, 0, "a fresh Helix budget has zero consumed calls");
        assert_eq!(*total, u64::from(HELIX_BUDGET_CAPACITY));
    }

    fn reward_revision_moved(revisions: &mut forge_platform_core::CollectionRevisions) -> bool {
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        let changed = std::pin::pin!(revisions.changed());
        matches!(
            changed.poll(&mut context),
            std::task::Poll::Ready(forge_platform_core::RevisionWait::Changed)
        )
    }

    #[tokio::test]
    async fn every_fresh_connection_bumps_the_reward_revision() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let mut revisions = b.lifecycle().reward_revisions();
        let mut moved = Vec::new();

        for state in [
            ChatConnectionState::Connected,
            ChatConnectionState::Reconnecting { attempt: 1 },
            ChatConnectionState::Connected,
        ] {
            b.on_chat_state_changed(state);
            moved.push(reward_revision_moved(&mut revisions));
        }

        assert_eq!(moved, vec![true, false, true]);
    }

    #[tokio::test]
    async fn a_repeated_connected_state_does_not_bump_the_reward_revision() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        b.on_chat_state_changed(ChatConnectionState::Connected);
        let mut revisions = b.lifecycle().reward_revisions();

        b.on_chat_state_changed(ChatConnectionState::Connected);

        assert!(!reward_revision_moved(&mut revisions));
    }

    fn bundle_recording_helix(
        broadcaster_id: &str,
    ) -> (
        Arc<TwitchIntegrationBundle>,
        Arc<crate::sub_actions::test_support::MockTransport>,
    ) {
        let transport = Arc::new(
            crate::sub_actions::test_support::MockTransport::returning_sequence(Vec::new()),
        );
        let (_, rx) = watch::channel(ChatConnectionState::Disconnected);
        let bundle = TwitchIntegrationBundle::for_test_with_transport(
            Some("streamer".to_owned()),
            rx,
            SubscriptionTracker::default(),
            Arc::new(MemoryCredentials::default()),
            BroadcasterTier::Standard,
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            broadcaster_id,
        );
        (bundle, transport)
    }

    async fn requests_reached(
        transport: &crate::sub_actions::test_support::MockTransport,
        path: &str,
        expected: usize,
    ) -> usize {
        let count = || {
            (0..transport.call_count())
                .filter(|index| transport.request(*index).path == path)
                .count()
        };
        tokio::time::timeout(HANG_GUARD, async {
            while count() < expected {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the spawned sync never reached the transport");
        count()
    }

    async fn every_task_released(bundle: &Arc<TwitchIntegrationBundle>) {
        let released = tokio::time::timeout(HANG_GUARD, async {
            while Arc::strong_count(bundle) > 1 {
                tokio::time::sleep(RELEASE_POLL).await;
            }
        })
        .await;
        assert!(
            released.is_ok(),
            "{} task(s) still hold the bundle after shutdown",
            Arc::strong_count(bundle) - 1
        );
    }

    const GOALS_PATH: &str = "/helix/goals";
    const SHIELD_PATH: &str = "/helix/moderation/shield_mode";
    const HANG_GUARD: Duration = Duration::from_secs(30);
    const RELEASE_POLL: Duration = Duration::from_millis(5);

    fn goal_requests(
        transport: &crate::sub_actions::test_support::MockTransport,
    ) -> Vec<HelixRequest> {
        (0..transport.call_count())
            .map(|index| transport.request(index))
            .filter(|request| request.path == GOALS_PATH)
            .collect()
    }

    #[tokio::test]
    async fn creator_goals_sync_once_per_fresh_connection() {
        let (b, transport) = bundle_recording_helix("1");
        let steps = [
            (ChatConnectionState::Connected, 1),
            (ChatConnectionState::Connected, 1),
            (ChatConnectionState::Reconnecting { attempt: 1 }, 1),
            (ChatConnectionState::Connected, 2),
            (ChatConnectionState::Disconnected, 2),
            (ChatConnectionState::Connecting, 2),
            (ChatConnectionState::Connected, 3),
        ];

        for (state, expected_syncs) in steps {
            b.on_chat_state_changed(state);
            assert_eq!(
                requests_reached(&transport, GOALS_PATH, expected_syncs).await,
                expected_syncs,
                "after {state:?}"
            );
        }
    }

    #[tokio::test]
    async fn creator_goal_sync_asks_for_the_configured_broadcaster() {
        let (b, transport) = bundle_recording_helix("4242");

        b.on_chat_state_changed(ChatConnectionState::Connected);
        requests_reached(&transport, GOALS_PATH, 1).await;

        let queries: Vec<Vec<(String, String)>> = goal_requests(&transport)
            .into_iter()
            .map(|request| request.query)
            .collect();
        assert_eq!(
            queries,
            vec![vec![("broadcaster_id".to_owned(), "4242".to_owned())]]
        );
    }

    #[test]
    fn the_rewards_manager_action_targets_a_collection_the_bundle_declares() {
        let b = make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let declared: Vec<CollectionId> =
            forge_platform_core::BuiltinCollections::collections(b.as_ref())
                .into_iter()
                .map(|metadata| metadata.id)
                .collect();

        let targets: Vec<CollectionId> = b
            .actions()
            .into_iter()
            .filter_map(|action| action.collection)
            .collect();

        assert_eq!(targets, declared);
    }

    #[test]
    fn hero_name_is_the_login_not_the_display_name() {
        let b = make_bundle(ChatConnectionState::Connected);
        let status: &dyn BuiltinStatus = b.as_ref();
        assert_eq!(status.hero_name().as_deref(), Some("streamer"));
        assert_ne!(status.hero_name().as_deref(), Some(status.display_name()));
    }

    #[test]
    fn name_badges_pin_user_id_and_tone_tier_by_level() {
        for (tier, tone) in [
            (BroadcasterTier::Standard, HeroBadgeTone::Neutral),
            (BroadcasterTier::Affiliate, HeroBadgeTone::Positive),
            (BroadcasterTier::Partner, HeroBadgeTone::Positive),
        ] {
            let b = make_bundle_with_tier(ChatConnectionState::Connected, tier);
            let status: &dyn BuiltinStatus = b.as_ref();
            let badges = status.name_badges();
            assert_eq!(badges[0].label, messages::user_id_badge("1").into());
            assert!(badges[0].monospace);
            let tier_badge = badges
                .iter()
                .find(|badge| badge.label == UserText::from(tier.label()))
                .unwrap_or_else(|| panic!("tier {tier:?} badge missing"));
            assert_eq!(tier_badge.tone, tone, "tier {tier:?}");
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
        assert_eq!(*title, UserText::from(labels::OAUTH_SCOPES));
        assert!(
            scopes
                .iter()
                .map(String::as_str)
                .eq(TWITCH_BROADCASTER_SCOPES.iter().copied())
        );
        let DetailSection::SubscriptionList { title, .. } = right.as_ref() else {
            panic!("expected SubscriptionList on the right");
        };
        assert_eq!(*title, UserText::from(labels::EVENTSUB_SUBSCRIPTIONS));
    }

    fn dropped_banner(kind: impl Into<UserText>) -> UserText {
        MessageRef::new(labels::SUBSCRIPTION_DROPPED)
            .with_arg("kind", kind.into())
            .into()
    }

    fn subscription_list(
        bundle: &TwitchIntegrationBundle,
    ) -> (Vec<SubscriptionRow>, Option<UserText>) {
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
        tracker.reset(vec![
            SubscriptionRecord {
                kind: "channel.chat.message".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Active,
                subscription_id: Some("sub-abc".to_owned()),
            },
            SubscriptionRecord {
                kind: "channel.subscribe".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Pending,
                subscription_id: None,
            },
            SubscriptionRecord {
                kind: "channel.cheer".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Retrying(crate::subscriptions::SubscriptionFailure::Refused(
                    503,
                )),
                subscription_id: None,
            },
            SubscriptionRecord {
                kind: "channel.follow".to_owned(),
                direction: None,
                version: "2".to_owned(),
                status: SubStatus::Failed(crate::subscriptions::SubscriptionFailure::MissingScope),
                subscription_id: None,
            },
        ]);
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
                Some(UserText::from(labels::RETRY_PENDING)),
            ),
            (
                &items[3],
                "channel.follow",
                SubscriptionStatus::Error,
                Some(UserText::from(messages::subscription_failure(
                    &crate::subscriptions::SubscriptionFailure::MissingScope,
                ))),
            ),
        ] {
            assert_eq!(row.name, UserText::from(VerbatimText::new(name)));
            assert_eq!(row.status, expected_status);
            assert_eq!(row.error_label, expected_error);
        }
        assert_eq!(
            banner,
            Some(dropped_banner(VerbatimText::new("channel.cheer")))
        );
    }

    #[test]
    fn a_given_up_row_raises_the_dropped_banner_without_a_retrying_one() {
        let tracker = SubscriptionTracker::default();
        tracker.reset(vec![
            SubscriptionRecord {
                kind: "channel.chat.message".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Active,
                subscription_id: Some("sub-abc".to_owned()),
            },
            SubscriptionRecord {
                kind: "channel.ban".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Failed(crate::subscriptions::SubscriptionFailure::Revoked(
                    "user_removed".to_owned(),
                )),
                subscription_id: None,
            },
        ]);
        let b = make_bundle_with_tracker(ChatConnectionState::Connected, tracker);

        let (_, banner) = subscription_list(&b);

        assert_eq!(
            banner,
            Some(dropped_banner(VerbatimText::new("channel.ban")))
        );
    }

    #[test]
    fn a_dropped_raid_row_names_its_direction_in_the_row_and_the_banner() {
        let tracker = SubscriptionTracker::default();
        tracker.reset(vec![SubscriptionRecord {
            kind: "channel.raid".to_owned(),
            direction: Some(crate::vocabulary::RaidDirection::Sent),
            version: "1".to_owned(),
            status: SubStatus::Failed(crate::subscriptions::SubscriptionFailure::Revoked(
                "user_removed".to_owned(),
            )),
            subscription_id: None,
        }]);
        let b = make_bundle_with_tracker(ChatConnectionState::Connected, tracker);

        let (items, banner) = subscription_list(&b);

        let outgoing = messages::subscription_label(
            "channel.raid",
            Some(crate::vocabulary::RaidDirection::Sent),
        );
        assert_eq!(
            (items[0].name.clone(), banner),
            (outgoing.clone(), Some(dropped_banner(outgoing)))
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
    fn every_gated_action_carries_the_liveness_of_the_entity_it_targets() {
        let b = make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        b.lifecycle
            .apply_notification("channel.poll.begin", &serde_json::Value::Null, "1");
        b.lifecycle
            .apply_notification("channel.prediction.lock", &serde_json::Value::Null, "1");
        b.lifecycle.raid_started();

        let by_label: BTreeMap<String, QuickActionLiveness> = b
            .actions()
            .into_iter()
            .map(|a| (a.label.code_or_text(), a.liveness))
            .collect();

        for (code, expected) in [
            (labels::START_POLL, QuickActionLiveness::Absent),
            (labels::END_POLL_NOW, QuickActionLiveness::Live),
            (labels::CANCEL_POLL, QuickActionLiveness::Live),
            (labels::START_PREDICTION, QuickActionLiveness::Absent),
            (labels::LOCK_PREDICTION, QuickActionLiveness::Absent),
            (labels::RESOLVE_PAY_OUT, QuickActionLiveness::Live),
            (labels::CANCEL_REFUND, QuickActionLiveness::Live),
            (labels::START_RAID, QuickActionLiveness::Absent),
            (labels::CANCEL_RAID, QuickActionLiveness::Live),
        ] {
            assert_eq!(
                by_label.get(&code.to_string()),
                Some(&expected),
                "{code} with an open poll, a locked prediction and a pending raid"
            );
        }
    }

    #[test]
    fn only_the_actions_that_take_something_away_are_marked_destructive() {
        let b = make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let mut destructive: Vec<String> = b
            .actions()
            .into_iter()
            .filter(|a| a.destructive)
            .map(|a| a.label.code_or_text())
            .collect();
        destructive.sort();
        let mut expected = vec![
            labels::BAN_USER.to_string(),
            labels::CANCEL_REFUND.to_string(),
            labels::CANCEL_POLL.to_string(),
            labels::CANCEL_RAID.to_string(),
            labels::CLEAR_CHAT.to_string(),
            labels::REJECT_REFUND.to_string(),
            labels::REMOVE_MOD.to_string(),
            labels::REMOVE_VIP.to_string(),
        ];
        expected.sort();
        assert_eq!(destructive, expected);
    }

    #[test]
    fn vip_and_mod_quick_actions_each_run_their_own_sub_action() {
        let b = make_bundle(ChatConnectionState::Connected);
        let actions = b.actions();

        for (label, kind_id) in [
            (
                labels::ADD_VIP,
                crate::vocabulary::sub_action_ids::MODERATION_ADD_VIP,
            ),
            (
                labels::REMOVE_VIP,
                crate::vocabulary::sub_action_ids::MODERATION_REMOVE_VIP,
            ),
            (
                labels::ADD_MOD,
                crate::vocabulary::sub_action_ids::MODERATION_ADD_MODERATOR,
            ),
            (
                labels::REMOVE_MOD,
                crate::vocabulary::sub_action_ids::MODERATION_REMOVE_MODERATOR,
            ),
        ] {
            let action = actions
                .iter()
                .find(|a| a.label == UserText::from(label.clone()));
            assert_eq!(
                action.map(|a| a.subaction_template.kind_id.as_str()),
                Some(kind_id),
                "{label:?}"
            );
        }
    }

    fn poison<T: Send + Sync>(lock: &std::sync::RwLock<T>) {
        std::thread::scope(|scope| {
            let holder = scope.spawn(|| {
                let _guard = lock.write().unwrap();
                panic!("poisoning the lock on purpose");
            });
            assert!(holder.join().is_err());
        });
    }

    #[tokio::test]
    async fn a_poisoned_connected_at_lock_still_records_both_connection_transitions() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        poison(&b.connected_at);
        let read = || {
            b.connected_at
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .is_some()
        };

        b.on_chat_state_changed(ChatConnectionState::Connected);
        let after_connect = read();
        b.on_chat_state_changed(ChatConnectionState::Disconnected);

        assert_eq!((after_connect, read()), (true, false));
    }

    #[tokio::test]
    async fn a_poisoned_token_expiry_lock_still_takes_the_stored_expiry() {
        let expiry = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        let (_tx, rx) = watch::channel(ChatConnectionState::Connected);
        let b = TwitchIntegrationBundle::for_test(
            Some("streamer".to_owned()),
            rx,
            SubscriptionTracker::default(),
            crate::credentials_manager::tests::seeded_repo(
                &crate::credentials_manager::tests::stub_cred(expiry),
            ),
            BroadcasterTier::Standard,
        );
        poison(&b.token_expires_at);

        b.refresh_identity().await;

        assert!(
            b.token_expires_at
                .read()
                .unwrap_or_else(|p| p.into_inner())
                .is_some()
        );
    }

    #[test]
    fn action_groups_appear_in_the_expected_order() {
        let b = make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let mut groups: Vec<UserText> = Vec::new();
        for a in b.actions() {
            if let Some(group) = a.group
                && !groups.contains(&group.label)
            {
                groups.push(group.label);
            }
        }
        assert_eq!(
            groups,
            [
                labels::STREAM_INFO,
                labels::POLLS,
                labels::PREDICTIONS,
                labels::CHAT,
                labels::MODERATION,
                labels::RAIDS_ADS,
                labels::CHANNEL_POINTS,
            ]
            .map(UserText::from)
        );
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
    async fn disconnect_without_live_session_is_idempotent() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let outcome = forge_platform_core::BuiltinControl::disconnect(b.as_ref()).await;
        assert_eq!(outcome, Ok(()));
    }

    fn form_field_keys(
        field: &forge_registry::FormField,
        out: &mut std::collections::BTreeSet<String>,
    ) {
        out.insert(field.key().to_owned());
        if let forge_registry::FormField::Optional { inner, .. } = field {
            form_field_keys(inner, out);
        }
    }

    #[test]
    fn every_quick_action_field_key_is_consumed_by_its_runner() {
        use crate::sub_actions::register_twitch_sub_actions;
        use crate::sub_actions::test_support::{MockCreds, MockTransport};
        use forge_registry::SubActionRegistry;

        let mut reg = SubActionRegistry::new();
        register_twitch_sub_actions(
            &mut reg,
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(MockCreds::empty()),
            TwitchLifecycle::new(),
            None,
        )
        .unwrap();

        let bundle =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);

        for action in bundle.actions() {
            if action.fields.is_empty() {
                continue;
            }
            let kind = &action.subaction_template.kind_id;
            let mut known: std::collections::BTreeSet<String> =
                action.subaction_template.config.keys().cloned().collect();
            if let Some(runner) = reg.get(kind) {
                known.extend(runner.default_config().into_keys());
                for f in runner.config_fields() {
                    form_field_keys(&f, &mut known);
                }
            }
            for field in &action.fields {
                assert!(
                    known.contains(&field.key),
                    "quick action {:?}: field key {:?} is not a config key of runner {kind}",
                    action.label,
                    field.key
                );
            }
        }
    }

    #[test]
    fn every_quick_action_template_filled_in_by_its_fields_passes_its_runner_validation() {
        use crate::sub_actions::register_twitch_sub_actions;
        use crate::sub_actions::test_support::{MockCreds, MockTransport};
        use forge_registry::SubActionRegistry;

        let mut reg = SubActionRegistry::new();
        register_twitch_sub_actions(
            &mut reg,
            Arc::new(MockTransport::returning(Ok(serde_json::Value::Null))),
            Arc::new(MockCreds::empty()),
            TwitchLifecycle::new(),
            None,
        )
        .unwrap();
        let bundle =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);

        let problems: BTreeMap<String, String> = bundle
            .actions()
            .into_iter()
            .filter(|action| !action.fields.is_empty())
            .filter_map(|action| {
                let typed: BTreeMap<String, QuickActionFieldValue> = action
                    .fields
                    .iter()
                    .filter_map(|field| match field.kind {
                        QuickActionFieldKind::Text => Some((
                            field.key.clone(),
                            QuickActionFieldValue::Text("Nova".to_owned()),
                        )),
                        QuickActionFieldKind::Multiline => Some((
                            field.key.clone(),
                            QuickActionFieldValue::Text("Yes\nNo".to_owned()),
                        )),
                        QuickActionFieldKind::Choice(QuickActionChoiceSource::OptionList(_)) => {
                            Some((
                                field.key.clone(),
                                QuickActionFieldValue::Text("live-item-1".to_owned()),
                            ))
                        }
                        _ => None,
                    })
                    .collect();
                let step = action.merge_config(&typed);
                let runner = reg.get(&step.kind_id)?;
                let error = runner.validate_config(&step.config).err()?;
                Some((action.label.code_or_text(), error.to_string()))
            })
            .collect();

        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn free_text_quick_action_fields_open_blank() {
        for action in
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate)
                .actions()
        {
            for field in &action.fields {
                if !matches!(
                    field.kind,
                    QuickActionFieldKind::Text | QuickActionFieldKind::Multiline
                ) {
                    continue;
                }
                let prefilled_default = match &field.default {
                    None => false,
                    Some(QuickActionFieldValue::Text(text)) => !text.is_empty(),
                    Some(_) => true,
                };
                let prefilled_config = matches!(
                    action.subaction_template.config.get(&field.key),
                    Some(value) if *value != forge_types::Variant::String(String::new())
                );
                assert!(
                    !prefilled_default && !prefilled_config,
                    "quick action {:?}: free-text field {:?} is prefilled (default {:?}, config {:?})",
                    action.label,
                    field.key,
                    field.default,
                    action.subaction_template.config.get(&field.key)
                );
            }
        }
    }

    #[test]
    fn every_static_choice_field_default_is_one_of_its_listed_options() {
        let bundle =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);

        let mut checked = 0;
        for action in bundle.actions() {
            for field in &action.fields {
                let QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)) =
                    &field.kind
                else {
                    continue;
                };
                let Some(QuickActionFieldValue::Text(default)) = &field.default else {
                    continue;
                };
                assert!(
                    options.iter().any(|opt| &opt.value == default),
                    "quick action {:?}: choice field {:?} default {default:?} is not among its options",
                    action.label,
                    field.key
                );
                checked += 1;
            }
        }
        assert!(
            checked >= 2,
            "expected at least the announcement-color and ad-duration static choices, got {checked}"
        );
    }

    #[tokio::test]
    async fn shutting_down_twice_stops_the_chat_session_and_every_task_holding_the_bundle() {
        let creds: Arc<dyn CredentialsRepo> = Arc::new(MemoryCredentials::default());
        let platform = Arc::new(TwitchPlatform::new(
            ChatSessionConfig {
                client_id: "test-client".to_owned(),
                broadcaster_id: "1".to_owned(),
                user_id: "1".to_owned(),
                endpoints: crate::sub_actions::test_support::unreachable_twitch_endpoints(),
            },
            Arc::clone(&creds),
            Arc::new(TwitchCredentialsManager::new(
                crate::sub_actions::test_support::unreachable_oauth_client("test-client"),
                creds,
            )),
            SubscriptionTracker::default(),
            crate::sub_actions::test_support::unreachable_helix_gateway(),
            TwitchLifecycle::new(),
        ));
        let bundle = TwitchIntegrationBundle::new(Some("streamer".to_owned()), platform);

        bundle.shutdown().await;
        bundle.shutdown().await;

        every_task_released(&bundle).await;
        assert_eq!(
            bundle.platform().connection_state(),
            ConnectionState::Disconnected
        );
    }

    struct ExpiryBridgeFixture {
        _server: wiremock::MockServer,
        bundle: Arc<TwitchIntegrationBundle>,
        health: HealthStream,
    }

    const ISSUED_TTL_SECS: u64 = 14400;

    async fn expiry_bridge_fixture() -> ExpiryBridgeFixture {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(
                ResponseTemplate::new(reqwest::StatusCode::OK.as_u16()).set_body_json(
                    serde_json::json!({
                        "access_token": "new_access",
                        "refresh_token": "rotated_refresh",
                        "expires_in": ISSUED_TTL_SECS,
                    }),
                ),
            )
            .mount(&server)
            .await;
        let stale_expiry = SystemTime::now() + Duration::from_secs(10);
        let repo: Arc<dyn CredentialsRepo> = crate::credentials_manager::tests::seeded_repo(
            &crate::credentials_manager::tests::stub_cred(stale_expiry),
        );
        let (_, state_rx) = watch::channel(ChatConnectionState::Disconnected);
        let built = TwitchIntegrationBundle::for_test(
            Some("streamer".to_owned()),
            state_rx,
            SubscriptionTracker::default(),
            Arc::clone(&repo),
            BroadcasterTier::Standard,
        );
        let mut inner = Arc::into_inner(built).unwrap();
        inner.credentials_manager = Arc::new(TwitchCredentialsManager::new(
            crate::sub_actions::test_support::oauth_client_at(&server.uri(), "test-client"),
            repo,
        ));
        let bundle = Arc::new(inner);
        TwitchIntegrationBundle::spawn_token_expiry_bridge(&bundle);
        let health = BuiltinHealth::stream(bundle.as_ref());
        ExpiryBridgeFixture {
            _server: server,
            bundle,
            health,
        }
    }

    async fn refresh_stale_token(
        bundle: &TwitchIntegrationBundle,
        failed_access: &str,
    ) -> Option<SystemTime> {
        bundle
            .credentials_manager()
            .refresh(&forge_types::OAuthToken::new(failed_access))
            .await
            .unwrap()
            .expires_at
    }

    async fn next_delta(health: &mut HealthStream) -> Option<HealthDelta> {
        tokio::time::timeout(HANG_GUARD, health.next())
            .await
            .ok()
            .flatten()
    }

    async fn deltas_left_after_retiring(fx: ExpiryBridgeFixture) -> usize {
        let ExpiryBridgeFixture {
            _server,
            bundle,
            mut health,
        } = fx;
        bundle.shutdown().await;
        drop(bundle);
        let count_until_closed = async {
            let mut left = 0;
            while health.next().await.is_some() {
                left += 1;
            }
            left
        };
        tokio::time::timeout(HANG_GUARD, count_until_closed)
            .await
            .expect("a task kept the health channel open after shutdown")
    }

    #[tokio::test]
    async fn token_refresh_updates_the_reported_expiry_and_emits_one_api_calls_delta_per_change() {
        let mut fx = expiry_bridge_fixture().await;
        let mut failed_access = "existing_access";
        for _ in 0..2 {
            let renewed = refresh_stale_token(&fx.bundle, failed_access).await;
            failed_access = "new_access";

            let delta = next_delta(&mut fx.health).await;
            assert_eq!(
                delta.map(|delta| delta.metric),
                Some(API_CALLS_METRIC),
                "a refresh must emit an API calls delta"
            );
            assert_eq!(BuiltinStatus::token_expiry(fx.bundle.as_ref()), renewed);
        }
        assert_eq!(
            deltas_left_after_retiring(fx).await,
            0,
            "one refresh must emit exactly one delta"
        );
    }

    #[tokio::test]
    async fn token_expiry_stays_unreported_until_a_refresh_publishes_one() {
        let fx = expiry_bridge_fixture().await;

        assert_eq!(BuiltinStatus::token_expiry(fx.bundle.as_ref()), None);
        assert_eq!(deltas_left_after_retiring(fx).await, 0);
    }

    #[tokio::test]
    async fn expiry_bridge_ignores_refreshes_after_the_bundle_is_retired() {
        let fx = expiry_bridge_fixture().await;
        let before = BuiltinStatus::token_expiry(fx.bundle.as_ref());
        fx.bundle.shutdown().await;

        refresh_stale_token(&fx.bundle, "existing_access").await;
        every_task_released(&fx.bundle).await;

        assert_eq!(BuiltinStatus::token_expiry(fx.bundle.as_ref()), before);
        assert_eq!(deltas_left_after_retiring(fx).await, 0);
    }

    fn shield_labels(b: &TwitchIntegrationBundle) -> Vec<String> {
        b.actions()
            .into_iter()
            .filter(|a| {
                a.subaction_template
                    .kind_id
                    .starts_with("twitch.moderation.shield_mode")
            })
            .map(|a| a.label.code_or_text())
            .collect()
    }

    #[test]
    fn the_quick_actions_carry_one_shield_toggle_that_follows_the_live_shield_state() {
        let b = make_bundle(ChatConnectionState::Connected);
        let before = shield_labels(&b);

        b.lifecycle
            .apply_notification("channel.shield_mode.begin", &serde_json::Value::Null, "1");

        assert_eq!(
            (before, shield_labels(&b)),
            (
                vec![labels::START_SHIELD_MODE.to_string()],
                vec![labels::END_SHIELD_MODE.to_string()]
            )
        );
    }

    #[test]
    fn a_shield_change_moves_the_published_quick_action_revision() {
        let b = make_bundle(ChatConnectionState::Connected);
        let Some(mut revisions) = QuickActions::revisions(b.as_ref()) else {
            panic!("twitch publishes quick action revisions");
        };

        b.lifecycle
            .apply_notification("channel.shield_mode.begin", &serde_json::Value::Null, "1");

        assert!(reward_revision_moved(&mut revisions));
    }

    #[tokio::test]
    async fn a_fresh_connection_seeds_the_shield_phase_from_helix_once() {
        let (b, transport) = bundle_recording_helix("4242");

        for state in [
            ChatConnectionState::Connected,
            ChatConnectionState::Connected,
            ChatConnectionState::Reconnecting { attempt: 1 },
            ChatConnectionState::Connected,
        ] {
            b.on_chat_state_changed(state);
        }

        assert_eq!(requests_reached(&transport, SHIELD_PATH, 2).await, 2);
    }

    #[test]
    fn every_code_shown_in_any_tier_and_connection_state_is_catalogued_in_both_languages() {
        use crate::emitted_codes::{emitted_codes, uncatalogued};
        use crate::vocabulary::messages::TWITCH_MESSAGES;

        let dropped = SubscriptionTracker::default();
        dropped.reset(vec![
            SubscriptionRecord {
                kind: "channel.cheer".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Retrying(crate::subscriptions::SubscriptionFailure::Refused(
                    503,
                )),
                subscription_id: None,
            },
            SubscriptionRecord {
                kind: "channel.ban".to_owned(),
                direction: None,
                version: "1".to_owned(),
                status: SubStatus::Failed(crate::subscriptions::SubscriptionFailure::Revoked(
                    "user_removed".to_owned(),
                )),
                subscription_id: None,
            },
        ]);
        let shielded = make_bundle(ChatConnectionState::Connected);
        shielded.lifecycle.apply_notification(
            "channel.shield_mode.begin",
            &serde_json::Value::Null,
            "1",
        );
        for (case, bundle) in [
            (
                "standard",
                make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Standard),
            ),
            (
                "affiliate",
                make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate),
            ),
            (
                "partner",
                make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Partner),
            ),
            (
                "disconnected",
                make_bundle(ChatConnectionState::Disconnected),
            ),
            (
                "dropped subscriptions",
                make_bundle_with_tracker(ChatConnectionState::Connected, dropped),
            ),
            ("shield active", shielded),
        ] {
            let emitted = emitted_codes(&serde_json::json!((
                bundle.actions(),
                bundle.sections(),
                bundle.name_badges()
            )));

            assert!(!emitted.is_empty(), "{case}");
            assert_eq!(
                uncatalogued(&emitted, &[TWITCH_MESSAGES, forge_types::COMMON_MESSAGES]),
                Vec::<String>::new(),
                "{case}"
            );
        }
    }
}
