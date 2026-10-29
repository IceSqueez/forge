use std::fmt::Display;

use serde::Deserialize;
use serde_json::{Value, json};

pub const PUSHER_APP_KEY: &str = "32cbd69e4b950bf97679";
const CONNECTION_PARAMS: &str = "protocol=7&client=js&version=7.6.0&flash=false";

pub const SUBSCRIBE: &str = "pusher:subscribe";
pub const PING: &str = "pusher:ping";
pub const PONG: &str = "pusher:pong";
pub const ERROR: &str = "pusher:error";
pub const INTERNAL_EVENT_PREFIX: &str = "pusher_internal:";
pub(crate) const ERROR_CODE_KEY: &str = "code";

const EVENT_KEY: &str = "event";
const DATA_KEY: &str = "data";
const CHANNEL_KEY: &str = "channel";

const CHATROOM_CHANNEL_PREFIX: &str = "chatrooms.";
const CHATROOM_CHANNEL_SUFFIX: &str = ".v2";

pub mod app_events {
    pub const CHAT_MESSAGE: &str = "App\\Events\\ChatMessageEvent";
    pub const MESSAGE_DELETED: &str = "App\\Events\\MessageDeletedEvent";
    pub const USER_BANNED: &str = "App\\Events\\UserBannedEvent";
    pub const SUBSCRIPTION: &str = "App\\Events\\SubscriptionEvent";
    pub const GIFTED_SUBSCRIPTIONS: &str = "App\\Events\\GiftedSubscriptionsEvent";
    pub const STREAM_HOST: &str = "App\\Events\\StreamHostEvent";
}

pub fn chatroom_channel(chatroom: impl Display) -> String {
    format!("{CHATROOM_CHANNEL_PREFIX}{chatroom}{CHATROOM_CHANNEL_SUFFIX}")
}

pub(crate) fn socket_url(ws_base: &str) -> String {
    format!("{ws_base}/{PUSHER_APP_KEY}?{CONNECTION_PARAMS}")
}

pub(crate) fn subscribe_frame(chatroom_id: u64) -> Value {
    json!({
        (EVENT_KEY): SUBSCRIBE,
        (DATA_KEY): { (CHANNEL_KEY): chatroom_channel(chatroom_id) },
    })
}

pub(crate) fn ping_frame() -> Value {
    json!({ (EVENT_KEY): PING, (DATA_KEY): {} })
}

#[derive(Deserialize)]
pub(crate) struct InboundFrame {
    pub(crate) event: String,
    #[serde(default)]
    pub(crate) data: Value,
}
