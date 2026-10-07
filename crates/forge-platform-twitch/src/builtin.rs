use std::sync::Arc;
use std::time::{Duration, SystemTime};

use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::{BroadcastStream, WatchStream};
use tokio_util::sync::CancellationToken;
use tracing::debug;

#[cfg(test)]
use forge_platform_core::TokenBucketRateLimiter;
use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ChatPlatform, CollectionId,
    CollectionRevisions, ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric,
    HealthStream, HealthValue, HeroBadge, HeroBadgeTone, LiveViewerSource, PlatformEndpoints,
    QuickAction, QuickActionAccent, QuickActionChoiceOption, QuickActionChoiceSource,
    QuickActionField, QuickActionFieldKind, QuickActionFieldValue, QuickActionLiveness,
    QuickActions, RateLimiter, SectionIcon, SubscriptionRow, SubscriptionStatus, ViewerReport,
    ViewerReportStream,
};
use forge_types::IntegrationId;
use std::collections::BTreeMap;

use forge_events::EventPublisher;
use forge_storage::CredentialsRepo;
use forge_types::{SubActionStep, Variant};

use crate::TWITCH_BROADCASTER_SCOPES;
use crate::chat::ChatConnectionState;
use crate::chat_platform::TwitchPlatform;
use crate::creator_goals;
use crate::credentials;
use crate::credentials_manager::TwitchCredentialsManager;
use crate::helix::{
    HelixHttpTransport, HelixMethod, HelixRequest, HelixTokenRefresher, HelixTokenSource,
    HelixTransport,
};
use crate::lifecycle::{LifecycleSnapshot, ShieldPhase, TwitchLifecycle};
use crate::reward_collection::REWARDS_COLLECTION;
use crate::sub_actions::identity::{BroadcasterTier, resolve_broadcaster_tier};
use crate::subscriptions::{SubStatus, SubscriptionTracker};

const API_CALLS_METRIC_INDEX: u8 = 3;
const VIEWER_POLL_INTERVAL: Duration = Duration::from_secs(60);

pub const HELIX_BUDGET_CAPACITY: u32 = 800;
pub const HELIX_BUDGET_WINDOW: Duration = Duration::from_secs(60);

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

#[derive(Clone)]
pub struct ChatSessionConfig {
    pub client_id: String,
    pub broadcaster_id: String,
    pub user_id: String,
    pub endpoints: PlatformEndpoints,
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
    rate_limiter: Arc<dyn RateLimiter>,
    tier: std::sync::RwLock<BroadcasterTier>,
    token_expires_at: std::sync::RwLock<Option<SystemTime>>,
    connected_at: std::sync::RwLock<Option<OffsetDateTime>>,
    lifecycle: TwitchLifecycle,
    retired: CancellationToken,
}

