use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use forge_storage::action::MockActionRepo;
use forge_storage::command::MockCommandRepo;
use forge_storage::credentials::MockCredentialsRepo;
use forge_storage::event_log::MockEventLogRepo;
use forge_storage::globals::MockGlobalsRepo;
use forge_storage::history::MockHistoryRepo;
use forge_storage::queue::MockQueueRepo;
use forge_storage::script::MockScriptRepo;
use forge_storage::settings::MockSettingsRepo;
use forge_storage::soundboard::MockSoundboardClipsRepo;
use forge_storage::trigger::MockTriggerRepo;
use forge_storage::user_globals::MockUserGlobalsRepo;
use forge_storage::viewer::MockViewerRepo;
use forge_storage::voice_aliases::MockVoiceAliasRepo;
use forge_storage::{
    ActionRepo, CommandRepo, CredentialId, CredentialsRepo, DataProvider, EventLogRepo,
    GlobalEntry, GlobalsRepo, HistoryRepo, QueueRepo, ScriptRecord, ScriptRepo, SettingsRepo,
    SoundboardClipsRepo, StorageError, TriggerRepo, UserGlobalEntry, UserGlobalsRepo, ViewerRepo,
    VoiceAliasRepo,
};
use forge_types::{ScriptId, Variant};
use time::OffsetDateTime;

pub struct TestDataProvider {
    pub action_repo: MockActionRepo,
    pub trigger_repo: MockTriggerRepo,
    pub command_repo: MockCommandRepo,
    pub queue_repo: MockQueueRepo,
    pub history_repo: MockHistoryRepo,
    pub event_log_repo: MockEventLogRepo,
    pub soundboard_clips_repo: MockSoundboardClipsRepo,
    pub voice_alias_repo: MockVoiceAliasRepo,
    pub viewer_repo: MockViewerRepo,
    pub globals_repo: MockGlobalsRepo,
    pub user_globals_repo: MockUserGlobalsRepo,
    pub settings_repo: MockSettingsRepo,
    pub script_repo: MockScriptRepo,
    pub credentials_repo: MockCredentialsRepo,
}

impl TestDataProvider {
    pub fn new() -> Self {
        Self {
            action_repo: MockActionRepo::new(),
            trigger_repo: MockTriggerRepo::new(),
            command_repo: MockCommandRepo::new(),
            queue_repo: MockQueueRepo::new(),
            history_repo: MockHistoryRepo::new(),
            event_log_repo: MockEventLogRepo::new(),
            soundboard_clips_repo: MockSoundboardClipsRepo::new(),
            voice_alias_repo: MockVoiceAliasRepo::new(),
            viewer_repo: MockViewerRepo::new(),
            globals_repo: MockGlobalsRepo::new(),
            user_globals_repo: MockUserGlobalsRepo::new(),
            settings_repo: MockSettingsRepo::new(),
            script_repo: MockScriptRepo::new(),
            credentials_repo: MockCredentialsRepo::new(),
        }
    }
}

impl Default for TestDataProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GlobalsRepo for TestDataProvider {
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
impl UserGlobalsRepo for TestDataProvider {
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
impl SettingsRepo for TestDataProvider {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
        self.settings_repo.get_string(key).await
    }

    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.settings_repo.set_string(key, value).await
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        self.settings_repo.delete(key).await
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        self.settings_repo.load_all().await
    }
}

#[async_trait]
impl ScriptRepo for TestDataProvider {
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
}

#[async_trait]
impl CredentialsRepo for TestDataProvider {
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
impl DataProvider for TestDataProvider {
    fn action_repo(&self) -> &dyn ActionRepo {
        &self.action_repo
    }

    fn trigger_repo(&self) -> &dyn TriggerRepo {
        &self.trigger_repo
    }

    fn command_repo(&self) -> &dyn CommandRepo {
        &self.command_repo
    }

    fn queue_repo(&self) -> &dyn QueueRepo {
        &self.queue_repo
    }

    fn history_repo(&self) -> &dyn HistoryRepo {
        &self.history_repo
    }

    fn event_log_repo(&self) -> &dyn EventLogRepo {
        &self.event_log_repo
    }

    fn soundboard_clips_repo(&self) -> &dyn SoundboardClipsRepo {
        &self.soundboard_clips_repo
    }

    fn voice_alias_repo(&self) -> &dyn VoiceAliasRepo {
        &self.voice_alias_repo
    }

    fn viewer_repo(&self) -> &dyn ViewerRepo {
        &self.viewer_repo
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(0)
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }
}

pub fn test_dp() -> Arc<dyn DataProvider> {
    Arc::new(TestDataProvider::new())
}

pub fn test_creds() -> Arc<dyn CredentialsRepo> {
    Arc::new(TestDataProvider::new())
}
