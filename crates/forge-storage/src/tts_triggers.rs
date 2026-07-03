use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::StorageError;

/// Which chat/platform sources are allowed to reach the TTS speak queue, plus
/// the message-formatting toggles applied when they do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtsTriggerSettings {
    pub command_enabled: bool,
    pub channel_points_enabled: bool,
    pub bits_enabled: bool,
    pub sub_messages_enabled: bool,
    pub read_username: bool,
    pub speak_emotes: bool,
    pub bits_skip_line: bool,
}

impl Default for TtsTriggerSettings {
    fn default() -> Self {
        Self {
            command_enabled: true,
            channel_points_enabled: true,
            bits_enabled: true,
            sub_messages_enabled: false,
            read_username: true,
            speak_emotes: false,
            bits_skip_line: true,
        }
    }
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait TtsTriggerSettingsRepo: Send + Sync {
    async fn get_trigger_settings(&self) -> Result<TtsTriggerSettings, StorageError>;

    async fn set_trigger_settings(&self, settings: &TtsTriggerSettings)
    -> Result<(), StorageError>;
}
