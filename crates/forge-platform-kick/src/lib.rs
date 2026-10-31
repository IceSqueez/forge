mod api;
mod auth;
mod ban_ledger;
#[cfg(test)]
mod ban_ledger_test_support;
mod ban_list;
mod builtin;
mod capabilities;
pub(crate) mod control;
#[cfg(test)]
#[path = "../../forge-platform-core/test_support/emitted_codes.rs"]
mod emitted_codes;
#[cfg(test)]
mod endpoints_test_support;
mod gateway;
mod integration;
#[cfg(test)]
mod log_capture;
mod platform;
mod pusher;
mod quick_actions;
mod sub_actions;
mod timestamp;
mod triggers;
pub mod vocabulary;

pub use api::categories::KickCategories;
pub use api::channel::KickChannel;
pub use api::chat_messages::KickSendChat;
pub use api::moderation::KickModeration;
pub use api::poller::spawn_kick_poller;
pub use api::rewards::KickRewards;
pub use auth::credentials::{CREDENTIAL_KEY, KickCredentials};
pub use auth::credentials_manager::KickCredentialsManager;
pub use auth::oauth_client::{
    KICK_OAUTH_BUDGET_CAPACITY, KICK_OAUTH_BUDGET_WINDOW, KickOAuthClient, kick_oauth_gateway,
};
pub use auth::{
    KICK_CLIENT_ID_ENV, KICK_CLIENT_SECRET_ENV, KickAuthBundle, KickAuthFlow, KickSignIn,
    client_credentials,
};
pub use builtin::{KickIntegrationBundle, register_kick_triggers};
pub use gateway::{
    KICK_API_BUDGET_CAPACITY, KICK_API_BUDGET_WINDOW, KICK_CHANNEL_INFO_BUDGET_CAPACITY,
    KICK_CHANNEL_INFO_BUDGET_WINDOW, kick_api_gateway, kick_channel_info_gateway,
};
pub use integration::KICK_INTEGRATION;
pub use platform::KickPlatform;
pub use pusher::chat::{KickChat, KickChatHandle};
pub use pusher::chatroom_lookup::ChannelInfoFetcher;
pub use pusher::{chat, error, protocol};
pub use sub_actions::{KickSubActionDeps, register_kick_sub_actions};

pub use api::categories::CategoryMatch;
pub use api::channel::ChannelSnapshot;
pub use api::poller::KickPollerHandle;
pub use api::poller::KickViewerSource;
pub use api::poller::PollerAuth;
pub use api::rewards::CreateRewardParams;
pub use api::rewards::RedemptionRecord;
pub use api::rewards::UpdateRewardParams;
pub use auth::KICK_AUTHORIZE_ENDPOINT;
pub use auth::LoopbackCode;
pub use capabilities::KICK_COMMUNITY_NOTE;
pub use pusher::chatroom_lookup::KickChannelInfo;
