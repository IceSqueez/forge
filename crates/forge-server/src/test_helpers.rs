use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use forge_registry::SubActionRegistry;
use forge_runtime::{
    ActionCancelRegistry, ActionEngineHandle, Catalog, EventBus, NullEventLogRepo,
    spawn_action_engine,
};
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
use forge_types::{ScriptId, Variant};
use time::OffsetDateTime;

use crate::ServerTiming;
use crate::audio_clips::AudioClipStore;
use crate::auth::AuthState;
use crate::bus_adapter::{BusAdapter, ClientFilterSet, ClientId};
use crate::config::ServerConfig;
use crate::origin::build_allowed_origins;
use crate::protocol::DispatchContext;
use crate::server::AppState;
use crate::server_info::ServerInfo;
use crate::ws_client::WsClient;

const TEST_BIND_ADDR: &str = "127.0.0.1:9515";
const TEST_PEER_ADDR: &str = "127.0.0.1:0";
const TEST_OVERLAY_ROOT: &str = "/tmp/forge-test-overlays";
const TEST_BEARER_TOKEN: &str = "test-token";
const STORE_REFUSAL_REASON: &str = "disk is full";

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

impl TestDataProvider {
    pub fn new() -> Self {
        let mut overlay_repo = MockOverlayRepo::new();
        overlay_repo.expect_get().returning(|_| Ok(None));

        Self {
            catalog_revision: CatalogRevision::new(),
            action_repo: Arc::new(MockActionRepo::new()),
            trigger_instance_repo: Arc::new(MockTriggerInstanceRepo::new()),
            queue_repo: Arc::new(MockQueueRepo::new()),
            history_repo: Arc::new(MockHistoryRepo::new()),
            event_log_repo: Arc::new(MockEventLogRepo::new()),
            soundboard_clips_repo: Arc::new(MockSoundboardClipsRepo::new()),
            overlay_repo: Arc::new(overlay_repo),
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

    pub fn action(&mut self) -> &mut MockActionRepo {
        Arc::get_mut(&mut self.action_repo).expect("action_repo already shared")
    }

    pub fn trigger_instance(&mut self) -> &mut MockTriggerInstanceRepo {
        Arc::get_mut(&mut self.trigger_instance_repo).expect("trigger_instance_repo already shared")
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

pub fn test_dp() -> Arc<dyn DataProvider> {
    Arc::new(TestDataProvider::new())
}

pub fn test_creds() -> Arc<dyn CredentialsRepo> {
    MemCreds::new()
}

pub fn mem_settings() -> Arc<MemorySettings> {
    Arc::new(MemorySettings::new())
}

#[derive(Default)]
pub struct MemCreds {
    slots: Mutex<HashMap<String, String>>,
    writes: Mutex<Vec<(String, String)>>,
    refuse_every_write: AtomicBool,
    refused_value: Mutex<Option<String>>,
}

impl MemCreds {
    pub fn new() -> Arc<Self> {
        Arc::default()
    }

    pub fn with_slot(id: &str, value: &str) -> Arc<Self> {
        let creds = Self::new();
        creds
            .slots
            .lock()
            .expect("mutex")
            .insert(id.to_owned(), value.to_owned());
        creds
    }

    pub fn with_token(token: &str) -> Arc<Self> {
        Self::with_slot(forge_storage::SERVER_BEARER_CREDENTIAL_ID, token)
    }

    pub fn stored(&self, id: &str) -> Option<String> {
        self.slots.lock().expect("mutex").get(id).cloned()
    }

    pub fn writes_to(&self, id: &str) -> Vec<String> {
        self.writes
            .lock()
            .expect("mutex")
            .iter()
            .filter(|(written, _)| written == id)
            .map(|(_, value)| value.clone())
            .collect()
    }

    pub fn refuse_every_write(&self, refuse: bool) {
        self.refuse_every_write.store(refuse, Ordering::SeqCst);
    }

    pub fn refuse_writes_of(&self, value: &str) {
        *self.refused_value.lock().expect("mutex") = Some(value.to_owned());
    }

    fn refuses(&self, value: &str) -> bool {
        self.refuse_every_write.load(Ordering::SeqCst)
            || self.refused_value.lock().expect("mutex").as_deref() == Some(value)
    }
}

#[async_trait]
impl CredentialsRepo for MemCreds {
    async fn store(&self, id: &CredentialId, plaintext_bundle: &str) -> Result<(), StorageError> {
        if self.refuses(plaintext_bundle) {
            return Err(StorageError::Connection {
                reason: STORE_REFUSAL_REASON.into(),
            });
        }
        self.slots
            .lock()
            .expect("mutex")
            .insert(id.as_str().to_owned(), plaintext_bundle.to_owned());
        self.writes
            .lock()
            .expect("mutex")
            .push((id.as_str().to_owned(), plaintext_bundle.to_owned()));
        Ok(())
    }

    async fn load(&self, id: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(self.stored(id.as_str()))
    }

    async fn delete(&self, id: &CredentialId) -> Result<bool, StorageError> {
        Ok(self
            .slots
            .lock()
            .expect("mutex")
            .remove(id.as_str())
            .is_some())
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(self
            .slots
            .lock()
            .expect("mutex")
            .keys()
            .map(|id| CredentialId::new(id.clone()))
            .collect())
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

pub(crate) fn test_engine(bus: &Arc<EventBus>, dp: &dyn DataProvider) -> Arc<ActionEngineHandle> {
    Arc::new(spawn_action_engine(
        Arc::clone(bus),
        Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        dp.action_repo(),
        dp.history_repo(),
        Arc::new(SubActionRegistry::new()),
        Arc::new(ActionCancelRegistry::new()),
    ))
}

pub(crate) fn app_state_over(
    auth: Arc<AuthState>,
    credentials: Arc<dyn CredentialsRepo>,
    dp: Arc<dyn DataProvider>,
) -> AppState {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let bus_adapter = BusAdapter::new(bus.clone());
    bus_adapter.spawn();
    let action_engine = test_engine(&bus, &*dp);
    let bind_addr: SocketAddr = TEST_BIND_ADDR.parse().expect("addr");
    AppState {
        auth,
        bus,
        bus_adapter,
        actions: dp.action_repo(),
        globals: Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        user_globals: Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
        overlays: dp.overlay_repo(),
        credentials,
        settings: mem_settings(),
        server_info: ServerInfo::serving(BTreeSet::new()),
        action_engine,
        audio_clips: AudioClipStore::new(),
        overlay_root: Arc::new(PathBuf::from(TEST_OVERLAY_ROOT)),
        overlay_cors_any_origin: true,
        bind_addr,
        timing: ServerTiming::default(),
        allowed_origins: Arc::new(build_allowed_origins(bind_addr, &[])),
    }
}

pub(crate) fn app_state(auth: Arc<AuthState>, credentials: Arc<dyn CredentialsRepo>) -> AppState {
    app_state_over(auth, credentials, test_dp())
}

pub(crate) fn server_config(
    settings: Arc<dyn SettingsRepo>,
    credentials: Arc<dyn CredentialsRepo>,
    dp: Arc<dyn DataProvider>,
) -> ServerConfig {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let action_engine = test_engine(&bus, &*dp);
    ServerConfig::new(
        settings,
        credentials,
        bus,
        dp.action_repo(),
        Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
        dp.overlay_repo(),
        action_engine,
    )
}

fn context_for(
    client: Arc<WsClient>,
    bus: Arc<EventBus>,
    bus_adapter: Arc<BusAdapter>,
    dp: Arc<dyn DataProvider>,
    auth_required_for_reads: bool,
) -> DispatchContext {
    let action_engine = test_engine(&bus, &*dp);
    DispatchContext {
        bus,
        bus_adapter,
        actions: dp.action_repo(),
        globals: Arc::clone(&dp) as Arc<dyn GlobalsRepo>,
        user_globals: Arc::clone(&dp) as Arc<dyn UserGlobalsRepo>,
        overlays: dp.overlay_repo(),
        auth_state: AuthState::for_test(auth_required_for_reads, TEST_BEARER_TOKEN),
        client,
        credentials: test_creds(),
        server_info: ServerInfo::serving(BTreeSet::new()),
        action_engine,
        overlay_root: Arc::new(PathBuf::from(TEST_OVERLAY_ROOT)),
        overlay_channel_swap: tokio::sync::Mutex::new(None),
        close_after_auth_failure: AtomicBool::new(false),
    }
}

pub(crate) fn dispatch_ctx(
    dp: Arc<dyn DataProvider>,
    auth_required_for_reads: bool,
) -> DispatchContext {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let bus_adapter = BusAdapter::new(bus.clone());
    let client = Arc::new(WsClient::new(
        ClientId::next(),
        TEST_PEER_ADDR.parse().expect("addr"),
        Arc::new(AtomicU64::new(0)),
    ));
    context_for(client, bus, bus_adapter, dp, auth_required_for_reads)
}

pub(crate) async fn registered_dispatch_ctx(
    dp: Arc<dyn DataProvider>,
    auth_required_for_reads: bool,
) -> DispatchContext {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let bus_adapter = BusAdapter::new(bus.clone());
    let (handle, _rx) = bus_adapter
        .register_client(ClientFilterSet::new(HashSet::new()))
        .await;
    let client = Arc::new(WsClient::new(
        handle.id,
        TEST_PEER_ADDR.parse().expect("addr"),
        Arc::clone(&handle.drop_counter),
    ));
    context_for(client, bus, bus_adapter, dp, auth_required_for_reads)
}

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
            drop(tracing::subscriber::set_global_default(InterestFloor));
            tracing::callsite::rebuild_interest_cache();
        });
    }

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

#[tokio::test]
async fn a_backend_without_transactional_writes_refuses_a_commit_instead_of_dropping_it() {
    let provider = TestDataProvider::new();

    let refused = provider
        .commit_changes(&[forge_storage::StoreChange::DeleteGlobal("score".to_owned())])
        .await;

    assert!(
        matches!(refused, Err(StorageError::Connection { .. })),
        "{refused:?}"
    );
}
