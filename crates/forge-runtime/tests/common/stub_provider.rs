use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use forge_storage::CatalogRevision;
use forge_storage::MockActionRepo;
use forge_storage::MockBanLedgerRepo;
use forge_storage::MockChatHistoryRepo;
use forge_storage::MockDonationRepo;
use forge_storage::MockEventLogRepo;
use forge_storage::MockGlobalsRepo;
use forge_storage::MockHistoryRepo;
use forge_storage::MockLatestValueRepo;
use forge_storage::MockMediaRepo;
use forge_storage::MockQueueRepo;
use forge_storage::MockScheduledRunRepo;
use forge_storage::MockScriptRepo;
use forge_storage::MockSoundboardClipsRepo;
use forge_storage::MockTriggerInstanceRepo;
use forge_storage::MockTtsFiltersRepo;
use forge_storage::MockUserGlobalsRepo;
use forge_storage::MockViewerRepo;
use forge_storage::MockVoiceAliasRepo;
use forge_storage::credentials::MockCredentialsRepo;
use forge_storage::test_support::MemorySettings;
use forge_storage::{
    ActionRepo, BanLedgerRepo, ChatHistoryRepo, CredentialId, CredentialsRepo, DataProvider,
    DonationRepo, EventLogRepo, ExecutionStatus, GlobalEntry, GlobalsRepo, HistoryRepo,
    LatestValueRepo, MediaRepo, QueueRepo, RowClasses, RowGate, ScheduledRunRepo, ScriptRecord,
    ScriptReferences, ScriptRepo, ScriptTelemetry, SettingRow, SettingsRepo, SoundboardClipsRepo,
    StorageError, TriggerInstanceRepo, TtsFiltersRepo, UserGlobalEntry, UserGlobalsRepo,
    ViewerRepo, VoiceAliasRepo,
};
use forge_storage::{MockOverlayRepo, OverlayRepo};
use forge_types::{ScriptId, TriggerInstanceId, Variant};
use time::OffsetDateTime;

pub(crate) struct StubProvider {
    pub action_repo: Arc<MockActionRepo>,
    pub trigger_instance_repo: Arc<MockTriggerInstanceRepo>,
    pub queue_repo: Arc<MockQueueRepo>,
    pub history_repo: Arc<MockHistoryRepo>,
    pub event_log_repo: Arc<MockEventLogRepo>,
    pub soundboard_clips_repo: Arc<MockSoundboardClipsRepo>,
    pub voice_alias_repo: Arc<MockVoiceAliasRepo>,
    pub viewer_repo: Arc<MockViewerRepo>,
    pub tts_filters_repo: Arc<MockTtsFiltersRepo>,
    pub chat_history_repo: Arc<MockChatHistoryRepo>,
    pub overlay_repo: Arc<MockOverlayRepo>,
    pub media_repo: Arc<MockMediaRepo>,
    pub donation_repo: Arc<MockDonationRepo>,
    pub latest_value_repo: Arc<MockLatestValueRepo>,
    pub ban_ledger_repo: Arc<MockBanLedgerRepo>,
    pub scheduled_run_repo: Arc<MockScheduledRunRepo>,
    pub scheduled_run_revision: CatalogRevision,
    pub globals_repo: Arc<MockGlobalsRepo>,
    pub user_globals_repo: Arc<MockUserGlobalsRepo>,
    pub settings_repo: MemorySettings,
    pub script_repo: Arc<MockScriptRepo>,
    pub credentials_repo: Arc<MockCredentialsRepo>,
    pub catalog_revision: CatalogRevision,
}

