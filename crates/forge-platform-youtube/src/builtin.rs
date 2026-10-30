use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, SystemTime};

use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ChatPlatform, ConnectionState,
    DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthMetricId, HealthStream,
    HealthText, HealthValue, HeroBadge, HeroBadgeTone, LiveViewerSource, ViewerReport,
};
use forge_registry::{RegistryError, TriggerRegistry};
use forge_types::{IntegrationId, MessageRef, VerbatimText};
use tokio::sync::{broadcast, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tokio_util::sync::CancellationToken;

use crate::chat_platform::YoutubePlatform;
use crate::credentials_manager::YoutubeCredentialsManager;
use crate::follow_lookup::YoutubeFollowLookup;
use crate::quota_state::{QUOTA_DAILY_LIMIT, SharedQuota, today_pacific};
use crate::triggers::channel_member::SupportNewMemberDescriptor;
use crate::triggers::channel_member_milestone::SupportMemberMilestoneDescriptor;
use crate::triggers::channel_user_banned::ChannelUserBannedDescriptor;
use crate::triggers::chat_command::ChatCommandDescriptor;
use crate::triggers::chat_message::ChatMessageDescriptor;
use crate::triggers::chat_super_chat::SupportSuperChatDescriptor;
use crate::triggers::chat_super_sticker::SupportSuperStickerDescriptor;
use crate::triggers::member_gift::ChannelMemberGiftDescriptor;
use crate::triggers::member_gift_received::ChannelMemberGiftReceivedDescriptor;
use crate::triggers::message_deleted::ChatMessageDeletedDescriptor;
use crate::triggers::stream_offline::ChannelBroadcastEndedDescriptor;
use crate::triggers::stream_online::ChannelBroadcastStartedDescriptor;
use crate::triggers::title_changed::ChannelBroadcastTitleChangedDescriptor;
use crate::viewer_poll::{YoutubeViewerPoll, YoutubeViewerSource};
use crate::vocabulary::messages;

const LIVE_CHAT_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_LIVE_CHAT);
const EVENTS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_EVENTS);
const API_CALLS_METRIC: HealthMetricId = HealthMetricId::new(messages::HEALTH_API_CALLS);

pub fn register_youtube_triggers(registry: &mut TriggerRegistry) -> Result<(), RegistryError> {
    let mut registry = registry.owned_by(crate::YOUTUBE_INTEGRATION.id);
    registry.register(Box::new(ChatMessageDescriptor))?;
    registry.register(Box::new(ChatCommandDescriptor))?;
    registry.register(Box::new(SupportSuperChatDescriptor))?;
    registry.register(Box::new(SupportSuperStickerDescriptor))?;
    registry.register(Box::new(SupportNewMemberDescriptor))?;
    registry.register(Box::new(SupportMemberMilestoneDescriptor))?;
    registry.register(Box::new(ChannelUserBannedDescriptor))?;
    registry.register(Box::new(ChatMessageDeletedDescriptor))?;
    registry.register(Box::new(ChannelMemberGiftDescriptor))?;
    registry.register(Box::new(ChannelMemberGiftReceivedDescriptor))?;
    registry.register(Box::new(ChannelBroadcastStartedDescriptor))?;
    registry.register(Box::new(ChannelBroadcastEndedDescriptor))?;
    registry.register(Box::new(ChannelBroadcastTitleChangedDescriptor))?;
    Ok(())
}

pub struct YoutubeIntegrationBundle {
    id: IntegrationId,
    channel_id: String,
    display_name: OnceLock<String>,
    token_expires_at: RwLock<Option<SystemTime>>,
    health_tx: broadcast::Sender<HealthDelta>,
    platform: Arc<YoutubePlatform>,
    credentials_manager: Arc<YoutubeCredentialsManager>,
    quota: SharedQuota,
    follow_lookup: YoutubeFollowLookup,
    viewer_report_tx: watch::Sender<ViewerReport>,
    viewer_report_rx: watch::Receiver<ViewerReport>,
    state_rx: watch::Receiver<ConnectionState>,
    retired: CancellationToken,
}

impl YoutubeIntegrationBundle {
    pub fn new(
        channel_id: String,
        platform: Arc<YoutubePlatform>,
        credentials_manager: Arc<YoutubeCredentialsManager>,
    ) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(forge_platform_core::HEALTH_CHANNEL_CAPACITY);
        let (viewer_report_tx, _) = watch::channel(ViewerReport::Absent);
        let viewer_report_rx = viewer_report_tx.subscribe();
        let state_rx = platform.state_receiver();

