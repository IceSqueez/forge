use async_trait::async_trait;
use forge_types::{Action, ActionId};
use time::OffsetDateTime;

use crate::StorageError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionExecution {
    pub action_id: ActionId,
    pub started_at: OffsetDateTime,
    pub duration_ms: u64,
    pub status: ExecutionStatus,
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

    async fn count(&self) -> Result<usize, StorageError>;
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError>;
    async fn ids_named(&self, name: &str) -> Result<Vec<ActionId>, StorageError>;
    async fn save(&self, action: &Action) -> Result<(), StorageError>;
    async fn delete(&self, id: ActionId) -> Result<bool, StorageError>;
    async fn telemetry(&self, id: ActionId) -> Result<ActionTelemetry, StorageError>;
    async fn record_execution(
        &self,
        action_id: ActionId,
        started_at: OffsetDateTime,
        duration_ms: u64,
        status: ExecutionStatus,
    ) -> Result<(), StorageError>;
    async fn record_executions(&self, executions: &[ActionExecution]) -> Result<(), StorageError>;
    async fn prune_executions_before(
        &self,
        cutoff: OffsetDateTime,
        chunk_rows: u32,
    ) -> Result<u64, StorageError>;

    async fn set_enabled(&self, id: ActionId, enabled: bool) -> Result<bool, StorageError>;

    async fn toggle_enabled(&self, id: ActionId) -> Result<Option<bool>, StorageError>;

    async fn duplicate(
        &self,
        source_id: ActionId,
        new_id: ActionId,
        new_name: &str,
    ) -> Result<(), StorageError>;

    async fn archive(&self, id: ActionId) -> Result<bool, StorageError> {
        Ok(self.archive_reporting_cancelled_runs(id).await?.is_some())
    }

    async fn archive_reporting_cancelled_runs(
        &self,
        id: ActionId,
    ) -> Result<Option<u64>, StorageError>;

    async fn restore(&self, id: ActionId) -> Result<bool, StorageError>;

    async fn list_archived(&self) -> Result<Vec<Action>, StorageError>;
}
