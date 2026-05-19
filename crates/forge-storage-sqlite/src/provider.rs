use std::collections::HashMap;

use async_trait::async_trait;
use forge_storage::{
    ActionRepo, CommandRepo, CredentialId, CredentialsRepo, DataProvider, EventLogRepo,
    GlobalEntry, GlobalTransit, GlobalsRepo, HistoryRepo, QueueRepo, ScriptRecord, ScriptRepo,
    SettingsRepo, StorageError, TriggerRepo, UserGlobalEntry, UserGlobalsRepo,
};
use forge_types::{ScriptId, Variant};
use time::OffsetDateTime;

use crate::error::SqliteStorageError;
use crate::{
    SqliteActionRepo, SqliteCommandRepo, SqliteCredentialsRepo, SqliteEventLogRepo,
    SqliteGlobalsRepo, SqliteHistoryRepo, SqliteQueueRepo, SqliteScriptRepo, SqliteSettingsRepo,
    SqliteTriggerRepo, SqliteUserGlobalsRepo, apply_migrations, connect,
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
    event_log: SqliteEventLogRepo,
}

impl SqliteBackend {
    pub async fn open(url: &str) -> Result<Self, SqliteStorageError> {
        let pool = connect(url).await?;
        apply_migrations(&pool).await?;
        let credentials = SqliteCredentialsRepo::new(pool.clone())?;
        Ok(Self::from_pool_and_credentials(pool, credentials))
    }

    #[doc(hidden)]
    pub async fn open_with_key(url: &str, key: [u8; 32]) -> Result<Self, SqliteStorageError> {
        let pool = connect(url).await?;
        apply_migrations(&pool).await?;
        let credentials = SqliteCredentialsRepo::new_with_key(pool.clone(), key);
        Ok(Self::from_pool_and_credentials(pool, credentials))
    }

    fn from_pool_and_credentials(
        pool: sqlx::SqlitePool,
        credentials: SqliteCredentialsRepo,
    ) -> Self {
        Self {
            globals: SqliteGlobalsRepo::new(pool.clone()),
            user_globals: SqliteUserGlobalsRepo::new(pool.clone()),
            settings: SqliteSettingsRepo::new(pool.clone()),
            action: SqliteActionRepo::new(pool.clone()),
            trigger: SqliteTriggerRepo::new(pool.clone()),
            command: SqliteCommandRepo::new(pool.clone()),
            queue: SqliteQueueRepo::new(pool.clone()),
            script: SqliteScriptRepo::new(pool.clone()),
            history: SqliteHistoryRepo::new(pool.clone()),
            event_log: SqliteEventLogRepo::new(pool.clone()),
            credentials,
            pool,
        }
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

    async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError> {
        self.globals.incr(name, amount).await
    }

    async fn export_all(&self) -> Result<Vec<GlobalTransit>, StorageError> {
        self.globals.export_all().await
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
impl ScriptRepo for SqliteBackend {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        self.script.get(id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        self.script.get_by_name(name).await
    }

    async fn save(&self, record: ScriptRecord) -> Result<(), StorageError> {
        self.script.save(record).await
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
impl DataProvider for SqliteBackend {
    fn action_repo(&self) -> &dyn ActionRepo {
        &self.action
    }

    fn trigger_repo(&self) -> &dyn TriggerRepo {
        &self.trigger
    }

    fn command_repo(&self) -> &dyn CommandRepo {
        &self.command
    }

    fn queue_repo(&self) -> &dyn QueueRepo {
        &self.queue
    }

    fn history_repo(&self) -> &dyn HistoryRepo {
        &self.history
    }

    fn event_log_repo(&self) -> &dyn EventLogRepo {
        &self.event_log
    }

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
}
