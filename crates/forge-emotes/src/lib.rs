#![doc = "Third-party chat emote catalogs (7TV, BetterTTV, FrankerFaceZ) for one Twitch channel, refreshed in the background."]

mod better_ttv;
mod catalog;
mod channel_emotes;
pub mod endpoint_surfaces;
mod error;
mod franker_face_z;
mod provider;
mod seven_tv;

pub use catalog::EmoteCatalog;
pub use channel_emotes::{ChannelEmotes, FAILED_SOURCE_RETRY, REFRESH_INTERVAL};
pub use error::EmotesError;
pub use provider::{EmoteSource, PRECEDENCE, ProviderEmote};
