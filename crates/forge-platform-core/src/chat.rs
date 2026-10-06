use std::sync::atomic::{AtomicU8, Ordering};

use async_trait::async_trait;
use forge_events::{Event, EventSource, EventStream};
use serde::{Deserialize, Serialize};

use crate::PlatformError;

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
    fn connection_state(&self) -> ConnectionState;
    async fn connect(&self) -> Result<(), PlatformError>;
    async fn disconnect(&self) -> Result<(), PlatformError>;
    async fn send_message(&self, channel: &str, text: &str) -> Result<(), PlatformError>;

    async fn send_reply(
        &self,
        channel: &str,
        _reply_parent_message_id: &str,
        text: &str,
    ) -> Result<(), PlatformError> {
        self.send_message(channel, text).await
    }

    async fn send_whisper(&self, _recipient_login: &str, _text: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported {
            feature: WHISPER_FEATURE.to_owned(),
        })
    }

    fn events(&self) -> EventStream;
}

const WHISPER_FEATURE: &str = "chat.whisper";

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

    struct PlainOnlyPlatform {
        sent: std::sync::Mutex<Vec<(String, String)>>,
    }

    impl PlainOnlyPlatform {
        fn new() -> Self {
            Self {
                sent: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn sent(&self) -> Vec<(String, String)> {
            self.sent.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ChatPlatform for PlainOnlyPlatform {
        fn connection_state(&self) -> ConnectionState {
            ConnectionState::Connected
        }
        async fn connect(&self) -> Result<(), PlatformError> {
            Ok(())
        }
        async fn disconnect(&self) -> Result<(), PlatformError> {
            Ok(())
        }
        async fn send_message(&self, channel: &str, text: &str) -> Result<(), PlatformError> {
            self.sent
                .lock()
                .unwrap()
                .push((channel.to_owned(), text.to_owned()));
            Ok(())
        }
        fn events(&self) -> EventStream {
            EventStream::new(tokio::sync::broadcast::channel(1).1)
        }
    }

    #[tokio::test]
    async fn send_reply_without_override_falls_back_to_a_plain_message_on_the_same_channel() {
        let platform = PlainOnlyPlatform::new();

        platform.send_reply("chan", "msg-1", "hello").await.unwrap();

        assert_eq!(
            platform.sent(),
            vec![("chan".to_owned(), "hello".to_owned())]
        );
    }

    #[tokio::test]
    async fn send_whisper_without_override_is_unsupported_and_never_posts_to_public_chat() {
        let platform = PlainOnlyPlatform::new();

        let err = platform.send_whisper("viewer", "secret").await.unwrap_err();

        assert!(
            matches!(&err, PlatformError::Unsupported { feature } if feature == "chat.whisper"),
            "got {err:?}"
        );
        assert!(platform.sent().is_empty(), "whisper leaked to public chat");
    }
}
