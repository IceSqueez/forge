pub mod config_keys;
pub mod event_kinds;
pub mod events;
mod eventsub_topics;
pub(crate) mod fields;
pub(crate) mod labels;
pub mod messages;
pub(crate) mod option_lists;
pub mod sub_action_ids;
pub mod topics;
pub mod trigger_ids;
mod values;

use forge_types::PlatformId;

pub use events::{
    BotStatusChanged, BotStatusTag, EventSubSubscriptionCreated, FallbackLevel,
    PlatformReauthRequired, SenderFallback,
};
pub use values::RaidDirection;

pub const PLATFORM: PlatformId = PlatformId::Twitch;
pub const PLATFORM_ID: &str = PLATFORM.as_str();
