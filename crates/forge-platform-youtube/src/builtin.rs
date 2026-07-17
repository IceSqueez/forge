use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags, ChatPlatform,
    ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthStream,
    HealthValue, LiveViewerSource, QuickAction, QuickActions, SectionIcon, ViewerReport,
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

/// Wraps the live `YoutubePlatform` + credentials manager so `forge-desktop` can
/// render the same detail-screen/health/quick-action surface every other
/// builtin exposes (`BuiltinStatus`/`BuiltinHealth`/`BuiltinContent`/`QuickActions`).
pub struct YoutubeIntegrationBundle {
    id: BuiltinId,
    channel_id: String,
    health_tx: broadcast::Sender<HealthDelta>,
    platform: Arc<YoutubePlatform>,
    credentials_manager: Arc<YoutubeCredentialsManager>,
    quota: Arc<tokio::sync::Mutex<QuotaState>>,
    viewer_report_tx: watch::Sender<ViewerReport>,
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
            health_tx: health_tx.clone(),
            platform,
            credentials_manager,
            quota,
            viewer_report_tx,
        });
        (bundle, health_tx)
    }

    pub fn viewer_source(&self) -> Box<dyn LiveViewerSource> {
        Box::new(YoutubeViewerSource::new(self.viewer_report_tx.subscribe()))
    }

    fn current_state(&self) -> ConnectionState {
        self.platform.connection_state()
    }

    pub(crate) fn credentials_manager(&self) -> &Arc<YoutubeCredentialsManager> {
        &self.credentials_manager
    }

    pub(crate) fn platform(&self) -> &Arc<YoutubePlatform> {
        &self.platform
    }

    /// Non-blocking: the poller task holds this lock only for the duration of a
    /// single quota charge, so a contended read here just falls back to a
    /// "no data yet" reading rather than stalling the sync `metrics()` call.
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
            label: "Quota".to_owned(),
            value,
        }
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
        Some("YouTube Data API v3 (polled)")
    }

    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }

    fn header_actions(&self) -> Vec<HeaderAction> {
        vec![
            HeaderAction::Reconnect,
            HeaderAction::RefreshToken,
            HeaderAction::Disconnect,
        ]
    }
}

impl BuiltinHealth for YoutubeIntegrationBundle {
    fn metrics(&self) -> [HealthMetric; 4] {
        let state = self.current_state();
        let (poll_label, poll_active) = match state {
            ConnectionState::Connected => ("Connected".to_owned(), true),
            ConnectionState::Connecting => ("Connecting".to_owned(), false),
            ConnectionState::Reconnecting => ("Reconnecting".to_owned(), false),
            ConnectionState::Disconnected => ("Disconnected".to_owned(), false),
        };

        [
            HealthMetric {
                label: "Chat poller".to_owned(),
                value: HealthValue::Status {
                    label: poll_label,
                    active: poll_active,
                    detail: Some("liveChatMessages.list".to_owned()),
                },
            },
            HealthMetric {
                label: "Channel".to_owned(),
                value: HealthValue::Text {
                    primary: self.channel_id.clone(),
                    secondary: None,
                },
            },
            self.quota_metric(),
            HealthMetric {
                label: "Auth".to_owned(),
                value: HealthValue::Status {
                    label: "OAuth 2.0 + PKCE".to_owned(),
                    active: true,
                    detail: None,
                },
            },
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

impl QuickActions for YoutubeIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = matches!(self.current_state(), ConnectionState::Connected);
        vec![
            QuickAction {
                label: "Refresh channel info".to_owned(),
                icon: SectionIcon::new("database"),
                enabled: true,
                subaction_template: SubActionStep {
                    kind_id: "core.log.write".to_owned(),
                    config: BTreeMap::from([
                        ("level".to_owned(), Variant::String("info".to_owned())),
                        (
                            "message".to_owned(),
                            Variant::String("youtube.channel_info_refresh_requested".to_owned()),
                        ),
                    ]),
                    enabled: true,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Send message".to_owned(),
                icon: SectionIcon::new("send"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "core.log.write".to_owned(),
                    config: BTreeMap::from([
                        ("level".to_owned(), Variant::String("info".to_owned())),
                        (
                            "message".to_owned(),
                            Variant::String("youtube.send_message_requested".to_owned()),
                        ),
                    ]),
                    enabled: true,
                    label: None,
                },
                picker: None,
            },
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
        // Each register() call must land a distinct kind id; a colliding id would
        // be silently lost (or error), making the registered count < the call count.
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
}
