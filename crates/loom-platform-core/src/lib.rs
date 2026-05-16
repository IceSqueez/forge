#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod capabilities;
pub mod chat;
pub mod error;
pub mod integration;
pub mod rate_limit;
pub use auth::AuthFlow;
pub use capabilities::PlatformCapabilities;
pub use chat::{ChatPlatform, ConnectionState};
pub use error::PlatformError;
pub use integration::{
    CatalogEntry, HealthColor, HealthMetric, IntegrationCatalog, IntegrationHealth, IntegrationId,
    IntegrationStatus, QuickAction, QuickActions,
};
pub use rate_limit::{RateLimitOutcome, RateLimiter};
