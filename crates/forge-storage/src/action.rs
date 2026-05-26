use async_trait::async_trait;
use forge_types::{Action, ActionId};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionTelemetry {
    pub last_fired_at: Option<OffsetDateTime>,
    pub runs_today: u64,
    pub avg_duration_ms: Option<u64>,
    pub errors_7d: u64,
}

#[cfg_attr(feature = "test-mocks", mockall::automock)]
#[async_trait]
pub trait ActionRepo: Send + Sync {
    async fn list(&self) -> Result<Vec<Action>, StorageError>;
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError>;
    async fn save(&self, action: &Action) -> Result<(), StorageError>;
    /// Returns true if a row was removed.
    async fn delete(&self, id: ActionId) -> Result<bool, StorageError>;
    async fn list_by_group<'a>(
        &'a self,
        group: Option<&'a str>,
    ) -> Result<Vec<Action>, StorageError>;
    async fn telemetry(&self, id: ActionId) -> Result<ActionTelemetry, StorageError>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn ActionRepo) {}
}
