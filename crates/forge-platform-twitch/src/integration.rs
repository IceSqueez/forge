use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::watch;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

use forge_platform_core::{
    CapabilityFlags, ConnectionState, ContentList, ContentListItem, DetailSection, HeaderAction,
    HealthDelta, HealthMetric, HealthStream, HealthValue, IntegrationContent, IntegrationHealth,
    IntegrationId, IntegrationStatus, ListFooter, QuickAction, QuickActions, SectionIcon,
    SubscriptionRow,
};
use forge_types::SubActionSpec;

use crate::TWITCH_BROADCASTER_SCOPES;
use crate::chat::ChatConnectionState;

pub struct TwitchIntegrationBundle {
    id: IntegrationId,
    login: Option<String>,
    state_rx: watch::Receiver<ChatConnectionState>,
    health_tx: broadcast::Sender<HealthDelta>,
}

impl TwitchIntegrationBundle {
    pub fn new(
        login: Option<String>,
        state_rx: watch::Receiver<ChatConnectionState>,
    ) -> (Arc<Self>, broadcast::Sender<HealthDelta>) {
        let (health_tx, _) = broadcast::channel(16);
        let bundle = Arc::new(Self {
            id: IntegrationId::new("twitch"),
            login,
            state_rx,
            health_tx: health_tx.clone(),
        });
        (bundle, health_tx)
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
}

impl IntegrationStatus for TwitchIntegrationBundle {
    fn id(&self) -> &IntegrationId {
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

impl IntegrationHealth for TwitchIntegrationBundle {
    fn metrics(&self) -> [HealthMetric; 4] {
        let chat_active = self.is_chat_connected();
        let chat_label = self.chat_label();

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
                // TODO Phase 2: fetch live sub count from EventSub WS session
                value: HealthValue::Text {
                    primary: "0 subs".to_owned(),
                    secondary: Some("WebSocket".to_owned()),
                },
            },
            HealthMetric {
                label: "Viewers".to_owned(),
                // TODO Phase 2: fetch from Helix get_streams
                value: HealthValue::Text {
                    primary: "\u{2014}".to_owned(),
                    secondary: None,
                },
            },
            HealthMetric {
                label: "API Calls".to_owned(),
                // TODO Phase 2: track Helix rate-limit headers
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

impl IntegrationContent for TwitchIntegrationBundle {
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
            title: "OAuth Scopes".to_owned(),
            icon: SectionIcon::new("lock"),
            count_label: Some(scopes_count),
            items: scope_items,
            footer: Some(ListFooter {
                cta_label: None,
                trailing_label: Some("all granted".to_owned()),
            }),
        };

        // TODO Phase 2: populate from live EventSub subscriptions
        let eventsub_section = DetailSection::SubscriptionList {
            title: "EventSub Subscriptions".to_owned(),
            icon: SectionIcon::new("bolt"),
            items: Vec::<SubscriptionRow>::new(),
            footer: Some(ListFooter {
                cta_label: Some("No subscriptions".to_owned()),
                trailing_label: None,
            }),
        };

        let scopes_section = DetailSection::TwoColumnLists {
            left: scopes_list,
            right: ContentList {
                title: "Granted Permissions".to_owned(),
                icon: SectionIcon::new("shield-check"),
                count_label: Some(format!("{}", TWITCH_BROADCASTER_SCOPES.len())),
                items: vec![],
                footer: Some(ListFooter {
                    cta_label: Some("All scopes granted at login".to_owned()),
                    trailing_label: None,
                }),
            },
        };

        vec![scopes_section, eventsub_section]
    }
}

impl QuickActions for TwitchIntegrationBundle {
    fn actions(&self) -> Vec<QuickAction> {
        let connected = self.is_chat_connected();
        vec![
            QuickAction {
                label: "Reconnect chat".to_owned(),
                icon: SectionIcon::new("refresh"),
                enabled: !connected,
                subaction_template: SubActionSpec::SendChat {
                    message: "platform.reconnect_requested".to_owned(),
                    target: "twitch".to_owned(),
                },
                picker: None,
            },
            QuickAction {
                label: "Refresh token".to_owned(),
                icon: SectionIcon::new("key"),
                enabled: true,
                subaction_template: SubActionSpec::Log {
                    level: forge_types::LogLevel::Info,
                    message: "twitch.refresh_token_requested".to_owned(),
                },
                picker: None,
            },
            QuickAction {
                label: "Copy user ID".to_owned(),
                icon: SectionIcon::new("copy"),
                enabled: true,
                subaction_template: SubActionSpec::Log {
                    level: forge_types::LogLevel::Info,
                    message: "twitch.copy_user_id_requested".to_owned(),
                },
                picker: None,
            },
            QuickAction {
                label: "Open Twitch dashboard".to_owned(),
                icon: SectionIcon::new("external-link"),
                enabled: true,
                subaction_template: SubActionSpec::Log {
                    level: forge_types::LogLevel::Info,
                    message: "twitch.open_dashboard_requested".to_owned(),
                },
                picker: None,
            },
        ]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use forge_platform_core::{IntegrationContent, IntegrationHealth, IntegrationStatus};
    use tokio::sync::watch;

    use super::*;

    fn make_bundle(state: ChatConnectionState) -> Arc<TwitchIntegrationBundle> {
        let (tx, rx) = watch::channel(state);
        let _ = tx;
        let (bundle, _) = TwitchIntegrationBundle::new(Some("streamer".to_owned()), rx);
        bundle
    }

    #[test]
    fn status_id_is_twitch() {
        let b = make_bundle(ChatConnectionState::Connected);
        let status: &dyn IntegrationStatus = b.as_ref();
        assert_eq!(status.id().as_str(), "twitch");
    }

    #[test]
    fn status_display_name() {
        let b = make_bundle(ChatConnectionState::Connected);
        let status: &dyn IntegrationStatus = b.as_ref();
        assert_eq!(status.display_name(), "Twitch");
    }

    #[test]
    fn status_version_is_none() {
        let b = make_bundle(ChatConnectionState::Connected);
        let status: &dyn IntegrationStatus = b.as_ref();
        assert!(status.version().is_none());
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
            let status: &dyn IntegrationStatus = b.as_ref();
            assert_eq!(status.connection(), expected, "failed for {chat_state:?}");
        }
    }

    #[test]
    fn status_header_actions_contain_refresh_and_disconnect() {
        let b = make_bundle(ChatConnectionState::Connected);
        let status: &dyn IntegrationStatus = b.as_ref();
        let actions = status.header_actions();
        assert!(actions.contains(&HeaderAction::RefreshToken));
        assert!(actions.contains(&HeaderAction::Disconnect));
    }

    #[test]
    fn health_metrics_returns_four_with_correct_labels() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn IntegrationHealth = b.as_ref();
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
        let health: &dyn IntegrationHealth = b.as_ref();
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
        let health: &dyn IntegrationHealth = b.as_ref();
        let metrics = health.metrics();
        let HealthValue::Status { active, label, .. } = &metrics[0].value else {
            panic!("expected Status variant");
        };
        assert!(!*active);
        assert_eq!(label, "Disconnected");
    }

    #[tokio::test]
    async fn health_stream_is_subscribable() {
        let b = make_bundle(ChatConnectionState::Connected);
        let health: &dyn IntegrationHealth = b.as_ref();
        let items: Vec<_> = health.stream().take(0).collect().await;
        assert!(items.is_empty());
    }

    #[test]
    fn content_sections_returns_two() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn IntegrationContent = b.as_ref();
        let sections = content.sections();
        assert_eq!(sections.len(), 2);
    }