impl TwitchIntegrationBundle {
    pub fn new(login: Option<String>, platform: Arc<TwitchPlatform>) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(16);
        let (viewer_report_tx, _) = watch::channel(ViewerReport::Absent);
        let config = platform.config().clone();
        let bus = platform.publisher();
        let credentials_manager = Arc::clone(platform.credentials_manager());
        let rate_limiter = Arc::clone(platform.rate_limiter());
        let transport = Self::build_helix_transport(
            &config,
            &bus,
            &credentials_manager,
            Arc::clone(&rate_limiter),
        );
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
            rate_limiter,
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
        bus: &Arc<dyn EventPublisher>,
        manager: &Arc<TwitchCredentialsManager>,
        rate_limiter: Arc<dyn RateLimiter>,
    ) -> Arc<dyn HelixTransport> {
        Arc::new(
            HelixHttpTransport::new(
                &config.endpoints,
                rate_limiter,
                Arc::clone(bus),
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
                let _ = bundle.health_tx.send(chat_delta);

                let eventsub_delta = HealthDelta {
                    index: 1,
                    new_value: bundle.eventsub_health_value(),
                };
                let _ = bundle.health_tx.send(eventsub_delta);

                let api_calls_delta = HealthDelta {
                    index: 3,
                    new_value: bundle.api_calls_health_value(),
                };
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
                let _ = bundle.viewer_report_tx.send(new_state.as_report());
                let delta = HealthDelta {
                    index: 2,
                    new_value: bundle.viewers_health_value(),
                };
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
        let _ = self.platform.disconnect().await;
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
            &crate::endpoints_test_support::unreachable_endpoints(),
            Arc::clone(&creds),
            "test-client".to_owned(),
        ));
        let config = ChatSessionConfig {
            client_id: "test-client".to_owned(),
            broadcaster_id: broadcaster_id.to_owned(),
            user_id: broadcaster_id.to_owned(),
            endpoints: crate::sub_actions::test_support::unreachable_twitch_endpoints(),
        };
        let rate_limiter: Arc<dyn RateLimiter> = Arc::new(TokenBucketRateLimiter::new(
            HELIX_BUDGET_CAPACITY,
            HELIX_BUDGET_WINDOW,
        ));
        let lifecycle = TwitchLifecycle::new();
        let platform = Arc::new(TwitchPlatform::new(
            config.clone(),
            Arc::clone(&creds),
            Arc::clone(&credentials_manager),
            tracker.clone(),
            Arc::clone(&rate_limiter),
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
            bus: Arc::new(crate::event_channel::PlatformEventChannel::new()),
            creds,
            credentials_manager,
            viewer_state: std::sync::RwLock::new(ViewerPollState::default()),
            viewer_report_tx,
            transport,
            rate_limiter,
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
        let usage = self.rate_limiter.usage();
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

const TIER_LOCKED_REASON: &str = "Requires Twitch Affiliate or Partner";
const SHIELD_SCOPE_MISSING_REASON: &str = "Sign in to Twitch again to grant Shield mode access";

fn blank() -> Variant {
    Variant::String(String::new())
}

fn config(pairs: impl IntoIterator<Item = (&'static str, Variant)>) -> BTreeMap<String, Variant> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn group_badge(group: &str) -> (SectionIcon, QuickActionAccent) {
    match group {
        "Stream info" => (SectionIcon::new("edit"), QuickActionAccent::Info),
        "Polls" => (SectionIcon::new("chart-bar"), QuickActionAccent::Brand),
        "Predictions" => (
            SectionIcon::new("crystal-ball"),
            QuickActionAccent::AccentPinkLight,
        ),
        "Chat" => (SectionIcon::new("message-2"), QuickActionAccent::Brand),
        "Moderation" => (SectionIcon::new("shield"), QuickActionAccent::Danger),
        "Raids & ads" => (SectionIcon::new("businessplan"), QuickActionAccent::Bits),
        "Channel Points" => (
            SectionIcon::new("diamond"),
            QuickActionAccent::AccentPinkLight,
        ),
        _ => (SectionIcon::new("dot"), QuickActionAccent::Brand),
    }
}

fn text_field(key: &str, label: &str, default: &str) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Text,
        default: Some(QuickActionFieldValue::Text(default.to_owned())),
        placeholder: None,
        hint: None,
        required: false,
    }
}

pub(crate) fn text_field_placeholder(
    key: &str,
    label: &str,
    default: &str,
    placeholder: &str,
) -> QuickActionField {
    QuickActionField {
        placeholder: Some(placeholder.to_owned()),
        ..text_field(key, label, default)
    }
}

fn multiline_field(key: &str, label: &str, default: &str) -> QuickActionField {
    QuickActionField {
        kind: QuickActionFieldKind::Multiline,
        ..text_field(key, label, default)
    }
}

pub(crate) fn toggle_field(key: &str, label: &str, default: bool) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Toggle,
        default: Some(QuickActionFieldValue::Toggle(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

pub(crate) fn int_field(
    key: &str,
    label: &str,
    default: i64,
    min: i64,
    max: i64,
) -> QuickActionField {
    QuickActionField {
        key: key.to_owned(),
        label: label.to_owned(),
        kind: QuickActionFieldKind::Int { min, max },
        default: Some(QuickActionFieldValue::Int(default)),
        placeholder: None,
        hint: None,
        required: false,
    }
}

fn choice_field(
    key: &str,
    label: &str,
    default: &str,
    options: &[(&str, &str)],
) -> QuickActionField {
    let options = options
        .iter()
        .map(|(value, label)| QuickActionChoiceOption {
            value: (*value).to_owned(),
            label: (*label).to_owned(),
        })
        .collect();
    QuickActionField {
        kind: QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)),
        ..text_field(key, label, default)
    }
}

fn choice_field_hint(
    key: &str,
    label: &str,
    default: &str,
    options: &[(&str, &str)],
    hint: &str,
) -> QuickActionField {
    QuickActionField {
        hint: Some(hint.to_owned()),
        ..choice_field(key, label, default, options)
    }
}

#[allow(clippy::too_many_arguments)]
fn quick_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    locked_reason: Option<String>,
    group: &str,
    destructive: bool,
    kind_id: &str,
    config: BTreeMap<String, Variant>,
    fields: Vec<QuickActionField>,
) -> QuickAction {
    let (group_icon, group_accent) = group_badge(group);
    QuickAction {
        label: label.to_owned(),
        icon: SectionIcon::new(icon),
        enabled,
        locked_reason,
        liveness: QuickActionLiveness::Unknown,
        group: Some(group.to_owned()),
        group_icon: Some(group_icon),
        group_accent: Some(group_accent),
        destructive,
        accent,
        subaction_template: SubActionStep {
            kind_id: kind_id.to_owned(),
            config,
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        },
        picker: None,
        fields,
        collection: None,
    }
}

fn manage_collection_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
    group: &str,
    collection: CollectionId,
) -> QuickAction {
    QuickAction {
        collection: Some(collection),
        ..quick_action(
            label,
            icon,
            accent,
            enabled,
            None,
            group,
            false,
            "",
            BTreeMap::new(),
            Vec::new(),
        )
    }
}

