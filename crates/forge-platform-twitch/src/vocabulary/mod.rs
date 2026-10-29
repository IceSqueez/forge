pub mod event_kinds;
mod events;
mod eventsub_topics;
pub(crate) mod fields;
pub mod sub_action_ids;
pub mod topics;
pub mod trigger_ids;
mod values;

use forge_types::PlatformId;

pub use events::{
    BotStatusChanged, BotStatusTag, EventSubSubscriptionCreated, FallbackLevel,
    PlatformReauthRequired, SenderFallback,
};
pub(crate) use values::RaidDirection;

pub const PLATFORM: PlatformId = PlatformId::Twitch;
pub const PLATFORM_ID: &str = PLATFORM.as_str();
