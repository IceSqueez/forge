pub mod auth;
pub mod builtin;
pub mod capabilities;
pub mod channel;
pub mod channel_info;
pub mod chat;
pub mod chat_platform;
pub(crate) mod control;
pub mod credentials;
pub mod credentials_manager;
pub mod error;
mod event_channel;
pub mod moderation;
pub mod poller;
pub(crate) mod reconnect;
pub mod rewards;
pub mod send;
pub mod sub_actions;
pub mod triggers;
pub mod viewer_poll;

pub use auth::{KickAuthBundle, KickAuthFlow, LoopbackCode, client_credentials, kick_auth_flow};
pub use builtin::{KickIntegrationBundle, register_kick_triggers};
pub use capabilities::kick_capabilities;
pub use channel::{ChannelSnapshot, KickChannel};
pub use channel_info::{ChannelInfoFetcher, KickChannelInfo};
pub use chat::{KickChat, KickChatHandle};
pub use chat_platform::KickPlatform;
pub use credentials::{CREDENTIAL_KEY, KickCredentials};
pub use credentials_manager::KickCredentialsManager;
pub use moderation::KickModeration;
pub use poller::spawn_kick_poller;
pub use rewards::{CreateRewardParams, KickRewards, RedemptionRecord, UpdateRewardParams};
pub use send::KickSendChat;
pub use sub_actions::{
    AcceptRedemptionRunner, BanUserRunner, CreateRewardRunner, DeleteMessageRunner,
    DeleteRewardRunner, KickSubActionDeps, RejectRedemptionRunner, SendMessageRunner,
    TimeoutUserRunner, UnbanUserRunner, UpdateInfoRunner, UpdateRewardRunner,
    register_kick_sub_actions,
};
pub use viewer_poll::{KickViewerPoll, KickViewerSource};