fn gated_on(action: QuickAction, liveness: QuickActionLiveness) -> QuickAction {
    QuickAction { liveness, ..action }
}

fn shield_mode_toggle(connected: bool, phase: ShieldPhase) -> QuickAction {
    let (label, icon, kind_id, liveness) = match phase {
        ShieldPhase::Active => (
            "End Shield mode",
            "shield-off",
            "twitch.moderation.shield_mode_off",
            QuickActionLiveness::Live,
        ),
        ShieldPhase::Inactive | ShieldPhase::Unknown | ShieldPhase::Unauthorized => (
            "Start Shield mode",
            "shield",
            "twitch.moderation.shield_mode_on",
            QuickActionLiveness::Unknown,
        ),
    };
    let unauthorized = phase == ShieldPhase::Unauthorized;
    gated_on(
        quick_action(
            label,
            icon,
            QuickActionAccent::Brand,
            connected && !unauthorized,
            unauthorized.then(|| SHIELD_SCOPE_MISSING_REASON.to_owned()),
            "Moderation",
            false,
            kind_id,
            BTreeMap::new(),
            Vec::new(),
        ),
        liveness,
    )
}

impl QuickActions for TwitchIntegrationBundle {
    fn revisions(&self) -> Option<CollectionRevisions> {
        Some(self.lifecycle.quick_action_revisions())
    }

    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.is_chat_connected();
        let tier_locked = self.tier() == BroadcasterTier::Standard;
        let locked_reason = tier_locked.then(|| TIER_LOCKED_REASON.to_owned());
        let gated = connected && !tier_locked;
        let live: LifecycleSnapshot = self.lifecycle.snapshot();

