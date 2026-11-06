use forge_types::{IntegrationId, MessageRef, RecoverRwLock, UserText, VerbatimText};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock, Weak};
use std::time::{Duration, SystemTime};

use forge_platform_core::{
    BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus, CapabilityFlags, ChatPlatform,
    ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthMetricId,
    HealthStream, HealthText, HealthValue, HeroBadge, HeroBadgeTone, LiveViewerSource,
    ViewerReport,
};
use forge_platform_core::{PlatformError, TokenSource};
use tokio::sync::{broadcast, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;

use crate::account::credentials_manager::YouTubeCredentialsManager;
use crate::api::quota_state::{QUOTA_DAILY_LIMIT, SharedQuota, today_pacific, until_next_reset};
use crate::broadcast::follow_lookup::YouTubeFollowLookup;
use crate::broadcast::viewer_poll::{YouTubeViewerPoll, YouTubeViewerSource};
use crate::chat::chat_platform::YouTubePlatform;
use crate::vocabulary::messages;

const QUOTA_RESET_WAKE_MARGIN: Duration = Duration::from_secs(5);

const LIVE_CHAT_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LIVE_CHAT);
const EVENTS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_EVENTS);
const API_CALLS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_API_CALLS);

pub struct YouTubeIntegrationBundle {
    id: IntegrationId,
    hero_name: RwLock<Option<String>>,
    identity_blocked_on_quota: AtomicBool,
    quota_wake_scheduled: AtomicBool,
    this: Weak<Self>,
    last_quota_used: AtomicU32,
    token_expires_at: RwLock<Option<SystemTime>>,
    health_tx: broadcast::Sender<HealthDelta>,
    platform: Arc<YouTubePlatform>,
    credentials_manager: Arc<YouTubeCredentialsManager>,
    quota: SharedQuota,
    follow_lookup: YouTubeFollowLookup,
    viewer_report_tx: watch::Sender<ViewerReport>,
    viewer_report_rx: watch::Receiver<ViewerReport>,
    state_rx: watch::Receiver<ConnectionState>,
    retired: CancellationToken,
}

impl YouTubeIntegrationBundle {
    pub fn new(
        platform: Arc<YouTubePlatform>,
        credentials_manager: Arc<YouTubeCredentialsManager>,
    ) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
        let (viewer_report_tx, _) = watch::channel(ViewerReport::Absent);
        let viewer_report_rx = viewer_report_tx.subscribe();
        let state_rx = platform.state_receiver();

        let token_source = Self::token_source(&credentials_manager);
        let quota = platform.api().quota().clone();
        let follow_lookup = YouTubeFollowLookup::new(
            platform.api().clone(),
            token_source.clone(),
            platform.channel_handle().clone(),
        );
        let retired = CancellationToken::new();

