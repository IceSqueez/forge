use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use forge_events::Event;
use forge_platform_core::{
    BannerLevel, BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags,
    ConnectionState, DetailSection, HeaderAction, HealthDelta, HealthMetric, HealthStream,
    HealthValue, QuickAction, QuickActions, SectionIcon,
};
use forge_registry::{RegistryError, TriggerRegistry};
use forge_types::{SubActionStep, Variant};
use tokio::sync::{Mutex, broadcast, mpsc, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use crate::capabilities::KICK_COMMUNITY_NOTE;
use crate::chat::KickChatHandle;
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
    state_rx: watch::Receiver<ConnectionState>,
    health_tx: broadcast::Sender<HealthDelta>,
    handle: Mutex<Option<KickChatHandle>>,
    credentials_manager: Arc<KickCredentialsManager>,
    http: reqwest::Client,
    event_tx: mpsc::Sender<Event>,
}

impl KickIntegrationBundle {
    pub fn new(
        slug: String,
        state_rx: watch::Receiver<ConnectionState>,
        credentials_manager: Arc<KickCredentialsManager>,
        http: reqwest::Client,
        event_tx: mpsc::Sender<Event>,
        initial_handle: Option<KickChatHandle>,
    ) -> (Arc<Self>, broadcast::Sender<HealthDelta>) {
        let (health_tx, _) = broadcast::channel(16);
        let bundle = Arc::new(Self {
            id: BuiltinId::new("kick"),
            slug,
            state_rx,
            health_tx: health_tx.clone(),
            handle: Mutex::new(initial_handle),
            credentials_manager,
            http,
            event_tx,
        });
        (bundle, health_tx)
    }

    fn current_state(&self) -> ConnectionState {
        *self.state_rx.borrow()
    }

    pub(crate) fn handle_slot(&self) -> &Mutex<Option<KickChatHandle>> {
        &self.handle
    }

    pub(crate) fn credentials_manager(&self) -> &Arc<KickCredentialsManager> {
        &self.credentials_manager
    }

    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub(crate) fn slug(&self) -> &str {
        &self.slug
    }

    pub(crate) fn event_tx(&self) -> &mpsc::Sender<Event> {
        &self.event_tx
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
        vec![HeaderAction::Reconnect, HeaderAction::Disconnect]
    }
}