        vec![
            quick_action(
                "Update title",
                "edit",
                QuickActionAccent::Info,
                connected,
                None,
                "Stream info",
                false,
                "twitch.channel.update_title",
                config([("title", blank())]),
                vec![
                    text_field("title", "Stream title", "GTNH: Aluminium grind continues")
                        .required(),
                ],
            ),
            quick_action(
                "Set category / game",
                "device-gamepad-2",
                QuickActionAccent::Brand,
                connected,
                None,
                "Stream info",
                false,
                "twitch.channel.update_category",
                config([("category_id", blank())]),
                vec![text_field("category_id", "Category", "Minecraft").required()],
            ),
            quick_action(
                "Update tags",
                "tags",
                QuickActionAccent::Success,
                connected,
                None,
                "Stream info",
                false,
                "twitch.channel.update_tags",
                config([("tags", blank())]),
                vec![text_field(
                    "tags",
                    "Tags (comma-sep)",
                    "Modded, Chill, English",
                )],
            ),
            quick_action(
                "Create stream marker",
                "bookmark",
                QuickActionAccent::Bits,
                connected,
                None,
                "Stream info",
                false,
                "twitch.channel.create_marker",
                config([("description", blank())]),
                vec![text_field_placeholder(
                    "description",
                    "Marker note",
                    "Nice moment",
                    "optional",
                )],
            ),
            gated_on(
                quick_action(
                    "Start poll",
                    "chart-bar",
                    QuickActionAccent::Brand,
                    gated,
                    locked_reason.clone(),
                    "Polls",
                    false,
                    "twitch.poll.start",
                    config([
                        ("title", blank()),
                        ("choices", blank()),
                        ("duration_seconds", Variant::Int(60)),
                    ]),
                    vec![
                        text_field("title", "Question", "Next game?").required(),
                        multiline_field(
                            "choices",
                            "Choices (one per line)",
                            "Factorio\nMinecraft\nSatisfactory",
                        )
                        .required(),
                        toggle_field(
                            "channel_points_voting_enabled",
                            "Channel Points voting",
                            false,
                        ),
                        int_field("duration_seconds", "Duration (seconds)", 60, 15, 1800),
                    ],
                ),
                live.poll_slot_free(),
            ),
            gated_on(
                quick_action(
                    "End poll (finish now)",
                    "player-stop",
                    QuickActionAccent::Warning,
                    gated,
                    locked_reason.clone(),
                    "Polls",
                    false,
                    "twitch.poll.end",
                    config([
                        ("poll_id", blank()),
                        ("status", Variant::String("terminated".to_owned())),
                    ]),
                    Vec::new(),
                ),
                live.poll_in_flight(),
            ),
            gated_on(
                quick_action(
                    "Cancel poll",
                    "x",
                    QuickActionAccent::Danger,
                    gated,
                    locked_reason.clone(),
                    "Polls",
                    true,
                    "twitch.poll.end",
                    config([
                        ("poll_id", blank()),
                        ("status", Variant::String("archived".to_owned())),
                    ]),
                    Vec::new(),
                ),
                live.poll_in_flight(),
            ),
            gated_on(
                quick_action(
                    "Start prediction",
                    "crystal-ball",
                    QuickActionAccent::AccentPinkLight,
                    gated,
                    locked_reason.clone(),
                    "Predictions",
                    false,
                    "twitch.prediction.start",
                    config([
                        ("title", blank()),
                        ("outcomes", blank()),
                        ("prediction_window_seconds", Variant::Int(120)),
                    ]),
                    vec![
                        text_field("title", "Title", "Will we beat the boss?").required(),
                        multiline_field(
                            "outcomes",
                            "Outcomes (one per line)",
                            "Yes, easy\nNo, we die",
                        )
                        .required(),
                        int_field(
                            "prediction_window_seconds",
                            "Window (seconds)",
                            120,
                            30,
                            1800,
                        ),
                    ],
                ),
                live.prediction_slot_free(),
            ),
            gated_on(
                quick_action(
                    "Lock prediction",
                    "lock",
                    QuickActionAccent::Warning,
                    gated,
                    locked_reason.clone(),
                    "Predictions",
                    false,
                    "twitch.prediction.lock",
                    config([("prediction_id", blank())]),
                    Vec::new(),
                ),
                live.prediction_lockable(),
            ),
            gated_on(
                quick_action(
                    "Resolve / pay out",
                    "trophy",
                    QuickActionAccent::Success,
                    gated,
                    locked_reason.clone(),
                    "Predictions",
                    false,
                    "twitch.prediction.resolve",
                    config([("prediction_id", blank()), ("winning_outcome_id", blank())]),
                    Vec::new(),
                ),
                live.prediction_settleable(),
            ),
            gated_on(
                quick_action(
                    "Cancel & refund",
                    "x",
                    QuickActionAccent::Danger,
                    gated,
                    locked_reason.clone(),
                    "Predictions",
                    true,
                    "twitch.prediction.cancel",
                    config([("prediction_id", blank())]),
                    Vec::new(),
                ),
                live.prediction_settleable(),
            ),
            quick_action(
                "Send message",
                "send",
                QuickActionAccent::Brand,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.send_message",
                config([
                    ("message", blank()),
                    ("target", Variant::String("twitch".to_owned())),
                ]),
                vec![multiline_field("message", "Message", "Hey chat \u{1f44b}").required()],
            ),
            quick_action(
                "Announcement",
                "speakerphone",
                QuickActionAccent::Warning,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.send_announcement",
                config([
                    ("message", blank()),
                    ("color", Variant::String("primary".to_owned())),
                ]),
                vec![
                    multiline_field("message", "Announcement", "Big news!").required(),
                    choice_field(
                        "color",
                        "Color",
                        "primary",
                        &[
                            ("primary", "Primary"),
                            ("blue", "Blue"),
                            ("green", "Green"),
                            ("orange", "Orange"),
                            ("purple", "Purple"),
                        ],
                    ),
                ],
            ),
            quick_action(
                "Clear chat",
                "message-off",
                QuickActionAccent::Danger,
                connected,
                None,
                "Chat",
                true,
                "twitch.chat.clear",
                BTreeMap::new(),
                Vec::new(),
            ),
            quick_action(
                "Slow mode",
                "clock-hour-4",
                QuickActionAccent::Info,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.set_mode",
                config([
                    ("slow_mode", Variant::String("on".to_owned())),
                    ("slow_mode_wait_seconds", Variant::Int(10)),
                ]),
                vec![int_field(
                    "slow_mode_wait_seconds",
                    "Wait time (seconds)",
                    10,
                    3,
                    120,
                )],
            ),
            quick_action(
                "Followers-only mode",
                "user-check",
                QuickActionAccent::Success,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.set_mode",
                config([
                    ("follower_mode", Variant::String("on".to_owned())),
                    ("follower_mode_min_minutes", Variant::Int(10)),
                ]),
                vec![int_field(
                    "follower_mode_min_minutes",
                    "Min. account age (minutes)",
                    10,
                    0,
                    129_600,
                )],
            ),
            quick_action(
                "Emote-only mode",
                "mood-smile",
                QuickActionAccent::Bits,
                connected,
                None,
                "Chat",
                false,
                "twitch.chat.set_mode",
                config([("emote_only", Variant::String("on".to_owned()))]),
                vec![choice_field(
                    "emote_only",
                    "Emote-only mode",
                    "on",
                    &[("unchanged", "Unchanged"), ("on", "On"), ("off", "Off")],
                )],
            ),
            quick_action(
                "Timeout user",
                "clock-pause",
                QuickActionAccent::Warning,
                connected,
                None,
                "Moderation",
                false,
                "twitch.moderation.timeout_user",
                config([
                    ("target_user_login", blank()),
                    ("duration_seconds", Variant::Int(600)),
                    ("reason", blank()),
                ]),
                vec![
                    text_field("target_user_login", "Username", "@spammer").required(),
                    int_field("duration_seconds", "Duration (seconds)", 600, 1, 1_209_600),
                    text_field_placeholder("reason", "Reason", "", "optional"),
                ],
            ),
            quick_action(
                "Ban user",
                "ban",
                QuickActionAccent::Danger,
                connected,
                None,
                "Moderation",
                true,
                "twitch.moderation.ban_user",
                config([("target_user_login", blank()), ("reason", blank())]),
                vec![
                    text_field("target_user_login", "Username", "@baduser").required(),
                    text_field("reason", "Reason", ""),
                ],
            ),
            quick_action(
                "Unban user",
                "lock-open",
                QuickActionAccent::Success,
                connected,
                None,
                "Moderation",
                false,
                "twitch.moderation.unban_user",
                config([("target_user_login", blank())]),
                vec![text_field("target_user_login", "Username", "@user").required()],
            ),
            shield_mode_toggle(connected, self.lifecycle.shield_phase()),
            quick_action(
                "Add / remove VIP",
                "star",
                QuickActionAccent::Warning,
                connected,
                None,
                "Moderation",
                false,
                "twitch.moderation.add_vip",
                config([("target_user_login", blank())]),
                vec![text_field("target_user_login", "Username", "@loyalfan").required()],
            ),
            quick_action(
                "Add / remove Mod",
                "sword",
                QuickActionAccent::Info,
                connected,
                None,
                "Moderation",
                false,
                "twitch.moderation.add_moderator",
                config([("target_user_login", blank())]),
                vec![text_field("target_user_login", "Username", "@trustedmod").required()],
            ),
            gated_on(
                quick_action(
                    "Start raid",
                    "flag",
                    QuickActionAccent::Bits,
                    connected,
                    None,
                    "Raids & ads",
                    false,
                    "twitch.channel.start_raid",
                    config([("to_broadcaster_login", blank())]),
                    vec![
                        text_field("to_broadcaster_login", "Raid target", "@factorio_streamer")
                            .required(),
                    ],
                ),
                live.raid_slot_free(),
            ),
            gated_on(
                quick_action(
                    "Cancel raid",
                    "flag-off",
                    QuickActionAccent::Danger,
                    connected,
                    None,
                    "Raids & ads",
                    true,
                    "twitch.channel.cancel_raid",
                    BTreeMap::new(),
                    Vec::new(),
                ),
                live.raid_in_flight(),
            ),
            quick_action(
                "Send shoutout",
                "speakerphone",
                QuickActionAccent::Warning,
                connected,
                None,
                "Raids & ads",
                false,
                "twitch.channel.send_shoutout",
                config([("to_broadcaster_login", blank())]),
                vec![
                    text_field("to_broadcaster_login", "Channel", "@factorio_streamer").required(),
                ],
            ),
            quick_action(
                "Run commercial",
                "clock",
                QuickActionAccent::Info,
                connected && !tier_locked,
                locked_reason.clone(),
                "Raids & ads",
                false,
                "twitch.channel.run_ad",
                config([("duration_seconds", Variant::String("90".to_owned()))]),
                vec![
                    choice_field_hint(
                        "duration_seconds",
                        "Duration",
                        "90",
                        &[
                            ("30", "30 seconds"),
                            ("60", "60 seconds"),
                            ("90", "90 seconds"),
                            ("120", "120 seconds"),
                            ("150", "150 seconds"),
                            ("180", "180 seconds"),
                        ],
                        "8+ min cooldown between ads",
                    )
                    .required(),
                ],
            ),
            quick_action(
                "Snooze next ad",
                "player-skip-forward",
                QuickActionAccent::Success,
                connected,
                None,
                "Raids & ads",
                false,
                "twitch.channel.snooze_ad",
                BTreeMap::new(),
                Vec::new(),
            ),
            manage_collection_action(
                "Manage rewards",
                "adjustments",
                QuickActionAccent::AccentPinkLight,
                connected,
                "Channel Points",
                CollectionId::new(REWARDS_COLLECTION),
            ),
            quick_action(
                "Fulfill redemption",
                "check",
                QuickActionAccent::Success,
                connected,
                None,
                "Channel Points",
                false,
                "twitch.channel_points.fulfill_redemption",
                config([("redemption_id", blank()), ("reward_id", blank())]),
                Vec::new(),
            ),
            quick_action(
                "Reject & refund",
                "x",
                QuickActionAccent::Danger,
                connected,
                None,
                "Channel Points",
                true,
                "twitch.channel_points.cancel_redemption",
                config([("redemption_id", blank()), ("reward_id", blank())]),
                Vec::new(),
            ),
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
        let HealthValue::Status {
            label,
            active,
            detail,
        } = &metrics[1].value
        else {
            panic!("expected Status variant for EventSub metric");
        };
        assert_eq!(label, "1 subs");
        assert!(*active, "one active subscription must light EventSub green");
        assert_eq!(detail.as_deref(), Some("WebSocket"));
    }

