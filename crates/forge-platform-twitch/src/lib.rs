#![doc = "Twitch platform integration: auth (alpha-2), chat ingestion (alpha-3+)."]

pub mod auth;
pub mod builtin;
pub mod chat;
pub mod chat_send_bridge;
pub mod credentials;
pub mod subscriptions;
pub mod triggers;

pub use auth::{
    TWITCH_BROADCASTER_SCOPES, TWITCH_DEVICE_ENDPOINT, TWITCH_TOKEN_ENDPOINT, TwitchAuthBundle,
    TwitchAuthFlow, TwitchDeviceCode, UserInfo, client_id, fetch_user_info, twitch_auth_flow,
};
pub use builtin::TwitchIntegrationBundle;
pub use chat::parsers::parse as parse_chat_event;
pub use chat::{
    ChatConnectionState, ChatSendError, SentMessageId, TwitchBadge, TwitchChat, TwitchChatEvent,
    TwitchChatHandle, send_chat,
};
pub use chat_send_bridge::{ChatSendBridge, ChatSendBridgeHandle};
pub use subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};
pub use triggers::register_twitch_triggers;
