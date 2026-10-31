mod client;
pub(crate) mod content;
pub(crate) mod credentials;
mod embed;
#[cfg(test)]
#[path = "../../forge-platform-core/test_support/emitted_codes.rs"]
mod emitted_codes;
mod error;
pub(crate) mod events;
mod health;
mod integration;
mod mention;
mod quick_actions;
pub(crate) mod ratelimit;
pub(crate) mod route;
mod runners;
mod sink;
mod status;
pub mod vocabulary;
mod webhooks;

pub use client::DiscordClient;
pub use content::WebhookPost;
pub use credentials::DISCORD_CRED_PREFIX;
pub use embed::DiscordEmbed;
pub use embed::DiscordEmbedField;
pub use error::DiscordError;
pub use error::ValidationFault;
pub use health::DiscordSendHealth;
pub use integration::DISCORD_INTEGRATION;
pub use mention::MentionPolicy;
pub use runners::register_discord_sub_actions;
pub use webhooks::validate_webhook_url;
