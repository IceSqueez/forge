#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod capabilities;
pub mod chat;
pub mod error;
pub mod integration;
pub mod oauth;
pub mod paths;
pub mod rate_limit;
pub use auth::AuthFlow;
pub use capabilities::PlatformCapabilities;
pub use chat::{ChatPlatform, ConnectionState};
pub use error::PlatformError;
pub use integration::{
    ActiveRow, BannerLevel, CapabilityFlags, ContentList, ContentListItem, DetailSection,
    HeaderAction, HealthBar, HealthDelta, HealthLevel, HealthMetric, HealthStream, HealthValue,
    InfoField, IntegrationContent, IntegrationHealth, IntegrationId, IntegrationStatus,
    KeyValueRow, ListFooter, PickerKind, QuickAction, QuickActions, RowAction, SectionIcon,
    StatColumn, SubscriptionRow, SubscriptionStatus, TokenColor, TrailingToken,
};
pub use oauth::{
    DeviceCodePoller, DeviceCodeRequest, DeviceCodeResponse, PollOutcome, TokenResponse,
};
pub use rate_limit::{RateLimitOutcome, RateLimiter};
