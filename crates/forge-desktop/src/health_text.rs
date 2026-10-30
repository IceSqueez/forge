use forge_components::{fmt_uptime, tr};
use forge_platform_core::{ConnectionState, HealthMetricId, HealthText};

use crate::user_text::{resolve_message, resolve_user_text};

pub fn health_metric_label(id: &HealthMetricId) -> String {
    resolve_message(&id.label()).primary
}

pub fn health_text(text: &HealthText) -> String {
    match text {
        HealthText::Text(text) => resolve_user_text(text).inline(),
        HealthText::Count(count) => count.to_string(),
        HealthText::Span(span) => fmt_uptime(span.as_secs()),
        HealthText::Connection(state) => connection_label(*state),
    }
}

pub fn connection_label(state: ConnectionState) -> String {
    match state {
        ConnectionState::Connected => tr!("common_status_connected"),
        ConnectionState::Connecting => tr!("integration_state_connecting_title"),
        ConnectionState::Reconnecting => tr!("integration_state_reconnecting_title"),
        ConnectionState::Disconnected => tr!("common_status_not_connected"),
    }
}