        let token_source = {
            let manager = Arc::clone(&credentials_manager);
            Arc::new(move || {
                let manager = Arc::clone(&manager);
                Box::pin(async move { manager.get_valid_access_token().await })
                    as futures::future::BoxFuture<'static, _>
            })
        };
        let quota = platform.api().quota().clone();
        let follow_lookup = YoutubeFollowLookup::new(
            platform.api().clone(),
            token_source.clone(),
            channel_id.clone(),
        );
        let retired = CancellationToken::new();
        let viewer_poll = YoutubeViewerPoll::new(
            platform.api().clone(),
            token_source,
            platform.active_broadcast_id(),
            viewer_report_tx.clone(),
        );
        let poll_retired = retired.clone();
        tokio::spawn(async move {
            tokio::select! {
                biased;
                () = poll_retired.cancelled() => {}
                () = viewer_poll.run() => {}
            }
        });

        let bundle = Arc::new(Self {
            id: crate::YOUTUBE_INTEGRATION.id,
            channel_id,
            display_name: OnceLock::new(),
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
        });
        Self::spawn_health_bridge(&bundle);
        Self::spawn_viewer_health_bridge(&bundle);
        Self::spawn_identity_refresh(&bundle);
        bundle
    }

    pub fn viewer_source(&self) -> Box<dyn LiveViewerSource> {
        Box::new(YoutubeViewerSource::new(self.viewer_report_tx.subscribe()))
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
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let bundle = Arc::clone(bundle);
        let mut state_rx = bundle.state_rx.clone();
        handle.spawn(async move {
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
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let bundle = Arc::clone(bundle);
        let mut reports_rx = bundle.viewer_report_tx.subscribe();
        handle.spawn(async move {
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

    fn spawn_identity_refresh(bundle: &Arc<Self>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let bundle = Arc::clone(bundle);
        handle.spawn(async move {
            bundle.refresh_identity().await;
        });
    }

    pub(crate) async fn refresh_identity(&self) {
        let Ok(Some(stored)) = self.credentials_manager.load().await else {
            return;
        };
        let stored = if stored.channel_handle.is_some() {
            stored
        } else {
            self.credentials_manager
                .ensure_channel_handle()
                .await
                .unwrap_or(stored)
        };
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the first resolved display name is kept"
        )]
        let _ = self.display_name.set(preferred_hero_name(
            &stored.channel_title,
            stored.channel_handle.as_deref(),
        ));
        if let Ok(mut guard) = self.token_expires_at.write() {
            *guard = Some(SystemTime::from(stored.expires_at));
        }
    }

    pub(crate) fn credentials_manager(&self) -> &Arc<YoutubeCredentialsManager> {
        &self.credentials_manager
    }

    pub(crate) fn platform(&self) -> &Arc<YoutubePlatform> {
        &self.platform
    }

    pub(crate) fn follow_lookup(&self) -> &YoutubeFollowLookup {
        &self.follow_lookup
    }

    fn quota_metric(&self) -> HealthMetric {
        let value = match self.quota.try_lock() {
            Ok(guard) => HealthValue::Ratio {
                used: u64::from(guard.used_on(today_pacific())),
                total: u64::from(QUOTA_DAILY_LIMIT),
                reset_hint: Some(message_text(messages::health_quota_resets_daily())),
            },
            Err(_) => HealthValue::Text {
                primary: message_text(messages::health_unavailable()),
                secondary: None,
            },
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

fn preferred_hero_name(channel_title: &str, channel_handle: Option<&str>) -> String {
    match channel_handle.map(str::trim).filter(|h| !h.is_empty()) {
        Some(handle) => format!("@{}", handle.trim_start_matches('@')),
        None => channel_title.to_owned(),
    }
}

impl BuiltinStatus for YoutubeIntegrationBundle {
    fn id(&self) -> &IntegrationId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "YouTube"
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        self.current_state()
    }

    fn uptime(&self) -> Option<Duration> {
        None
    }

    fn endpoint(&self) -> Option<&str> {
        Some("Connected via OAuth (PKCE)")
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

    fn hero_name(&self) -> Option<&str> {
        self.display_name.get().map(String::as_str)
    }

    fn name_badges(&self) -> Vec<HeroBadge> {
        vec![HeroBadge {
            label: format!("channel_id {}", self.channel_id),
            tone: HeroBadgeTone::Neutral,
            monospace: true,
        }]
    }
}

impl BuiltinHealth for YoutubeIntegrationBundle {
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

impl BuiltinContent for YoutubeIntegrationBundle {
    fn sections(&self) -> Vec<DetailSection> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use async_trait::async_trait;
    use forge_registry::{FormField, SubActionRegistry};
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use futures::future::BoxFuture;
    use time::OffsetDateTime;
    use tokio_stream::StreamExt;

    use super::*;
    use crate::active_broadcast_id::ActiveBroadcastIdHandle;
    use crate::credentials::YoutubeCredentials;
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::moderation::YoutubeModeration;
    use crate::send_chat::YoutubeSendChat;
    use crate::stream_metadata::YoutubeStreamMetadata;
    use crate::sub_actions::register_youtube_sub_actions;
    use forge_platform_core::{
        PlatformError, QuickActionChoiceSource, QuickActionFieldKind, QuickActionFieldValue,
        QuickActions,
    };

    struct EmptyRepo;
    #[async_trait]
    impl CredentialsRepo for EmptyRepo {
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

    struct StoredRepo(String);
    #[async_trait]
    impl CredentialsRepo for StoredRepo {
        async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
            Ok(())
        }
        async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(Some(self.0.clone()))
        }
        async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
            Ok(true)
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

    fn token_source()
    -> Arc<dyn Fn() -> BoxFuture<'static, Result<String, PlatformError>> + Send + Sync> {
        Arc::new(|| Box::pin(async { Ok("test-token".to_owned()) }))
    }

    fn make_bundle(
        repo: Arc<dyn CredentialsRepo>,
    ) -> (Arc<YoutubeIntegrationBundle>, Arc<YoutubePlatform>) {
        make_bundle_with_quota(repo, SharedQuota::default())
    }

    fn make_bundle_with_quota(
        repo: Arc<dyn CredentialsRepo>,
        quota: SharedQuota,
    ) -> (Arc<YoutubeIntegrationBundle>, Arc<YoutubePlatform>) {
        let manager = Arc::new(YoutubeCredentialsManager::new(
            repo,
            crate::endpoints_test_support::unreachable_oauth_client(),
            crate::endpoints_test_support::unreachable_data_api(),
        ));
        let platform = Arc::new(YoutubePlatform::new(
            crate::endpoints_test_support::data_api_charging("http://127.0.0.1:0", quota),
            "UCabc123".to_owned(),
            Arc::clone(&manager),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        ));
        let bundle =
            YoutubeIntegrationBundle::new("UCabc123".to_owned(), Arc::clone(&platform), manager);
        (bundle, platform)
    }

    fn sub_action_registry() -> SubActionRegistry {
        let api = crate::endpoints_test_support::unreachable_data_api();
        let sender = Arc::new(YoutubeSendChat::new(
            api.clone(),
            token_source(),
            LiveChatIdHandle::new(),
        ));
        let moderation = Arc::new(YoutubeModeration::new(
            api.clone(),
            token_source(),
            crate::ban_ledger_test_support::broadcaster_source(),
            LiveChatIdHandle::new(),
            crate::ban_ledger_test_support::MemoryBanLedger::shared(),
        ));
        let metadata = Arc::new(YoutubeStreamMetadata::new(
            api.clone(),
            token_source(),
            ActiveBroadcastIdHandle::new(),
        ));
        let stream_stats = Arc::new(crate::stream_stats::YoutubeStreamStats::new(
            api.clone(),
            token_source(),
            ActiveBroadcastIdHandle::new(),
        ));
        let ad_break = Arc::new(crate::ad_break::YoutubeAdBreak::new(
            api.clone(),
            token_source(),
            ActiveBroadcastIdHandle::new(),
        ));
        let thumbnail = Arc::new(crate::thumbnail::YoutubeThumbnail::new(
            api.clone(),
            token_source(),
            ActiveBroadcastIdHandle::new(),
        ));
        let channel_lookup = Arc::new(crate::channel_lookup::YoutubeChannelLookup::new(
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
        use FormField::*;
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

    fn stored_credential_json(
        channel_title: &str,
        channel_handle: Option<&str>,
        expires_at: OffsetDateTime,
    ) -> String {
        serde_json::to_string(&YoutubeCredentials {
            access_token: "access".to_owned(),
            refresh_token: "refresh".to_owned(),
            client_id: "client".to_owned(),
            channel_id: "UCabc123".to_owned(),
            channel_title: channel_title.to_owned(),
            channel_handle: channel_handle.map(str::to_owned),
            expires_at,
        })
        .unwrap()
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_youtube_triggers(&mut reg).unwrap();
        let result = register_youtube_triggers(&mut reg);
        assert!(result.is_err());
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
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));

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
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));

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
        for action in QuickActions::actions(make_bundle(Arc::new(EmptyRepo)).0.as_ref()) {
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
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));

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
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));
        let groups: Vec<String> = QuickActions::actions(bundle.as_ref())
            .into_iter()
            .filter_map(|a| a.group)
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
                "Broadcast".to_owned(),
                "Polls".to_owned(),
                "Chat".to_owned(),
                "Content".to_owned(),
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
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));
        let destructive: BTreeSet<String> = QuickActions::actions(bundle.as_ref())
            .into_iter()
            .filter(|a| a.destructive)
            .map(|a| a.label)
            .collect();

        assert_eq!(
            destructive,
            BTreeSet::from(["Delete message".to_owned(), "Ban user".to_owned()])
        );
    }

    #[tokio::test]
    async fn metrics_declare_unique_catalogued_ids() {
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));

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
        let quota = SharedQuota::new(crate::quota_state::QuotaState {
            used_today: 9_500,
            last_reset_date: previous_day,
        });
        let (bundle, _platform) = make_bundle_with_quota(Arc::new(EmptyRepo), quota);

        let metric = bundle.quota_metric();

        assert!(
            matches!(metric.value, HealthValue::Ratio { used: 0, .. }),
            "got {:?}",
            metric.value
        );
    }

    #[tokio::test]
    async fn header_actions_offer_refresh_token_and_disconnect_only() {
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));
        assert_eq!(
            BuiltinStatus::header_actions(bundle.as_ref()),
            vec![HeaderAction::RefreshToken, HeaderAction::Disconnect]
        );
    }

    #[tokio::test]
    async fn name_badges_is_single_neutral_monospace_channel_id() {
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));
        assert_eq!(
            BuiltinStatus::name_badges(bundle.as_ref()),
            vec![HeroBadge {
                label: "channel_id UCabc123".to_owned(),
                tone: HeroBadgeTone::Neutral,
                monospace: true,
            }]
        );
    }

    #[tokio::test]
    async fn identity_fields_are_absent_before_credential_loads() {
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));
        assert_eq!(BuiltinStatus::hero_name(bundle.as_ref()), None);
        assert_eq!(BuiltinStatus::token_expiry(bundle.as_ref()), None);
    }

    #[tokio::test]
    async fn refresh_identity_populates_hero_name_and_token_expiry() {
        let expires_at = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
        let repo = Arc::new(StoredRepo(stored_credential_json(
            "GTNH Live",
            Some("@gtnhlive"),
            expires_at,
        )));
        let (bundle, _platform) = make_bundle(repo);

        bundle.refresh_identity().await;

        assert_eq!(BuiltinStatus::hero_name(bundle.as_ref()), Some("@gtnhlive"));
        assert_eq!(
            BuiltinStatus::token_expiry(bundle.as_ref()),
            Some(SystemTime::from(expires_at))
        );
    }

    #[tokio::test]
    async fn health_stream_emits_chat_delta_active_when_platform_connects() {
        let (bundle, platform) = make_bundle(Arc::new(EmptyRepo));
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
            HealthValue::Status { active: true, .. }
        ));
    }

    #[tokio::test]
    async fn health_stream_emits_events_delta_mirroring_chat_state_on_connect() {
        let (bundle, platform) = make_bundle(Arc::new(EmptyRepo));
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
            HealthValue::Status { active: true, .. }
        ));
    }

    #[tokio::test]
    async fn shutting_down_twice_disconnects_and_stops_every_bridge_that_holds_the_bundle() {
        let (bundle, platform) = make_bundle(Arc::new(EmptyRepo));
        platform.connect().await.unwrap();

        bundle.shutdown().await;
        bundle.shutdown().await;

        let released = tokio::time::timeout(Duration::from_secs(30), async {
            while Arc::strong_count(&bundle) > 1 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(
            released.is_ok(),
            "{} task(s) still hold the bundle after shutdown",
            Arc::strong_count(&bundle) - 1
        );
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
            r#"{"duration_secs":{"type":"int","value":30}}"#,
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
}
