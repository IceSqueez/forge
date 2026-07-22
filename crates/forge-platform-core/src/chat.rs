use std::sync::atomic::{AtomicU8, Ordering};

use async_trait::async_trait;
use forge_events::{Event, EventSource, EventStream};
use serde::{Deserialize, Serialize};

use crate::{AuthFlow, PlatformCapabilities, PlatformError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

impl ConnectionState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting",
            Self::Connected => "Connected",
            Self::Reconnecting => "Reconnecting",
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    fn encode(self) -> u8 {
        match self {
            Self::Disconnected => 0,
            Self::Connecting => 1,
            Self::Connected => 2,
            Self::Reconnecting => 3,
        }
    }

    fn decode(value: u8) -> Self {
        match value {
            1 => Self::Connecting,
            2 => Self::Connected,
            3 => Self::Reconnecting,
            _ => Self::Disconnected,
        }
    }
}

pub struct AtomicConnectionState(AtomicU8);

impl AtomicConnectionState {
    pub fn new(initial: ConnectionState) -> Self {
        Self(AtomicU8::new(initial.encode()))
    }

    pub fn load(&self) -> ConnectionState {
        ConnectionState::decode(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, state: ConnectionState) {
        self.0.store(state.encode(), Ordering::Relaxed);
    }
}

#[async_trait]
pub trait ChatPlatform: Send + Sync {
    fn platform_id(&self) -> &'static str;
    fn auth_flow(&self) -> &AuthFlow;
    fn capabilities(&self) -> &PlatformCapabilities;
    fn connection_state(&self) -> ConnectionState;
    async fn connect(&self) -> Result<(), PlatformError>;
    async fn disconnect(&self) -> Result<(), PlatformError>;
    /// Fails with `PlatformError::Unsupported` if `capabilities().can_send_chat` is false.
    async fn send_message(&self, channel: &str, text: &str) -> Result<(), PlatformError>;
    fn events(&self) -> EventStream;
}

pub const CONNECTION_STATE_CHANGED_KIND: &str = "platform.connection.changed";

pub fn connection_state_changed_event(platform_id: &str, state: ConnectionState) -> Event {
    Event::new(
        EventSource::Core,
        CONNECTION_STATE_CHANGED_KIND,
        serde_json::json!({ "platform_id": platform_id, "state": state }),
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn connection_state_serde_format_is_lowercase_for_each_variant() {
        for (state, expected) in [
            (ConnectionState::Disconnected, "disconnected"),
            (ConnectionState::Connecting, "connecting"),
            (ConnectionState::Connected, "connected"),
            (ConnectionState::Reconnecting, "reconnecting"),
        ] {
            let json = serde_json::to_string(&state).unwrap();
            assert_eq!(json, format!("\"{expected}\""));
            let back: ConnectionState = serde_json::from_str(&json).unwrap();
            assert_eq!(back, state);
        }
    }

    #[allow(dead_code)]
    fn trait_is_dyn_safe(_: &dyn ChatPlatform) {}
}