    #[test]
    fn eventsub_metric_inactive_when_no_subscriptions_are_active() {
        let tracker = SubscriptionTracker::default();
        {
            let mut records = tracker.write().unwrap();
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
        let HealthValue::Status { label, active, .. } = &metrics[1].value else {
            panic!("expected Status variant for EventSub metric");
        };
        assert_eq!(label, "0 subs");
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
        let HealthValue::Ratio { used, total, .. } = &metrics[3].value else {
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
            Arc::new(NullCreds),
            BroadcasterTier::Standard,
            Arc::clone(&transport) as Arc<dyn HelixTransport>,
            broadcaster_id,
        );
        (bundle, transport)
    }

    async fn settle_spawned_tasks() {
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
    }

    fn goal_requests(
        transport: &crate::sub_actions::test_support::MockTransport,
    ) -> Vec<HelixRequest> {
        (0..transport.call_count())
            .map(|index| transport.request(index))
            .filter(|request| request.path == "/helix/goals")
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
            settle_spawned_tasks().await;
            assert_eq!(
                goal_requests(&transport).len(),
                expected_syncs,
                "after {state:?}"
            );
        }
    }

    #[tokio::test]
    async fn creator_goal_sync_asks_for_the_configured_broadcaster() {
        let (b, transport) = bundle_recording_helix("4242");

        b.on_chat_state_changed(ChatConnectionState::Connected);
        settle_spawned_tasks().await;

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
        assert_eq!(status.hero_name(), Some("streamer"));
        assert_ne!(status.hero_name(), Some(status.display_name()));
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
            assert_eq!(badges[0].label, "user_id 1");
            assert!(badges[0].monospace);
            let tier_badge = badges
                .iter()
                .find(|badge| badge.label == tier.label())
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

        let find_commercial = |bundle: &TwitchIntegrationBundle| {
            bundle
                .actions()
                .into_iter()
                .find(|a| a.subaction_template.kind_id == "twitch.channel.run_ad")
                .unwrap()
        };

        let locked = find_commercial(&standard);
        assert_eq!(locked.locked_reason.as_deref(), Some(TIER_LOCKED_REASON));
        assert!(
            !locked.enabled,
            "commercial must be disabled under standard"
        );

        let unlocked = find_commercial(&affiliate);
        assert!(unlocked.locked_reason.is_none());
        assert!(unlocked.enabled, "commercial must be enabled at affiliate");

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
    fn every_poll_and_prediction_action_locks_for_standard_and_unlocks_for_affiliate() {
        let standard =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Standard);
        let affiliate =
            make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let is_gated =
            |a: &QuickAction| matches!(a.group.as_deref(), Some("Polls" | "Predictions"));

        let standard_actions = standard.actions();
        let gated: Vec<&QuickAction> = standard_actions.iter().filter(|a| is_gated(a)).collect();
        assert!(
            gated.len() >= 6,
            "expected every Polls and Predictions action, got {}",
            gated.len()
        );
        for a in gated {
            assert_eq!(
                a.locked_reason.as_deref(),
                Some(TIER_LOCKED_REASON),
                "{} must lock under standard tier",
                a.label
            );
            assert!(
                !a.enabled,
                "{} must be disabled under standard tier",
                a.label
            );
        }

        for a in affiliate.actions().iter().filter(|a| is_gated(a)) {
            assert!(
                a.locked_reason.is_none(),
                "{} must unlock at affiliate tier",
                a.label
            );
            assert!(a.enabled, "{} must be enabled at affiliate tier", a.label);
        }
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
            .map(|a| (a.label, a.liveness))
            .collect();

        for (label, expected) in [
            ("Start poll", QuickActionLiveness::Absent),
            ("End poll (finish now)", QuickActionLiveness::Live),
            ("Cancel poll", QuickActionLiveness::Live),
            ("Start prediction", QuickActionLiveness::Absent),
            ("Lock prediction", QuickActionLiveness::Absent),
            ("Resolve / pay out", QuickActionLiveness::Live),
            ("Cancel & refund", QuickActionLiveness::Live),
            ("Start raid", QuickActionLiveness::Absent),
            ("Cancel raid", QuickActionLiveness::Live),
        ] {
            assert_eq!(
                by_label.get(label),
                Some(&expected),
                "{label} with an open poll, a locked prediction and a pending raid"
            );
        }
    }

