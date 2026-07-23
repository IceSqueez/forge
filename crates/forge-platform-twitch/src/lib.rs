#![doc = "Twitch platform integration: auth (alpha-2), chat ingestion (alpha-3+)."]

pub mod auth;
pub mod builtin;
pub mod chat;
pub mod chat_platform;
mod control;
pub mod credentials;
pub mod credentials_manager;
mod event_channel;
pub mod helix;
mod payload_fields;
pub mod sub_actions;
pub mod subscriptions;
pub mod triggers;

pub use auth::{
    DeviceCodeInfo, TWITCH_BROADCASTER_SCOPES, TWITCH_DEVICE_ENDPOINT, TWITCH_TOKEN_ENDPOINT,
    TwitchAuthBundle, TwitchAuthFlow, UserInfo, client_id, twitch_auth_flow,
};
pub use builtin::{ChatSessionConfig, TwitchIntegrationBundle};
pub use chat::{
    ChatConnectionState, ChatSendError, SentMessageId, TwitchChat, TwitchChatHandle, send_chat,
};
pub use chat_platform::TwitchPlatform;
pub use credentials::CredentialsTokenSource;
pub use credentials_manager::TwitchCredentialsManager;
pub use helix::{
    HelixError, HelixHttpTransport, HelixMethod, HelixRequest, HelixTokenRefresher,
    HelixTokenSource, HelixTransport,
};
pub use sub_actions::identity::BroadcasterTier;
pub use sub_actions::register_twitch_sub_actions;
pub use subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};
pub use triggers::register_twitch_triggers;
