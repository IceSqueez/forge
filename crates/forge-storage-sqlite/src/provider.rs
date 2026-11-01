use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use forge_paths::AppPaths;
use forge_storage::{
    ActionRepo, ActionTelemetry, BanLedgerRepo, CatalogRevision, ChatHistoryRepo,
    CredentialsKeyLoss, CredentialsRepo, DataProvider, DonationRepo, EXPECTED_SCHEMA_VERSION,
    EventLogRepo, GlobalsRepo, HistoryRepo, LatestValueRepo, MediaRepo, OverlayRepo, QueueRepo,
    RevisingActionRepo, RevisingQueueRepo, RevisingScheduledRunRepo, RevisingScriptRepo,
    RevisingTriggerInstanceRepo, ScheduledRunRepo, ScriptRepo, SettingsRepo, SoundboardClipsRepo,
    StorageError, StoreChange, TriggerInstanceRepo, TtsFiltersRepo, UserGlobalsRepo, ViewerRepo,
    VoiceAliasRepo,
};
use forge_types::ActionId;
use time::OffsetDateTime;
use tokio::sync::{Notify, watch};
use tokio::task::AbortHandle;

use crate::error::SqliteStorageError;
use crate::pool::SqlitePools;
use crate::pool::connect_pools;
use crate::repos::{
    SqliteActionRepo, SqliteBanLedgerRepo, SqliteChatHistoryRepo, SqliteCredentialsRepo,
    SqliteDonationRepo, SqliteEventLogRepo, SqliteGlobalsRepo, SqliteHistoryRepo,
    SqliteLatestValueRepo, SqliteMediaRepo, SqliteOverlayRepo, SqliteQueueRepo,
    SqliteScheduledRunRepo, SqliteScriptRepo, SqliteSettingsRepo, SqliteSoundboardClipsRepo,
    SqliteTriggerInstanceRepo, SqliteTtsFiltersRepo, SqliteUserGlobalsRepo, SqliteViewerRepo,
    SqliteVoiceAliasRepo,
};
use crate::schema::migrations::apply as apply_migrations;
use crate::tasks::checkpoint_task::spawn_checkpoint_task;
use crate::tasks::global_read_tally::spawn_read_flush_task;
use crate::tasks::retention_task::{
    RetentionAwareSettings, RetentionSignals, RetentionTargets, spawn_retention_task,
};

const PRUNE_INTERVAL_PRODUCTION: Duration = Duration::from_secs(3600);

fn spawn_stale_incoming_sweep(media: Arc<SqliteMediaRepo>) {
    tokio::spawn(async move {
        match media.sweep_stale_incoming().await {
            Ok(0) => {}
            Ok(removed) => tracing::info!(removed, "stale media staging files removed"),
            Err(error) => tracing::warn!(%error, "stale media staging sweep failed"),
        }
        match media.sweep_unindexed_blobs().await {
            Ok(0) => {}
            Ok(removed) => tracing::info!(removed, "unindexed media files removed"),
            Err(error) => tracing::warn!(%error, "unindexed media sweep failed"),
        }
    });
}

async fn report_stranded_credentials(
    pool: &SqlitePools,
    credentials: &SqliteCredentialsRepo,
) -> Result<(), SqliteStorageError> {
    let stranded = credentials.stored_count().await?;
    if stranded == 0 {
        return Ok(());
    }
    tracing::error!(
        stranded,
        "credentials key file is missing and a new key was created; stored logins cannot be read and must be entered again"
    );
    let settings = SqliteSettingsRepo::new(pool.clone());
    if let Err(e) =
        forge_storage::record_credentials_key_loss(&settings, CredentialsKeyLoss { stranded }).await
    {
        tracing::warn!(error = %e, "could not record the credentials key loss for boot");
    }
    Ok(())
}