    #[test]
    fn exactly_six_actions_are_marked_destructive() {
        let b = make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let mut destructive: Vec<String> = b
            .actions()
            .into_iter()
            .filter(|a| a.destructive)
            .map(|a| a.label)
            .collect();
        destructive.sort();
        let mut expected = vec![
            "Ban user".to_owned(),
            "Cancel & refund".to_owned(),
            "Cancel poll".to_owned(),
            "Cancel raid".to_owned(),
            "Clear chat".to_owned(),
            "Reject & refund".to_owned(),
        ];
        expected.sort();
        assert_eq!(destructive, expected);
    }

    #[test]
    fn action_groups_appear_in_the_expected_order() {
        let b = make_bundle_with_tier(ChatConnectionState::Connected, BroadcasterTier::Affiliate);
        let mut groups: Vec<String> = Vec::new();
        for a in b.actions() {
            if let Some(group) = a.group
                && !groups.contains(&group)
            {
                groups.push(group);
            }
        }
        assert_eq!(
            groups,
            [
                "Stream info",
                "Polls",
                "Predictions",
                "Chat",
                "Moderation",
                "Raids & ads",
                "Channel Points",
            ]
            .map(str::to_owned)
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
        use forge_registry::FormField::*;
        let key = match field {
            Text { key, .. }
            | TextArea { key, .. }
            | Code { key, .. }
            | Integer { key, .. }
            | Toggle { key, .. }
            | FilePicker { key, .. }
            | DateTime { key, .. }
            | Select { key, .. }
            | UnitAmount { key, .. }
            | Duration { key, .. }
            | DynamicSelect { key, .. }
            | DependentSelect { key, .. }
            | Optional { key, .. }
            | SubChain { key, .. }
            | CaseList { key, .. }
            | Slider { key, .. }
            | Swatch { key, .. } => *key,
        };
        out.insert(key.to_owned());
        if let Optional { inner, .. } = field {
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
        let creds: Arc<dyn CredentialsRepo> = Arc::new(NullCreds);
        let platform = Arc::new(TwitchPlatform::new(
            ChatSessionConfig {
                client_id: "test-client".to_owned(),
                broadcaster_id: "1".to_owned(),
                user_id: "1".to_owned(),
                endpoints: crate::sub_actions::test_support::unreachable_twitch_endpoints(),
            },
            Arc::clone(&creds),
            Arc::new(TwitchCredentialsManager::new(
                &crate::endpoints_test_support::unreachable_endpoints(),
                creds,
                "test-client".to_owned(),
            )),
            SubscriptionTracker::default(),
            Arc::new(TokenBucketRateLimiter::new(
                HELIX_BUDGET_CAPACITY,
                HELIX_BUDGET_WINDOW,
            )),
            TwitchLifecycle::new(),
        ));
        let bundle = TwitchIntegrationBundle::new(Some("streamer".to_owned()), platform);

        bundle.shutdown().await;
        bundle.shutdown().await;

        let released = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while Arc::strong_count(&bundle) > 1 {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(
            released.is_ok(),
            "{} task(s) still hold the bundle after shutdown",
            Arc::strong_count(&bundle) - 1
        );
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
    const SETTLE: Duration = Duration::from_secs(1);

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
        let repo: Arc<dyn CredentialsRepo> = crate::credentials_manager::tests::InMemRepo::seeded(
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
        inner.credentials_manager = Arc::new(TwitchCredentialsManager::with_endpoint(
            repo,
            "test-client".to_owned(),
            format!("{}/token", server.uri()),
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
        tokio::time::timeout(SETTLE, health.next())
            .await
            .ok()
            .flatten()
    }

    #[tokio::test]
    async fn token_refresh_updates_the_reported_expiry_and_emits_one_api_calls_delta_per_change() {
        let mut fx = expiry_bridge_fixture().await;
        let mut failed_access = "existing_access";
        for _ in 0..2 {
            let renewed = refresh_stale_token(&fx.bundle, failed_access).await;
            failed_access = "new_access";
            tokio::time::pause();

            let delta = next_delta(&mut fx.health).await;
            assert_eq!(
                delta.map(|delta| delta.index),
                Some(API_CALLS_METRIC_INDEX),
                "a refresh must emit an API calls delta"
            );
            assert!(
                next_delta(&mut fx.health).await.is_none(),
                "one refresh must emit exactly one delta"
            );
            assert_eq!(BuiltinStatus::token_expiry(fx.bundle.as_ref()), renewed);
            tokio::time::resume();
        }
    }

    #[tokio::test]
    async fn token_expiry_stays_unreported_until_a_refresh_publishes_one() {
        let mut fx = expiry_bridge_fixture().await;
        tokio::time::pause();

        assert!(next_delta(&mut fx.health).await.is_none());
        assert_eq!(BuiltinStatus::token_expiry(fx.bundle.as_ref()), None);
    }

    #[tokio::test]
    async fn expiry_bridge_ignores_refreshes_after_the_bundle_is_retired() {
        let mut fx = expiry_bridge_fixture().await;
        let before = BuiltinStatus::token_expiry(fx.bundle.as_ref());
        fx.bundle.shutdown().await;

        refresh_stale_token(&fx.bundle, "existing_access").await;
        tokio::time::pause();

        assert!(next_delta(&mut fx.health).await.is_none());
        assert_eq!(BuiltinStatus::token_expiry(fx.bundle.as_ref()), before);
    }
}