        Arc::new_cyclic(|this| Self {
            id: crate::YOUTUBE_INTEGRATION.id,
            hero_name: RwLock::new(None),
            identity_blocked_on_quota: AtomicBool::new(false),
            quota_wake_scheduled: AtomicBool::new(false),
            this: this.clone(),
            last_quota_used: AtomicU32::new(0),
            token_expires_at: RwLock::new(None),
            health_tx,
            platform,
            credentials_manager,
            quota,
            follow_lookup,
            viewer_report_tx,
            viewer_report_rx,
            state_rx,
            retired,
        })
    }

    fn token_source(manager: &Arc<YouTubeCredentialsManager>) -> TokenSource {
        let manager = Arc::clone(manager);
        Arc::new(move || {
            let manager = Arc::clone(&manager);
            Box::pin(async move { manager.get_valid_access_token().await })
        })
    }

    pub fn start_background_tasks(bundle: &Arc<Self>) {
        Self::spawn_viewer_poll(bundle);
        Self::spawn_health_bridge(bundle);
        Self::spawn_viewer_health_bridge(bundle);
        Self::spawn_identity_refresh(bundle);
        Self::spawn_token_expiry_bridge(bundle);
    }

    fn spawn_viewer_poll(bundle: &Arc<Self>) {
        let viewer_poll = YouTubeViewerPoll::new(
            bundle.platform.api().clone(),
            Self::token_source(&bundle.credentials_manager),
            bundle.platform.active_broadcast_id(),
            bundle.viewer_report_tx.clone(),
        );
        let retired = bundle.retired.clone();
        tokio::spawn(async move {
            tokio::select! {
                biased;
                () = retired.cancelled() => {}
                () = viewer_poll.run() => {}
            }
        });
    }

    pub fn viewer_source(&self) -> Box<dyn LiveViewerSource> {
        Box::new(YouTubeViewerSource::new(self.viewer_report_tx.subscribe()))
    }

    pub async fn shutdown(&self) {
        self.retired.cancel();
        if let Err(error) = self.platform.disconnect().await {
            tracing::warn!(%error, "youtube chat disconnect failed at shutdown");
        }
    }

    fn current_state(&self) -> ConnectionState {
        self.platform.connection_state()
    }

    fn spawn_health_bridge(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        let mut state_rx = bundle.state_rx.clone();
        tokio::spawn(async move {
            let mut previous = *state_rx.borrow();
            loop {
                tokio::select! {
                    biased;
                    () = bundle.retired.cancelled() => break,
                    changed = state_rx.changed() => if changed.is_err() { break },
                }
                let current = *state_rx.borrow();

                let chat_delta = HealthDelta {
                    metric: LIVE_CHAT_METRIC,
                    new_value: chat_poller_health_value(current),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(chat_delta);

                let events_delta = HealthDelta {
                    metric: EVENTS_METRIC,
                    new_value: events_health_value(current),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(events_delta);

                if current.is_connected() && !previous.is_connected() {
                    Self::spawn_identity_refresh(&bundle);
                }
                previous = current;
            }
        });
    }

    fn spawn_viewer_health_bridge(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        let mut reports_rx = bundle.viewer_report_tx.subscribe();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    () = bundle.retired.cancelled() => break,
                    changed = reports_rx.changed() => if changed.is_err() { break },
                }
                let delta = HealthDelta {
                    metric: HealthMetricId::VIEWERS,
                    new_value: viewers_health_value(*reports_rx.borrow()),
                };
                #[expect(
                    clippy::let_underscore_must_use,
                    reason = "having no subscribers is a normal state"
                )]
                let _ = bundle.health_tx.send(delta);
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
                *bundle.token_expires_at.write_recover() = expiry;
            }
        });
    }

    fn spawn_identity_refresh(bundle: &Arc<Self>) {
        let bundle = Arc::clone(bundle);
        tokio::spawn(async move {
            bundle.refresh_identity().await;
        });
    }

    pub(crate) async fn refresh_identity(&self) {
        let Ok(Some(stored)) = self.credentials_manager.load().await else {
            return;
        };
        self.token_expires_at
            .write_recover()
            .get_or_insert_with(|| SystemTime::from(stored.expires_at));
        let stored = if stored.identity_pending() {
            match self.credentials_manager.resolve_identity().await {
                Ok(resolved) => {
                    self.identity_blocked_on_quota
                        .store(false, Ordering::Relaxed);
                    self.platform.channel_handle().adopt(&resolved.channel_id);
                    resolved
                }
                Err(PlatformError::QuotaExhausted) => {
                    self.identity_blocked_on_quota
                        .store(true, Ordering::Relaxed);
                    self.schedule_wake_at_quota_reset();
                    return;
                }
                Err(error) => {
                    tracing::warn!(%error, "youtube channel identity lookup failed");
                    return;
                }
            }
        } else {
            stored
        };
        let stored = if stored.channel_handle.is_some() {
            stored
        } else {
            self.credentials_manager
                .ensure_channel_handle()
                .await
                .unwrap_or(stored)
        };
        *self.hero_name.write_recover() = Some(
            handle_hero_name(stored.channel_handle.as_deref())
                .unwrap_or_else(|| stored.channel_title.clone()),
        );
    }

    fn schedule_wake_at_quota_reset(&self) {
        let Some(bundle) = self.this.upgrade() else {
            return;
        };
        if bundle.quota_wake_scheduled.swap(true, Ordering::AcqRel) {
            return;
        }
        let delay = until_next_reset(time::OffsetDateTime::now_utc()) + QUOTA_RESET_WAKE_MARGIN;
        tokio::spawn(async move {
            tokio::select! {
                biased;
                () = bundle.retired.cancelled() => return,
                () = tokio::time::sleep(delay) => {}
            }
            bundle.quota_wake_scheduled.store(false, Ordering::Release);
            if let Err(error) = BuiltinControl::reconnect(bundle.as_ref()).await {
                tracing::warn!(?error, "youtube reconnect after quota reset failed");
            }
        });
    }

    pub(crate) async fn settle_pending_identity(&self) {
        if self.platform.channel_handle().is_pending() {
            self.refresh_identity().await;
        }
    }

    pub(crate) fn credentials_manager(&self) -> &Arc<YouTubeCredentialsManager> {
        &self.credentials_manager
    }

    pub(crate) fn platform(&self) -> &Arc<YouTubePlatform> {
        &self.platform
    }

    pub(crate) fn follow_lookup(&self) -> &YouTubeFollowLookup {
        &self.follow_lookup
    }

    fn quota_used_today(&self) -> u32 {
        match self.quota.try_lock() {
            Ok(guard) => {
                let used = guard.used_on(today_pacific());
                self.last_quota_used.store(used, Ordering::Relaxed);
                used
            }
            Err(_) => self.last_quota_used.load(Ordering::Relaxed),
        }
    }

    fn quota_used_up(&self, used: u32) -> bool {
        used >= QUOTA_DAILY_LIMIT || self.identity_blocked_on_quota.load(Ordering::Relaxed)
    }

    fn quota_metric(&self) -> HealthMetric {
        let used = self.quota_used_today();
        let reset_hint = if self.quota_used_up(used) {
            messages::quota_exhausted()
        } else {
            messages::health_quota_resets_daily()
        };
        let value = HealthValue::Ratio {
            used: u64::from(used),
            total: u64::from(QUOTA_DAILY_LIMIT),
            reset_hint: Some(message_text(reset_hint)),
        };
        HealthMetric {
            id: API_CALLS_METRIC,
            value,
        }
    }
}

