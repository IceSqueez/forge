use async_trait::async_trait;
use forge_events::EventStream;
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

#[async_trait]
pub trait ChatPlatform: Send + Sync {
    fn platform_id(&self) -> &'static str;
    fn auth_flow(&self) -> &AuthFlow;
    fn capabilities(&self) -> &PlatformCapabilities;
    fn connection_state(&self) -> ConnectionState;
    async fn connect(&mut self) -> Result<(), PlatformError>;
    async fn disconnect(&mut self) -> Result<(), PlatformError>;
    /// Fails with `PlatformError::Unsupported` if `capabilities().can_send_chat` is false.
    async fn send_message(&self, channel: &str, text: &str) -> Result<(), PlatformError>;
    fn events(&self) -> EventStream;
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
