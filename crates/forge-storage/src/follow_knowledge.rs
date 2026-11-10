use std::fmt;

use async_trait::async_trait;
use forge_types::{PlatformId, Redacted};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FollowKnowledgeKey {
    pub platform: PlatformId,
    pub viewer_id: String,
}

impl fmt::Debug for FollowKnowledgeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FollowKnowledgeKey")
            .field("platform", &self.platform)
            .field("viewer_id", &Redacted)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnownFollow {
    FollowingSince(OffsetDateTime),
    NotFollowing,
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowKnowledgeEntry {
    pub key: FollowKnowledgeKey,
    pub follow: KnownFollow,
    pub observed_at: OffsetDateTime,
    pub fresh_until: OffsetDateTime,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait FollowKnowledgeRepo: Send + Sync {
    async fn get(
        &self,
        key: &FollowKnowledgeKey,
    ) -> Result<Option<FollowKnowledgeEntry>, StorageError>;

    async fn upsert(&self, entry: &FollowKnowledgeEntry) -> Result<(), StorageError>;

    async fn seed(&self, entries: &[FollowKnowledgeEntry]) -> Result<(), StorageError>;

    async fn is_seeded(&self) -> Result<bool, StorageError>;

    async fn prune_expired(&self, now: OffsetDateTime) -> Result<u64, StorageError>;

    async fn clear_platform(&self, platform: PlatformId) -> Result<u64, StorageError>;
}
