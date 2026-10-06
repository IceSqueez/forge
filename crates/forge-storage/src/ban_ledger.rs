use std::fmt;

use async_trait::async_trait;
use forge_types::redaction::Redacted;
use time::{Duration, OffsetDateTime};

use crate::{StorageError, ViewerPlatform};

pub const BAN_LEDGER_LIST_CAP: usize = 500;

pub const BAN_TERM_MATCH_TOLERANCE: Duration = Duration::SECOND;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BanLedgerKey {
    pub platform: ViewerPlatform,
    pub channel_id: String,
    pub viewer_id: String,
}

impl fmt::Debug for BanLedgerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BanLedgerKey")
            .field("platform", &self.platform)
            .field("channel_id", &self.channel_id)
            .field("viewer_id", &Redacted)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BanOrigin {
    IssuedByForge,
    Observed,
}

impl BanOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IssuedByForge => "forge",
            Self::Observed => "observed",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "forge" => Some(Self::IssuedByForge),
            "observed" => Some(Self::Observed),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BanLedgerEntry {
    pub key: BanLedgerKey,
    pub viewer_name: String,
    pub reason: Option<String>,
    pub moderator: Option<String>,
    pub banned_at: OffsetDateTime,
    pub expires_at: Option<OffsetDateTime>,
    pub platform_ban_id: Option<String>,
    pub origin: BanOrigin,
}

impl BanLedgerEntry {
    pub fn is_active_at(&self, now: OffsetDateTime) -> bool {
        self.expires_at.is_none_or(|expires_at| expires_at > now)
    }

    pub fn has_same_term_as(&self, other: &Self) -> bool {
        match (self.expires_at, other.expires_at) {
            (None, None) => true,
            (Some(own_expiry), Some(other_expiry)) => {
                let own_term = own_expiry - self.banned_at;
                let other_term = other_expiry - other.banned_at;
                (own_term - other_term).abs() < BAN_TERM_MATCH_TOLERANCE
            }
            _ => false,
        }
    }
}

impl fmt::Debug for BanLedgerEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BanLedgerEntry")
            .field("key", &self.key)
            .field("viewer_name", &Redacted)
            .field("reason", &self.reason.as_ref().map(|_| Redacted))
            .field("moderator", &self.moderator.as_ref().map(|_| Redacted))
            .field("banned_at", &self.banned_at)
            .field("expires_at", &self.expires_at)
            .field(
                "platform_ban_id",
                &self.platform_ban_id.as_ref().map(|_| Redacted),
            )
            .field("origin", &self.origin)
            .finish()
    }
}

