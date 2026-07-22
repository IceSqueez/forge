use async_trait::async_trait;
use forge_types::{Action, ActionId};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Success,
    Error,
}

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
    async fn record_execution(
        &self,
        action_id: ActionId,
        started_at: OffsetDateTime,
        duration_ms: u64,
        status: ExecutionStatus,
    ) -> Result<(), StorageError>;
    /// Returns rows removed.
    async fn prune_executions_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError>;

    /// Errors with [`StorageError::NotFound`] if `source_id` does not exist. The default
    /// impl copies only the `Action` row itself, not its trigger-instance links (this
    /// trait has no visibility into `action_trigger_instances`); a real backend should
    /// override this to re-point links in the same transaction.
    async fn duplicate(
        &self,
        source_id: ActionId,
        new_id: ActionId,
        new_name: &str,
    ) -> Result<(), StorageError> {
        let mut copy = self
            .get(source_id)
            .await?
            .ok_or_else(|| StorageError::NotFound {
                key: source_id.to_string(),
            })?;
        copy.id = new_id;
        copy.name = new_name.to_owned();
        self.save(&copy).await
    }

    /// Soft delete (not [`Self::delete`]): hides `id` from `get`/`list`/`list_by_group`
    /// until [`Self::restore`]; telemetry survives untouched. Default impl has no
    /// generic archived-state representation and returns [`StorageError::NotReady`];
    /// a real backend must override this.
    async fn archive(&self, _id: ActionId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Reverses [`Self::archive`]; see its default-impl caveat.
    async fn restore(&self, _id: ActionId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Mirror of `list`, which excludes archived entries. Default impl reports none.
    async fn list_archived(&self) -> Result<Vec<Action>, StorageError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn _trait_is_dyn_safe(_: &dyn ActionRepo) {}
}
