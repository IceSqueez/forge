#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use forge_storage::action::MockActionRepo;
use forge_storage::chat_history::MockChatHistoryRepo;
use forge_storage::credentials::MockCredentialsRepo;
use forge_storage::event_log::MockEventLogRepo;
use forge_storage::globals::MockGlobalsRepo;
use forge_storage::history::MockHistoryRepo;
use forge_storage::overlay::{MockOverlayRepo, OverlayRepo};
use forge_storage::queue::MockQueueRepo;
use forge_storage::script::MockScriptRepo;
use forge_storage::settings::MockSettingsRepo;
use forge_storage::soundboard::MockSoundboardClipsRepo;
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::tts_filters::MockTtsFiltersRepo;
use forge_storage::user_globals::MockUserGlobalsRepo;
use forge_storage::viewer::MockViewerRepo;
use forge_storage::voice_aliases::MockVoiceAliasRepo;
use forge_storage::{
    ActionRepo, ChatHistoryRepo, CredentialId, CredentialsRepo, DataProvider, EventLogRepo,
    ExecutionStatus, GlobalEntry, GlobalsRepo, HistoryRepo, QueueRepo, ScriptRecord, ScriptRepo,
    ScriptTelemetry, SettingsRepo, SoundboardClipsRepo, StorageError, TriggerInstanceRepo,
    TtsFiltersRepo, UserGlobalEntry, UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
};
use forge_types::{ScriptId, Variant};
use time::OffsetDateTime;

pub struct TestDataProvider {
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
    pub globals_repo: Arc<MockGlobalsRepo>,
    pub user_globals_repo: Arc<MockUserGlobalsRepo>,
    pub settings_repo: Arc<MockSettingsRepo>,
    pub script_repo: Arc<MockScriptRepo>,
    pub credentials_repo: Arc<MockCredentialsRepo>,
}

impl TestDataProvider {
    pub fn new() -> Self {
        let mut overlay_repo = MockOverlayRepo::new();
        overlay_repo.expect_get().returning(|_| Ok(None));

        Self {
            action_repo: Arc::new(MockActionRepo::new()),
            trigger_instance_repo: Arc::new(MockTriggerInstanceRepo::new()),
            queue_repo: Arc::new(MockQueueRepo::new()),
            history_repo: Arc::new(MockHistoryRepo::new()),
            event_log_repo: Arc::new(MockEventLogRepo::new()),
            soundboard_clips_repo: Arc::new(MockSoundboardClipsRepo::new()),
            overlay_repo: Arc::new(overlay_repo),
            voice_alias_repo: Arc::new(MockVoiceAliasRepo::new()),
            viewer_repo: Arc::new(MockViewerRepo::new()),
            tts_filters_repo: Arc::new(MockTtsFiltersRepo::new()),
            chat_history_repo: Arc::new(MockChatHistoryRepo::new()),
            globals_repo: Arc::new(MockGlobalsRepo::new()),
            user_globals_repo: Arc::new(MockUserGlobalsRepo::new()),
            settings_repo: Arc::new(MockSettingsRepo::new()),
            script_repo: Arc::new(MockScriptRepo::new()),
            credentials_repo: Arc::new(MockCredentialsRepo::new()),
        }
    }

    pub fn action(&mut self) -> &mut MockActionRepo {
        Arc::get_mut(&mut self.action_repo).expect("action_repo already shared")
    }

    pub fn queue(&mut self) -> &mut MockQueueRepo {
        Arc::get_mut(&mut self.queue_repo).expect("queue_repo already shared")
    }

    pub fn history(&mut self) -> &mut MockHistoryRepo {
        Arc::get_mut(&mut self.history_repo).expect("history_repo already shared")
    }

    pub fn event_log(&mut self) -> &mut MockEventLogRepo {
        Arc::get_mut(&mut self.event_log_repo).expect("event_log_repo already shared")
    }

    pub fn soundboard(&mut self) -> &mut MockSoundboardClipsRepo {
        Arc::get_mut(&mut self.soundboard_clips_repo).expect("soundboard_clips_repo already shared")
    }

    pub fn voice_alias(&mut self) -> &mut MockVoiceAliasRepo {
        Arc::get_mut(&mut self.voice_alias_repo).expect("voice_alias_repo already shared")
    }

    pub fn viewer(&mut self) -> &mut MockViewerRepo {
        Arc::get_mut(&mut self.viewer_repo).expect("viewer_repo already shared")
    }

    pub fn overlay(&mut self) -> &mut MockOverlayRepo {
        Arc::get_mut(&mut self.overlay_repo).expect("overlay_repo already shared")
    }

    pub fn globals(&mut self) -> &mut MockGlobalsRepo {
        Arc::get_mut(&mut self.globals_repo).expect("globals_repo already shared")
    }

    pub fn user_globals(&mut self) -> &mut MockUserGlobalsRepo {
        Arc::get_mut(&mut self.user_globals_repo).expect("user_globals_repo already shared")
    }

    pub fn settings(&mut self) -> &mut MockSettingsRepo {
        Arc::get_mut(&mut self.settings_repo).expect("settings_repo already shared")
    }

    pub fn script(&mut self) -> &mut MockScriptRepo {
        Arc::get_mut(&mut self.script_repo).expect("script_repo already shared")
    }

