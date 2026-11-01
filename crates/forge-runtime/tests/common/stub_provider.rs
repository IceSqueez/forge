#![expect(clippy::expect_used, reason = "test fixtures")]
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
    ActionRepo, BanLedgerRepo, ChatHistoryRepo, CredentialsRepo, DataProvider, DonationRepo,
    EventLogRepo, GlobalsRepo, HistoryRepo, LatestValueRepo, MediaRepo, QueueRepo,
    ScheduledRunRepo, ScriptRepo, SettingsRepo, SoundboardClipsRepo, StorageError,
    TriggerInstanceRepo, TtsFiltersRepo, UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
};
use forge_storage::{MockOverlayRepo, OverlayRepo};
use forge_types::TriggerInstanceId;

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
impl DataProvider for StubProvider {
    fn globals_repo(&self) -> Arc<dyn GlobalsRepo> {
        Arc::clone(&self.globals_repo) as Arc<dyn GlobalsRepo>
    }

    fn user_globals_repo(&self) -> Arc<dyn UserGlobalsRepo> {
        Arc::clone(&self.user_globals_repo) as Arc<dyn UserGlobalsRepo>
    }

    fn settings_repo(&self) -> Arc<dyn SettingsRepo> {
        Arc::new(self.settings_repo.clone()) as Arc<dyn SettingsRepo>
    }

    fn script_repo(&self) -> Arc<dyn ScriptRepo> {
        Arc::clone(&self.script_repo) as Arc<dyn ScriptRepo>
    }

    fn credentials_repo(&self) -> Arc<dyn CredentialsRepo> {
        Arc::clone(&self.credentials_repo) as Arc<dyn CredentialsRepo>
    }

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

    async fn commit_changes(
        &self,
        _changes: &[forge_storage::StoreChange],
    ) -> Result<(), forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }
}
