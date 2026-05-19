use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use forge_events::Event;
use forge_storage::{
    ActionRepo, CommandRepo, CredentialId, CredentialsRepo, DataProvider, EventLogRepo,
    GlobalEntry, GlobalsRepo, HistoryRepo, QueueRepo, ScriptRecord, ScriptRepo, SettingsRepo,
    StorageError, TriggerRepo, UserGlobalEntry, UserGlobalsRepo,
};
use forge_types::{
    Action, ActionId, Command, CommandId, ExecutionContext, Queue, QueueId, ScriptId, Trigger,
    TriggerId, Variant,
};
use time::OffsetDateTime;

pub struct NullDp;

#[async_trait]
impl GlobalsRepo for NullDp {
    async fn get(&self, _name: &str) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _name: &str,
        _value: Variant,
        _persisted: bool,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _name: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn incr(&self, _name: &str, _amount: i64) -> Result<Variant, StorageError> {
        Err(StorageError::NotReady)
    }
}

#[async_trait]
impl UserGlobalsRepo for NullDp {
    async fn get(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        Ok(None)
    }

    async fn set(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
        _value: Variant,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
        _name: &str,
    ) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_for_user(
        &self,
        _broadcaster_id: &str,
        _user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }

    async fn list_for_broadcaster(
        &self,
        _broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl SettingsRepo for NullDp {
    async fn get_string(&self, _key: &str) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn set_string(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _key: &str) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(HashMap::new())
    }
}

#[async_trait]
impl ScriptRepo for NullDp {
    async fn get(&self, _id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _record: ScriptRecord) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ScriptId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl CredentialsRepo for NullDp {
    async fn store(&self, _id: &CredentialId, _plaintext_bundle: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn load(&self, _id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn delete(&self, _id: &CredentialId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(vec![])
    }

    async fn last_refresh(
        &self,
        _id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _id: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

#[async_trait]
impl ActionRepo for NullDp {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_by_group(&self, _group: Option<&str>) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl TriggerRepo for NullDp {
    async fn list_for_action(&self, _action_id: ActionId) -> Result<Vec<Trigger>, StorageError> {
        Ok(vec![])
    }

    async fn save(&self, _trigger: &Trigger) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: TriggerId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl CommandRepo for NullDp {
    async fn list(&self) -> Result<Vec<Command>, StorageError> {
        Ok(vec![])
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Command>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _command: &Command) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: CommandId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl QueueRepo for NullDp {
    async fn list(&self) -> Result<Vec<Queue>, StorageError> {
        Ok(vec![])
    }

    async fn get(&self, _id: QueueId) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn get_by_name(&self, _name: &str) -> Result<Option<Queue>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _queue: &Queue) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, _id: QueueId) -> Result<bool, StorageError> {
        Ok(false)
    }
}

#[async_trait]
impl HistoryRepo for NullDp {
    async fn save(&self, _ctx: &ExecutionContext) -> Result<(), StorageError> {
        Ok(())
    }

    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(vec![])
    }
}

#[async_trait]
impl EventLogRepo for NullDp {
    async fn insert(&self, _event: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _id: forge_types::EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn recent_since(
        &self,
        _limit: usize,
        _since: Option<forge_types::EventId>,
    ) -> Result<Vec<Event>, StorageError> {
        Ok(vec![])
    }

    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

#[async_trait]
impl DataProvider for NullDp {
    fn action_repo(&self) -> &dyn ActionRepo {
        self
    }

    fn trigger_repo(&self) -> &dyn TriggerRepo {
        self
    }

    fn command_repo(&self) -> &dyn CommandRepo {
        self
    }

    fn queue_repo(&self) -> &dyn QueueRepo {
        self
    }

    fn history_repo(&self) -> &dyn HistoryRepo {
        self
    }

    fn event_log_repo(&self) -> &dyn EventLogRepo {
        self
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(0)
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }
}

pub fn null_dp() -> Arc<dyn DataProvider> {
    Arc::new(NullDp)
}
