#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod builtin;
pub mod capabilities;
pub mod chat;
pub mod error;
pub mod paths;
pub mod rate_limit;
pub use auth::AuthFlow;
pub use builtin::{
    ActiveRow, BadgeTone, BannerLevel, BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinId,
    BuiltinStatus, CapabilityFlags, ContentList, ContentListItem, ControlFailure, ControlOutcome,
    DetailSection, HeaderAction, HealthBar, HealthDelta, HealthLevel, HealthMetric, HealthStream,
    HealthValue, InfoField, KeyValueRow, ListFooter, PickerKind, QuickAction, QuickActions,
    RowAction, SectionIcon, StatColumn, SubscriptionRow, SubscriptionStatus, TokenColor,
    TrailingToken,
};
pub use capabilities::PlatformCapabilities;
pub use chat::{ChatPlatform, ConnectionState};
pub use error::PlatformError;
pub use rate_limit::{RateLimitOutcome, RateLimiter, TokenBucketRateLimiter};
