#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod capabilities;
pub mod error;
pub use auth::AuthFlow;
pub use capabilities::PlatformCapabilities;
pub use error::PlatformError;