const LIVE_CHAT_ENDPOINT: &str = "liveChatMessages.list";

fn message_text(message: MessageRef) -> HealthText {
    HealthText::Text(message.into())
}

fn chat_poller_health_value(state: ConnectionState) -> HealthValue {
    HealthValue::Status {
        label: HealthText::Connection(state),
        active: state.is_connected(),
        detail: Some(HealthText::Text(
            VerbatimText::new(LIVE_CHAT_ENDPOINT).into(),
        )),
    }
}

fn events_health_value(state: ConnectionState) -> HealthValue {
    HealthValue::Status {
        label: HealthText::Connection(state),
        active: state.is_connected(),
        detail: Some(message_text(messages::health_shared_chat_feed())),
    }
}

fn viewers_health_value(report: ViewerReport) -> HealthValue {
    match report {
        ViewerReport::Live { count } => HealthValue::Text {
            primary: HealthText::Count(count),
            secondary: Some(message_text(messages::health_viewers_live())),
        },
        ViewerReport::Absent => HealthValue::Text {
            primary: HealthText::Count(0),
            secondary: None,
        },
    }
}

fn handle_hero_name(channel_handle: Option<&str>) -> Option<String> {
    channel_handle
        .map(str::trim)
        .filter(|handle| !handle.is_empty())
        .map(|handle| format!("@{}", handle.trim_start_matches('@')))
}

impl BuiltinStatus for YouTubeIntegrationBundle {
    fn id(&self) -> &IntegrationId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "YouTube"
    }

    fn version(&self) -> Option<String> {
        None
    }

    fn connection(&self) -> ConnectionState {
        self.current_state()
    }

    fn uptime(&self) -> Option<Duration> {
        None
    }

    fn endpoint(&self) -> Option<UserText> {
        Some(messages::ENDPOINT_SUMMARY.into())
    }

    fn capability_flags(&self) -> CapabilityFlags {
        if self.quota_used_up(self.quota_used_today()) {
            return CapabilityFlags {
                limited: true,
                label: Some(messages::quota_exhausted().into()),
            };
        }
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![HeaderAction::RefreshToken, HeaderAction::Disconnect]
    }

    fn token_expiry(&self) -> Option<SystemTime> {
        *self.token_expires_at.read_recover()
    }

    fn hero_name(&self) -> Option<String> {
        self.hero_name.read_recover().clone()
    }

    fn name_badges(&self) -> Vec<HeroBadge> {
        if self.platform.channel_handle().is_pending() {
            return Vec::new();
        }
        vec![HeroBadge {
            label: messages::channel_id_badge(&self.platform.channel_id()).into(),
            tone: HeroBadgeTone::Neutral,
            is_identifier: true,
        }]
    }

    fn credential_state(&self) -> forge_platform_core::CredentialState {
        self.credentials_manager.credential_state().current()
    }

    fn status_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
        Some(self.credentials_manager.credential_state().revisions())
    }
}

impl BuiltinHealth for YouTubeIntegrationBundle {
    fn metrics(&self) -> [HealthMetric; 4] {
        let state = self.current_state();
        [
            HealthMetric {
                id: LIVE_CHAT_METRIC,
                value: chat_poller_health_value(state),
            },
            HealthMetric {
                id: EVENTS_METRIC,
                value: events_health_value(state),
            },
            HealthMetric {
                id: HealthMetricId::VIEWERS,
                value: viewers_health_value(*self.viewer_report_rx.borrow()),
            },
            self.quota_metric(),
        ]
    }

    fn stream(&self) -> HealthStream {
        let rx = self.health_tx.subscribe();
        Box::pin(BroadcastStream::new(rx).filter_map(|r| r.ok()))
    }
}

