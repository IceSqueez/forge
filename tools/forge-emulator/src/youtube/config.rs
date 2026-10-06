use std::fmt;

use serde::{Deserialize, Serialize};

use crate::fixture::{REDACTED, YouTubeAccount};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum YouTubeRefreshAnswer {
    #[default]
    Accept,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YouTubeSubscriber {
    pub channel_id: String,
    pub subscribed_at: String,
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FakeYouTubeSetup {
    pub live_at_boot: bool,
    pub broadcast_id: String,
    pub live_chat_id: String,
    pub broadcast_title: String,
    pub concurrent_viewers: u64,
    pub polling_interval_ms: u64,
    pub refresh: YouTubeRefreshAnswer,
    pub subscribers: Vec<YouTubeSubscriber>,
}

impl Default for FakeYouTubeSetup {
    fn default() -> Self {
        Self {
            live_at_boot: false,
            broadcast_id: "EmuBroadcst".to_owned(),
            live_chat_id: "EmulatorLiveChatId0001".to_owned(),
            broadcast_title: "forge emulator stream".to_owned(),
            concurrent_viewers: 42,
            polling_interval_ms: 3000,
            refresh: YouTubeRefreshAnswer::Accept,
            subscribers: Vec::new(),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct FakeYouTubeConfig {
    pub channel_id: String,
    pub channel_title: String,
    pub channel_handle: String,
    pub access_token: String,
    pub refresh_token: String,
    pub live: bool,
    pub broadcast_id: String,
    pub live_chat_id: String,
    pub broadcast_title: String,
    pub concurrent_viewers: u64,
    pub polling_interval_ms: u64,
    pub refresh: YouTubeRefreshAnswer,
    pub subscribers: Vec<YouTubeSubscriber>,
}

impl FakeYouTubeConfig {
    pub fn for_account(account: &YouTubeAccount, setup: &FakeYouTubeSetup) -> Self {
        Self {
            channel_id: account.channel_id.clone(),
            channel_title: account.channel_title.clone(),
            channel_handle: account.channel_handle.clone(),
            access_token: account.access_token.clone(),
            refresh_token: account.refresh_token.clone(),
            live: setup.live_at_boot,
            broadcast_id: setup.broadcast_id.clone(),
            live_chat_id: setup.live_chat_id.clone(),
            broadcast_title: setup.broadcast_title.clone(),
            concurrent_viewers: setup.concurrent_viewers,
            polling_interval_ms: setup.polling_interval_ms,
            refresh: setup.refresh,
            subscribers: setup.subscribers.clone(),
        }
    }
}

impl Default for FakeYouTubeConfig {
    fn default() -> Self {
        Self::for_account(&YouTubeAccount::default(), &FakeYouTubeSetup::default())
    }
}

impl fmt::Debug for FakeYouTubeConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeYouTubeConfig")
            .field("channel_id", &self.channel_id)
            .field("channel_title", &self.channel_title)
            .field("channel_handle", &self.channel_handle)
            .field("access_token", &REDACTED)
            .field("refresh_token", &REDACTED)
            .field("live", &self.live)
            .field("broadcast_id", &self.broadcast_id)
            .field("live_chat_id", &self.live_chat_id)
            .field("broadcast_title", &self.broadcast_title)
            .field("concurrent_viewers", &self.concurrent_viewers)
            .field("polling_interval_ms", &self.polling_interval_ms)
            .field("refresh", &self.refresh)
            .field("subscribers", &self.subscribers)
            .finish()
    }
}
