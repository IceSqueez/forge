use std::sync::Arc;
use std::time::Duration;

use forge_components::tr;
use forge_platform_core::{
    BuiltinContent, BuiltinHealth, BuiltinId, BuiltinStatus, CapabilityFlags, ConnectionState,
    DetailSection, HeaderAction, HealthMetric, HealthStream, HealthValue, QuickAction,
    QuickActions, SectionIcon,
};

use crate::integrations::BuiltinObject;

const MISSING_VALUE: &str = "-";

struct UnavailableStatus {
    id: BuiltinId,
    display_name: String,
}

impl BuiltinStatus for UnavailableStatus {
    fn id(&self) -> &BuiltinId {
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
                label: tr!("builtin_unavailable_metric_status"),
                value: HealthValue::Status {
                    label: tr!("common_status_not_connected"),
                    active: false,
                    detail: None,
                },
            },
            blank_metric(tr!("builtin_unavailable_metric_uptime")),
            blank_metric(tr!("builtin_unavailable_metric_endpoint")),
            blank_metric(tr!("builtin_unavailable_metric_version")),
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

fn blank_metric(label: String) -> HealthMetric {
    HealthMetric {
        label,
        value: HealthValue::Text {
            primary: MISSING_VALUE.to_owned(),
            secondary: None,
        },
    }
}

fn identity(id: &BuiltinId) -> (String, &'static str) {
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

pub fn unavailable_builtin(id: &BuiltinId) -> BuiltinObject {
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
        obs_client: None,
    }
}