pub struct SqliteBackend {
    pub(crate) pool: SqlitePools,
    globals: Arc<SqliteGlobalsRepo>,
    user_globals: Arc<SqliteUserGlobalsRepo>,
    settings: Arc<RetentionAwareSettings>,
    action: Arc<dyn ActionRepo>,
    trigger_instance: Arc<dyn TriggerInstanceRepo>,
    queue: Arc<dyn QueueRepo>,
    catalog_revision: CatalogRevision,
    scheduled_run: Arc<dyn ScheduledRunRepo>,
    scheduled_run_revision: CatalogRevision,
    script: Arc<dyn ScriptRepo>,
    credentials: Arc<SqliteCredentialsRepo>,
    history: Arc<SqliteHistoryRepo>,
    event_log: Arc<SqliteEventLogRepo>,
    soundboard: Arc<SqliteSoundboardClipsRepo>,
    voice_alias: Arc<SqliteVoiceAliasRepo>,
    viewer: Arc<SqliteViewerRepo>,
    tts_filters: Arc<SqliteTtsFiltersRepo>,
    chat_history: Arc<SqliteChatHistoryRepo>,
    overlay: Arc<SqliteOverlayRepo>,
    media: Arc<SqliteMediaRepo>,
    donation: Arc<SqliteDonationRepo>,
    latest_value: Arc<SqliteLatestValueRepo>,
    ban_ledger: Arc<SqliteBanLedgerRepo>,
    shutdown: Arc<Notify>,
    retention_running: watch::Receiver<()>,
    checkpoint: AbortHandle,
}

impl Drop for SqliteBackend {
    fn drop(&mut self) {
        self.checkpoint.abort();
        self.shutdown.notify_one();
        self.globals.read_tally().close();
    }
}

