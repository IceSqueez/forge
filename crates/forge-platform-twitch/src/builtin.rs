use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::{Mutex, watch};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState,
    ContentList, ContentListItem, DetailSection, HeaderAction, HealthDelta, HealthMetric,
    HealthStream, HealthValue, ListFooter, QuickAction, QuickActions, SectionIcon, TrailingToken,
};
use std::collections::BTreeMap;

use forge_events::EventPublisher;
use forge_storage::CredentialsRepo;
use forge_types::{SubActionStep, Variant};

use crate::TWITCH_BROADCASTER_SCOPES;
use crate::chat::{ChatConnectionState, TwitchChat, TwitchChatHandle};
use crate::subscriptions::{SubStatus, SubscriptionTracker};

/// Inputs required to (re)build a chat session for the bundle's broadcaster.
pub struct ChatSessionConfig {
    pub client_id: String,
    pub broadcaster_id: String,
    pub user_id: String,
}

pub struct TwitchIntegrationBundle {
    id: BuiltinId,
    login: Option<String>,
    state_rx: watch::Receiver<ChatConnectionState>,
    health_tx: broadcast::Sender<HealthDelta>,
    tracker: SubscriptionTracker,
    config: ChatSessionConfig,
    bus: Arc<dyn EventPublisher>,
    creds: Arc<dyn CredentialsRepo>,
    // Parked here so the &self-async control verbs can take() the consume-on-shutdown
    // handle without racing a concurrent disconnect/reconnect.
    handle: Mutex<Option<TwitchChatHandle>>,
}

impl TwitchIntegrationBundle {
    pub fn new(
        login: Option<String>,
        config: ChatSessionConfig,
        bus: Arc<dyn EventPublisher>,
        creds: Arc<dyn CredentialsRepo>,
        tracker: SubscriptionTracker,
        handle: TwitchChatHandle,
    ) -> (Arc<Self>, broadcast::Sender<HealthDelta>) {
        let (health_tx, _) = broadcast::channel(16);
        let state_rx = handle.state_receiver();
        let bundle = Arc::new(Self {
            id: BuiltinId::new("twitch"),
            login,
            state_rx,
            health_tx: health_tx.clone(),
            tracker,
            config,
            bus,
            creds,
            handle: Mutex::new(Some(handle)),
        });
        (bundle, health_tx)
    }

    pub(crate) fn spawn_chat(&self, token: forge_types::OAuthToken) -> TwitchChatHandle {
        TwitchChat::new(
            token,
            self.config.client_id.clone(),
            self.config.broadcaster_id.clone(),
            self.config.user_id.clone(),
            Arc::clone(&self.bus),
            self.tracker.clone(),
        )
        .start()
    }

    pub(crate) fn credentials(&self) -> &Arc<dyn CredentialsRepo> {
        &self.creds
    }

    pub(crate) fn config(&self) -> &ChatSessionConfig {
        &self.config
    }

    pub(crate) fn handle_slot(&self) -> &Mutex<Option<TwitchChatHandle>> {
        &self.handle
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        login: Option<String>,
        state_rx: watch::Receiver<ChatConnectionState>,
        tracker: SubscriptionTracker,
        creds: Arc<dyn CredentialsRepo>,
    ) -> Arc<Self> {
        let (health_tx, _) = broadcast::channel(16);
        Arc::new(Self {
            id: BuiltinId::new("twitch"),
            login,
            state_rx,
            health_tx,
            tracker,
            config: ChatSessionConfig {
                client_id: "test-client".to_owned(),
                broadcaster_id: "1".to_owned(),
                user_id: "1".to_owned(),
            },
            bus: Arc::new(crate::event_channel::PlatformEventChannel::new()),
            creds,
            handle: Mutex::new(None),
        })
    }

    fn chat_connection_state(&self) -> ChatConnectionState {
        *self.state_rx.borrow()
    }

    fn is_chat_connected(&self) -> bool {
        self.chat_connection_state() == ChatConnectionState::Connected
    }

