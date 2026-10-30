use std::sync::Arc;
use std::time::Duration;

use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinStatus, CapabilityFlags, ConnectionState, DetailSection,
    HeaderAction, HealthMetric, HealthMetricId, HealthStream, HealthText, HealthValue, QuickAction,
    QuickActions, SectionIcon,
};
use forge_types::{IntegrationId, MessageCode, UserText, VerbatimText};

use crate::integrations::BuiltinObject;

const MISSING_VALUE: &str = "-";

const DESKTOP_NAMESPACE: &str = "desktop";
const STATUS_METRIC: HealthMetricId =
    HealthMetricId::new(MessageCode::from_static(DESKTOP_NAMESPACE, "health_status"));
const UPTIME_METRIC: HealthMetricId =
    HealthMetricId::new(MessageCode::from_static(DESKTOP_NAMESPACE, "health_uptime"));
const ENDPOINT_METRIC: HealthMetricId = HealthMetricId::new(MessageCode::from_static(
    DESKTOP_NAMESPACE,
    "health_endpoint",
));
const VERSION_METRIC: HealthMetricId = HealthMetricId::new(MessageCode::from_static(
    DESKTOP_NAMESPACE,
    "health_version",
));

struct UnavailableStatus {
    id: IntegrationId,
    display_name: String,
}

impl BuiltinStatus for UnavailableStatus {
    fn id(&self) -> &IntegrationId {
        &self.id
    }
    fn display_name(&self) -> &str {
        &self.display_name
    }
    fn version(&self) -> Option<&str> {
        None
    }
    fn connection(&self) -> ConnectionState {
        ConnectionState::Disconnected
    }
    fn uptime(&self) -> Option<Duration> {
        None
    }
    fn endpoint(&self) -> Option<&str> {
        None
    }
    fn capability_flags(&self) -> CapabilityFlags {
        CapabilityFlags {
            limited: false,
            label: None,
        }
    }
    fn header_actions(&self) -> Vec<HeaderAction> {
        Vec::new()
    }
}

struct UnavailableHealth;

impl BuiltinHealth for UnavailableHealth {
    fn metrics(&self) -> [HealthMetric; 4] {
        [
            HealthMetric {
                id: STATUS_METRIC,
                value: HealthValue::Status {
                    label: HealthText::Connection(ConnectionState::Disconnected),
                    active: false,
                    detail: None,
                },
            },
            blank_metric(UPTIME_METRIC),
            blank_metric(ENDPOINT_METRIC),
            blank_metric(VERSION_METRIC),
        ]
    }

    fn stream(&self) -> HealthStream {
        Box::pin(futures_util::stream::empty())
    }
}

struct UnavailableContent;

impl BuiltinContent for UnavailableContent {
    fn sections(&self) -> Vec<DetailSection> {
        Vec::new()
    }
}

struct UnavailableQuickActions;

impl QuickActions for UnavailableQuickActions {
    fn actions(&self) -> Vec<QuickAction> {
        Vec::new()
    }
}

fn blank_metric(id: HealthMetricId) -> HealthMetric {
    HealthMetric {
        id,
        value: HealthValue::Text {
            primary: HealthText::Text(UserText::Verbatim(VerbatimText::new(MISSING_VALUE))),
            secondary: None,
        },
    }
}

fn identity(id: &IntegrationId) -> (String, &'static str) {
    match id.as_str() {
        "twitch" => ("Twitch".to_owned(), "brand-twitch"),
        "youtube" => ("YouTube".to_owned(), "brand-youtube"),
        "kick" => ("Kick".to_owned(), "brand-kick"),
        "discord" => ("Discord".to_owned(), "brand-discord"),
        "midi" => ("MIDI".to_owned(), "piano"),
        "hotkey" => ("Hotkeys".to_owned(), "keyboard"),
        other => (other.to_owned(), "broadcast"),
    }
}

pub fn unavailable_builtin(id: &IntegrationId) -> BuiltinObject {
    let (display_name, icon) = identity(id);
    BuiltinObject {
        icon: SectionIcon::new(icon),
        status: Arc::new(UnavailableStatus {
            id: id.clone(),
            display_name,
        }),
        health: Arc::new(UnavailableHealth),
        content: Arc::new(UnavailableContent),
        quick: Arc::new(UnavailableQuickActions),
        control: None,
        collections: None,
        option_source: None,
        obs_client: None,
        vtube_client: None,
        follow: None,
        ban_list: None,
    }
}