impl SqliteBackend {
    pub async fn open(url: &str, paths: &AppPaths) -> Result<Self, SqliteStorageError> {
        let pool = Self::migrate_and_gate(url).await?;
        let key_file = forge_paths::credentials_key_path_in(&paths.data_dir());
        let key = crate::codec::crypto::load_or_create_key(&key_file)?;
        crate::credential_reseal_v063::reseal_unbound_credentials(&pool, &key.key).await;
        let credentials = SqliteCredentialsRepo::new_with_key(pool.clone(), key.key);
        if key.minted {
            report_stranded_credentials(&pool, &credentials).await?;
        }
        Ok(Self::from_pool_and_credentials(
            pool,
            credentials,
            PRUNE_INTERVAL_PRODUCTION,
            paths.media_dir(),
        ))
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn open_for_test(
        url: &str,
        key: [u8; 32],
        media_root: std::path::PathBuf,
        prune_interval: Option<Duration>,
    ) -> Result<Self, SqliteStorageError> {
        let pool = Self::migrate_and_gate(url).await?;
        let credentials = SqliteCredentialsRepo::new_with_key(pool.clone(), key);
        Ok(Self::from_pool_and_credentials(
            pool,
            credentials,
            prune_interval.unwrap_or(PRUNE_INTERVAL_PRODUCTION),
            media_root,
        ))
    }

    async fn migrate_and_gate(url: &str) -> Result<SqlitePools, SqliteStorageError> {
        let pool = connect_pools(url).await?;
        if let Err(e) = crate::schema::migrations::prepare_journal(pool.writer()).await {
            pool.close().await;
            return Err(e);
        }
        apply_migrations(pool.writer()).await?;
        crate::schema::registry_migration::migrate_registry_format(pool.writer()).await?;

        let found = crate::schema::migrations::applied_version(pool.reader()).await?;
        if found != EXPECTED_SCHEMA_VERSION {
            pool.close().await;
            return Err(SqliteStorageError::SchemaMismatch {
                expected: EXPECTED_SCHEMA_VERSION,
                found,
            });
        }

        Ok(pool)
    }

    fn from_pool_and_credentials(
        pool: SqlitePools,
        credentials: SqliteCredentialsRepo,
        prune_interval: Duration,
        media_root: std::path::PathBuf,
    ) -> Self {
        let shutdown = Arc::new(Notify::new());
        let retention_window_changed = Arc::new(Notify::new());
        let catalog_revision = CatalogRevision::new();
        let scheduled_run_revision = CatalogRevision::new();
        let scheduled_run = RevisingScheduledRunRepo::wrap(
            Arc::new(SqliteScheduledRunRepo::new(pool.clone())),
            scheduled_run_revision.clone(),
        );

        let checkpoint = spawn_checkpoint_task(
            pool.checkpointer().clone(),
            pool.writer().clone(),
            pool.busy_timeout(),
        );
        let settings = Arc::new(RetentionAwareSettings::new(
            SqliteSettingsRepo::new(pool.clone()),
            Arc::clone(&retention_window_changed),
        ));
        let history = Arc::new(SqliteHistoryRepo::new(pool.clone()));
        let event_log = Arc::new(SqliteEventLogRepo::new(pool.clone()));
        let (retention_alive, retention_running) = watch::channel(());
        spawn_retention_task(
            RetentionTargets {
                event_log: Arc::clone(&event_log),
                history: Arc::clone(&history),
                action: Arc::new(SqliteActionRepo::new(pool.clone())),
                script: Arc::new(SqliteScriptRepo::new(pool.clone())),
                settings: Arc::clone(&settings) as Arc<dyn SettingsRepo>,
                scheduled_run: Arc::clone(&scheduled_run),
            },
            prune_interval,
            RetentionSignals {
                window_changed: Arc::clone(&retention_window_changed),
                shutdown: Arc::clone(&shutdown),
            },
            retention_alive,
        );

        let media = Arc::new(SqliteMediaRepo::new(pool.clone(), media_root));
        spawn_stale_incoming_sweep(Arc::clone(&media));

        let globals = Arc::new(SqliteGlobalsRepo::new(pool.clone()));
        spawn_read_flush_task(Arc::clone(globals.read_tally()), pool.writer().clone());

        Self {
            checkpoint,
            globals,
            user_globals: Arc::new(SqliteUserGlobalsRepo::new(pool.clone())),
            settings,
            action: RevisingActionRepo::wrap_cascading_scheduled_runs(
                Arc::new(SqliteActionRepo::new(pool.clone())),
                catalog_revision.clone(),
                scheduled_run_revision.clone(),
            ),
            trigger_instance: RevisingTriggerInstanceRepo::wrap(
                Arc::new(SqliteTriggerInstanceRepo::new(pool.clone())),
                catalog_revision.clone(),
            ),
            queue: RevisingQueueRepo::wrap(
                Arc::new(SqliteQueueRepo::new(pool.clone())),
                catalog_revision.clone(),
            ),
            catalog_revision: catalog_revision.clone(),
            scheduled_run,
            scheduled_run_revision,
            script: RevisingScriptRepo::wrap(
                Arc::new(SqliteScriptRepo::new(pool.clone())),
                catalog_revision.clone(),
            ),
            history,
            event_log,
            soundboard: Arc::new(SqliteSoundboardClipsRepo::new(pool.clone())),
            voice_alias: Arc::new(SqliteVoiceAliasRepo::new(pool.clone())),
            viewer: Arc::new(SqliteViewerRepo::new(pool.clone())),
            tts_filters: Arc::new(SqliteTtsFiltersRepo::new(pool.clone())),
            chat_history: Arc::new(SqliteChatHistoryRepo::new(pool.clone())),
            overlay: Arc::new(SqliteOverlayRepo::new(pool.clone())),
            media,
            donation: Arc::new(SqliteDonationRepo::new(pool.clone())),
            latest_value: Arc::new(SqliteLatestValueRepo::new(pool.clone())),
            ban_ledger: Arc::new(SqliteBanLedgerRepo::new(pool.clone())),
            credentials: Arc::new(credentials),
            shutdown,
            retention_running,
            pool,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn action_telemetry_at_for_test(
        &self,
        id: ActionId,
        now: OffsetDateTime,
        start_of_today: i64,
    ) -> Result<ActionTelemetry, StorageError> {
        SqliteActionRepo::new(self.pool.clone())
            .telemetry_at(id, now, start_of_today)
            .await
    }

    pub fn retention_pruner_stopped(&self) -> impl Future<Output = ()> + Send + 'static {
        let mut running = self.retention_running.clone();
        async move { while running.changed().await.is_ok() {} }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn insert_trigger_instance_without_scope_for_test(
        &self,
        id: &str,
        kind_id: &str,
        name: &str,
    ) -> Result<(), SqliteStorageError> {
        sqlx::query(
            "INSERT INTO trigger_instances (id, kind_id, name, overrides, enabled, user_defined)
             VALUES (?, ?, ?, '{}', 1, 0)",
        )
        .bind(id)
        .bind(kind_id)
        .bind(name)
        .execute(self.pool.writer())
        .await
        .map_err(SqliteStorageError::Sqlx)?;
        Ok(())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn soundboard_clips_repo_impl(&self) -> &SqliteSoundboardClipsRepo {
        &self.soundboard
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn viewer_repo_impl(&self) -> &SqliteViewerRepo {
        &self.viewer
    }
}

#[async_trait]
impl DataProvider for SqliteBackend {
    fn globals_repo(&self) -> Arc<dyn GlobalsRepo> {
        Arc::clone(&self.globals) as Arc<dyn GlobalsRepo>
    }

    fn user_globals_repo(&self) -> Arc<dyn UserGlobalsRepo> {
        Arc::clone(&self.user_globals) as Arc<dyn UserGlobalsRepo>
    }

    fn settings_repo(&self) -> Arc<dyn SettingsRepo> {
        Arc::clone(&self.settings) as Arc<dyn SettingsRepo>
    }

    fn script_repo(&self) -> Arc<dyn ScriptRepo> {
        Arc::clone(&self.script)
    }

    fn credentials_repo(&self) -> Arc<dyn CredentialsRepo> {
        Arc::clone(&self.credentials) as Arc<dyn CredentialsRepo>
    }

    fn action_repo(&self) -> Arc<dyn ActionRepo> {
        Arc::clone(&self.action)
    }

    async fn commit_changes(&self, changes: &[StoreChange]) -> Result<(), StorageError> {
        let write = crate::unit_of_work::commit(self.pool.clone(), changes.to_vec());
        if changes.iter().any(StoreChange::touches_catalog) {
            self.catalog_revision.after(write).await?;
        } else {
            write.await?;
        }
        if changes.iter().any(StoreChange::removes_action) {
            self.scheduled_run_revision.advance();
        }
        for key in changes.iter().filter_map(StoreChange::changed_setting) {
            self.settings.signal_if_retention(key);
        }
        Ok(())
    }

    fn trigger_instance_repo(&self) -> Arc<dyn TriggerInstanceRepo> {
        Arc::clone(&self.trigger_instance)
    }

    fn queue_repo(&self) -> Arc<dyn QueueRepo> {
        Arc::clone(&self.queue)
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

    fn tts_filters_repo(&self) -> Arc<dyn TtsFiltersRepo> {
        Arc::clone(&self.tts_filters) as Arc<dyn TtsFiltersRepo>
    }

    fn chat_history_repo(&self) -> Arc<dyn ChatHistoryRepo> {
        Arc::clone(&self.chat_history) as Arc<dyn ChatHistoryRepo>
    }

    fn overlay_repo(&self) -> Arc<dyn OverlayRepo> {
        Arc::clone(&self.overlay) as Arc<dyn OverlayRepo>
    }

    fn media_repo(&self) -> Arc<dyn MediaRepo> {
        Arc::clone(&self.media) as Arc<dyn MediaRepo>
    }

    fn donation_repo(&self) -> Arc<dyn DonationRepo> {
        Arc::clone(&self.donation) as Arc<dyn DonationRepo>
    }

    fn latest_value_repo(&self) -> Arc<dyn LatestValueRepo> {
        Arc::clone(&self.latest_value) as Arc<dyn LatestValueRepo>
    }

    fn ban_ledger_repo(&self) -> Arc<dyn BanLedgerRepo> {
        Arc::clone(&self.ban_ledger) as Arc<dyn BanLedgerRepo>
    }

    fn scheduled_run_repo(&self) -> Arc<dyn ScheduledRunRepo> {
        Arc::clone(&self.scheduled_run)
    }

    fn catalog_revision(&self) -> CatalogRevision {
        self.catalog_revision.clone()
    }

    fn scheduled_run_revision(&self) -> CatalogRevision {
        self.scheduled_run_revision.clone()
    }

    async fn export(&self, path: &std::path::Path) -> Result<(), StorageError> {
        let path_str = path.to_str().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "non-utf8 export path")
        })?;

        crate::snapshot::create_owner_only_target(path).await?;
        crate::snapshot::vacuum_into(self.pool.snapshot_source(), path_str)
            .await
            .map_err(|e| StorageError::Connection {
                reason: e.to_string(),
            })
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        crate::schema::migrations::applied_version(self.pool.reader())
            .await
            .map_err(StorageError::from)
    }

    async fn shutdown(&self) {
        self.shutdown.notify_one();
        let read_tally = self.globals.read_tally();
        read_tally.close();
        read_tally.flush(self.pool.writer()).await;
        self.pool.close().await;
    }
}

#[cfg(test)]
mod tests {
    use forge_storage::settings::core_settings;
    use forge_storage::{DataProvider, StoreChange};

    use super::SqliteBackend;

    #[tokio::test]
    async fn export_completes_while_the_writer_holds_an_open_transaction_and_omits_its_rows() {
        let dir = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}",
            dir.path().join(forge_storage::DATABASE_FILE_NAME).display()
        );
        let backend =
            SqliteBackend::open_for_test(&url, [0xab; 32], dir.path().join("media"), None)
                .await
                .unwrap();
        sqlx::query("INSERT INTO settings (key, value) VALUES ('committed', 'v')")
            .execute(backend.pool.writer())
            .await
            .unwrap();
        let mut open_write = backend.pool.writer().begin().await.unwrap();
        sqlx::query("INSERT INTO settings (key, value) VALUES ('uncommitted', 'v')")
            .execute(&mut *open_write)
            .await
            .unwrap();
        let export_path = dir.path().join("export.db");

        backend
            .export(&export_path)
            .await
            .expect("export must not wait for the writer connection");
        open_write.rollback().await.unwrap();
        backend.shutdown().await;

        let exported = crate::pool::connect(&format!("sqlite://{}", export_path.display()))
            .await
            .unwrap();
        let keys: Vec<String> = sqlx::query_scalar(
            "SELECT key FROM settings WHERE key IN ('committed', 'uncommitted')",
        )
        .fetch_all(&exported)
        .await
        .unwrap();
        exported.close().await;
        assert_eq!(keys, vec!["committed".to_string()]);
    }

    async fn backend_with_settings(
        dir: &tempfile::TempDir,
        settings: &[(&str, &str)],
    ) -> SqliteBackend {
        let url = format!(
            "sqlite://{}",
            dir.path().join(forge_storage::DATABASE_FILE_NAME).display()
        );
        let backend =
            SqliteBackend::open_for_test(&url, [0xab; 32], dir.path().join("media"), None)
                .await
                .unwrap();
        for (key, value) in settings {
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
                .bind(key)
                .bind(value)
                .execute(backend.pool.writer())
                .await
                .unwrap();
        }
        backend
    }

    async fn setting_keys(database: &std::path::Path) -> Vec<String> {
        let pool = crate::pool::connect(&format!("sqlite://{}", database.display()))
            .await
            .unwrap();
        let keys = sqlx::query_scalar("SELECT key FROM settings WHERE key = 'kept' OR key LIKE 'backup_schedule.%' ORDER BY key")
            .fetch_all(&pool)
            .await
            .unwrap();
        pool.close().await;
        keys
    }

    const PROGRESS: [(&str, &str); 2] = [
        (
            core_settings::BACKUP_SCHEDULE_LAST_SUCCESS_AT.name(),
            "2026-10-21T12:00:00Z",
        ),
        (
            core_settings::BACKUP_SCHEDULE_CONTENT_FINGERPRINT.name(),
            "abc123",
        ),
    ];

    #[tokio::test]
    async fn export_drops_schedule_progress_but_keeps_every_other_setting() {
        let dir = tempfile::tempdir().unwrap();
        let backend = backend_with_settings(&dir, &[PROGRESS[0], PROGRESS[1], ("kept", "v")]).await;
        let export_path = dir.path().join("export.db");

        backend.export(&export_path).await.unwrap();
        backend.shutdown().await;

        assert_eq!(setting_keys(&export_path).await, ["kept".to_owned()]);
    }

    #[tokio::test]
    async fn export_leaves_schedule_progress_in_the_live_database() {
        let dir = tempfile::tempdir().unwrap();
        let backend = backend_with_settings(&dir, &PROGRESS).await;

        backend.export(&dir.path().join("export.db")).await.unwrap();
        backend.shutdown().await;

        assert_eq!(
            setting_keys(&dir.path().join(forge_storage::DATABASE_FILE_NAME))
                .await
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn export_is_byte_identical_whether_or_not_schedule_progress_is_stored() {
        let dir = tempfile::tempdir().unwrap();
        let backend = backend_with_settings(&dir, &[("kept", "v")]).await;
        let without = dir.path().join("without.db");
        backend.export(&without).await.unwrap();
        for (key, value) in PROGRESS {
            sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?)")
                .bind(key)
                .bind(value)
                .execute(backend.pool.writer())
                .await
                .unwrap();
        }
        let with = dir.path().join("with.db");

        backend.export(&with).await.unwrap();
        backend.shutdown().await;

        assert!(std::fs::read(&without).unwrap() == std::fs::read(&with).unwrap());
    }

    async fn retention_signal_is_waiting(backend: &SqliteBackend) -> bool {
        tokio::select! {
            biased;
            () = backend.settings.window_changed().notified() => true,
            () = std::future::ready(()) => false,
        }
    }

    #[tokio::test]
    async fn a_committed_change_signals_the_pruner_only_for_a_retention_setting() {
        let event_log =
            || forge_storage::setting_row(&core_settings::EVENT_LOG_RETENTION_DAYS, &3).unwrap();
        let action_history = || {
            forge_storage::setting_row(&core_settings::ACTION_HISTORY_RETENTION_DAYS, &3).unwrap()
        };
        let unrelated =
            || forge_storage::setting_row(&core_settings::TTS_MASTER_VOLUME, &0.5).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let backend = backend_with_settings(&dir, &[]).await;

        for (label, change, signalled) in [
            ("event log put", StoreChange::PutSetting(event_log()), true),
            (
                "event log delete",
                StoreChange::DeleteSetting(event_log()),
                true,
            ),
            (
                "action history put",
                StoreChange::PutSetting(action_history()),
                true,
            ),
            ("unrelated put", StoreChange::PutSetting(unrelated()), false),
        ] {
            backend.commit_changes(&[change]).await.unwrap();

            assert_eq!(
                retention_signal_is_waiting(&backend).await,
                signalled,
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn a_failed_batch_does_not_signal_the_pruner() {
        let dir = tempfile::tempdir().unwrap();
        let backend = backend_with_settings(&dir, &[]).await;

        let failed = backend
            .commit_changes(&[
                StoreChange::PutSetting(
                    forge_storage::setting_row(&core_settings::EVENT_LOG_RETENTION_DAYS, &3)
                        .unwrap(),
                ),
                StoreChange::DeleteQueue(forge_types::DEFAULT_QUEUE_ID),
            ])
            .await;

        assert!(failed.is_err());
        assert!(!retention_signal_is_waiting(&backend).await);
    }
}