    fn chat_label(&self) -> String {
        match self.chat_connection_state() {
            ChatConnectionState::Connected => {
                if let Some(login) = &self.login {
                    format!("Joined #{login}")
                } else {
                    "Connected".to_owned()
                }
            }
            ChatConnectionState::Connecting => "Connecting".to_owned(),
            ChatConnectionState::Reconnecting { .. } => "Reconnecting".to_owned(),
            ChatConnectionState::Disconnected => "Disconnected".to_owned(),
        }
    }

    fn active_sub_count(&self) -> usize {
        let records = self.tracker.read().unwrap_or_else(|p| p.into_inner());
        records
            .iter()
            .filter(|r| matches!(r.status, SubStatus::Active))
            .count()
    }
}

impl BuiltinStatus for TwitchIntegrationBundle {
    fn id(&self) -> &BuiltinId {
        &self.id
    }

    fn display_name(&self) -> &str {
        "Twitch"
    }

    fn version(&self) -> Option<&str> {
        None
    }

    fn connection(&self) -> ConnectionState {
        match self.chat_connection_state() {
            ChatConnectionState::Connected => ConnectionState::Connected,
            ChatConnectionState::Connecting => ConnectionState::Connecting,
            ChatConnectionState::Reconnecting { .. } => ConnectionState::Reconnecting,
            ChatConnectionState::Disconnected => ConnectionState::Disconnected,
        }
    }