impl BuiltinContent for YouTubeIntegrationBundle {
    fn sections(&self) -> Vec<DetailSection> {
        Vec::new()
    }

    fn content_revisions(&self) -> Option<forge_platform_core::CollectionRevisions> {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::register_youtube_triggers;
    use forge_registry::TriggerRegistry;
    use forge_storage::test_support::MemoryCredentials;
    use forge_types::OAuthToken;
    use std::collections::{BTreeMap, BTreeSet};

    use forge_registry::{FormField, SubActionRegistry};
    use forge_storage::CredentialsRepo;
    use futures::future::BoxFuture;
    use time::OffsetDateTime;
    use tokio_stream::StreamExt;

    use super::*;
    use crate::account::credentials::YouTubeCredentials;
    use crate::broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::broadcast::live_chat_id::LiveChatIdHandle;
    use crate::broadcast::stream_metadata::YouTubeStreamMetadata;
    use crate::chat::moderation::YouTubeModeration;
    use crate::chat::send_chat::YouTubeSendChat;
    use crate::sub_actions::register_youtube_sub_actions;
    use crate::vocabulary::labels;
    use forge_platform_core::{
        PlatformError, QuickActionChoiceSource, QuickActionFieldKind, QuickActionFieldValue,
        QuickActions,
    };

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn make_bundle(
        repo: Arc<dyn CredentialsRepo>,
    ) -> (Arc<YouTubeIntegrationBundle>, Arc<YouTubePlatform>) {
        make_bundle_with_quota(repo, SharedQuota::default())
    }

    fn make_bundle_with_quota(
        repo: Arc<dyn CredentialsRepo>,
        quota: SharedQuota,
    ) -> (Arc<YouTubeIntegrationBundle>, Arc<YouTubePlatform>) {
        let manager = Arc::new(YouTubeCredentialsManager::new(
            repo,
            crate::endpoints_test_support::unreachable_oauth_client(),
            crate::endpoints_test_support::unreachable_data_api(),
        ));
        let platform = Arc::new(YouTubePlatform::new(
            crate::endpoints_test_support::data_api_charging("http://127.0.0.1:0", quota),
            "UCabc123".to_owned(),
            Arc::clone(&manager),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
        ));
        let bundle = YouTubeIntegrationBundle::new(Arc::clone(&platform), manager);
        YouTubeIntegrationBundle::start_background_tasks(&bundle);
        (bundle, platform)
    }

    fn sub_action_registry() -> SubActionRegistry {
        let api = crate::endpoints_test_support::unreachable_data_api();
        let sender = Arc::new(YouTubeSendChat::new(
            api.clone(),
            token_source(),
            LiveChatIdHandle::new(),
        ));
        let moderation = Arc::new(YouTubeModeration::new(
            api.clone(),
            token_source(),
            crate::chat::ban_ledger_test_support::broadcaster_source(),
            LiveChatIdHandle::new(),
            crate::chat::ban_ledger_test_support::MemoryBanLedger::shared(),
        ));
        let metadata = Arc::new(YouTubeStreamMetadata::new(
            api.clone(),
            token_source(),
            ActiveBroadcastIdHandle::new(),
        ));
        let stream_stats = Arc::new(crate::broadcast::stream_stats::YouTubeStreamStats::new(
            api.clone(),
            token_source(),
            ActiveBroadcastIdHandle::new(),
        ));
        let ad_break = Arc::new(crate::broadcast::ad_break::YouTubeAdBreak::new(
            api.clone(),
            token_source(),
            ActiveBroadcastIdHandle::new(),
        ));
        let thumbnail = Arc::new(crate::broadcast::thumbnail::YouTubeThumbnail::new(
            api.clone(),
            token_source(),
            ActiveBroadcastIdHandle::new(),
        ));
        let channel_lookup = Arc::new(crate::broadcast::channel_lookup::YouTubeChannelLookup::new(
            api,
            token_source(),
        ));
        let mut reg = SubActionRegistry::new();
        register_youtube_sub_actions(
            &mut reg,
            sender,
            moderation,
            metadata,
            stream_stats,
            ad_break,
            thumbnail,
            channel_lookup,
        )
        .unwrap();
        reg
    }

    fn form_field_keys(field: &FormField, out: &mut BTreeSet<String>) {
        out.insert(field.key().to_owned());
        if let FormField::Optional { inner, .. } = field {
            form_field_keys(inner, out);
        }
    }

    fn stored_credential_json(
        channel_title: &str,
        channel_handle: Option<&str>,
        expires_at: OffsetDateTime,
    ) -> String {
        serde_json::to_string(&YouTubeCredentials {
            access_token: OAuthToken::new("access"),
            refresh_token: OAuthToken::new("refresh"),
            client_id: "client".to_owned(),
            channel_id: "UCabc123".to_owned(),
            channel_title: channel_title.to_owned(),
            channel_handle: channel_handle.map(str::to_owned),
            expires_at,
        })
        .unwrap()
    }

    fn signed_in_repo() -> Arc<MemoryCredentials> {
        Arc::new(MemoryCredentials::holding(
            crate::account::credentials::YOUTUBE_CREDENTIAL_ID,
            &stored_credential_json(
                "Live",
                None,
                OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap(),
            ),
        ))
    }

    #[test]
    fn all_kind_ids_are_reachable() {
        let mut reg = TriggerRegistry::new();
        register_youtube_triggers(&mut reg).unwrap();

        let ids = [
            "youtube.chat.message",
            "youtube.chat.command",
            "youtube.chat.super_chat",
            "youtube.chat.super_sticker",
            "youtube.channel.member",
            "youtube.channel.member_milestone",
            "youtube.channel.user_banned",
            "youtube.chat.message_deleted",
            "youtube.channel.member_gift",
            "youtube.channel.member_gift_received",
            "youtube.stream.online",
            "youtube.stream.offline",
            "youtube.stream.title_changed",
        ];

        for id in ids {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }

    #[test]
    fn chat_poller_health_value_maps_each_connection_state() {
        for (state, active) in [
            (ConnectionState::Connected, true),
            (ConnectionState::Connecting, false),
            (ConnectionState::Reconnecting, false),
            (ConnectionState::Disconnected, false),
        ] {
            assert_eq!(
                chat_poller_health_value(state),
                HealthValue::Status {
                    label: HealthText::Connection(state),
                    active,
                    detail: Some(HealthText::Text(
                        VerbatimText::new(LIVE_CHAT_ENDPOINT).into()
                    )),
                },
                "unexpected health value for {state:?}"
            );
        }
    }

    #[test]
    fn viewers_health_value_shows_count_when_live_and_zero_when_absent() {
        let live = || message_text(messages::health_viewers_live());
        for (report, primary, secondary) in [
            (ViewerReport::Live { count: 0 }, 0, Some(live())),
            (ViewerReport::Live { count: 1234 }, 1234, Some(live())),
            (ViewerReport::Absent, 0, None),
        ] {
            assert_eq!(
                viewers_health_value(report),
                HealthValue::Text {
                    primary: HealthText::Count(primary),
                    secondary,
                },
                "unexpected viewers value for {report:?}"
            );
        }
    }

    #[tokio::test]
    async fn every_quick_action_key_is_backed_by_its_runner_config() {
        let reg = sub_action_registry();
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));

        for action in QuickActions::actions(bundle.as_ref()) {
            let kind = &action.subaction_template.kind_id;
            assert!(
                reg.get(kind).is_some(),
                "quick action {:?} targets unregistered runner {kind}",
                action.label
            );
            let runner = reg.get(kind).unwrap();

            let mut runner_keys: BTreeSet<String> = runner.default_config().into_keys().collect();
            for field in runner.config_fields() {
                form_field_keys(&field, &mut runner_keys);
            }

            for config_key in action.subaction_template.config.keys() {
                assert!(
                    runner_keys.contains(config_key),
                    "quick action {:?}: config key {config_key:?} is not consumed by runner {kind}",
                    action.label
                );
            }
            for field in &action.fields {
                assert!(
                    runner_keys.contains(&field.key),
                    "quick action {:?}: field key {:?} is not consumed by runner {kind}",
                    action.label,
                    field.key
                );
            }
        }
    }

