use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_storage::{
    ActionRepo, CredentialId, CredentialsRepo, DataProvider, EventLogRepo, GlobalEntry,
    GlobalTransit, GlobalsRepo, HistoryRepo, QueueRepo, ScriptRecord, ScriptRepo, SettingsRepo,
    SoundboardClipsRepo, StorageError, TriggerInstanceRepo, UserGlobalEntry, UserGlobalsRepo,
    ViewerRepo, VoiceAliasRepo,
};
use forge_types::{ScriptId, Variant};
use time::OffsetDateTime;
use tokio::sync::Notify;

use crate::error::SqliteStorageError;
use crate::retention_task::spawn_retention_task;
use crate::{
    SqliteActionRepo, SqliteCredentialsRepo, SqliteEventLogRepo, SqliteGlobalsRepo,
    SqliteHistoryRepo, SqliteQueueRepo, SqliteScriptRepo, SqliteSettingsRepo,
    SqliteSoundboardClipsRepo, SqliteTriggerInstanceRepo, SqliteUserGlobalsRepo, SqliteViewerRepo,
    SqliteVoiceAliasRepo, apply_migrations, connect,
};

const PRUNE_INTERVAL_PRODUCTION: Duration = Duration::from_secs(3600);

pub struct SqliteBackend {
    pool: sqlx::SqlitePool,
    globals: SqliteGlobalsRepo,
    user_globals: SqliteUserGlobalsRepo,
    settings: SqliteSettingsRepo,
    action: Arc<SqliteActionRepo>,
    trigger_instance: Arc<SqliteTriggerInstanceRepo>,
    queue: Arc<SqliteQueueRepo>,
    script: SqliteScriptRepo,
    credentials: SqliteCredentialsRepo,
    history: Arc<SqliteHistoryRepo>,
    event_log: Arc<SqliteEventLogRepo>,
    soundboard: Arc<SqliteSoundboardClipsRepo>,
    voice_alias: Arc<SqliteVoiceAliasRepo>,
    viewer: Arc<SqliteViewerRepo>,
    shutdown: Arc<Notify>,
}

impl SqliteBackend {
    pub async fn open(url: &str) -> Result<Self, SqliteStorageError> {
        let pool = connect(url).await?;
        apply_migrations(&pool).await?;
        crate::registry_migration::migrate_registry_format(&pool).await?;
        let credentials = SqliteCredentialsRepo::new(pool.clone())?;
        Ok(Self::from_pool_and_credentials(
            pool,
            credentials,
            PRUNE_INTERVAL_PRODUCTION,
        ))
    }

    #[doc(hidden)]
    pub async fn open_with_key(url: &str, key: [u8; 32]) -> Result<Self, SqliteStorageError> {
        let pool = connect(url).await?;
        apply_migrations(&pool).await?;
        crate::registry_migration::migrate_registry_format(&pool).await?;
        let credentials = SqliteCredentialsRepo::new_with_key(pool.clone(), key);
        Ok(Self::from_pool_and_credentials(
            pool,
            credentials,
            PRUNE_INTERVAL_PRODUCTION,
        ))
    }

    #[doc(hidden)]
    pub async fn open_with_key_and_interval(
        url: &str,
        key: [u8; 32],
        prune_interval: Duration,
    ) -> Result<Self, SqliteStorageError> {
        let pool = connect(url).await?;
        apply_migrations(&pool).await?;
        crate::registry_migration::migrate_registry_format(&pool).await?;
        let credentials = SqliteCredentialsRepo::new_with_key(pool.clone(), key);
        Ok(Self::from_pool_and_credentials(
            pool,
            credentials,
            prune_interval,
        ))
    }

    pub fn shutdown_retention_pruner(&self) {
        self.shutdown.notify_one();
    }