    fn uptime(&self) -> Option<Duration> {
        None
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
}

impl BuiltinHealth for TwitchIntegrationBundle {
    fn metrics(&self) -> [HealthMetric; 4] {
        let chat_active = self.is_chat_connected();
        let chat_label = self.chat_label();
        let active_count = self.active_sub_count();

        [
            HealthMetric {
                label: "Chat IRC".to_owned(),
                value: HealthValue::Status {
                    label: chat_label,
                    active: chat_active,
                    detail: self.login.as_ref().map(|l| format!("#{l}")),
                },
            },
            HealthMetric {
                label: "EventSub".to_owned(),
                value: HealthValue::Text {
                    primary: format!("{active_count} subs"),
                    secondary: Some("WebSocket".to_owned()),
                },
            },
            HealthMetric {
                label: "Viewers".to_owned(),
                value: HealthValue::Text {
                    primary: "\u{2014}".to_owned(),
                    secondary: None,
                },
            },
            HealthMetric {
                label: "API Calls".to_owned(),
                value: HealthValue::Text {
                    primary: "\u{2014}".to_owned(),
                    secondary: None,
                },
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
        let scope_items: Vec<ContentListItem> = TWITCH_BROADCASTER_SCOPES
            .iter()
            .map(|s| ContentListItem {
                icon: SectionIcon::new("check"),
                name: (*s).to_owned(),
                monospace_name: true,
                active: true,
                active_label: None,
                trailing: vec![],
                enabled: true,
            })
            .collect();

        let scopes_count = format!("{}", scope_items.len());
        let scopes_list = ContentList {
            title: "OAuth scopes".to_owned(),
            icon: SectionIcon::new("key"),
            count_label: Some(scopes_count),
            items: scope_items,
            footer: Some(ListFooter {
                cta_label: Some("Request more scopes".to_owned()),
                trailing_label: None,
            }),
        };

        let records = self.tracker.read().unwrap_or_else(|p| p.into_inner());
        let active_count = records
            .iter()
            .filter(|r| matches!(r.status, SubStatus::Active))
            .count();

        let sub_items: Vec<ContentListItem> = records
            .iter()
            .map(|r| {
                let (active, status_label) = match &r.status {
                    SubStatus::Active => (true, "active".to_owned()),
                    SubStatus::Pending => (false, "pending".to_owned()),
                    SubStatus::Failed(_) => (false, "failed".to_owned()),
                };
                ContentListItem {
                    icon: SectionIcon::new("circle"),
                    name: r.kind.clone(),
                    monospace_name: true,
                    active,
                    active_label: None,
                    trailing: vec![
                        TrailingToken::Label(format!("v{}", r.version)),
                        TrailingToken::Label(status_label),
                    ],
                    enabled: true,
                }
            })
            .collect();

        let eventsub_list = ContentList {
            title: "EventSub subscriptions".to_owned(),
            icon: SectionIcon::new("rss"),
            count_label: Some(format!("{active_count} active")),
            items: sub_items,
            footer: Some(ListFooter {
                cta_label: Some("Subscribe to event".to_owned()),
                trailing_label: Some("subscribing on session start".to_owned()),
            }),
        };

        vec![DetailSection::TwoColumnLists {
            left: scopes_list,
            right: eventsub_list,
        }]
    }
}

impl QuickActions for TwitchIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.is_chat_connected();
        vec![
            QuickAction {
                label: "Send chat message".to_owned(),
                icon: SectionIcon::new("send"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "twitch.chat.send_message".to_owned(),
                    config: BTreeMap::from([
                        ("message".to_owned(), Variant::String(String::new())),
                        ("target".to_owned(), Variant::String("twitch".to_owned())),
                    ]),
                    enabled: true,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Run shoutout".to_owned(),
                icon: SectionIcon::new("flag"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "twitch.channel.send_shoutout".to_owned(),
                    config: BTreeMap::from([(
                        "to_broadcaster_login".to_owned(),
                        Variant::String(String::new()),
                    )]),
                    enabled: true,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Run commercial".to_owned(),
                icon: SectionIcon::new("clock"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "twitch.channel.run_ad".to_owned(),
                    config: BTreeMap::from([(
                        "duration_seconds".to_owned(),
                        Variant::String("90".to_owned()),
                    )]),
                    enabled: true,
                    label: None,
                },
                picker: None,
            },
            QuickAction {
                label: "Update title".to_owned(),
                icon: SectionIcon::new("edit"),
                enabled: connected,
                subaction_template: SubActionStep {
                    kind_id: "twitch.channel.update_title".to_owned(),
                    config: BTreeMap::from([("title".to_owned(), Variant::String(String::new()))]),
                    enabled: true,
                    label: None,
                },
                picker: None,
            },
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
        let (tx, rx) = watch::channel(state);
        let _ = tx;
        TwitchIntegrationBundle::for_test(
            Some("streamer".to_owned()),
            rx,
            tracker,
            Arc::new(NullCreds),
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
    fn health_metrics_returns_four_with_correct_labels() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        assert_eq!(metrics.len(), 4);
        assert_eq!(metrics[0].label, "Chat IRC");
        assert_eq!(metrics[1].label, "EventSub");
        assert_eq!(metrics[2].label, "Viewers");
        assert_eq!(metrics[3].label, "API Calls");
    }

    #[test]
    fn chat_irc_metric_active_when_connected() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Status { active, label, .. } = &metrics[0].value else {
            panic!("expected Status variant");
        };
        assert!(*active);
        assert_eq!(label, "Joined #streamer");
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
        let HealthValue::Text { primary, .. } = &metrics[1].value else {
            panic!("expected Text variant for EventSub metric");
        };
        assert_eq!(primary, "1 subs");
    }

    #[tokio::test]
    async fn health_stream_is_subscribable() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn BuiltinHealth = b.as_ref();
        let items: Vec<_> = health.stream().take(0).collect().await;
        assert!(items.is_empty());
    }

    #[test]
    fn content_sections_returns_one_two_column() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        assert_eq!(sections.len(), 1);
        assert!(matches!(&sections[0], DetailSection::TwoColumnLists { .. }));
    }

    #[test]
    fn content_scopes_section_has_all_broadcaster_scopes() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        let DetailSection::TwoColumnLists { left, .. } = &sections[0] else {
            panic!("expected TwoColumnLists in first section");
        };
        assert_eq!(left.items.len(), TWITCH_BROADCASTER_SCOPES.len());
        for (item, scope) in left.items.iter().zip(TWITCH_BROADCASTER_SCOPES.iter()) {
            assert_eq!(item.name, *scope);
            assert!(item.active);
        }
    }

