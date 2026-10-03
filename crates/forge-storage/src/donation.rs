use async_trait::async_trait;
use forge_types::{Donation, IntegrationId};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, PartialEq)]
pub struct StoredDonation {
    pub donation: Donation,
    pub received_at: OffsetDateTime,
    pub announced_at: Option<OffsetDateTime>,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait DonationRepo: Send + Sync {
    async fn insert_if_new(
        &self,
        donation: &Donation,
        received_at: OffsetDateTime,
    ) -> Result<bool, StorageError>;

    async fn mark_announced(
        &self,
        provider: &IntegrationId,
        donation_id: &str,
        announced_at: OffsetDateTime,
    ) -> Result<bool, StorageError>;

    async fn list_recent(&self, limit: usize) -> Result<Vec<StoredDonation>, StorageError>;

    async fn has_donations_from(&self, provider: &IntegrationId) -> Result<bool, StorageError>;

    async fn list_unannounced_occurred_since(
        &self,
        not_before: OffsetDateTime,
    ) -> Result<Vec<StoredDonation>, StorageError>;

    async fn mark_announced_all_occurred_before(
        &self,
        not_before: OffsetDateTime,
        announced_at: OffsetDateTime,
    ) -> Result<u64, StorageError>;
}
