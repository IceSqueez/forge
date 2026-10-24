#![doc = "Twitch platform integration: auth (alpha-2), chat ingestion (alpha-3+)."]

pub mod auth;
mod ban_list;
mod bot_account;
mod bounded_client;
pub mod builtin;
mod cadence;
pub mod chat;
pub mod chat_platform;
mod chat_sender;
mod control;
mod creator_goals;
pub mod credentials;
pub mod credentials_manager;
mod custom_rewards;
#[cfg(test)]
mod endpoints_test_support;
mod event_channel;
pub mod event_kinds;
mod follow_lookup;
pub mod helix;
mod helix_gateway;
pub mod integration;
mod lifecycle;
#[cfg(test)]
mod log_capture;
mod oauth_client;
mod payload_fields;
mod reward_collection;
pub mod sub_actions;
pub mod subscriptions;
mod timestamp;
pub mod triggers;

pub use auth::{
    DeviceCodeInfo, TWITCH_BOT_SCOPES, TWITCH_BROADCASTER_SCOPES, TwitchAuthBundle, TwitchAuthFlow,
    TwitchSignIn, UserInfo, client_id, twitch_auth_flow,
};
pub use bot_account::{
    BOT_STATUS_CHANGED_KIND, BotLoginError, BotModerator, BotModeratorError, BotStatus,
    TwitchBotAccount,
};
pub use builtin::{
    ChatSessionConfig, HELIX_BUDGET_CAPACITY, HELIX_BUDGET_WINDOW, TwitchIntegrationBundle,
};
pub use chat::{
    ChatConnectionState, ChatSendError, SentMessageId, TwitchChat, TwitchChatHandle, send_chat,
};
pub use chat_platform::TwitchPlatform;
pub use chat_sender::SENDER_FALLBACK_KIND;
pub use credentials::{CredentialsTokenSource, TWITCH_BOT_CREDENTIAL_ID, TWITCH_CREDENTIAL_ID};
pub use credentials_manager::TwitchCredentialsManager;
pub use helix::{
    HelixError, HelixHttpTransport, HelixMethod, HelixRequest, HelixTokenRefresher,
    HelixTokenSource, HelixTransport,
};
pub use helix_gateway::{HELIX_GATEWAY_PROFILE, helix_gateway};
pub use integration::TWITCH_INTEGRATION;
pub use lifecycle::TwitchLifecycle;
pub use oauth_client::{
    TWITCH_OAUTH_BUDGET_CAPACITY, TWITCH_OAUTH_BUDGET_WINDOW, TWITCH_OAUTH_GATEWAY_PROFILE,
    TwitchOAuthClient, twitch_oauth_gateway,
};
pub use sub_actions::identity::BroadcasterTier;
pub use sub_actions::register_twitch_sub_actions;
pub use subscriptions::{SubStatus, SubscriptionRecord, SubscriptionTracker};
pub use triggers::register_twitch_triggers;
