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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_frame_names_the_pusher_subscribe_event_and_the_chatroom_channel() {
        assert_eq!(
            subscribe_frame(4242),
            serde_json::from_str::<Value>(
                r#"{"data":{"channel":"chatrooms.4242.v2"},"event":"pusher:subscribe"}"#
            )
            .unwrap()
        );
    }

    #[test]
    fn ping_frame_names_the_pusher_ping_event_with_empty_data() {
        assert_eq!(
            ping_frame(),
            serde_json::from_str::<Value>(r#"{"data":{},"event":"pusher:ping"}"#).unwrap()
        );
    }

    #[test]
    fn socket_url_appends_app_key_and_protocol_query_to_the_base() {
        assert_eq!(
            socket_url("wss://ws-us2.pusher.com/app"),
            "wss://ws-us2.pusher.com/app/32cbd69e4b950bf97679?protocol=7&client=js&version=7.6.0&flash=false"
        );
    }

    #[test]
    fn chatroom_channel_accepts_numeric_ids_and_slugs() {
        assert_eq!(chatroom_channel(7_u64), "chatrooms.7.v2");
        assert_eq!(chatroom_channel("streamer"), "chatrooms.streamer.v2");
    }

    #[test]
    fn control_event_names_match_the_pusher_wire_spelling() {
        assert_eq!(
            [SUBSCRIBE, PING, PONG, ERROR, INTERNAL_EVENT_PREFIX],
            [
                "pusher:subscribe",
                "pusher:ping",
                "pusher:pong",
                "pusher:error",
                "pusher_internal:"
            ]
        );
    }

    #[test]
    fn app_event_names_match_the_kick_wire_spelling() {
        assert_eq!(
            [
                app_events::CHAT_MESSAGE,
                app_events::MESSAGE_DELETED,
                app_events::USER_BANNED,
                app_events::SUBSCRIPTION,
                app_events::GIFTED_SUBSCRIPTIONS,
                app_events::STREAM_HOST,
            ],
            [
                "App\\Events\\ChatMessageEvent",
                "App\\Events\\MessageDeletedEvent",
                "App\\Events\\UserBannedEvent",
                "App\\Events\\SubscriptionEvent",
                "App\\Events\\GiftedSubscriptionsEvent",
                "App\\Events\\StreamHostEvent",
            ]
        );
    }

    #[test]
    fn inbound_frame_defaults_missing_data_to_null() {
        let frame: InboundFrame = serde_json::from_str(r#"{"event":"pusher:pong"}"#).unwrap();
        assert_eq!(frame.event, "pusher:pong");
        assert!(frame.data.is_null());
    }
}