    #[test]
    fn content_scopes_section_has_all_broadcaster_scopes() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn IntegrationContent = b.as_ref();
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
    fn content_eventsub_section_is_subscription_list() {
        let b = make_bundle(ChatConnectionState::Connected);
        let content: &dyn IntegrationContent = b.as_ref();
        let sections = content.sections();
        assert!(matches!(
            &sections[1],
            DetailSection::SubscriptionList { .. }
        ));
    }

    #[test]
    fn quick_actions_returns_four() {
        let b = make_bundle(ChatConnectionState::Connected);
        let qa: &dyn QuickActions = b.as_ref();
        assert_eq!(qa.actions().len(), 4);
    }

    #[test]
    fn quick_actions_labels_in_order() {
        let b = make_bundle(ChatConnectionState::Connected);
        let actions = b.actions();
        assert_eq!(actions[0].label, "Reconnect chat");
        assert_eq!(actions[1].label, "Refresh token");
        assert_eq!(actions[2].label, "Copy user ID");
        assert_eq!(actions[3].label, "Open Twitch dashboard");
    }

    #[test]
    fn reconnect_chat_disabled_when_connected() {
        let b = make_bundle(ChatConnectionState::Connected);
        let actions = b.actions();
        assert!(
            !actions[0].enabled,
            "reconnect must be disabled when connected"
        );
    }

    #[test]
    fn reconnect_chat_enabled_when_disconnected() {
        let b = make_bundle(ChatConnectionState::Disconnected);
        let actions = b.actions();
        assert!(
            actions[0].enabled,
            "reconnect must be enabled when disconnected"
        );
    }
}
