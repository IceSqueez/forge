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
    DonationRepo, EventLogRepo, GlobalsRepo, HistoryRepo, LatestValueRepo, MediaRepo, QueueRepo,
    ScheduledRunRepo, ScriptRepo, SettingsRepo, SoundboardClipsRepo, StorageError,
    TriggerInstanceRepo, TtsFiltersRepo, UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
};
use forge_storage::{MockOverlayRepo, OverlayRepo};
pub use forge_test_support::log_capture;
use time::OffsetDateTime;

use crate::ServerTiming;
use crate::access::auth::AuthState;
use crate::access::origin::build_allowed_origins;
use crate::config::ServerConfig;
use crate::delivery::bus_adapter::{BusAdapter, ClientFilterSet};
use crate::delivery::ws_client::WsClient;
use crate::files::audio_clips::AudioClipStore;
use crate::lifecycle::server::AppState;
use crate::lifecycle::server_info::ServerInfo;
use crate::protocol::{DispatchContext, Services, Session};

const TEST_BIND_ADDR: &str = "127.0.0.1:9515";
const TEST_PEER_ADDR: &str = "127.0.0.1:0";
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
impl DataProvider for TestDataProvider {
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
        Err(forge_storage::StorageError::Connection {
            reason: "this backend has no transactional writes".to_owned(),
        })
    }
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
    Arc::new(spawn_action_engine(forge_runtime::ActionEngineParts {
        bus: Arc::clone(bus),
        catalog: Catalog::new(
            dp.action_repo(),
            dp.trigger_instance_repo(),
            dp.catalog_revision(),
        ),
        actions: dp.action_repo(),
        history: dp.history_repo(),
        sub_action_registry: Arc::new(SubActionRegistry::new()),
        cancel_registry: Arc::new(ActionCancelRegistry::new()),
        date_names: forge_runtime::DateNamesHandle::default(),
        config: forge_runtime::Config::default(),
    }))
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
        globals: dp.globals_repo(),
        user_globals: dp.user_globals_repo(),
        overlays: dp.overlay_repo(),
        credentials,
        settings: mem_settings(),
        server_info: ServerInfo::serving(BTreeSet::new()),
        action_engine,
        audio_clips: AudioClipStore::new(),
        overlay_root: Arc::new(PathBuf::new()),
        default_overlay_root: Arc::new(PathBuf::new()),
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
        PathBuf::new(),
        settings,
        credentials,
        bus,
        dp.action_repo(),
        dp.globals_repo(),
        dp.user_globals_repo(),
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
        services: Services {
            bus,
            bus_adapter,
            actions: dp.action_repo(),
            globals: dp.globals_repo(),
            user_globals: dp.user_globals_repo(),
            overlays: dp.overlay_repo(),
            auth_state: AuthState::for_test(auth_required_for_reads, TEST_BEARER_TOKEN),
            credentials: test_creds(),
            server_info: ServerInfo::serving(BTreeSet::new()),
            action_engine,
            overlay_root: Arc::new(PathBuf::new()),
        },
        session: Session::new(client),
    }
}

pub(crate) fn dispatch_ctx(
    dp: Arc<dyn DataProvider>,
    auth_required_for_reads: bool,
) -> DispatchContext {
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let bus_adapter = BusAdapter::new(bus.clone());
    let client = Arc::new(WsClient::new(
        bus_adapter.allocate_client_id(),
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