pub fn merge_ban_entry(
    stored: Option<BanLedgerEntry>,
    incoming: BanLedgerEntry,
    now: OffsetDateTime,
) -> BanLedgerEntry {
    let Some(stored) = stored else {
        return incoming;
    };
    let continues_stored_ban = incoming.origin == BanOrigin::Observed
        && stored.is_active_at(now)
        && stored.has_same_term_as(&incoming);
    if !continues_stored_ban {
        return incoming;
    }
    BanLedgerEntry {
        viewer_name: incoming.viewer_name,
        reason: stored.reason.or(incoming.reason),
        moderator: stored.moderator.or(incoming.moderator),
        platform_ban_id: stored.platform_ban_id.or(incoming.platform_ban_id),
        ..stored
    }
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait BanLedgerRepo: Send + Sync {
    async fn upsert(
        &self,
        entry: &BanLedgerEntry,
        now: OffsetDateTime,
    ) -> Result<BanLedgerEntry, StorageError>;

    async fn remove(&self, key: &BanLedgerKey) -> Result<bool, StorageError>;

    async fn get(
        &self,
        key: &BanLedgerKey,
        now: OffsetDateTime,
    ) -> Result<Option<BanLedgerEntry>, StorageError>;

    async fn list_active(
        &self,
        platform: ViewerPlatform,
        channel_id: &str,
        now: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<BanLedgerEntry>, StorageError>;
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use time::{Duration, OffsetDateTime};

    use super::{
        BAN_TERM_MATCH_TOLERANCE, BanLedgerEntry, BanLedgerKey, BanOrigin, merge_ban_entry,
    };
    use crate::ViewerPlatform;

    const TEN_MINUTES: Duration = Duration::minutes(10);

    fn at(seconds: i64) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_790_000_000 + seconds).expect("a valid instant")
    }

    fn entry(
        origin: BanOrigin,
        banned_at: OffsetDateTime,
        term: Option<Duration>,
        name: &str,
    ) -> BanLedgerEntry {
        BanLedgerEntry {
            key: BanLedgerKey {
                platform: ViewerPlatform::Kick,
                channel_id: "channel".to_owned(),
                viewer_id: "viewer".to_owned(),
            },
            viewer_name: name.to_owned(),
            reason: None,
            moderator: None,
            banned_at,
            expires_at: term.map(|term| banned_at + term),
            platform_ban_id: None,
            origin,
        }
    }

    fn stored_with_ban_id(term: Option<Duration>) -> BanLedgerEntry {
        BanLedgerEntry {
            reason: Some("stored reason".to_owned()),
            moderator: Some("stored moderator".to_owned()),
            platform_ban_id: Some("stale-ban-id".to_owned()),
            ..entry(BanOrigin::IssuedByForge, at(0), term, "old_name")
        }
    }

    fn observed(banned_at: OffsetDateTime, term: Option<Duration>) -> BanLedgerEntry {
        entry(BanOrigin::Observed, banned_at, term, "new_name")
    }

    #[test]
    fn merge_returns_the_incoming_entry_verbatim_when_it_does_not_continue_the_stored_ban() {
        let issued_by_forge = BanLedgerEntry {
            platform_ban_id: None,
            ..entry(
                BanOrigin::IssuedByForge,
                at(30),
                Some(TEN_MINUTES),
                "forge_name",
            )
        };
        let cases = [
            (
                "no stored entry",
                None,
                observed(at(30), Some(TEN_MINUTES)),
                at(30),
            ),
            (
                "issued by forge over an active same-term ban",
                Some(stored_with_ban_id(Some(TEN_MINUTES))),
                issued_by_forge,
                at(30),
            ),
            (
                "observed exactly at the stored expiry",
                Some(stored_with_ban_id(Some(TEN_MINUTES))),
                observed(at(600), Some(TEN_MINUTES)),
                at(600),
            ),
            (
                "observed long after the stored expiry",
                Some(stored_with_ban_id(Some(TEN_MINUTES))),
                observed(at(5_000), Some(TEN_MINUTES)),
                at(5_000),
            ),
            (
                "observed with a different timeout duration",
                Some(stored_with_ban_id(Some(TEN_MINUTES))),
                observed(at(30), Some(Duration::minutes(20))),
                at(30),
            ),
            (
                "observed timeout over a permanent ban",
                Some(stored_with_ban_id(None)),
                observed(at(30), Some(TEN_MINUTES)),
                at(30),
            ),
            (
                "observed permanent ban over a timeout",
                Some(stored_with_ban_id(Some(TEN_MINUTES))),
                observed(at(30), None),
                at(30),
            ),
            (
                "observed term longer by exactly the tolerance",
                Some(stored_with_ban_id(Some(TEN_MINUTES))),
                observed(at(30), Some(TEN_MINUTES + BAN_TERM_MATCH_TOLERANCE)),
                at(30),
            ),
            (
                "observed term shorter by exactly the tolerance",
                Some(stored_with_ban_id(Some(TEN_MINUTES))),
                observed(at(30), Some(TEN_MINUTES - BAN_TERM_MATCH_TOLERANCE)),
                at(30),
            ),
        ];

        for (label, stored, incoming, now) in cases {
            let merged = merge_ban_entry(stored, incoming.clone(), now);
            assert_eq!(merged, incoming, "{label}");
        }
    }

    #[test]
    fn merge_observed_continuation_keeps_the_stored_ban_identity_and_takes_the_new_name() {
        let just_under = BAN_TERM_MATCH_TOLERANCE - Duration::MILLISECOND;
        let cases = [
            ("both permanent", None, observed(at(30), None), at(30)),
            (
                "same timeout term",
                Some(TEN_MINUTES),
                observed(at(30), Some(TEN_MINUTES)),
                at(30),
            ),
            (
                "term longer by just under the tolerance",
                Some(TEN_MINUTES),
                observed(at(30), Some(TEN_MINUTES + just_under)),
                at(30),
            ),
            (
                "term shorter by just under the tolerance",
                Some(TEN_MINUTES),
                observed(at(30), Some(TEN_MINUTES - just_under)),
                at(30),
            ),
            (
                "one millisecond before the stored expiry",
                Some(TEN_MINUTES),
                observed(at(30), Some(TEN_MINUTES)),
                at(600) - Duration::MILLISECOND,
            ),
        ];

        for (label, stored_term, incoming, now) in cases {
            let stored = stored_with_ban_id(stored_term);
            let expected = BanLedgerEntry {
                viewer_name: "new_name".to_owned(),
                ..stored.clone()
            };

            let merged = merge_ban_entry(Some(stored), incoming, now);

            assert_eq!(merged, expected, "{label}");
        }
    }

    #[test]
    fn merge_observed_continuation_fills_only_the_missing_reason_moderator_and_ban_id() {
        let bare_stored = entry(BanOrigin::Observed, at(0), Some(TEN_MINUTES), "old_name");
        let incoming = BanLedgerEntry {
            reason: Some("incoming reason".to_owned()),
            moderator: Some("incoming moderator".to_owned()),
            platform_ban_id: Some("incoming-ban-id".to_owned()),
            ..observed(at(30), Some(TEN_MINUTES))
        };
        let full_stored = stored_with_ban_id(Some(TEN_MINUTES));
        let cases = [
            (
                "stored fields missing",
                bare_stored.clone(),
                (
                    Some("incoming reason"),
                    Some("incoming moderator"),
                    Some("incoming-ban-id"),
                ),
            ),
            (
                "stored fields present",
                full_stored,
                (
                    Some("stored reason"),
                    Some("stored moderator"),
                    Some("stale-ban-id"),
                ),
            ),
        ];

        for (label, stored, (reason, moderator, ban_id)) in cases {
            let merged = merge_ban_entry(Some(stored), incoming.clone(), at(30));
            assert_eq!(
                (
                    merged.reason.as_deref(),
                    merged.moderator.as_deref(),
                    merged.platform_ban_id.as_deref(),
                ),
                (reason, moderator, ban_id),
                "{label}"
            );
        }
    }

    #[test]
    fn ban_ledger_debug_hides_viewer_identity_reason_moderator_and_ban_id() {
        let entry = BanLedgerEntry {
            key: BanLedgerKey {
                platform: ViewerPlatform::Kick,
                channel_id: "channel".to_owned(),
                viewer_id: "SENTINEL_VIEWER_ID".to_owned(),
            },
            viewer_name: "SENTINEL_NAME".to_owned(),
            reason: Some("SENTINEL_REASON".to_owned()),
            moderator: Some("SENTINEL_MODERATOR".to_owned()),
            banned_at: at(0),
            expires_at: None,
            platform_ban_id: Some("SENTINEL_BAN_ID".to_owned()),
            origin: BanOrigin::Observed,
        };

        let rendered = format!("{entry:?} {:?}", entry.key);

        assert!(!rendered.contains("SENTINEL"), "{rendered}");
        assert!(rendered.contains("Kick"), "{rendered}");
    }
}
