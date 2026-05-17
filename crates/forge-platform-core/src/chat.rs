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
    fn disconnected_serde_roundtrip() {
        let json = serde_json::to_string(&ConnectionState::Disconnected).unwrap();
        assert_eq!(json, r#""disconnected""#);
        let back: ConnectionState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConnectionState::Disconnected);
    }

    #[test]
    fn connecting_serde_roundtrip() {
        let json = serde_json::to_string(&ConnectionState::Connecting).unwrap();
        assert_eq!(json, r#""connecting""#);
        let back: ConnectionState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConnectionState::Connecting);
    }

    #[test]
    fn connected_serde_roundtrip() {
        let json = serde_json::to_string(&ConnectionState::Connected).unwrap();
        assert_eq!(json, r#""connected""#);
        let back: ConnectionState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConnectionState::Connected);
    }

    #[test]
    fn reconnecting_serde_roundtrip() {
        let json = serde_json::to_string(&ConnectionState::Reconnecting).unwrap();
        assert_eq!(json, r#""reconnecting""#);
        let back: ConnectionState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConnectionState::Reconnecting);
    }

    #[allow(dead_code)]
    fn trait_is_dyn_safe(_: &dyn ChatPlatform) {}
}