    #[test]
    fn content_sections_two_column_scopes_and_eventsub() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        assert_eq!(sections.len(), 1);
        let DetailSection::TwoColumnLists { left, right } = &sections[0] else {
            panic!("expected TwoColumnLists");
        };
        assert_eq!(left.title, "OAuth scopes");
        assert_eq!(left.items.len(), TWITCH_BROADCASTER_SCOPES.len());
        assert_eq!(right.title, "EventSub subscriptions");
    }

    #[test]
    fn content_eventsub_empty_tracker_shows_zero_active() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        let DetailSection::TwoColumnLists { right, .. } = &sections[0] else {
            panic!("expected TwoColumnLists");
        };
        assert_eq!(right.count_label.as_deref(), Some("0 active"));
        assert!(right.items.is_empty());
    }

    #[test]
    fn content_eventsub_populated_tracker_renders_items() {
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
        let content: &dyn BuiltinContent = b.as_ref();
        let sections = content.sections();
        let DetailSection::TwoColumnLists { right, .. } = &sections[0] else {
            panic!("expected TwoColumnLists");
        };
        assert_eq!(right.count_label.as_deref(), Some("1 active"));
        assert_eq!(right.items.len(), 3);

        let active_item = &right.items[0];
        assert_eq!(active_item.name, "channel.chat.message");
        assert!(active_item.active);
        assert!(active_item.monospace_name);
        assert_eq!(
            active_item.trailing,
            vec![
                TrailingToken::Label("v1".to_owned()),
                TrailingToken::Label("active".to_owned()),
            ]
        );

        let pending_item = &right.items[1];
        assert_eq!(pending_item.name, "channel.subscribe");
        assert!(!pending_item.active);
        assert_eq!(
            pending_item.trailing,
            vec![
                TrailingToken::Label("v1".to_owned()),
                TrailingToken::Label("pending".to_owned()),
            ]
        );

        let failed_item = &right.items[2];
        assert_eq!(failed_item.name, "channel.cheer");
        assert!(!failed_item.active);
        assert_eq!(
            failed_item.trailing,
            vec![
                TrailingToken::Label("v1".to_owned()),
                TrailingToken::Label("failed".to_owned()),
            ]
        );
    }

    #[test]
    fn quick_actions_enabled_when_chat_connected() {
        let b = make_bundle(ChatConnectionState::Connected);
        let actions = b.actions();
        assert!(actions.iter().all(|a| a.enabled));
    }

    #[test]
    fn quick_actions_disabled_when_chat_disconnected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let actions = b.actions();
        assert!(actions.iter().all(|a| !a.enabled));
    }

    // Compile-time object-safety guard: the bundle must coerce into
    // `Arc<dyn BuiltinControl>` so the generic UI renderer can dispatch
    // lifecycle verbs through one trait object.
    #[test]
    fn bundle_coerces_to_dyn_builtin_control() {
        fn accepts(_: Arc<dyn forge_platform_core::BuiltinControl>) {}
        let b = make_bundle(ChatConnectionState::Connected);
        accepts(b);
    }

    // Missing-credentials guard: both `refresh_token` and `reconnect` `load()`
    // creds up front. With a store that yields None they must report
    // NotConnected without touching the network (NullCreds never connects).
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

    // Disconnect with an empty handle slot is the reachable no-session branch:
    // it must reject with NotConnected rather than silently succeed.
    #[tokio::test]
    async fn disconnect_with_no_live_session_reports_not_connected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let outcome = forge_platform_core::BuiltinControl::disconnect(b.as_ref()).await;
        assert_eq!(
            outcome,
            Err(forge_platform_core::ControlFailure::NotConnected)
        );
    }
}
