mod account;
mod api;
mod assembly;
mod broadcast;
mod builtin;
mod chat;
pub(crate) mod control;
#[cfg(test)]
#[path = "../../forge-platform-core/test_support/emitted_codes.rs"]
mod emitted_codes;
pub mod endpoint_surfaces;
#[cfg(test)]
mod endpoints_test_support;
mod integration;
mod quick_actions;
mod sub_actions;
mod triggers;
mod viewer_step;
pub mod vocabulary;

pub use account::auth::LoopbackCode;
pub use account::auth::{
    GOOGLE_AUTHORIZE_ENDPOINT, GoogleAuthFlow, YoutubeAuthBundle, YoutubeSignIn, client_credentials,
};
pub use account::credentials::{YOUTUBE_CREDENTIAL_ID, YoutubeBroadcaster, YoutubeCredentials};
pub use account::credentials_manager::YoutubeCredentialsManager;
pub use account::oauth_client::{
    GOOGLE_OAUTH_BUDGET_CAPACITY, GOOGLE_OAUTH_BUDGET_WINDOW, GoogleOAuthClient,
    google_oauth_gateway,
};
pub use api::data_api::YoutubeDataApi;
pub use api::quota_state::SharedQuota;
pub use assembly::YoutubeAssembly;
pub use assembly::YoutubeSession;
pub use broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
pub use broadcast::ad_break::YoutubeAdBreak;
pub use broadcast::channel_lookup::YoutubeChannelLookup;
pub use broadcast::channel_lookup::YoutubeChannelProfile;
pub use broadcast::live_chat_id::LiveChatIdHandle;
pub use broadcast::stream_metadata::YoutubeStreamMetadata;
pub use broadcast::stream_stats::YoutubeStreamDetails;
pub use broadcast::stream_stats::YoutubeStreamStats;
pub use broadcast::thumbnail::YoutubeThumbnail;
pub use broadcast::viewer_poll::YoutubeViewerPoll;
pub use builtin::YoutubeIntegrationBundle;
pub use chat::chat_platform::YoutubePlatform;
pub use chat::chat_poller::ChatPollTimings;
pub use chat::chat_poller::{ChatPollerStop, ChatPollerStopGuard, YoutubeChatPoller};
pub use chat::moderation::YoutubeModeration;
pub use chat::send_chat::YoutubeSendChat;
pub use integration::{YOUTUBE_CHAT_LIMITS, YOUTUBE_INTEGRATION};
pub use sub_actions::register_youtube_sub_actions;
pub use triggers::register_youtube_triggers;
pub use viewer_step::viewer_step;
