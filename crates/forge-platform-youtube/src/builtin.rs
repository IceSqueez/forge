use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, SystemTime};

use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags, ChatPlatform,
    ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthStream,
    HealthValue, HeroBadge, HeroBadgeTone, LiveViewerSource, QuickAction, QuickActionAccent,
    QuickActionChoiceOption, QuickActionChoiceSource, QuickActionField, QuickActionFieldKind,
    QuickActionFieldValue, QuickActions, SectionIcon, ViewerReport,
};
use forge_registry::{RegistryError, TriggerRegistry};
use forge_types::{SubActionStep, Variant};
use tokio::sync::{broadcast, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::chat_platform::YoutubePlatform;
use crate::credentials_manager::YoutubeCredentialsManager;
use crate::quota_state::QuotaState;
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

/// YouTube's Data API v3 default daily quota budget (project-level, shared
/// across every endpoint the account calls). See `PLATFORMS_NOTES.md`.
const QUOTA_DAILY_BUDGET: u64 = 10_000;

pub fn register_youtube_triggers(registry: &mut TriggerRegistry) -> Result<(), RegistryError> {
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
    id: BuiltinId,
    channel_id: String,
    display_name: OnceLock<String>,
    token_expires_at: RwLock<Option<SystemTime>>,
    health_tx: broadcast::Sender<HealthDelta>,
    platform: Arc<YoutubePlatform>,
    credentials_manager: Arc<YoutubeCredentialsManager>,
    quota: Arc<tokio::sync::Mutex<QuotaState>>,
    viewer_report_tx: watch::Sender<ViewerReport>,
    viewer_report_rx: watch::Receiver<ViewerReport>,
    state_rx: watch::Receiver<ConnectionState>,
}

impl YoutubeIntegrationBundle {
    pub fn new(
        channel_id: String,
        platform: Arc<YoutubePlatform>,
        credentials_manager: Arc<YoutubeCredentialsManager>,
        quota: Arc<tokio::sync::Mutex<QuotaState>>,
    ) -> (Arc<Self>, broadcast::Sender<HealthDelta>) {
        let (health_tx, _) = broadcast::channel(16);
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
        tokio::spawn(
            YoutubeViewerPoll::new(
                token_source,
                platform.active_broadcast_id(),
                Arc::clone(&quota),
                viewer_report_tx.clone(),
            )
            .run(),
        );

        let bundle = Arc::new(Self {
            id: BuiltinId::new("youtube"),
            channel_id,
            display_name: OnceLock::new(),
            token_expires_at: RwLock::new(None),
            health_tx: health_tx.clone(),
            platform,
            credentials_manager,
            quota,
            viewer_report_tx,
            viewer_report_rx,
            state_rx,
        });
        Self::spawn_health_bridge(&bundle);
        Self::spawn_viewer_health_bridge(&bundle);
        Self::spawn_identity_refresh(&bundle);
        (bundle, health_tx)
    }

    pub fn viewer_source(&self) -> Box<dyn LiveViewerSource> {
        Box::new(YoutubeViewerSource::new(self.viewer_report_tx.subscribe()))
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
            while state_rx.changed().await.is_ok() {
                let current = *state_rx.borrow();

                let chat_delta = HealthDelta {
                    index: 0,
                    new_value: chat_poller_health_value(current),
                };
                let _ = bundle.health_tx.send(chat_delta);

                let events_delta = HealthDelta {
                    index: 1,
                    new_value: events_health_value(current),
                };
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
            while reports_rx.changed().await.is_ok() {
                let delta = HealthDelta {
                    index: 2,
                    new_value: viewers_health_value(*reports_rx.borrow()),
                };
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

    /// Missing/unloadable credentials leave the previously cached name and expiry in place
    /// rather than resetting them; `display_name` is set once, since the same bundle instance
    /// never outlives a channel switch (a new OAuth connect recreates the bundle). Credentials
    /// stored before handle tracking existed are backfilled here via `ensure_channel_handle`.
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

    /// Non-blocking: a contended read falls back to "no data yet" rather than stalling.
    fn quota_metric(&self) -> HealthMetric {
        let value = match self.quota.try_lock() {
            Ok(guard) => HealthValue::Ratio {
                used: u64::from(guard.used_today),
                total: QUOTA_DAILY_BUDGET,
                reset_hint: Some("resets daily (Pacific)".to_owned()),
            },
            Err(_) => HealthValue::Text {
                primary: "unavailable".to_owned(),
                secondary: None,
            },
        };
        HealthMetric {
            label: "API Calls".to_owned(),
            value,
        }
    }
}

fn chat_poller_health_value(state: ConnectionState) -> HealthValue {
    HealthValue::Status {
        label: state.label().to_owned(),
        active: state.is_connected(),
        detail: Some("liveChatMessages.list".to_owned()),
    }
}

/// Super chat, membership, and moderation events fan out from the same polled feed as chat
/// messages - YouTube has no separate event-subscription channel to report on.
fn events_health_value(state: ConnectionState) -> HealthValue {
    HealthValue::Status {
        label: state.label().to_owned(),
        active: state.is_connected(),
        detail: Some("shared chat feed".to_owned()),
    }
}

fn viewers_health_value(report: ViewerReport) -> HealthValue {
    match report {
        ViewerReport::Live { count } => HealthValue::Text {
            primary: count.to_string(),
            secondary: Some("live".to_owned()),
        },
        ViewerReport::Absent => HealthValue::Text {
            primary: "0".to_owned(),
            secondary: None,
        },
    }
}

/// A handle displays as `@handle` (normalized to exactly one leading `@`); credentials stored
/// before handle tracking existed fall back to the channel title.
fn preferred_hero_name(channel_title: &str, channel_handle: Option<&str>) -> String {
    match channel_handle.map(str::trim).filter(|h| !h.is_empty()) {
        Some(handle) => format!("@{}", handle.trim_start_matches('@')),
        None => channel_title.to_owned(),
    }
}

impl BuiltinStatus for YoutubeIntegrationBundle {
    fn id(&self) -> &BuiltinId {
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
                label: "Live Chat".to_owned(),
                value: chat_poller_health_value(state),
            },
            HealthMetric {
                label: "Events".to_owned(),
                value: events_health_value(state),
            },
            HealthMetric {
                label: "Viewers".to_owned(),
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

fn blank() -> Variant {
    Variant::String(String::new())
}

fn config(pairs: impl IntoIterator<Item = (&'static str, Variant)>) -> BTreeMap<String, Variant> {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

fn group_badge(group: &str) -> (SectionIcon, QuickActionAccent) {
    match group {
        "Broadcast" => (SectionIcon::new("broadcast"), QuickActionAccent::Danger),
        "Polls" => (SectionIcon::new("chart-bar"), QuickActionAccent::Brand),
        "Chat" => (SectionIcon::new("message-2"), QuickActionAccent::Brand),
        "Content" => (SectionIcon::new("video"), QuickActionAccent::Bits),
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
    }
}

fn text_field_placeholder(
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

#[allow(clippy::too_many_arguments)]
fn quick_action(
    label: &str,
    icon: &str,
    accent: QuickActionAccent,
    enabled: bool,
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
        locked_reason: None,
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
    }
}

impl QuickActions for YoutubeIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.current_state().is_connected();

        vec![
            quick_action(
                "Update title",
                "edit",
                QuickActionAccent::Info,
                connected,
                "Broadcast",
                false,
                "youtube.stream.update_title",
                config([("title", blank())]),
                vec![text_field("title", "Title", "GTNH: stream live")],
            ),
            quick_action(
                "Update description",
                "file-text",
                QuickActionAccent::Brand,
                connected,
                "Broadcast",
                false,
                "youtube.stream.update_description",
                config([("description", blank())]),
                vec![multiline_field(
                    "description",
                    "Description",
                    "Modded Minecraft grind.",
                )],
            ),
            quick_action(
                "Set category",
                "category",
                QuickActionAccent::Success,
                connected,
                "Broadcast",
                false,
                "youtube.stream.update_category",
                config([("category_id", Variant::String("20".to_owned()))]),
                vec![text_field_placeholder(
                    "category_id",
                    "Category ID",
                    "20",
                    "20 (Gaming), 24 (Entertainment)\u{2026}",
                )],
            ),
            quick_action(
                "Change privacy",
                "eye",
                QuickActionAccent::Warning,
                connected,
                "Broadcast",
                false,
                "youtube.stream.update_privacy",
                config([("privacy_status", Variant::String("public".to_owned()))]),
                vec![choice_field(
                    "privacy_status",
                    "Visibility",
                    "public",
                    &[
                        ("public", "Public"),
                        ("unlisted", "Unlisted"),
                        ("private", "Private"),
                    ],
                )],
            ),
            quick_action(
                "Set thumbnail",
                "photo",
                QuickActionAccent::Bits,
                connected,
                "Broadcast",
                false,
                "youtube.stream.set_thumbnail",
                config([("image_path", blank())]),
                vec![text_field("image_path", "Image path", "~/thumb.png")],
            ),
            quick_action(
                "Insert ad break (cuepoint)",
                "player-skip-forward",
                QuickActionAccent::Info,
                connected,
                "Broadcast",
                false,
                "youtube.stream.insert_ad_break",
                config([("duration_secs", Variant::Int(30))]),
                Vec::new(),
            ),
            quick_action(
                "Create live poll",
                "chart-bar",
                QuickActionAccent::Brand,
                connected,
                "Polls",
                false,
                "youtube.chat.create_poll",
                config([("question", blank()), ("options", blank())]),
                vec![
                    text_field("question", "Question", "What next?"),
                    multiline_field(
                        "options",
                        "Choices (one per line)",
                        "Keep grinding\nBoss fight",
                    ),
                ],
            ),
            quick_action(
                "Send message",
                "send",
                QuickActionAccent::Brand,
                connected,
                "Chat",
                false,
                "youtube.chat.send_message",
                config([("message", blank())]),
                vec![multiline_field(
                    "message",
                    "Message",
                    "Hey everyone \u{1f44b}",
                )],
            ),
            quick_action(
                "Delete message",
                "trash",
                QuickActionAccent::Danger,
                connected,
                "Chat",
                true,
                "youtube.chat.delete_message",
                config([("message_id", blank())]),
                vec![text_field_placeholder(
                    "message_id",
                    "Message ID",
                    "",
                    "message id",
                )],
            ),
            quick_action(
                "Timeout user",
                "clock-pause",
                QuickActionAccent::Warning,
                connected,
                "Chat",
                false,
                "youtube.moderation.timeout_user",
                config([
                    ("channel_id", blank()),
                    ("duration_seconds", Variant::Int(300)),
                ]),
                vec![text_field("channel_id", "User", "@spammer")],
            ),
            quick_action(
                "Ban user",
                "ban",
                QuickActionAccent::Danger,
                connected,
                "Chat",
                true,
                "youtube.moderation.ban_user",
                config([("channel_id", blank())]),
                vec![text_field("channel_id", "User", "@baduser")],
            ),
            quick_action(
                "Lookup viewer",
                "user",
                QuickActionAccent::Success,
                connected,
                "Content",
                false,
                "youtube.lookup.viewer",
                config([("identifier", blank())]),
                vec![text_field("identifier", "Username", "koval_dev")],
            ),
            quick_action(
                "Stream stats",
                "chart-line",
                QuickActionAccent::Info,
                connected,
                "Content",
                false,
                "youtube.lookup.stream_stats",
                BTreeMap::new(),
                Vec::new(),
            ),
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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
    use crate::auth::GoogleAuthFlow;
    use crate::credentials::YoutubeCredentials;
    use crate::live_chat_id::LiveChatIdHandle;
    use crate::moderation::YoutubeModeration;
    use crate::send_chat::YoutubeSendChat;
    use crate::stream_metadata::YoutubeStreamMetadata;
    use crate::sub_actions::register_youtube_sub_actions;
    use forge_platform_core::PlatformError;

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
        let manager = Arc::new(YoutubeCredentialsManager::new(
            repo,
            GoogleAuthFlow::new("test_cid".to_owned(), "test_secret".to_owned()),
        ));
        let platform = Arc::new(YoutubePlatform::new(
            "UCabc123".to_owned(),
            Arc::clone(&manager),
            LiveChatIdHandle::new(),
            ActiveBroadcastIdHandle::new(),
            Arc::new(tokio::sync::Mutex::new(QuotaState::default())),
        ));
        let (bundle, _health_tx) = YoutubeIntegrationBundle::new(
            "UCabc123".to_owned(),
            Arc::clone(&platform),
            manager,
            Arc::new(tokio::sync::Mutex::new(QuotaState::default())),
        );
        (bundle, platform)
    }

    fn sub_action_registry() -> SubActionRegistry {
        let quota = Arc::new(tokio::sync::Mutex::new(QuotaState::default()));
        let sender = Arc::new(YoutubeSendChat::new(
            token_source(),
            LiveChatIdHandle::new(),
            Arc::clone(&quota),
        ));
        let moderation = Arc::new(YoutubeModeration::new(
            token_source(),
            LiveChatIdHandle::new(),
            Arc::clone(&quota),
        ));
        let metadata = Arc::new(YoutubeStreamMetadata::new(
            token_source(),
            ActiveBroadcastIdHandle::new(),
            Arc::clone(&quota),
        ));
        let stream_stats = Arc::new(crate::stream_stats::YoutubeStreamStats::new(
            token_source(),
            ActiveBroadcastIdHandle::new(),
            Arc::clone(&quota),
        ));
        let ad_break = Arc::new(crate::ad_break::YoutubeAdBreak::new(
            token_source(),
            ActiveBroadcastIdHandle::new(),
            Arc::clone(&quota),
        ));
        let thumbnail = Arc::new(crate::thumbnail::YoutubeThumbnail::new(
            token_source(),
            ActiveBroadcastIdHandle::new(),
            Arc::clone(&quota),
        ));
        let channel_lookup = Arc::new(crate::channel_lookup::YoutubeChannelLookup::new(
            token_source(),
            quota,
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
            | DynamicSelect { key, .. }
            | Optional { key, .. }
            | SubChain { key, .. }
            | CaseList { key, .. } => *key,
        };
        out.insert(key.to_owned());
        if let Optional { inner, .. } = field {
            form_field_keys(inner, out);
        }
    }

    fn stored_credential_json(channel_title: &str, expires_at: OffsetDateTime) -> String {
        serde_json::to_string(&YoutubeCredentials {
            access_token: "access".to_owned(),
            refresh_token: "refresh".to_owned(),
            client_id: "client".to_owned(),
            channel_id: "UCabc123".to_owned(),
            channel_title: channel_title.to_owned(),
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
        for (state, label, active) in [
            (ConnectionState::Connected, "Connected", true),
            (ConnectionState::Connecting, "Connecting", false),
            (ConnectionState::Reconnecting, "Reconnecting", false),
            (ConnectionState::Disconnected, "Disconnected", false),
        ] {
            assert_eq!(
                chat_poller_health_value(state),
                HealthValue::Status {
                    label: label.to_owned(),
                    active,
                    detail: Some("liveChatMessages.list".to_owned()),
                },
                "unexpected health value for {state:?}"
            );
        }
    }

    #[test]
    fn viewers_health_value_shows_count_when_live_and_zero_when_absent() {
        for (report, primary, secondary) in [
            (ViewerReport::Live { count: 0 }, "0", Some("live")),
            (ViewerReport::Live { count: 1234 }, "1234", Some("live")),
            (ViewerReport::Absent, "0", None),
        ] {
            assert_eq!(
                viewers_health_value(report),
                HealthValue::Text {
                    primary: primary.to_owned(),
                    secondary: secondary.map(str::to_owned),
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
    async fn quick_action_groups_are_contiguous_broadcast_before_chat() {
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
        assert_eq!(order, vec!["Broadcast".to_owned(), "Chat".to_owned()]);

        if let Some(first_chat) = groups.iter().position(|g| g == "Chat") {
            assert!(
                groups[first_chat..].iter().all(|g| g == "Chat"),
                "a Broadcast action appears after the Chat section starts: {groups:?}"
            );
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
    async fn metrics_are_ordered_livechat_events_viewers_apicalls() {
        let (bundle, _platform) = make_bundle(Arc::new(EmptyRepo));
        let metrics = BuiltinHealth::metrics(bundle.as_ref());

        let labels: Vec<&str> = metrics.iter().map(|m| m.label.as_str()).collect();
        // Why: spawn_health_bridge/spawn_viewer_health_bridge push HealthDelta{index:0/1/2}
        // for chat/events/viewers; these slots must stay aligned or a delta updates the wrong tile.
        assert_eq!(labels, ["Live Chat", "Events", "Viewers", "API Calls"]);
        assert!(matches!(metrics[0].value, HealthValue::Status { .. }));
        assert!(matches!(metrics[1].value, HealthValue::Status { .. }));
        assert!(matches!(metrics[2].value, HealthValue::Text { .. }));
        assert!(matches!(metrics[3].value, HealthValue::Ratio { .. }));
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
        let repo = Arc::new(StoredRepo(stored_credential_json("GTNH Live", expires_at)));
        let (bundle, _platform) = make_bundle(repo);

        bundle.refresh_identity().await;

        assert_eq!(BuiltinStatus::hero_name(bundle.as_ref()), Some("GTNH Live"));
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

        let delta = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let delta = health.next().await.unwrap();
                if delta.index == 0 {
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

        let delta = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let delta = health.next().await.unwrap();
                if delta.index == 1 {
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
}
