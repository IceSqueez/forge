#![doc = "ChatPlatform trait, AuthFlow taxonomy, Integration page traits."]

pub mod auth;
mod ban_list;
mod builtin;
mod capabilities;
pub mod chat;
mod collections;
mod connection_announcer;
mod credential_errors;
mod donation;
pub mod donation_test;
#[cfg(test)]
#[path = "../test_support/emitted_codes.rs"]
mod emitted_codes;
mod endpoints;
mod error;
mod follow_lookup;
mod integration;
mod live_viewers;
mod option_source;
mod sender;
pub use auth::AuthFlow;
pub use ban_list::{
    BanDuration, BanEntry, BanListOutcome, BanListSource, BanListUnavailable, BanPage,
    BanPageToken, UnbanAbility, UnbanRefusal,
};
pub use builtin::{
    ActiveRow, BannerLevel, BuiltinContent, BuiltinControl, BuiltinHealth, BuiltinStatus,
    CapabilityFlags, ContentList, ContentListItem, ControlFailure, ControlOutcome, DetailSection,
    HEALTH_CHANNEL_CAPACITY, HeaderAction, HealthBar, HealthDelta, HealthLevel, HealthMetric,
    HealthMetricId, HealthStream, HealthText, HealthValue, HeroBadge, HeroBadgeTone, InfoField,
    KeyValueRow, ListFooter, QuickAction, QuickActionAccent, QuickActionChoiceOption,
    QuickActionChoiceSource, QuickActionField, QuickActionFieldKind, QuickActionFieldValue,
    QuickActionGroup, QuickActionLiveness, QuickActions, RowAction, SectionIcon, StatColumn,
    SubscriptionRow, SubscriptionStatus, TokenColor, TrailingToken,
};
pub use capabilities::PlatformCapabilities;
pub use chat::{
    AtomicConnectionState, ChatPlatform, ConnectionState, ConnectionTransition,
    connection_state_changed_event,
};
pub use collections::{
    BuiltinCollections, CollectionFailure, CollectionField, CollectionId, CollectionItem,
    CollectionItemAccess, CollectionItemId, CollectionMetadata, CollectionOutcome,
    CollectionRevisionSignal, CollectionRevisions, CollectionToggle, RevisionWait,
};
pub use connection_announcer::ConnectionAnnouncer;
pub use credential_errors::{credential_storage_error, reauth_required};
pub use donation::{DonationProvider, DonationStream};
pub use donation_test::{
    TEST_DONATION_AMOUNT, TEST_DONATION_AMOUNT_KEY, TEST_DONATION_CURRENCY,
    TEST_DONATION_CURRENCY_KEY, TEST_DONATION_DONOR, TEST_DONATION_DONOR_KEY,
    TEST_DONATION_MESSAGE, TEST_DONATION_MESSAGE_KEY, TEST_DONATION_MESSAGES,
    TEST_DONATION_PROVIDER_KEY, TEST_DONATION_SUB_ACTION, test_donation_quick_action,
    test_donation_step,
};
pub use endpoints::{EndpointRefusal, EndpointSurface, PlatformEndpoints};
pub use error::{DeviceAuthEnd, HTTP_UNAUTHORIZED, PlatformError};
pub use follow_lookup::{FollowLookup, FollowStatus};
pub use integration::{ConnectionAffordance, IntegrationCategory, IntegrationDeclaration};
pub use live_viewers::{LiveViewerSource, ViewerReport, ViewerReportStream};
pub use option_source::{
    BuiltinOptionSource, OptionEntry, OptionHint, OptionListId, OptionListMetadata,
};
pub use sender::{
    ActingSender, SENDER_FIELD, SENT_AS_FIELD, SenderPreference, SenderRole, unknown_sender_reason,
};
