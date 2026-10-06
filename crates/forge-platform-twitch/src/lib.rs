#![doc = "Twitch platform integration: auth (alpha-2), chat ingestion (alpha-3+)."]

pub mod auth;
pub mod builtin;
pub mod chat;
pub mod chat_platform;
mod control;
mod creator_goals;
pub mod credentials;
pub mod credentials_manager;
mod custom_rewards;
mod event_channel;
mod follow_lookup;
pub mod helix;
pub mod integration;
mod lifecycle;
#[cfg(test)]
mod log_capture;
mod payload_fields;
mod reward_collection;
pub mod sub_actions;
pub mod subscriptions;
pub mod triggers;

pub use auth::{
    DeviceCodeInfo, TWITCH_BROADCASTER_SCOPES, TwitchAuthBundle, TwitchAuthFlow, UserInfo,
    client_id, twitch_auth_flow,
};
pub use builtin::{
    ChatSessionConfig, HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW, TwitchIntegrationBundle,
};
pub use chat::{
    ChatConnectionState, ChatSendError, SentMessageId, TwitchChat, TwitchChatHandle, send_chat,
};
pub use chat_platform::TwitchPlatform;
pub use credentials::{CredentialsTokenSource, TWITCH_CREDENTIAL_ID};
pub use credentials_manager::TwitchCredentialsManager;
pub use helix::{
    HelixError, HelixHttpTransport, HelixMethod, HelixRequest, HelixTokenRefresher,
    HelixTokenSource, HelixTransport,
};
pub use integration::TWITCH_INTEGRATION;
pub use lifecycle::TwitchLifecycle;
pub use sub_actions::identity::BroadcasterTier;
pub use sub_actions::register_twitch_sub_actions;
pub use subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};
pub use triggers::register_twitch_triggers;
