#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod builtin;
pub mod capabilities;
pub mod chat;
pub mod error;
pub mod live_viewers;
pub mod net;
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
pub use chat::{
    CONNECTION_STATE_CHANGED_KIND, ChatPlatform, ConnectionState, connection_state_changed_event,
};
pub use error::PlatformError;
pub use live_viewers::{LiveViewerSource, ViewerReport, ViewerReportStream};
pub use net::is_private_or_special;
pub use rate_limit::{RateLimitOutcome, RateLimiter, TokenBucketRateLimiter};