impl StubProvider {
    pub(crate) fn new() -> Self {
        Self {
            catalog_revision: CatalogRevision::new(),
            action_repo: Arc::new(MockActionRepo::new()),
            trigger_instance_repo: Arc::new(MockTriggerInstanceRepo::new()),
            queue_repo: Arc::new(MockQueueRepo::new()),
            history_repo: Arc::new(MockHistoryRepo::new()),
            event_log_repo: Arc::new(MockEventLogRepo::new()),
            soundboard_clips_repo: Arc::new(MockSoundboardClipsRepo::new()),
            overlay_repo: Arc::new(MockOverlayRepo::new()),
            media_repo: Arc::new(MockMediaRepo::new()),
            donation_repo: Arc::new(MockDonationRepo::new()),
            latest_value_repo: Arc::new(MockLatestValueRepo::new()),
            ban_ledger_repo: Arc::new(MockBanLedgerRepo::new()),
            scheduled_run_repo: Arc::new(MockScheduledRunRepo::new()),
            scheduled_run_revision: CatalogRevision::new(),
            voice_alias_repo: Arc::new(MockVoiceAliasRepo::new()),
            viewer_repo: Arc::new(MockViewerRepo::new()),
            tts_filters_repo: Arc::new(MockTtsFiltersRepo::new()),
            chat_history_repo: Arc::new(MockChatHistoryRepo::new()),
            globals_repo: Arc::new(MockGlobalsRepo::new()),
            user_globals_repo: Arc::new(MockUserGlobalsRepo::new()),
            settings_repo: MemorySettings::new(),
            script_repo: Arc::new(MockScriptRepo::new()),
            credentials_repo: Arc::new(MockCredentialsRepo::new()),
        }
    }

    pub(crate) fn action(&mut self) -> &mut MockActionRepo {
        Arc::get_mut(&mut self.action_repo).expect("action_repo already shared")
    }

    pub(crate) fn trigger_instance(&mut self) -> &mut MockTriggerInstanceRepo {
        Arc::get_mut(&mut self.trigger_instance_repo).expect("trigger_instance_repo already shared")
    }

    pub(crate) fn queue(&mut self) -> &mut MockQueueRepo {
        Arc::get_mut(&mut self.queue_repo).expect("queue_repo already shared")
    }

    pub(crate) fn history(&mut self) -> &mut MockHistoryRepo {
        Arc::get_mut(&mut self.history_repo).expect("history_repo already shared")
    }

    pub(crate) fn event_log(&mut self) -> &mut MockEventLogRepo {
        Arc::get_mut(&mut self.event_log_repo).expect("event_log_repo already shared")
    }

    pub(crate) fn soundboard(&mut self) -> &mut MockSoundboardClipsRepo {
        Arc::get_mut(&mut self.soundboard_clips_repo).expect("soundboard_clips_repo already shared")
    }

    pub(crate) fn voice_alias(&mut self) -> &mut MockVoiceAliasRepo {
        Arc::get_mut(&mut self.voice_alias_repo).expect("voice_alias_repo already shared")
    }

    pub(crate) fn viewer(&mut self) -> &mut MockViewerRepo {
        Arc::get_mut(&mut self.viewer_repo).expect("viewer_repo already shared")
    }

    pub(crate) fn overlay(&mut self) -> &mut MockOverlayRepo {
        Arc::get_mut(&mut self.overlay_repo).expect("overlay_repo already shared")
    }

    pub(crate) fn globals(&mut self) -> &mut MockGlobalsRepo {
        Arc::get_mut(&mut self.globals_repo).expect("globals_repo already shared")
    }

    pub(crate) fn user_globals(&mut self) -> &mut MockUserGlobalsRepo {
        Arc::get_mut(&mut self.user_globals_repo).expect("user_globals_repo already shared")
    }

    pub(crate) fn script(&mut self) -> &mut MockScriptRepo {
        Arc::get_mut(&mut self.script_repo).expect("script_repo already shared")
    }

    pub(crate) fn credentials(&mut self) -> &mut MockCredentialsRepo {
        Arc::get_mut(&mut self.credentials_repo).expect("credentials_repo already shared")
    }
}

impl StubProvider {
    pub(crate) fn idle() -> Self {
        let mut provider = Self::new();
        provider.action().expect_list().returning(|| Ok(Vec::new()));
        provider
            .trigger_instance()
            .expect_upsert_default()
            .returning(|_, _| Ok(TriggerInstanceId::new()));
        provider.queue().expect_list().returning(|| Ok(Vec::new()));
        let mut latest = MockLatestValueRepo::new();
        latest.expect_list_slot().returning(|_| Ok(Vec::new()));
        provider.latest_value_repo = Arc::new(latest);
        let mut media = MockMediaRepo::new();
        media.expect_blob_of().returning(|_| Ok(None));
        media.expect_release_all().returning(|_, _| Ok(0));
        provider.media_repo = Arc::new(media);
        provider
    }
}

