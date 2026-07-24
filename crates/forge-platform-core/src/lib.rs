#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod backoff;
pub mod builtin;
pub mod capabilities;
pub mod chat;
pub mod error;
pub mod live_viewers;
pub mod net;
pub mod paths;
pub mod poll;
pub mod rate_limit;
pub use auth::AuthFlow;
pub use backoff::Backoff;
pub use builtin::{
    ActiveRow, BannerLevel, BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinId,
    BuiltinStatus, CapabilityFlags, ContentList, ContentListItem, ControlFailure, ControlOutcome,
    DetailSection, HeaderAction, HealthBar, HealthDelta, HealthLevel, HealthMetric, HealthStream,
    HealthValue, HeroBadge, HeroBadgeTone, InfoField, KeyValueRow, ListFooter, PickerKind,
    QuickAction, QuickActionAccent, QuickActions, RowAction, SectionIcon, StatColumn,
    SubscriptionRow, SubscriptionStatus, TokenColor, TrailingToken,
};
pub use capabilities::PlatformCapabilities;
pub use chat::{
    AtomicConnectionState, CONNECTION_STATE_CHANGED_KIND, ChatPlatform, ConnectionState,
    connection_state_changed_event,
};
pub use error::PlatformError;
pub use live_viewers::{LiveViewerSource, ViewerReport, ViewerReportStream};
pub use net::is_private_or_special;
pub use poll::DedupSet;
pub use rate_limit::{
    MAX_ACQUIRE_ATTEMPTS, MAX_THROTTLE_WAIT, RateLimitOutcome, RateLimiter, TokenBucketRateLimiter,
    acquire_or_wait,
};
