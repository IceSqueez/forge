use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{
    BannerLevel, BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags,
    ChatPlatform, ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric,
    HealthStream, HealthValue, QuickAction, QuickActions, SectionIcon,
};
use forge_registry::{RegistryError, TriggerRegistry};
use forge_types::{SubActionStep, Variant};
use tokio::sync::{broadcast, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::capabilities::KICK_COMMUNITY_NOTE;
use crate::chat_platform::KickPlatform;
use crate::credentials_manager::KickCredentialsManager;
use crate::triggers::ban::BanDescriptor;
use crate::triggers::chat::ChatDescriptor;
use crate::triggers::chat_command::ChatCommandDescriptor;
use crate::triggers::host::HostDescriptor;
use crate::triggers::livestream_metadata::LivestreamMetadataDescriptor;
use crate::triggers::livestream_status::LivestreamStatusDescriptor;
use crate::triggers::message_deleted::MessageDeletedDescriptor;
use crate::triggers::reward_redeemed::RewardRedeemedDescriptor;
use crate::triggers::sub::SubDescriptor;
use crate::triggers::sub_gift::SubGiftDescriptor;

pub fn register_kick_triggers(registry: &mut TriggerRegistry) -> Result<(), RegistryError> {
    registry.register(Box::new(ChatDescriptor))?;
    registry.register(Box::new(ChatCommandDescriptor))?;
    registry.register(Box::new(SubDescriptor))?;
    registry.register(Box::new(SubGiftDescriptor))?;
    registry.register(Box::new(BanDescriptor))?;
    registry.register(Box::new(MessageDeletedDescriptor))?;
    registry.register(Box::new(HostDescriptor))?;
    registry.register(Box::new(LivestreamStatusDescriptor))?;
    registry.register(Box::new(LivestreamMetadataDescriptor))?;
    registry.register(Box::new(RewardRedeemedDescriptor))?;
    Ok(())
}

pub struct KickIntegrationBundle {
    id: BuiltinId,
    slug: String,
    health_tx: broadcast::Sender<HealthDelta>,
    platform: Arc<KickPlatform>,
    credentials_manager: Arc<KickCredentialsManager>,
    state_rx: watch::Receiver<ConnectionState>,
}

impl KickIntegrationBundle {
    pub fn new(
        slug: String,
        platform: Arc<KickPlatform>,
        credentials_manager: Arc<KickCredentialsManager>,
    ) -> (Arc<Self>, broadcast::Sender<HealthDelta>) {
        let (health_tx, _) = broadcast::channel(16);
        let state_rx = platform.state_receiver();
        let bundle = Arc::new(Self {
            id: BuiltinId::new("kick"),
            slug,
            health_tx: health_tx.clone(),
            platform,
            credentials_manager,
            state_rx,
        });
        Self::spawn_health_bridge(&bundle);
        (bundle, health_tx)
    }

    fn current_state(&self) -> ConnectionState {
        self.platform.connection_state()
    }

    fn ws_health_value(&self) -> HealthValue {
        let state = self.current_state();
        HealthValue::Status {
            label: state.label().to_owned(),
            active: state.is_connected(),
            detail: Some(format!("chatrooms.{}.v2", self.slug)),
        }
    }

    fn spawn_health_bridge(bundle: &Arc<Self>) {
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let bundle = Arc::clone(bundle);
        let mut state_rx = bundle.state_rx.clone();
        handle.spawn(async move {
            while state_rx.changed().await.is_ok() {
                let delta = HealthDelta {
                    index: 0,
                    new_value: bundle.ws_health_value(),
                };
                let _ = bundle.health_tx.send(delta);
            }
        });
    }

    pub(crate) fn credentials_manager(&self) -> &Arc<KickCredentialsManager> {
        &self.credentials_manager
    }

    pub(crate) fn platform(&self) -> &Arc<KickPlatform> {
        &self.platform
    }
}

impl BuiltinStatus for KickIntegrationBundle {
    fn id(&self) -> &BuiltinId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "Kick"
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
        Some("Pusher WS (read) + OAuth API (write)")
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

impl BuiltinHealth for KickIntegrationBundle {
    fn metrics(&self) -> [HealthMetric; 4] {
        [
            HealthMetric {
                label: "Pusher WS".to_owned(),
                value: self.ws_health_value(),
            },
            HealthMetric {
                label: "Chat mode".to_owned(),
                value: HealthValue::Text {
                    primary: "Read + send".to_owned(),
                    secondary: Some("send via OAuth API".to_owned()),
                },
            },
            HealthMetric {
                label: "Channel".to_owned(),
                value: HealthValue::Text {
                    primary: self.slug.clone(),
                    secondary: None,
                },
            },
            HealthMetric {
                label: "Auth".to_owned(),
                value: HealthValue::Status {
                    label: "OAuth 2.1 + PKCE".to_owned(),
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

impl BuiltinContent for KickIntegrationBundle {
    fn sections(&self) -> Vec<DetailSection> {
        vec![DetailSection::WarningBanner {
            level: BannerLevel::Warning,
            title: "Hybrid chat transport".to_owned(),
            body: KICK_COMMUNITY_NOTE.to_owned(),
            cta: None,
        }]
    }
}

impl QuickActions for KickIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.current_state().is_connected();
        vec![
            QuickAction {
                label: "Resync chatroom".to_owned(),
                icon: SectionIcon::new("refresh-cw"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "core.log.write".to_owned(),
                    config: BTreeMap::from([
                        ("level".to_owned(), Variant::String("info".to_owned())),
                        (
                            "message".to_owned(),
                            Variant::String("kick.chatroom_resync_requested".to_owned()),
                        ),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
                    label: None,
                },
                picker: None,
            },
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
                            Variant::String("kick.channel_info_refresh_requested".to_owned()),
                        ),
                    ]),
                    enabled: true,
                    continue_on_error: false,
                    condition: None,
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
                            Variant::String("kick.send_message_requested".to_owned()),
                        ),
                    ]),
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
#[allow(clippy::unwrap_used)]
mod tests {
    use forge_registry::KindPlatformContract;
    use forge_types::PlatformId;

    use super::*;

    #[test]
    fn register_adds_all_trigger_descriptors() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        assert_eq!(reg.all().count(), 10);
    }

    #[test]
    fn duplicate_registration_returns_error() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        let result = register_kick_triggers(&mut reg);
        assert!(result.is_err());
    }

    #[test]
    fn all_kind_ids_are_reachable() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        for id in [
            "kick.chat.message",
            "kick.chat.command",
            "kick.channel.subscriber",
            "kick.channel.subscription_gift",
            "kick.channel.banned",
            "kick.chat.message_deleted",
            "kick.channel.host_received",
            "kick.channel.livestream_status",
            "kick.channel.livestream_metadata",
            "kick.channel.reward_redeemed",
        ] {
            assert!(reg.get(id).is_some(), "missing kind id: {id}");
        }
    }

    #[test]
    fn all_descriptors_are_platform_specific_kick() {
        let mut reg = TriggerRegistry::new();
        register_kick_triggers(&mut reg).unwrap();
        for descriptor in reg.all() {
            assert_eq!(
                descriptor.platform_contract(),
                KindPlatformContract::PlatformSpecific(PlatformId::Kick),
                "descriptor '{}' must be platform-specific Kick",
                descriptor.id()
            );
        }
    }

    mod lifecycle {
        use std::time::Duration as StdDuration;

        use async_trait::async_trait;
        use forge_platform_core::{
            BuiltinControl, ControlFailure, PlatformError, RateLimitOutcome, RateLimiter,
        };
        use forge_storage::{CredentialId, CredentialsRepo, StorageError};
        use time::OffsetDateTime;

        use super::super::*;
        use crate::chat_platform::KickPlatform;

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

        struct GrantLimiter;
        #[async_trait]
        impl RateLimiter for GrantLimiter {
            async fn acquire(&self, _: u32) -> Result<RateLimitOutcome, PlatformError> {
                Ok(RateLimitOutcome::Granted)
            }
            fn remaining(&self) -> u32 {
                60
            }
            async fn observe_remote_throttle(&self, _: StdDuration) {}
        }

        fn disconnected_bundle() -> Arc<KickIntegrationBundle> {
            let manager = Arc::new(KickCredentialsManager::new(
                Arc::new(EmptyRepo),
                "test_cid".to_owned(),
            ));
            let platform = Arc::new(KickPlatform::new(
                "test_channel".to_owned(),
                manager.clone(),
                Arc::new(GrantLimiter),
            ));
            let (bundle, _) =
                KickIntegrationBundle::new("test_channel".to_owned(), platform, manager);
            bundle
        }

        #[test]
        fn pusher_ws_health_metric_reports_disconnected_with_slug_topic_while_offline() {
            let bundle = disconnected_bundle();
            let ws = &bundle.metrics()[0];
            assert_eq!(ws.label, "Pusher WS");
            assert_eq!(
                ws.value,
                HealthValue::Status {
                    label: "Disconnected".to_owned(),
                    active: false,
                    detail: Some("chatrooms.test_channel.v2".to_owned()),
                }
            );
        }

        #[test]
        fn send_message_quick_action_is_disabled_while_disconnected() {
            let bundle = disconnected_bundle();
            let action = bundle
                .actions()
                .into_iter()
                .find(|a| a.label == "Send message")
                .unwrap();
            assert!(!action.enabled);
        }

        #[test]
        fn content_surfaces_the_mandatory_kick_disclaimer_banner() {
            let bundle = disconnected_bundle();
            assert!(
                bundle
                    .sections()
                    .iter()
                    .any(|s| matches!(s, DetailSection::WarningBanner { .. }))
            );
        }

        #[tokio::test]
        async fn reconnect_without_stored_credentials_reports_not_connected() {
            let bundle = disconnected_bundle();
            let outcome = BuiltinControl::reconnect(bundle.as_ref()).await;
            assert_eq!(outcome, Err(ControlFailure::NotConnected));
        }

        #[tokio::test]
        async fn refresh_token_without_stored_credentials_reports_not_connected() {
            let bundle = disconnected_bundle();
            let outcome = BuiltinControl::refresh_token(bundle.as_ref()).await;
            assert_eq!(outcome, Err(ControlFailure::NotConnected));
        }

        #[tokio::test]
        async fn dyn_control_disconnect_while_disconnected_reports_not_connected() {
            let control: Arc<dyn BuiltinControl> = disconnected_bundle();
            let outcome = control.disconnect().await;
            assert_eq!(outcome, Err(ControlFailure::NotConnected));
        }
    }
}