impl Default for StubProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GlobalsRepo for StubProvider {
    async fn get(&self, name: &str) -> Result<Option<Variant>, StorageError> {
        self.globals_repo.get(name).await
    }

    async fn set(&self, name: &str, value: Variant, persisted: bool) -> Result<(), StorageError> {
        self.globals_repo.set(name, value, persisted).await
    }

    async fn delete(&self, name: &str) -> Result<bool, StorageError> {
        self.globals_repo.delete(name).await
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        self.globals_repo.list().await
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        self.globals_repo.storage_bytes().await
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        self.globals_repo.last_save_at().await
    }

    async fn incr(&self, name: &str, amount: i64) -> Result<Variant, StorageError> {
        self.globals_repo.incr(name, amount).await
    }
}

#[async_trait]
impl UserGlobalsRepo for StubProvider {
    async fn get(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<Option<Variant>, StorageError> {
        self.user_globals_repo
            .get(broadcaster_id, user_id, name)
            .await
    }

    async fn set(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
        value: Variant,
    ) -> Result<(), StorageError> {
        self.user_globals_repo
            .set(broadcaster_id, user_id, name, value)
            .await
    }

    async fn delete(
        &self,
        broadcaster_id: &str,
        user_id: &str,
        name: &str,
    ) -> Result<bool, StorageError> {
        self.user_globals_repo
            .delete(broadcaster_id, user_id, name)
            .await
    }

    async fn list_for_user(
        &self,
        broadcaster_id: &str,
        user_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        self.user_globals_repo
            .list_for_user(broadcaster_id, user_id)
            .await
    }

    async fn list_for_broadcaster(
        &self,
        broadcaster_id: &str,
    ) -> Result<Vec<UserGlobalEntry>, StorageError> {
        self.user_globals_repo
            .list_for_broadcaster(broadcaster_id)
            .await
    }
}

#[async_trait]
impl SettingsRepo for StubProvider {
    async fn get_raw(&self, gate: RowGate, key: &str) -> Result<Option<String>, StorageError> {
        self.settings_repo.get_raw(gate, key).await
    }

    async fn delete_raw(&self, gate: RowGate, key: &str) -> Result<bool, StorageError> {
        self.settings_repo.delete_raw(gate, key).await
    }

    async fn load_rows(&self, gate: RowGate) -> Result<Vec<SettingRow>, StorageError> {
        self.settings_repo.load_rows(gate).await
    }

    async fn put_row(
        &self,
        gate: RowGate,
        key: &str,
        value: &str,
        classes: RowClasses,
    ) -> Result<(), StorageError> {
        self.settings_repo.put_row(gate, key, value, classes).await
    }
}

#[async_trait]
impl ScriptRepo for StubProvider {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        self.script_repo.get(id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        self.script_repo.get_by_name(name).await
    }

    async fn save(&self, record: ScriptRecord) -> Result<(), StorageError> {
        self.script_repo.save(record).await
    }

    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError> {
        self.script_repo.delete(id).await
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        self.script_repo.list().await
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        self.script_repo.list_enabled().await
    }

    async fn record_execution(
        &self,
        script_id: ScriptId,
        started_at: OffsetDateTime,
        duration_ms: u64,
        status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        self.script_repo
            .record_execution(script_id, started_at, duration_ms, status)
            .await
    }

    async fn telemetry(&self, id: ScriptId) -> Result<ScriptTelemetry, StorageError> {
        self.script_repo.telemetry(id).await
    }

    async fn prune_executions_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        self.script_repo.prune_executions_before(cutoff).await
    }

    async fn rename_cascading(
        &self,
        id: ScriptId,
        new_name: &str,
    ) -> Result<ScriptReferences, StorageError> {
        self.script_repo.rename_cascading(id, new_name).await
    }

    async fn referrers(&self, name: &str) -> Result<ScriptReferences, StorageError> {
        self.script_repo.referrers(name).await
    }

    async fn archive_script(&self, id: ScriptId) -> Result<bool, StorageError> {
        self.script_repo.archive_script(id).await
    }

