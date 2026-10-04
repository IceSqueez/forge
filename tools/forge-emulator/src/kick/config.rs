use std::fmt;

use serde::{Deserialize, Serialize};

use crate::fixture::{KickAccount, REDACTED};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshAnswer {
    #[default]
    Accept,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FakeKickSetup {
    pub chatroom_id: u64,
    pub live_at_boot: bool,
    pub stream_title: String,
    pub category_id: u64,
    pub category_name: String,
    pub viewer_count: u64,
    pub refresh: RefreshAnswer,
}

impl Default for FakeKickSetup {
    fn default() -> Self {
        Self {
            chatroom_id: 300_000_001,
            live_at_boot: false,
            stream_title: "forge emulator stream".to_owned(),
            category_id: 15,
            category_name: "Just Chatting".to_owned(),
            viewer_count: 42,
            refresh: RefreshAnswer::Accept,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct FakeKickConfig {
    pub client_id: String,
    pub client_secret: String,
    pub user_id: u64,
    pub username: String,
    pub access_token: String,
    pub refresh_token: String,
    pub chatroom_id: u64,
    pub live: bool,
    pub stream_title: String,
    pub category_id: u64,
    pub category_name: String,
    pub viewer_count: u64,
    pub refresh: RefreshAnswer,
}

impl FakeKickConfig {
    pub fn for_account(account: &KickAccount, setup: &FakeKickSetup) -> Self {
        Self {
            client_id: account.client_id.clone(),
            client_secret: account.client_secret.clone(),
            user_id: account.user_id,
            username: account.username.clone(),
            access_token: account.access_token.clone(),
            refresh_token: account.refresh_token.clone(),
            chatroom_id: setup.chatroom_id,
            live: setup.live_at_boot,
            stream_title: setup.stream_title.clone(),
            category_id: setup.category_id,
            category_name: setup.category_name.clone(),
            viewer_count: setup.viewer_count,
            refresh: setup.refresh,
        }
    }

    pub fn chat_channel(&self) -> String {
        format!("chatrooms.{}.v2", self.chatroom_id)
    }
}

impl Default for FakeKickConfig {
    fn default() -> Self {
        Self::for_account(&KickAccount::default(), &FakeKickSetup::default())
    }
}

impl fmt::Debug for FakeKickConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeKickConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &REDACTED)
            .field("user_id", &self.user_id)
            .field("username", &self.username)
            .field("access_token", &REDACTED)
            .field("refresh_token", &REDACTED)
            .field("chatroom_id", &self.chatroom_id)
            .field("live", &self.live)
            .field("stream_title", &self.stream_title)
            .field("category_id", &self.category_id)
            .field("category_name", &self.category_name)
            .field("viewer_count", &self.viewer_count)
            .field("refresh", &self.refresh)
            .finish()
    }
}
