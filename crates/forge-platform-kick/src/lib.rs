mod api_gateway;
pub mod auth;
mod ban_ledger;
#[cfg(test)]
mod ban_ledger_test_support;
mod ban_list;
mod builtin;
pub mod capabilities;
mod categories;
mod channel;
mod channel_info;
pub mod chat;
mod chat_platform;
mod chat_session;
pub(crate) mod control;
mod credentials;
mod credentials_manager;
#[cfg(test)]
mod endpoints_test_support;
pub mod error;
mod event_channel;
mod integration;
#[cfg(test)]
mod log_capture;
mod moderation;
mod normalize;
mod oauth_client;
mod payload_fields;
mod poller;
mod rewards;
mod send;
mod sub_actions;
mod timestamp;
mod triggers;

pub use api_gateway::{
    KICK_API_BUDGET_CAPACITY, KICK_API_BUDGET_WINDOW, KICK_CHANNEL_INFO_BUDGET_CAPACITY,
    KICK_CHANNEL_INFO_BUDGET_WINDOW, KICK_GATEWAY_PROFILE, kick_api_gateway,
    kick_channel_info_gateway,
};
pub use auth::{
    KickAuthBundle, KickAuthFlow, KickSignIn, LoopbackCode, client_credentials, kick_auth_flow,
};
pub use builtin::{KickIntegrationBundle, register_kick_triggers};
pub use capabilities::kick_capabilities;
pub use categories::{CategoryMatch, KickCategories};
pub use channel::{ChannelSnapshot, KickChannel};
pub use channel_info::{ChannelInfoFetcher, KickChannelInfo};
pub use chat::{KickChat, KickChatHandle};
pub use chat_platform::KickPlatform;
pub use credentials::{CREDENTIAL_KEY, KickCredentials};
pub use credentials_manager::KickCredentialsManager;
pub use integration::KICK_INTEGRATION;
pub use moderation::KickModeration;
pub use oauth_client::{
    KICK_OAUTH_BUDGET_CAPACITY, KICK_OAUTH_BUDGET_WINDOW, KICK_OAUTH_GATEWAY_PROFILE,
    KickOAuthClient, kick_oauth_gateway,
};
pub use poller::{KickPollerHandle, KickViewerSource, PollerAuth, spawn_kick_poller};
pub use rewards::{CreateRewardParams, KickRewards, RedemptionRecord, UpdateRewardParams};
pub use send::KickSendChat;
pub use sub_actions::{
    AcceptRedemptionRunner, BanUserRunner, CreateRewardRunner, DeleteMessageRunner,
    DeleteRewardRunner, KickSubActionDeps, LookupCategoryRunner, LookupStreamStatsRunner,
    LookupUserRunner, RejectRedemptionRunner, SendMessageRunner, TimeoutUserRunner,
    UnbanUserRunner, UpdateInfoRunner, UpdateRewardRunner, register_kick_sub_actions,
};