    async fn restore_script(&self, id: ScriptId) -> Result<bool, StorageError> {
        self.script_repo.restore_script(id).await
    }

    async fn list_archived_scripts(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        self.script_repo.list_archived_scripts().await
    }
}

#[async_trait]
impl CredentialsRepo for StubProvider {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        self.credentials_repo.store(id, plaintext_bundle).await
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        self.credentials_repo.load(id).await
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        self.credentials_repo.delete(id).await
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        self.credentials_repo.list_ids().await
    }

    async fn last_refresh(
        &self,
        id: &CredentialId,
    ) -> Result<Option<OffsetDateTime>, StorageError> {
        self.credentials_repo.last_refresh(id).await
    }

    async fn mark_refreshed(&self, id: &CredentialId) -> Result<(), StorageError> {
        self.credentials_repo.mark_refreshed(id).await
    }
}

#[async_trait]
impl DataProvider for StubProvider {
    fn action_repo(&self) -> Arc<dyn ActionRepo> {
        Arc::clone(&self.action_repo) as Arc<dyn ActionRepo>
    }

    fn trigger_instance_repo(&self) -> Arc<dyn TriggerInstanceRepo> {
        Arc::clone(&self.trigger_instance_repo) as Arc<dyn TriggerInstanceRepo>
    }

    fn queue_repo(&self) -> Arc<dyn QueueRepo> {
        Arc::clone(&self.queue_repo) as Arc<dyn QueueRepo>
    }

    fn history_repo(&self) -> Arc<dyn HistoryRepo> {
        Arc::clone(&self.history_repo) as Arc<dyn HistoryRepo>
    }

    fn event_log_repo(&self) -> Arc<dyn EventLogRepo> {
        Arc::clone(&self.event_log_repo) as Arc<dyn EventLogRepo>
    }

    fn soundboard_clips_repo(&self) -> Arc<dyn SoundboardClipsRepo> {
        Arc::clone(&self.soundboard_clips_repo) as Arc<dyn SoundboardClipsRepo>
    }

    fn voice_alias_repo(&self) -> Arc<dyn VoiceAliasRepo> {
        Arc::clone(&self.voice_alias_repo) as Arc<dyn VoiceAliasRepo>
    }

    fn viewer_repo(&self) -> Arc<dyn ViewerRepo> {
        Arc::clone(&self.viewer_repo) as Arc<dyn ViewerRepo>
    }

    fn tts_filters_repo(&self) -> Arc<dyn TtsFiltersRepo> {
        Arc::clone(&self.tts_filters_repo) as Arc<dyn TtsFiltersRepo>
    }

    fn chat_history_repo(&self) -> Arc<dyn ChatHistoryRepo> {
        Arc::clone(&self.chat_history_repo) as Arc<dyn ChatHistoryRepo>
    }

    fn overlay_repo(&self) -> Arc<dyn OverlayRepo> {
        Arc::clone(&self.overlay_repo) as Arc<dyn OverlayRepo>
    }

    fn media_repo(&self) -> Arc<dyn MediaRepo> {
        Arc::clone(&self.media_repo) as Arc<dyn MediaRepo>
    }

    fn donation_repo(&self) -> Arc<dyn DonationRepo> {
        Arc::clone(&self.donation_repo) as Arc<dyn DonationRepo>
    }

    fn latest_value_repo(&self) -> Arc<dyn LatestValueRepo> {
        Arc::clone(&self.latest_value_repo) as Arc<dyn LatestValueRepo>
    }

    fn ban_ledger_repo(&self) -> Arc<dyn BanLedgerRepo> {
        Arc::clone(&self.ban_ledger_repo) as Arc<dyn BanLedgerRepo>
    }

    fn scheduled_run_repo(&self) -> Arc<dyn ScheduledRunRepo> {
        Arc::clone(&self.scheduled_run_repo) as Arc<dyn ScheduledRunRepo>
    }

    fn catalog_revision(&self) -> CatalogRevision {
        self.catalog_revision.clone()
    }

    fn scheduled_run_revision(&self) -> CatalogRevision {
        self.scheduled_run_revision.clone()
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(forge_storage::EXPECTED_SCHEMA_VERSION)
    }

    async fn shutdown(&self) {}
}
