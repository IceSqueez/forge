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
    /// Removes execution telemetry rows started before `cutoff`; returns rows removed.
    async fn prune_executions_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError>;

    /// Copies `source_id`'s row into a new action `new_id` named `new_name`.
    ///
    /// Errors with [`StorageError::NotFound`] if `source_id` does not exist.
    ///
    /// The default impl composes `get`/`save` and copies only the `Action`
    /// row itself — it does **not** carry over trigger-instance links, since
    /// this trait has no visibility into `action_trigger_instances`. A real
    /// backend should override this with a single transaction that also
    /// re-points every linked trigger instance to `new_id`, so the duplicate
    /// ends up with the same trigger links as the source instead of zero.
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

    /// Marks `id` archived: invisible to `get`, `list`, and `list_by_group` until
    /// [`Self::restore`] is called. The row and its telemetry survive untouched — this
    /// is a soft delete, not [`Self::delete`]. Returns `false` when `id` does not
    /// exist or is already archived.
    ///
    /// The default impl has no generic representation of archived state without a
    /// dedicated column, so it returns [`StorageError::NotReady`]; a real backend
    /// must override this.
    async fn archive(&self, _id: ActionId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Clears the marker set by [`Self::archive`], restoring `id` to `get`/`list`/
    /// `list_by_group` visibility. Returns `false` when `id` does not exist or is not
    /// currently archived.
    ///
    /// See [`Self::archive`] for the default-impl caveat.
    async fn restore(&self, _id: ActionId) -> Result<bool, StorageError> {
        Err(StorageError::NotReady)
    }

    /// Returns archived actions only — the mirror of `list`, which excludes them.
    ///
    /// The default impl reports no archived entries, consistent with a backend that
    /// does not support archiving.
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
