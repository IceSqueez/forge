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