impl BuiltinHealth for KickIntegrationBundle {
    fn metrics(&self) -> [HealthMetric; 4] {
        let state = self.current_state();
        let (ws_label, ws_active) = match state {
            ConnectionState::Connected => ("Connected".to_owned(), true),
            ConnectionState::Connecting => ("Connecting".to_owned(), false),
            ConnectionState::Reconnecting => ("Reconnecting".to_owned(), false),
            ConnectionState::Disconnected => ("Disconnected".to_owned(), false),
        };

        [
            HealthMetric {
                label: "Pusher WS".to_owned(),
                value: HealthValue::Status {
                    label: ws_label,
                    active: ws_active,
                    detail: Some(format!("chatrooms.{}.v2", self.slug)),
                },
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
        let connected = matches!(self.current_state(), ConnectionState::Connected);
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
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    use async_trait::async_trait;
    use forge_registry::KindPlatformContract;
    use forge_storage::{CredentialId, CredentialsRepo, StorageError};
    use forge_types::PlatformId;
    use time::OffsetDateTime;

    use super::*;

    struct NullRepo(StdMutex<HashMap<String, String>>);

    impl NullRepo {
        fn empty() -> Arc<Self> {
            Arc::new(Self(StdMutex::new(HashMap::new())))
        }
    }

    #[async_trait]
    impl CredentialsRepo for NullRepo {
        async fn store(&self, id: &CredentialId, v: &str) -> Result<(), StorageError> {
            self.0
                .lock()
                .unwrap()
                .insert(id.as_str().to_owned(), v.to_owned());
            Ok(())
        }

        async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
            Ok(self.0.lock().unwrap().get(id.as_str()).cloned())
        }

        async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
            Ok(self.0.lock().unwrap().remove(id.as_str()).is_some())
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

    fn make_bundle() -> Arc<KickIntegrationBundle> {
        let (tx, rx) = watch::channel(ConnectionState::Disconnected);
        let mgr = Arc::new(KickCredentialsManager::new(
            NullRepo::empty(),
            reqwest::Client::new(),
            "test_cid".to_owned(),
        ));
        let (event_tx, _event_rx) = mpsc::channel(8);
        let (bundle, _) = KickIntegrationBundle::new(
            "test_channel".to_owned(),
            rx,
            mgr,
            reqwest::Client::new(),
            event_tx,
            None,
        );
        drop(tx);
        bundle
    }

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

    #[test]
    fn bundle_send_message_action_enabled_when_connected() {
        let (tx, rx) = watch::channel(ConnectionState::Connected);
        let mgr = Arc::new(KickCredentialsManager::new(
            NullRepo::empty(),
            reqwest::Client::new(),
            "test_cid".to_owned(),
        ));
        let (event_tx, _event_rx) = mpsc::channel(8);
        let (bundle, _) = KickIntegrationBundle::new(
            "test_channel".to_owned(),
            rx,
            mgr,
            reqwest::Client::new(),
            event_tx,
            None,
        );
        drop(tx);
        let send_action = bundle
            .actions()
            .into_iter()
            .find(|a| a.label == "Send message")
            .unwrap();
        assert!(send_action.enabled);
    }

    #[test]
    fn bundle_send_message_action_disabled_when_disconnected() {
        let bundle = make_bundle();
        let send_action = bundle
            .actions()
            .into_iter()
            .find(|a| a.label == "Send message")
            .unwrap();
        assert!(!send_action.enabled);
    }

    #[test]
    fn bundle_content_has_warning_banner() {
        let bundle = make_bundle();
        let sections = bundle.sections();
        assert!(
            sections
                .iter()
                .any(|s| matches!(s, DetailSection::WarningBanner { .. }))
        );
    }

    #[test]
    fn bundle_header_includes_reconnect() {
        let bundle = make_bundle();
        assert!(bundle.header_actions().contains(&HeaderAction::Reconnect));
    }

    // Compile-time object-safety guard: the bundle must coerce into
    // `Arc<dyn BuiltinControl>` for the generic UI lifecycle renderer.
    #[test]
    fn bundle_coerces_to_dyn_builtin_control() {
        fn accepts(_: Arc<dyn forge_platform_core::BuiltinControl>) {}
        accepts(make_bundle());
    }

    // Missing-credentials guard: both `refresh_token` and `reconnect` call
    // `credentials_manager().load()` first. With an empty store (NullRepo) the
    // load yields None and both must short-circuit to NotConnected before any
    // network call (refresh grant / Pusher WS connect) is attempted.
    #[tokio::test]
    async fn refresh_token_without_stored_credentials_reports_not_connected() {
        let bundle = make_bundle();
        let outcome = forge_platform_core::BuiltinControl::refresh_token(bundle.as_ref()).await;
        assert_eq!(
            outcome,
            Err(forge_platform_core::ControlFailure::NotConnected)
        );
    }

    #[tokio::test]
    async fn reconnect_without_stored_credentials_reports_not_connected() {
        let bundle = make_bundle();
        let outcome = forge_platform_core::BuiltinControl::reconnect(bundle.as_ref()).await;
        assert_eq!(
            outcome,
            Err(forge_platform_core::ControlFailure::NotConnected)
        );
    }

    // Disconnect with an empty handle slot is the reachable no-session branch.
    #[tokio::test]
    async fn disconnect_with_no_live_session_reports_not_connected() {
        let bundle = make_bundle();
        let outcome = forge_platform_core::BuiltinControl::disconnect(bundle.as_ref()).await;
        assert_eq!(
            outcome,
            Err(forge_platform_core::ControlFailure::NotConnected)
        );
    }
}
