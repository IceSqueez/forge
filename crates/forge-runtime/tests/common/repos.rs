use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use forge_storage::{ActionRepo, ActionTelemetry, ExecutionStatus, StorageError};
use forge_types::{Action, ActionId};
use time::OffsetDateTime;

pub struct InMemoryActionRepo {
    map: Mutex<HashMap<ActionId, Action>>,
    writes: AtomicUsize,
    fail_writes: bool,
    fail_get: bool,
}

impl InMemoryActionRepo {
    pub fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            writes: AtomicUsize::new(0),
            fail_writes: false,
            fail_get: false,
        }
    }

    pub fn failing() -> Self {
        Self {
            fail_writes: true,
            ..Self::new()
        }
    }

    pub fn failing_get() -> Self {
        Self {
            fail_get: true,
            ..Self::new()
        }
    }

    pub fn seed(&self, action: Action) {
        self.map.lock().unwrap().insert(action.id, action);
    }

    pub fn enabled_of(&self, id: ActionId) -> Option<bool> {
        self.map.lock().unwrap().get(&id).map(|a| a.enabled)
    }

    pub fn write_count(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ActionRepo for InMemoryActionRepo {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(self.map.lock().unwrap().values().cloned().collect())
    }
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        if self.fail_get {
            return Err(StorageError::Connection {
                reason: "forced get failure".to_owned(),
            });
        }
        Ok(self.map.lock().unwrap().get(&id).cloned())
    }
    async fn save(&self, action: &Action) -> Result<(), StorageError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if self.fail_writes {
            return Err(StorageError::Connection {
                reason: "forced write failure".to_owned(),
            });
        }
        self.map.lock().unwrap().insert(action.id, action.clone());
        Ok(())
    }
    async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        Ok(self.map.lock().unwrap().remove(&id).is_some())
    }
    async fn telemetry(&self, _id: ActionId) -> Result<ActionTelemetry, StorageError> {
        Ok(ActionTelemetry::default())
    }
    async fn record_execution(
        &self,
        _action_id: ActionId,
        _started_at: OffsetDateTime,
        _duration_ms: u64,
        _status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn prune_executions_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}
