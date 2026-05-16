use std::collections::HashMap;

use async_trait::async_trait;
use loom_events::EventSource;
use loom_storage::{
    ActionRecord, ActionRepo, CommandRecord, CommandRepo, CredentialId, CredentialsRepo,
    DataProvider, GlobalEntry, GlobalsRepo, HistoryRecord, HistoryRepo, NewHistoryRecord,
    QueueRecord, QueueRepo, ScriptRecord, ScriptRepo, SettingsRepo, StorageError, TriggerRecord,
    TriggerRepo, UserGlobalEntry, UserGlobalsRepo,
};
use loom_types::{ActionId, CommandId, EventId, QueueId, ScriptId, TriggerId, Variant};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;
use crate::{
    SqliteActionRepo, SqliteCommandRepo, SqliteCredentialsRepo, SqliteGlobalsRepo,
    SqliteHistoryRepo, SqliteQueueRepo, SqliteScriptRepo, SqliteSettingsRepo, SqliteTriggerRepo,
    SqliteUserGlobalsRepo, apply_migrations, connect,
};

pub struct SqliteBackend {
    pool: sqlx::SqlitePool,
    globals: SqliteGlobalsRepo,
    user_globals: SqliteUserGlobalsRepo,
    settings: SqliteSettingsRepo,
    action: SqliteActionRepo,
    trigger: SqliteTriggerRepo,
    command: SqliteCommandRepo,
    queue: SqliteQueueRepo,
    script: SqliteScriptRepo,
    credentials: SqliteCredentialsRepo,
    history: SqliteHistoryRepo,
}

impl SqliteBackend {
    pub async fn open(url: &str) -> Result<Self, SqliteStorageError> {
        let pool = connect(url).await?;
        apply_migrations(&pool).await?;

        let credentials = SqliteCredentialsRepo::new(pool.clone())?;

        Ok(Self {
            globals: SqliteGlobalsRepo::new(pool.clone()),
            user_globals: SqliteUserGlobalsRepo::new(pool.clone()),
            settings: SqliteSettingsRepo::new(pool.clone()),
            action: SqliteActionRepo::new(pool.clone()),
            trigger: SqliteTriggerRepo::new(pool.clone()),
            command: SqliteCommandRepo::new(pool.clone()),
            queue: SqliteQueueRepo::new(pool.clone()),
            script: SqliteScriptRepo::new(pool.clone()),
            history: SqliteHistoryRepo::new(pool.clone()),
            credentials,
            pool,
        })
    }
}

#[async_trait]
impl GlobalsRepo for SqliteBackend {
    async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
        self.globals.get(name).await
    }

    async fn set(&self, name: &str, value: Variant, persisted: bool) -> Result<(), StorageError> {
        self.globals.set(name, value, persisted).await
    }

    async fn delete(&self, name: &str) -> Result<bool, StorageError> {
        self.globals.delete(name).await
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        self.globals.list().await
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        self.globals.storage_bytes().await
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        self.globals.last_save_at().await
    }
}

#[async_trait]
impl UserGlobalsRepo for SqliteBackend {
    async fn get(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        self.user_globals.get(broadcaster_id, user_id, name).await
    }

    async fn set(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
        value: Variant,
    ) -> Result<(), StorageError> {
        self.user_globals
            .set(broadcaster_id, user_id, name, value)
            .await
    }

    async fn delete(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<bool, StorageError> {
        self.user_globals
            .delete(broadcaster_id, user_id, name)
            .await
    }

    async fn list_for_user(
        &self,
        broadcaster_id: &str,
        user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        self.user_globals
            .list_for_user(broadcaster_id, user_id)
            .await
    }

    async fn list_for_broadcaster(
        &self,
        broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        self.user_globals.list_for_broadcaster(broadcaster_id).await
    }
}

#[async_trait]
impl SettingsRepo for SqliteBackend {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.settings.get_string(key).await
    }

    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.settings.set_string(key, value).await
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        self.settings.delete(key).await
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        self.settings.load_all().await
    }
}

#[async_trait]
impl ActionRepo for SqliteBackend {
    async fn get(&self, id: ActionId) -> Result<Option<ActionRecord>, StorageError> {
        self.action.get(id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ActionRecord>, StorageError> {
        self.action.get_by_name(name).await
    }

    async fn upsert(&self, record: ActionRecord) -> Result<(), StorageError> {
        self.action.upsert(record).await
    }

    async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        self.action.delete(id).await
    }

    async fn list(&self) -> Result<Vec<ActionRecord>, StorageError> {
        self.action.list().await
    }
}

#[async_trait]
impl TriggerRepo for SqliteBackend {
    async fn get(&self, id: TriggerId) -> Result<Option<TriggerRecord>, StorageError> {
        self.trigger.get(id).await
    }

    async fn upsert(&self, record: TriggerRecord) -> Result<(), StorageError> {
        self.trigger.upsert(record).await
    }