    pub fn credentials(&mut self) -> &mut MockCredentialsRepo {
        Arc::get_mut(&mut self.credentials_repo).expect("credentials_repo already shared")
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

    async fn schema_version(&self) -> Result<u32, StorageError> {
        Ok(0)
    }

    async fn export(&self, _path: &Path) -> Result<(), StorageError> {
        Ok(())
    }

    async fn shutdown(&self) {}
}

pub fn test_dp() -> Arc<dyn DataProvider> {
    Arc::new(TestDataProvider::new())
}

pub fn test_creds() -> Arc<dyn CredentialsRepo> {
    Arc::new(TestDataProvider::new())
}

pub struct MemSettings(std::sync::Mutex<HashMap<String, String>>);

impl MemSettings {
    pub fn new() -> Arc<Self> {
        Arc::new(Self(std::sync::Mutex::new(HashMap::new())))
    }
}

#[async_trait]
impl SettingsRepo for MemSettings {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self.0.lock().expect("mutex").get(key).cloned())
    }

    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.0
            .lock()
            .expect("mutex")
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.0.lock().expect("mutex").remove(key).is_some())
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(self.0.lock().expect("mutex").clone())
    }
}

/// Thread-local tracing capture; the runtime shares the capturing thread so spawned
/// connection tasks reach the subscriber, which is why callers are plain `#[test]` fns.
pub mod log_capture {
    use std::collections::BTreeMap;
    use std::future::Future;
    use std::sync::{Arc, Mutex, OnceLock};

    use tracing::Level;
    use tracing::field::{Field, Visit};
    use tracing::span;
    use tracing::subscriber::Interest;

    pub struct CapturedLine {
        target: String,
        level: Level,
        fields: BTreeMap<String, String>,
    }

    impl CapturedLine {
        pub fn field(&self, name: &str) -> Option<&str> {
            self.fields.get(name).map(String::as_str)
        }

        pub fn mentions(&self, needle: &str) -> bool {
            self.fields.values().any(|value| value.contains(needle))
        }

        pub fn from_forge(&self) -> bool {
            self.target.starts_with(FORGE_TARGET_PREFIX)
        }
    }

    const FORGE_TARGET_PREFIX: &str = "forge_";

    #[derive(Default)]
    struct FieldCollector(BTreeMap<String, String>);

    impl Visit for FieldCollector {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().to_owned(), format!("{value:?}"));
        }

        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.insert(field.name().to_owned(), value.to_owned());
        }
    }

    // Why: the callsite interest cache is process-global while a capture subscriber is
    // thread-local. Other tests in this binary reach the same callsites with no subscriber
    // installed, which caches `Interest::never()` - and `never` short-circuits the event
    // before `enabled()` is consulted, so a capture running in parallel records nothing.
    // This floor is installed once as the process-wide global default and answers
    // `sometimes` for every callsite. It captures nothing itself.
    struct InterestFloor;

    impl tracing::Subscriber for InterestFloor {
        fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
            Interest::sometimes()
        }

        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            false
        }

        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

        fn event(&self, _: &tracing::Event<'_>) {}

        fn enter(&self, _: &span::Id) {}

        fn exit(&self, _: &span::Id) {}
    }

    fn install_interest_floor() {
        static INSTALLED: OnceLock<()> = OnceLock::new();
        INSTALLED.get_or_init(|| {
            let _ = tracing::subscriber::set_global_default(InterestFloor);
            tracing::callsite::rebuild_interest_cache();
        });
    }

    // Why: `register_callsite` answers `sometimes` on purpose - a cached `always` from
    // another capture running in parallel would hand a TRACE line to a WARN-only assertion.
    struct CaptureSubscriber {
        lines: Arc<Mutex<Vec<CapturedLine>>>,
        max: Level,
    }

    impl tracing::Subscriber for CaptureSubscriber {
        fn register_callsite(&self, _: &'static tracing::Metadata<'static>) -> Interest {
            Interest::sometimes()
        }

        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            *metadata.level() <= self.max
        }

        fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
            span::Id::from_u64(1)
        }

        fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

        fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

        fn event(&self, event: &tracing::Event<'_>) {
            let mut collector = FieldCollector::default();
            event.record(&mut collector);
            self.lines
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(CapturedLine {
                    target: event.metadata().target().to_owned(),
                    level: *event.metadata().level(),
                    fields: collector.0,
                });
        }

        fn enter(&self, _: &span::Id) {}

        fn exit(&self, _: &span::Id) {}
    }

    pub fn capture_blocking<F: Future>(max: Level, future: F) -> (F::Output, Vec<CapturedLine>) {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let subscriber = CaptureSubscriber {
            lines: Arc::clone(&lines),
            max,
        };
        install_interest_floor();
        let output = tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("current-thread runtime must build");
            runtime.block_on(future)
        });
        let captured = std::mem::take(&mut *lines.lock().unwrap_or_else(|e| e.into_inner()));
        (output, captured)
    }

    pub fn forge_warnings(lines: &[CapturedLine]) -> Vec<&CapturedLine> {
        lines
            .iter()
            .filter(|line| line.from_forge() && line.level == Level::WARN)
            .collect()
    }
}
