#![doc = "Twitch platform integration: auth (alpha-2), chat ingestion (alpha-3+)."]

pub mod auth;
pub mod chat;

pub use auth::{
    TWITCH_BROADCASTER_SCOPES, TWITCH_DEVICE_ENDPOINT, TWITCH_TOKEN_ENDPOINT, UserInfo, client_id,
    fetch_user_info, new_twitch_poller, request_twitch_device_code, twitch_auth_flow,
};
pub use chat::{
    ChatConnectionState, ChatSendError, SentMessageId, TwitchChat, TwitchChatHandle, send_chat,
};
