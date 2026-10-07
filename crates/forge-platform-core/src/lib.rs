#![doc = "ChatPlatform trait, AuthFlow taxonomy, RateLimiter, Integration page traits."]

pub mod auth;
pub mod backoff;
pub mod ban_list;
pub mod builtin;
pub mod capabilities;
pub mod chat;
pub mod collections;
pub mod credential_errors;
pub mod donation;
pub mod donation_test;
pub mod endpoints;
pub mod error;
pub mod follow_lookup;
pub mod integration;
pub mod live_viewers;
pub mod net;
pub mod paths;
pub mod poll;
pub mod rate_limit;
pub use auth::AuthFlow;
pub use backoff::Backoff;
pub use ban_list::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
    BanPageToken, UnbanAbility, UnbanRefusal,
};
pub use builtin::{
    ActiveRow, BannerLevel, BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus,
    CapabilityFlags, ContentList, ContentListItem, ControlFailure, ControlOutcome, DetailSection,
    HeaderAction, HealthBar, HealthDelta, HealthLevel, HealthMetric, HealthStream, HealthValue,
    HeroBadge, HeroBadgeTone, InfoField, KeyValueRow, ListFooter, PickerKind, QuickAction,
    QuickActionAccent, QuickActionChoiceOption, QuickActionChoiceSource, QuickActionField,
    QuickActionFieldKind, QuickActionFieldValue, QuickActionLiveness, QuickActions, RowAction,
    SectionIcon, StatColumn, SubscriptionRow, SubscriptionStatus, TokenColor, TrailingToken,
};
pub use capabilities::PlatformCapabilities;
pub use chat::{
    AtomicConnectionState, CONNECTION_STATE_CHANGED_KIND, ChatPlatform, ConnectionState,
    connection_state_changed_event,
};
pub use collections::{
    BuiltinCollections, CollectionFailure, CollectionField, CollectionId, CollectionItem,
    CollectionItemAccess, CollectionItemId, CollectionMetadata, CollectionOutcome,
    CollectionRevisionSignal, CollectionRevisions, CollectionToggle, RevisionWait,
};
pub use credential_errors::{credential_storage_error, reauth_required};
pub use donation::{DonationProvider, DonationStream};
pub use donation_test::{
    TEST_DONATION_AMOUNT, TEST_DONATION_AMOUNT_KEY, TEST_DONATION_CURRENCY,
    TEST_DONATION_CURRENCY_KEY, TEST_DONATION_DONOR, TEST_DONATION_DONOR_KEY,
    TEST_DONATION_MESSAGE, TEST_DONATION_MESSAGE_KEY, TEST_DONATION_PROVIDER_KEY,
    TEST_DONATION_SUB_ACTION, test_donation_quick_action, test_donation_step,
};
pub use endpoints::{EndpointRefusal, EndpointSurface, PlatformEndpoints};
pub use error::{HTTP_UNAUTHORIZED, NON_HTTP_STATUS, PlatformError};
pub use follow_lookup::{FollowLookup, FollowStatus};
pub use integration::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
pub use live_viewers::{LiveViewerSource, ViewerReport, ViewerReportStream};
pub use net::is_private_or_special;
pub use poll::DedupSet;
pub use rate_limit::{
    DEFAULT_RETRY_AFTER_SECS, MAX_ACQUIRE_ATTEMPTS, MAX_THROTTLE_WAIT, RateLimitOutcome,
    RateLimitUsage, RateLimiter, TokenBucketRateLimiter, acquire_or_wait,
};
