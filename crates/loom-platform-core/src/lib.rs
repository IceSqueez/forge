#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod error;
pub use auth::AuthFlow;
pub use error::PlatformError;