    async fn delete(&self, id: TriggerId) -> Result<bool, StorageError> {
        self.trigger.delete(id).await
    }

    async fn list(&self) -> Result<Vec<TriggerRecord>, StorageError> {
        self.trigger.list().await
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<TriggerRecord>, StorageError> {
        self.trigger.list_for_action(action_id).await
    }

    async fn list_enabled_by_source(
        &self,
        source: EventSource,
    ) -> Result<Vec<TriggerRecord>, StorageError> {
        self.trigger.list_enabled_by_source(source).await
    }
}

#[async_trait]
impl CommandRepo for SqliteBackend {
    async fn get(&self, id: CommandId) -> Result<Option<CommandRecord>, StorageError> {
        self.command.get(id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<CommandRecord>, StorageError> {
        self.command.get_by_name(name).await
    }

    async fn upsert(&self, record: CommandRecord) -> Result<(), StorageError> {
        self.command.upsert(record).await
    }

    async fn delete(&self, id: CommandId) -> Result<bool, StorageError> {
        self.command.delete(id).await
    }

    async fn list(&self) -> Result<Vec<CommandRecord>, StorageError> {
        self.command.list().await
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
    ) -> Result<Vec<CommandRecord>, StorageError> {
        self.command.list_for_action(action_id).await
    }
}

#[async_trait]
impl QueueRepo for SqliteBackend {
    async fn get(&self, id: QueueId) -> Result<Option<QueueRecord>, StorageError> {
        self.queue.get(id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<QueueRecord>, StorageError> {
        self.queue.get_by_name(name).await
    }

    async fn upsert(&self, record: QueueRecord) -> Result<(), StorageError> {
        self.queue.upsert(record).await
    }

    async fn delete(&self, id: QueueId) -> Result<bool, StorageError> {
        self.queue.delete(id).await
    }

    async fn list(&self) -> Result<Vec<QueueRecord>, StorageError> {
        self.queue.list().await
    }

    async fn set_paused(&self, id: QueueId, paused: bool) -> Result<(), StorageError> {
        self.queue.set_paused(id, paused).await
    }
}

#[async_trait]
impl ScriptRepo for SqliteBackend {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        self.script.get(id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        self.script.get_by_name(name).await
    }

    async fn upsert(&self, record: ScriptRecord) -> Result<(), StorageError> {
        self.script.upsert(record).await
    }

    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError> {
        self.script.delete(id).await
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        self.script.list().await
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        self.script.list_enabled().await
    }
}

#[async_trait]
impl CredentialsRepo for SqliteBackend {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        self.credentials.store(id, plaintext_bundle).await
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        self.credentials.load(id).await
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        self.credentials.delete(id).await
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        self.credentials.list_ids().await
    }

    async fn last_refresh(
        &self,
        id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        self.credentials.last_refresh(id).await
    }

    async fn mark_refreshed(&self, id: &CredentialId) -> Result<(), StorageError> {
        self.credentials.mark_refreshed(id).await
    }
}

#[async_trait]
impl HistoryRepo for SqliteBackend {
    async fn record(&self, new: NewHistoryRecord) -> Result<i64, StorageError> {
        self.history.record(new).await
    }

    async fn get(&self, id: i64) -> Result<Option<HistoryRecord>, StorageError> {
        self.history.get(id).await
    }

    async fn list_for_action(
        &self,
        action_id: ActionId,
        limit: u32,
    ) -> Result<Vec<HistoryRecord>, StorageError> {
        self.history.list_for_action(action_id, limit).await
    }

    async fn list_recent(&self, limit: u32) -> Result<Vec<HistoryRecord>, StorageError> {
        self.history.list_recent(limit).await
    }

    async fn list_caused_by(&self, event_id: EventId) -> Result<Vec<HistoryRecord>, StorageError> {
        self.history.list_caused_by(event_id).await
    }

    async fn prune_older_than(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        self.history.prune_older_than(cutoff).await
    }
}

#[async_trait]
impl DataProvider for SqliteBackend {
    async fn schema_version(&self) -> Result<u32, StorageError> {
        let version: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StorageError::Connection {
                reason: e.to_string(),
            })?;

        Ok(version.unwrap_or(0) as u32)
    }

    async fn export(&self, path: &std::path::Path) -> Result<(), StorageError> {
        let path_str = path.to_str().ok_or_else(|| StorageError::Connection {
            reason: "non-utf8 export path".into(),
        })?;

        sqlx::query("VACUUM INTO ?")
            .bind(path_str)
            .execute(&self.pool)
            .await
            .map_err(|e| StorageError::Connection {
                reason: e.to_string(),
            })?;

        Ok(())
    }

    /// Replaces the current database with the file at `path`. Destructive.
    async fn import(&self, _path: &std::path::Path) -> Result<(), StorageError> {
        Err(StorageError::Connection {
            reason: "import not yet implemented".into(),
        })
    }
}
