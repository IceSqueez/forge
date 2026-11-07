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
#[expect(
    dead_code,
    unreachable_pub,
    unnameable_types,
    clippy::enum_variant_names
)]
mod stream_list_proto;
mod sub_actions;
mod triggers;
mod viewer_step;
pub mod vocabulary;

pub use account::auth::LoopbackCode;
pub use account::auth::{
    GOOGLE_AUTHORIZE_ENDPOINT, GoogleAuthFlow, YouTubeAuthBundle, YouTubeSignIn, client_credentials,
};
pub use account::credentials::{YOUTUBE_CREDENTIAL_ID, YouTubeBroadcaster, YouTubeCredentials};
pub use account::credentials_manager::YouTubeCredentialsManager;
pub use account::oauth_client::{
    GOOGLE_OAUTH_BUDGET_CAPACITY, GOOGLE_OAUTH_BUDGET_WINDOW, GoogleOAuthClient,
    google_oauth_gateway,
};
pub use account::policy_links::{FORGE_PRIVACY_POLICY_URL, YOUTUBE_TERMS_OF_SERVICE_URL};
pub use api::data_api::YouTubeDataApi;
pub use api::quota_state::SharedQuota;
pub use assembly::YouTubeAssembly;
pub use assembly::YouTubeSession;
pub use broadcast::active_broadcast_id::ActiveBroadcastIdHandle;
pub use broadcast::ad_break::YouTubeAdBreak;
pub use broadcast::channel_lookup::YouTubeChannelLookup;
pub use broadcast::channel_lookup::YouTubeChannelProfile;
pub use broadcast::live_chat_id::LiveChatIdHandle;
pub use broadcast::stream_metadata::YouTubeStreamMetadata;
pub use broadcast::stream_stats::YouTubeStreamDetails;
pub use broadcast::stream_stats::YouTubeStreamStats;
pub use broadcast::thumbnail::YouTubeThumbnail;
pub use builtin::YouTubeIntegrationBundle;
pub use chat::chat_platform::YouTubePlatform;
pub use chat::chat_poller::ChatPollTimings;
pub use chat::chat_poller::{ChatPollerStop, ChatPollerStopGuard, YouTubeChatPoller};
pub use chat::moderation::YouTubeModeration;
pub use chat::send_chat::YouTubeSendChat;
pub use integration::{YOUTUBE_CHAT_LIMITS, YOUTUBE_INTEGRATION};
pub use sub_actions::register_youtube_sub_actions;
pub use triggers::register_youtube_triggers;
pub use viewer_step::viewer_step;
