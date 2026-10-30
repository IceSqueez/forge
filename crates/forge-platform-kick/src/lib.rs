mod api;
pub mod auth;
mod ban_ledger;
#[cfg(test)]
mod ban_ledger_test_support;
mod ban_list;
mod builtin;
pub mod capabilities;
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

pub use api::categories::{CategoryMatch, KickCategories};
pub use api::channel::{ChannelSnapshot, KickChannel};
pub use api::chat_messages::KickSendChat;
pub use api::moderation::KickModeration;
pub use api::poller::{KickPollerHandle, KickViewerSource, PollerAuth, spawn_kick_poller};
pub use api::rewards::{CreateRewardParams, KickRewards, RedemptionRecord, UpdateRewardParams};
pub use auth::credentials::{CREDENTIAL_KEY, KickCredentials};
pub use auth::credentials_manager::KickCredentialsManager;
pub use auth::oauth_client::{
    KICK_OAUTH_BUDGET_CAPACITY, KICK_OAUTH_BUDGET_WINDOW, KICK_OAUTH_GATEWAY_PROFILE,
    KickOAuthClient, kick_oauth_gateway,
};
pub use auth::{
    KickAuthBundle, KickAuthFlow, KickSignIn, LoopbackCode, client_credentials, kick_auth_flow,
};
pub use builtin::{KickIntegrationBundle, register_kick_triggers};
pub use capabilities::kick_capabilities;
pub use gateway::{
    KICK_API_BUDGET_CAPACITY, KICK_API_BUDGET_WINDOW, KICK_CHANNEL_INFO_BUDGET_CAPACITY,
    KICK_CHANNEL_INFO_BUDGET_WINDOW, KICK_GATEWAY_PROFILE, kick_api_gateway,
    kick_channel_info_gateway,
};
pub use integration::KICK_INTEGRATION;
pub use platform::KickPlatform;
pub use pusher::chat::{KickChat, KickChatHandle};
pub use pusher::chatroom_lookup::{ChannelInfoFetcher, KickChannelInfo};
pub use pusher::{chat, error, protocol};
pub use sub_actions::{
    AcceptRedemptionRunner, BanUserRunner, CreateRewardRunner, DeleteMessageRunner,
    DeleteRewardRunner, KickSubActionDeps, LookupCategoryRunner, LookupStreamStatsRunner,
    LookupUserRunner, RejectRedemptionRunner, SendMessageRunner, TimeoutUserRunner,
    UnbanUserRunner, UpdateInfoRunner, UpdateRewardRunner, register_kick_sub_actions,
};
