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
    channel_title: OnceLock<String>,
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
            channel_title: OnceLock::new(),
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

    /// Missing/unloadable credentials leave the previously cached title and expiry in place
    /// rather than resetting them; `channel_title` is set once, since the same bundle instance
    /// never outlives a channel switch (a new OAuth connect recreates the bundle).
    pub(crate) async fn refresh_identity(&self) {
        let Ok(Some(stored)) = self.credentials_manager.load().await else {
            return;
        };
        let _ = self.channel_title.set(stored.channel_title);
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
        detail: Some("same liveChatMessages.list feed".to_owned()),
    }
}

fn viewers_health_value(report: ViewerReport) -> HealthValue {
    match report {
        ViewerReport::Live { count } => HealthValue::Text {
            primary: count.to_string(),
            secondary: Some("live".to_owned()),
        },
        ViewerReport::Absent => HealthValue::Text {
            primary: "-".to_owned(),
            secondary: None,
        },
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
        self.channel_title.get().map(String::as_str)
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
        "Chat" => (SectionIcon::new("message-2"), QuickActionAccent::Brand),
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
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn register_does_not_drop_descriptors_to_collisions() {
        let mut reg = TriggerRegistry::new();
        register_youtube_triggers(&mut reg).unwrap();
        let registered = reg.all().count();
        let unique_ids: std::collections::HashSet<_> =
            reg.all().map(|d| d.id().to_owned()).collect();
        assert_eq!(
            registered,
            unique_ids.len(),
            "duplicate kind ids registered: {registered} descriptors but {} unique ids",
            unique_ids.len()
        );
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

    mod health_bridge {
        use std::time::Duration;

        use async_trait::async_trait;
        use forge_platform_core::BuiltinHealth;
        use forge_storage::{CredentialId, CredentialsRepo, StorageError};
        use time::OffsetDateTime;
        use tokio_stream::StreamExt;

        use super::super::*;
        use crate::active_broadcast_id::ActiveBroadcastIdHandle;
        use crate::auth::GoogleAuthFlow;
        use crate::live_chat_id::LiveChatIdHandle;
        use crate::quota_state::QuotaState;

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

        fn bundle_and_platform() -> (Arc<YoutubeIntegrationBundle>, Arc<YoutubePlatform>) {
            let manager = Arc::new(YoutubeCredentialsManager::new(
                Arc::new(EmptyRepo),
                GoogleAuthFlow::new("test_cid".to_owned(), "test_secret".to_owned()),
            ));
            let platform = Arc::new(YoutubePlatform::new(
                "UCtest".to_owned(),
                Arc::clone(&manager),
                LiveChatIdHandle::new(),
                ActiveBroadcastIdHandle::new(),
                Arc::new(tokio::sync::Mutex::new(QuotaState::default())),
            ));
            let (bundle, _health_tx) = YoutubeIntegrationBundle::new(
                "UCtest".to_owned(),
                Arc::clone(&platform),
                manager,
                Arc::new(tokio::sync::Mutex::new(QuotaState::default())),
            );
            (bundle, platform)
        }

        #[tokio::test]
        async fn health_stream_emits_connected_delta_when_platform_connects() {
            let (bundle, platform) = bundle_and_platform();
            let mut health = BuiltinHealth::stream(bundle.as_ref());

            platform.connect().await.unwrap();

            let expected = chat_poller_health_value(ConnectionState::Connected);
            let delta = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let delta = health.next().await.unwrap();
                    if delta.new_value == expected {
                        return delta;
                    }
                }
            })
            .await
            .unwrap();
            assert_eq!(delta.index, 0);
        }
    }
}