    fn from_pool_and_credentials(
        pool: sqlx::SqlitePool,
        credentials: SqliteCredentialsRepo,
        prune_interval: Duration,
    ) -> Self {
        let shutdown = Arc::new(Notify::new());

        let repo_for_task =
            Arc::new(SqliteEventLogRepo::new(pool.clone())) as Arc<dyn EventLogRepo>;
        let settings_for_task =
            Arc::new(SqliteSettingsRepo::new(pool.clone())) as Arc<dyn SettingsRepo>;

        spawn_retention_task(
            repo_for_task,
            settings_for_task,
            prune_interval,
            Arc::clone(&shutdown),
        );

        Self {
            globals: SqliteGlobalsRepo::new(pool.clone()),
            user_globals: SqliteUserGlobalsRepo::new(pool.clone()),
            settings: SqliteSettingsRepo::new(pool.clone()),
            action: Arc::new(SqliteActionRepo::new(pool.clone())),
            trigger_instance: Arc::new(SqliteTriggerInstanceRepo::new(pool.clone())),
            queue: Arc::new(SqliteQueueRepo::new(pool.clone())),
            script: SqliteScriptRepo::new(pool.clone()),
            history: Arc::new(SqliteHistoryRepo::new(pool.clone())),
            event_log: Arc::new(SqliteEventLogRepo::new(pool.clone())),
            soundboard: Arc::new(SqliteSoundboardClipsRepo::new(pool.clone())),
            voice_alias: Arc::new(SqliteVoiceAliasRepo::new(pool.clone())),
            viewer: Arc::new(SqliteViewerRepo::new(pool.clone())),
            credentials,
            shutdown,
            pool,
        }
    }

    #[doc(hidden)]
    pub async fn insert_execution_for_test(
        &self,
        action_id: forge_types::ActionId,
        started_at_secs: i64,
        duration_ms: i64,
        status: &str,
    ) -> Result<(), SqliteStorageError> {
        sqlx::query(
            "INSERT INTO action_executions (action_id, started_at, duration_ms, status)
             VALUES (?, ?, ?, ?)",
        )
        .bind(action_id.to_string())
        .bind(started_at_secs)
        .bind(duration_ms)
        .bind(status)
        .execute(&self.pool)
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    #[doc(hidden)]
    pub async fn insert_action_trigger_instance_for_test(
        &self,
        action_id: forge_types::ActionId,
        trigger_instance_id: forge_types::TriggerInstanceId,
        position: i64,
    ) -> Result<(), SqliteStorageError> {
        self.trigger_instance
            .link_action(action_id, trigger_instance_id, position)
            .await
            .map_err(|e| SqliteStorageError::Decode(e.to_string()))
    }

    pub fn soundboard_clips_repo_impl(&self) -> &SqliteSoundboardClipsRepo {
        &self.soundboard
    }

    pub fn viewer_repo_impl(&self) -> &SqliteViewerRepo {
        &self.viewer
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
    fn action_repo(&self) -> Arc<dyn ActionRepo> {
        Arc::clone(&self.action) as Arc<dyn ActionRepo>
    }

    fn trigger_instance_repo(&self) -> Arc<dyn TriggerInstanceRepo> {
        Arc::clone(&self.trigger_instance) as Arc<dyn TriggerInstanceRepo>
    }

    fn queue_repo(&self) -> Arc<dyn QueueRepo> {
        Arc::clone(&self.queue) as Arc<dyn QueueRepo>
    }

    fn history_repo(&self) -> Arc<dyn HistoryRepo> {
        Arc::clone(&self.history) as Arc<dyn HistoryRepo>
    }

    fn event_log_repo(&self) -> Arc<dyn EventLogRepo> {
        Arc::clone(&self.event_log) as Arc<dyn EventLogRepo>
    }

    fn soundboard_clips_repo(&self) -> Arc<dyn SoundboardClipsRepo> {
        Arc::clone(&self.soundboard) as Arc<dyn SoundboardClipsRepo>
    }

    fn voice_alias_repo(&self) -> Arc<dyn VoiceAliasRepo> {
        Arc::clone(&self.voice_alias) as Arc<dyn VoiceAliasRepo>
    }

    fn viewer_repo(&self) -> Arc<dyn ViewerRepo> {
        Arc::clone(&self.viewer) as Arc<dyn ViewerRepo>
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
