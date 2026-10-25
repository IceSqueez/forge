pub mod api_gateway;
pub mod auth;
mod ban_ledger;
#[cfg(test)]
mod ban_ledger_test_support;
mod ban_list;
pub mod builtin;
pub mod capabilities;
pub mod categories;
pub mod channel;
pub mod channel_info;
pub mod chat;
pub mod chat_platform;
mod chat_session;
pub(crate) mod control;
pub mod credentials;
pub mod credentials_manager;
#[cfg(test)]
mod endpoints_test_support;
pub mod error;
mod event_channel;
pub mod integration;
#[cfg(test)]
mod log_capture;
pub mod moderation;
mod normalize;
mod payload_fields;
pub mod poller;
pub mod rewards;
pub mod send;
pub mod sub_actions;
mod timestamp;
pub mod triggers;

pub use api_gateway::{
    KICK_API_BUDGET_CAPACITY, KICK_API_BUDGET_WINDOW, KICK_CHANNEL_INFO_BUDGET_CAPACITY,
    KICK_CHANNEL_INFO_BUDGET_WINDOW, KICK_GATEWAY_PROFILE, kick_api_gateway,
    kick_channel_info_gateway,
};
pub use auth::{KickAuthBundle, KickAuthFlow, LoopbackCode, client_credentials, kick_auth_flow};
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
pub use poller::{KickPollerHandle, KickViewerSource, PollerAuth, spawn_kick_poller};
pub use rewards::{CreateRewardParams, KickRewards, RedemptionRecord, UpdateRewardParams};
pub use send::KickSendChat;
pub use sub_actions::{
    AcceptRedemptionRunner, BanUserRunner, CreateRewardRunner, DeleteMessageRunner,
    DeleteRewardRunner, KickSubActionDeps, LookupCategoryRunner, LookupStreamStatsRunner,
    LookupUserRunner, RejectRedemptionRunner, SendMessageRunner, TimeoutUserRunner,
    UnbanUserRunner, UpdateInfoRunner, UpdateRewardRunner, register_kick_sub_actions,
};
