#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod capabilities;
pub mod chat;
pub mod error;
pub mod builtin;
pub mod paths;
pub mod rate_limit;
pub use auth::AuthFlow;
pub use capabilities::PlatformCapabilities;
pub use chat::{ChatPlatform, ConnectionState};
pub use error::PlatformError;
pub use builtin::{
    ActiveRow, BadgeTone, BannerLevel, CapabilityFlags, ContentList, ContentListItem,
    DetailSection, HeaderAction, HealthBar, HealthDelta, HealthLevel, HealthMetric, HealthStream,
    HealthValue, InfoField, BuiltinContent, BuiltinHealth, BuiltinId,
    BuiltinStatus, KeyValueRow, ListFooter, PickerKind, QuickAction, QuickActions, RowAction,
    SectionIcon, StatColumn, SubscriptionRow, SubscriptionStatus, TokenColor, TrailingToken,
};
pub use rate_limit::{RateLimitOutcome, RateLimiter};