    fn filled_values(
        action: &forge_platform_core::QuickAction,
    ) -> BTreeMap<String, QuickActionFieldValue> {
        action
            .fields
            .iter()
            .filter(|field| field.default.is_none())
            .map(|field| {
                let sample = match field.key.as_str() {
                    "options" => "Pizza\nSushi",
                    "channel_id" => "UC1",
                    _ => "supplied",
                };
                (
                    field.key.clone(),
                    QuickActionFieldValue::Text(sample.to_owned()),
                )
            })
            .collect()
    }

    #[tokio::test]
    async fn every_partial_quick_action_template_merged_with_its_field_values_validates() {
        let reg = sub_action_registry();
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));

        for action in QuickActions::actions(bundle.as_ref()) {
            let step = action.merge_config(&filled_values(&action));
            let verdict = reg
                .get(&step.kind_id)
                .unwrap()
                .validate_config(&step.config);

            assert!(
                verdict.is_ok(),
                "quick action {:?} merges a config its runner rejects: {verdict:?}",
                action.label
            );
        }
    }

    #[tokio::test]
    async fn free_text_quick_action_fields_open_blank() {
        for action in QuickActions::actions(
            make_bundle(Arc::new(MemoryCredentials::default()))
                .0
                .as_ref(),
        ) {
            for field in &action.fields {
                if !matches!(
                    field.kind,
                    QuickActionFieldKind::Text | QuickActionFieldKind::Multiline
                ) || (action.subaction_template.kind_id == "youtube.stream.update_category"
                    && field.key == "category_id")
                {
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

    #[tokio::test]
    async fn static_choice_options_equal_runner_enum_and_include_default() {
        let reg = sub_action_registry();
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));

        let mut checked = 0;
        for action in QuickActions::actions(bundle.as_ref()) {
            let runner = reg.get(&action.subaction_template.kind_id).unwrap();
            let runner_selects: BTreeMap<String, BTreeSet<String>> = runner
                .config_fields()
                .into_iter()
                .filter_map(|f| match f {
                    FormField::Select { key, options, .. } => Some((
                        key.to_owned(),
                        options.iter().map(|o| (*o).to_owned()).collect(),
                    )),
                    _ => None,
                })
                .collect();

            for field in &action.fields {
                let QuickActionFieldKind::Choice(QuickActionChoiceSource::Static(options)) =
                    &field.kind
                else {
                    continue;
                };
                let option_values: BTreeSet<String> =
                    options.iter().map(|o| o.value.clone()).collect();

                if let Some(runner_options) = runner_selects.get(&field.key) {
                    assert_eq!(
                        &option_values, runner_options,
                        "quick action {:?}: choice {:?} options diverge from runner enum",
                        action.label, field.key
                    );
                }
                if let Some(QuickActionFieldValue::Text(default)) = &field.default {
                    assert!(
                        option_values.contains(default),
                        "quick action {:?}: default {default:?} is not among choice {:?} options",
                        action.label,
                        field.key
                    );
                }
                checked += 1;
            }
        }
        assert!(checked >= 1, "expected at least the privacy static choice");
    }

    #[tokio::test]
    async fn quick_action_groups_are_contiguous_in_design_order() {
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));
        let groups: Vec<String> = QuickActions::actions(bundle.as_ref())
            .into_iter()
            .filter_map(|a| a.group)
            .map(|group| group.label.code_or_text())
            .collect();

        let mut order = Vec::new();
        for group in &groups {
            if !order.contains(group) {
                order.push(group.clone());
            }
        }
        assert_eq!(
            order,
            vec![
                labels::BROADCAST.to_string(),
                labels::POLLS.to_string(),
                labels::CHAT.to_string(),
                labels::CONTENT.to_string(),
            ]
        );

        let mut seen = BTreeSet::new();
        let mut previous: Option<&String> = None;
        for group in &groups {
            if previous != Some(group) {
                assert!(
                    seen.insert(group.clone()),
                    "group {group:?} reappears after another group started: {groups:?}"
                );
            }
            previous = Some(group);
        }
    }

    #[tokio::test]
    async fn only_delete_message_and_ban_user_are_destructive() {
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));
        let destructive: BTreeSet<String> = QuickActions::actions(bundle.as_ref())
            .into_iter()
            .filter(|a| a.destructive)
            .map(|a| a.label.code_or_text())
            .collect();

        assert_eq!(
            destructive,
            BTreeSet::from([
                labels::DELETE_MESSAGE.to_string(),
                labels::BAN_USER.to_string()
            ])
        );
    }

    #[tokio::test]
    async fn metrics_declare_unique_catalogued_ids() {
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));

        let ids: std::collections::HashSet<_> = BuiltinHealth::metrics(bundle.as_ref())
            .into_iter()
            .map(|metric| metric.id)
            .collect();

        assert_eq!(ids.len(), 4);
        for id in &ids {
            assert!(
                *id == HealthMetricId::VIEWERS
                    || crate::vocabulary::messages::YOUTUBE_MESSAGES
                        .codes
                        .contains(id.label().code()),
                "{id} is not catalogued"
            );
        }
    }

    #[tokio::test]
    async fn api_calls_metric_reads_zero_once_the_pacific_day_has_rolled_over() {
        let previous_day = time::Date::from_calendar_date(2020, time::Month::January, 1).unwrap();
        let quota = SharedQuota::new(crate::api::quota_state::QuotaState {
            used_today: 9_500,
            last_reset_date: previous_day,
        });
        let (bundle, _platform) =
            make_bundle_with_quota(Arc::new(MemoryCredentials::default()), quota);

        let metric = bundle.quota_metric();

        assert!(
            matches!(metric.value, HealthValue::Ratio { used: 0, .. }),
            "got {:?}",
            metric.value
        );
    }

    #[tokio::test]
    async fn header_actions_offer_refresh_token_and_disconnect_only() {
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));
        assert_eq!(
            BuiltinStatus::header_actions(bundle.as_ref()),
            vec![HeaderAction::RefreshToken, HeaderAction::Disconnect]
        );
    }

    #[tokio::test]
    async fn name_badges_is_single_neutral_monospace_channel_id() {
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));
        assert_eq!(
            BuiltinStatus::name_badges(bundle.as_ref()),
            vec![HeroBadge {
                label: messages::channel_id_badge("UCabc123").into(),
                tone: HeroBadgeTone::Neutral,
                is_identifier: true,
            }]
        );
    }

    #[tokio::test]
    async fn identity_fields_are_absent_before_credential_loads() {
        let (bundle, _platform) = make_bundle(Arc::new(MemoryCredentials::default()));
        assert_eq!(BuiltinStatus::hero_name(bundle.as_ref()), None);
        assert_eq!(BuiltinStatus::token_expiry(bundle.as_ref()), None);
    }

    #[tokio::test]
    async fn refresh_identity_populates_hero_name_and_token_expiry() {
        let expires_at = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let repo = Arc::new(MemoryCredentials::holding(
            crate::account::credentials::YOUTUBE_CREDENTIAL_ID,
            &stored_credential_json("GTNH Live", Some("@gtnhlive"), expires_at),
        ));
        let (bundle, _platform) = make_bundle(repo);

        bundle.refresh_identity().await;

        assert_eq!(
            BuiltinStatus::hero_name(bundle.as_ref()).as_deref(),
            Some("@gtnhlive")
        );
        assert_eq!(
            BuiltinStatus::token_expiry(bundle.as_ref()),
            Some(SystemTime::from(expires_at))
        );
    }

    #[tokio::test]
    async fn health_stream_emits_chat_delta_inactive_connecting_when_platform_connects() {
        let (bundle, platform) = make_bundle(signed_in_repo());
        let mut health = BuiltinHealth::stream(bundle.as_ref());

        platform.connect().await.unwrap();

        let delta = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let delta = health.next().await.unwrap();
                if delta.metric == LIVE_CHAT_METRIC {
                    return delta;
                }
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            delta.new_value,
            HealthValue::Status {
                label: HealthText::Connection(ConnectionState::Connecting),
                active: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn health_stream_emits_events_delta_mirroring_connecting_state_on_connect() {
        let (bundle, platform) = make_bundle(signed_in_repo());
        let mut health = BuiltinHealth::stream(bundle.as_ref());

        platform.connect().await.unwrap();

        let delta = tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let delta = health.next().await.unwrap();
                if delta.metric == EVENTS_METRIC {
                    return delta;
                }
            }
        })
        .await
        .unwrap();
        assert!(matches!(
            delta.new_value,
            HealthValue::Status {
                label: HealthText::Connection(ConnectionState::Connecting),
                active: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn shutting_down_twice_disconnects_and_stops_every_bridge_that_holds_the_bundle() {
        let (bundle, platform) = make_bundle(signed_in_repo());
        platform.connect().await.unwrap();

        bundle.shutdown().await;
        bundle.shutdown().await;

        forge_test_support::settle_until("every bridge released the bundle", || {
            Arc::strong_count(&bundle) == 1
        })
        .await;
        assert!(!platform.connection_state().is_connected());
    }

    const STORED_DEFAULT_CONFIGS: [(&str, &str); 16] = [
        (
            "youtube.chat.create_poll",
            r#"{"options":{"type":"string","value":""},"question":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.chat.delete_message",
            r#"{"message_id":{"type":"string","value":"%message_id%"}}"#,
        ),
        (
            "youtube.chat.send_message",
            r#"{"message":{"type":"string","value":""}}"#,
        ),
        ("youtube.lookup.stream_stats", r#"{}"#),
        (
            "youtube.lookup.viewer",
            r#"{"identifier":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.moderation.add_moderator",
            r#"{"channel_id":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.moderation.ban_user",
            r#"{"channel_id":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.moderation.remove_moderator",
            r#"{"channel_id":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.moderation.timeout_user",
            r#"{"channel_id":{"type":"string","value":""},"duration_seconds":{"type":"int","value":300}}"#,
        ),
        (
            "youtube.moderation.unban_user",
            r#"{"channel_id":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.stream.insert_ad_break",
            r#"{"duration_seconds":{"type":"int","value":30}}"#,
        ),
        (
            "youtube.stream.set_thumbnail",
            r#"{"image_path":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.stream.update_category",
            r#"{"category_id":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.stream.update_description",
            r#"{"description":{"type":"string","value":""}}"#,
        ),
        (
            "youtube.stream.update_privacy",
            r#"{"privacy_status":{"type":"string","value":"public"}}"#,
        ),
        (
            "youtube.stream.update_title",
            r#"{"title":{"type":"string","value":""}}"#,
        ),
    ];

    #[test]
    fn every_runner_seeds_the_same_stored_default_config_as_before_declared_fields() {
        let reg = sub_action_registry();
        let registered: BTreeSet<&str> = reg.all().map(|runner| runner.id()).collect();
        let pinned: BTreeSet<&str> = STORED_DEFAULT_CONFIGS.iter().map(|(id, _)| *id).collect();
        assert_eq!(registered, pinned);

        for (id, stored) in STORED_DEFAULT_CONFIGS {
            let expected: serde_json::Value = serde_json::from_str(stored).unwrap();

            assert_eq!(
                serde_json::to_value(reg.get(id).unwrap().default_config()).unwrap(),
                expected,
                "{id} seeds a different default config"
            );
        }
    }

    #[test]
    fn whitespace_only_required_text_is_rejected_as_required_at_edit_time() {
        let cases = [
            ("youtube.chat.send_message", "message"),
            ("youtube.moderation.ban_user", "channel_id"),
            ("youtube.moderation.add_moderator", "channel_id"),
            ("youtube.stream.update_title", "title"),
            ("youtube.stream.update_description", "description"),
            ("youtube.stream.update_category", "category_id"),
            ("youtube.stream.set_thumbnail", "image_path"),
            ("youtube.lookup.viewer", "identifier"),
        ];
        let reg = sub_action_registry();

        for (id, key) in cases {
            let config = BTreeMap::from([(
                key.to_owned(),
                forge_types::Variant::String(" \t ".to_owned()),
            )]);

            let problem = reg
                .get(id)
                .unwrap()
                .validate_config(&config)
                .err()
                .map(forge_registry::RegistryError::into_config_problem);

            assert_eq!(
                problem,
                Some(forge_registry::ConfigProblem::field(
                    key,
                    forge_registry::ConfigReason::Required
                )),
                "{id}"
            );
        }
    }

    #[tokio::test]
    async fn every_code_the_bundle_shows_is_catalogued_in_both_languages() {
        use crate::emitted_codes::{emitted_codes, uncatalogued};
        use crate::vocabulary::messages::YOUTUBE_MESSAGES;

        let (bundle, _platform) = make_bundle(signed_in_repo());
        let emitted = emitted_codes(&serde_json::json!((
            QuickActions::actions(bundle.as_ref()),
            BuiltinContent::sections(bundle.as_ref()),
        )));

        assert!(!emitted.is_empty());
        assert_eq!(
            uncatalogued(&emitted, &[YOUTUBE_MESSAGES, forge_types::COMMON_MESSAGES]),
            Vec::<String>::new()
        );
    }

    #[tokio::test]
    async fn each_identity_refresh_replaces_the_hero_name() {
        let expires_at = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let repo = Arc::new(MemoryCredentials::holding(
            crate::account::credentials::YOUTUBE_CREDENTIAL_ID,
            &stored_credential_json("Old Title", Some("@old"), expires_at),
        ));
        let (bundle, _platform) = make_bundle(Arc::clone(&repo) as Arc<dyn CredentialsRepo>);
        bundle.refresh_identity().await;

        for (title, handle, hero) in [
            ("Old Title", Some("@renamed"), "@renamed"),
            ("New Title", None, "New Title"),
        ] {
            repo.put(
                crate::account::credentials::YOUTUBE_CREDENTIAL_ID,
                &stored_credential_json(title, handle, expires_at),
            );
            bundle.refresh_identity().await;

            assert_eq!(
                BuiltinStatus::hero_name(bundle.as_ref()).as_deref(),
                Some(hero)
            );
        }
    }

    #[tokio::test]
    async fn api_calls_metric_keeps_the_last_reading_while_the_quota_is_busy() {
        let quota = SharedQuota::default();
        let today = today_pacific();
        quota.lock().await.charge(7, today).unwrap();
        let (bundle, _platform) =
            make_bundle_with_quota(Arc::new(MemoryCredentials::default()), quota.clone());
        bundle.quota_metric();

        let held = quota.lock().await;
        let metric = bundle.quota_metric();
        drop(held);

        assert!(
            matches!(metric.value, HealthValue::Ratio { used: 7, .. }),
            "got {:?}",
            metric.value
        );
    }
}
