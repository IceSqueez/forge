#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use forge_audio::{AudioError, AudioSink, ControlledPlayback, PcmBuffer, PlaybackHandle};
use forge_events::Event;
use forge_registry::{
    ActorBlock, ActorIdentity, EventFilter, FormField, KindPlatformContract, LoginSlot,
    SubActionRegistry, TriggerCategory, TriggerKindDescriptor, TriggerRegistry, TriggerVariables,
};
use forge_runtime::{ActionCancelRegistry, Catalog, EventBus, spawn_action_engine};
use forge_server::{ServerConfig, ServerHandle, stopped_server};
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{
    ActionRepo, ActionStats, ActionTelemetry, CatalogRevision, ChatHistoryRepo, CredentialId,
    CredentialsRepo, DataProvider, EventLogRepo, ExecutionStatus, GlobalEntry, GlobalsRepo,
    HistoryRepo, MediaRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId,
    OverlayRepo, QueueRepo, ScriptRecord, ScriptRepo, ScriptTelemetry, SettingsRepo,
    SoundboardClipsRepo, StorageError, TriggerInstanceRepo, TtsFiltersRepo, UserGlobalEntry,
    UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
};
use forge_types::{
    Action, ActionId, ActorRole, EventId, ExecutionContext, PlatformId, ScriptId, TriggerConfig,
    Variant,
};
use time::OffsetDateTime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const RUNTIME_PUMPS: usize = 8;

pub(crate) type SettingWrite = (String, String);

pub(crate) struct TestBackend {
    values: Mutex<HashMap<String, String>>,
    writes: UnboundedSender<SettingWrite>,
    overlays: Option<Arc<dyn OverlayRepo>>,
}

pub(crate) fn test_backend() -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(None)
}

pub(crate) fn test_backend_with_overlays(
    overlays: Arc<dyn OverlayRepo>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(Some(overlays))
}

fn backend_over(
    overlays: Option<Arc<dyn OverlayRepo>>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    let (writes, rx) = unbounded_channel();
    (
        Arc::new(TestBackend {
            values: Mutex::new(HashMap::new()),
            writes,
            overlays,
        }),
        rx,
    )
}

#[async_trait::async_trait]
impl SettingsRepo for TestBackend {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
        Ok(self.values.lock().unwrap().get(key).cloned())
    }

    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
        self.values
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        let _ = self.writes.send((key.to_owned(), value.to_owned()));
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.values.lock().unwrap().remove(key).is_some())
    }

    async fn load_all(&self) -> Result<HashMap<String, String>, StorageError> {
        Ok(self.values.lock().unwrap().clone())
    }
}

#[async_trait::async_trait]
impl CredentialsRepo for TestBackend {
    async fn store(&self, _: &CredentialId, _: &str) -> Result<(), StorageError> {
        Ok(())
    }

    async fn load(&self, _: &CredentialId) -> Result<Option<String>, StorageError> {
        Ok(None)
    }

    async fn delete(&self, _: &CredentialId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn list_ids(&self) -> Result<Vec<CredentialId>, StorageError> {
        Ok(Vec::new())
    }

    async fn last_refresh(&self, _: &CredentialId) -> Result<Option<OffsetDateTime>, StorageError> {
        Ok(None)
    }

    async fn mark_refreshed(&self, _: &CredentialId) -> Result<(), StorageError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl GlobalsRepo for TestBackend {
    async fn get(&self, _: &str) -> Result<Option<Variant>, StorageError> {
        unreachable!("globals are out of scope for the settings pane")
    }

    async fn set(&self, _: &str, _: Variant, _: bool) -> Result<(), StorageError> {
        unreachable!("globals are out of scope for the settings pane")
    }

    async fn delete(&self, _: &str) -> Result<bool, StorageError> {
        unreachable!("globals are out of scope for the settings pane")
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        unreachable!("globals are out of scope for the settings pane")
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        unreachable!("globals are out of scope for the settings pane")
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        unreachable!("globals are out of scope for the settings pane")
    }

    async fn incr(&self, _: &str, _: i64) -> Result<Variant, StorageError> {
        unreachable!("globals are out of scope for the settings pane")
    }
}

#[async_trait::async_trait]
impl UserGlobalsRepo for TestBackend {
    async fn get(&self, _: &str, _: &str, _: &str) -> Result<Option<Variant>, StorageError> {
        unreachable!("user globals are out of scope for the settings pane")
    }

    async fn set(&self, _: &str, _: &str, _: &str, _: Variant) -> Result<(), StorageError> {
        unreachable!("user globals are out of scope for the settings pane")
    }

    async fn delete(&self, _: &str, _: &str, _: &str) -> Result<bool, StorageError> {
        unreachable!("user globals are out of scope for the settings pane")
    }

    async fn list_for_user(&self, _: &str, _: &str) -> Result<Vec<UserGlobalEntry>, StorageError> {
        unreachable!("user globals are out of scope for the settings pane")
    }

    async fn list_for_broadcaster(&self, _: &str) -> Result<Vec<UserGlobalEntry>, StorageError> {
        unreachable!("user globals are out of scope for the settings pane")
    }
}

#[async_trait::async_trait]
impl ScriptRepo for TestBackend {
    async fn get(&self, _: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }

    async fn get_by_name(&self, _: &str) -> Result<Option<ScriptRecord>, StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }

    async fn save(&self, _: ScriptRecord) -> Result<(), StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }

    async fn delete(&self, _: ScriptId) -> Result<bool, StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }

    async fn record_execution(
        &self,
        _: ScriptId,
        _: OffsetDateTime,
        _: u64,
        _: ExecutionStatus,
    ) -> Result<(), StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }

    async fn telemetry(&self, _: ScriptId) -> Result<ScriptTelemetry, StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }

    async fn prune_executions_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
        unreachable!("scripts are out of scope for the settings pane")
    }
}

#[async_trait::async_trait]
impl DataProvider for TestBackend {
    fn action_repo(&self) -> Arc<dyn ActionRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn trigger_instance_repo(&self) -> Arc<dyn TriggerInstanceRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn queue_repo(&self) -> Arc<dyn QueueRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn history_repo(&self) -> Arc<dyn HistoryRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn event_log_repo(&self) -> Arc<dyn EventLogRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn soundboard_clips_repo(&self) -> Arc<dyn SoundboardClipsRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn voice_alias_repo(&self) -> Arc<dyn VoiceAliasRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn viewer_repo(&self) -> Arc<dyn ViewerRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn tts_filters_repo(&self) -> Arc<dyn TtsFiltersRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn chat_history_repo(&self) -> Arc<dyn ChatHistoryRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn overlay_repo(&self) -> Arc<dyn OverlayRepo> {
        self.overlays
            .clone()
            .unwrap_or_else(|| unreachable!("the settings pane reaches no sub-repo"))
    }

    fn media_repo(&self) -> Arc<dyn MediaRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn catalog_revision(&self) -> CatalogRevision {
        unreachable!("the settings pane reaches no catalog")
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        unreachable!("the settings pane never reads the schema version")
    }

    async fn export(&self, _: &std::path::Path) -> Result<(), StorageError> {
        unreachable!("the settings pane never exports")
    }

    async fn shutdown(&self) {}
}

pub(crate) struct StubEventLog;

#[async_trait::async_trait]
impl EventLogRepo for StubEventLog {
    async fn insert(&self, _: &Event) -> Result<(), StorageError> {
        Ok(())
    }

    async fn get(&self, _: EventId) -> Result<Option<Event>, StorageError> {
        Ok(None)
    }

    async fn recent(&self, _: usize) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn recent_since(&self, _: usize, _: Option<EventId>) -> Result<Vec<Event>, StorageError> {
        Ok(Vec::new())
    }

    async fn prune_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

pub(crate) struct StubOverlays;

#[async_trait::async_trait]
impl OverlayRepo for StubOverlays {
    async fn list(&self) -> Result<Vec<OverlayDefinition>, StorageError> {
        Ok(Vec::new())
    }

    async fn get(&self, _: &OverlayId) -> Result<Option<OverlayDefinition>, StorageError> {
        Ok(None)
    }

    async fn get_by_credential(
        &self,
        _: &OverlayCredential,
    ) -> Result<Option<OverlayDefinition>, StorageError> {
        Ok(None)
    }

    async fn create(&self, _: &str, _: &str, _: u32) -> Result<OverlayDefinition, StorageError> {
        unreachable!("the settings pane never creates an overlay")
    }

    async fn save(&self, _: &OverlayDefinition) -> Result<(), StorageError> {
        unreachable!("the settings pane never saves an overlay")
    }

    async fn set_enabled(&self, _: &OverlayId, _: bool) -> Result<bool, StorageError> {
        unreachable!("the settings pane never toggles an overlay")
    }

    async fn delete(&self, _: &OverlayId) -> Result<bool, StorageError> {
        unreachable!("the settings pane never deletes an overlay")
    }

    async fn get_retained_content(
        &self,
        _: &OverlayId,
    ) -> Result<Option<OverlayConfig>, StorageError> {
        Ok(None)
    }

    async fn set_retained_content(
        &self,
        _: &OverlayId,
        _: &OverlayConfig,
    ) -> Result<(), StorageError> {
        unreachable!("the settings pane never writes overlay content")
    }
}

pub(crate) struct StubActions;

#[async_trait::async_trait]
impl ActionRepo for StubActions {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(Vec::new())
    }

    async fn get(&self, _: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(None)
    }

    async fn save(&self, _: &Action) -> Result<(), StorageError> {
        unreachable!("the settings pane never writes an action")
    }

    async fn delete(&self, _: ActionId) -> Result<bool, StorageError> {
        unreachable!("the settings pane never deletes an action")
    }

    async fn list_by_group<'a>(&'a self, _: Option<&'a str>) -> Result<Vec<Action>, StorageError> {
        Ok(Vec::new())
    }

    async fn telemetry(&self, _: ActionId) -> Result<ActionTelemetry, StorageError> {
        unreachable!("the settings pane never reads action telemetry")
    }

    async fn record_execution(
        &self,
        _: ActionId,
        _: OffsetDateTime,
        _: u64,
        _: ExecutionStatus,
    ) -> Result<(), StorageError> {
        unreachable!("the settings pane never runs an action")
    }

    async fn prune_executions_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

pub(crate) fn stub_catalog() -> Arc<Catalog> {
    Catalog::new(
        Arc::new(StubActions),
        Arc::new(MockTriggerInstanceRepo::new()),
        CatalogRevision::new(),
    )
}

pub(crate) struct StubHistory;

#[async_trait::async_trait]
impl HistoryRepo for StubHistory {
    async fn save(&self, _: &ExecutionContext) -> Result<(), StorageError> {
        unreachable!("the settings pane never records a run")
    }

    async fn recent_for_action(
        &self,
        _: ActionId,
        _: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(Vec::new())
    }

    async fn recent_for_builtin(
        &self,
        _: &str,
        _: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(Vec::new())
    }

    async fn stats_summary(
        &self,
        _: OffsetDateTime,
    ) -> Result<HashMap<ActionId, ActionStats>, StorageError> {
        Ok(HashMap::new())
    }

    async fn prune_before(&self, _: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Declares {
    Nothing,
    AnEmptyList,
    APrincipal,
}

pub(crate) struct StubTrigger {
    id: String,
    label: String,
    category: TriggerCategory,
    declares: Declares,
    fields: Vec<FormField>,
}

impl StubTrigger {
    pub(crate) fn new(
        id: &str,
        label: &str,
        category: TriggerCategory,
        declares: Declares,
    ) -> Self {
        Self {
            id: id.to_owned(),
            label: label.to_owned(),
            category,
            declares,
            fields: Vec::new(),
        }
    }

    pub(crate) fn with_fields(mut self, fields: Vec<FormField>) -> Self {
        self.fields = fields;
        self
    }
}

impl TriggerKindDescriptor for StubTrigger {
    fn id(&self) -> &str {
        &self.id
    }

    fn category(&self) -> TriggerCategory {
        self.category
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn summary(&self) -> &str {
        &self.label
    }

    fn search_text(&self) -> &str {
        &self.label
    }

    fn icon_name(&self) -> &str {
        "bell"
    }

    fn platform_contract(&self) -> KindPlatformContract {
        KindPlatformContract::PlatformSpecific(PlatformId::Twitch)
    }

    fn default_config(&self) -> TriggerConfig {
        TriggerConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        self.fields.clone()
    }

    fn condition_display(&self, _: &TriggerConfig) -> String {
        String::new()
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: None,
            kind_prefix: None,
        }
    }

    fn matches_trigger(&self, _: &TriggerConfig, _: &Event) -> bool {
        true
    }

    fn variables(&self) -> Option<TriggerVariables> {
        match self.declares {
            Declares::Nothing => None,
            Declares::AnEmptyList => Some(TriggerVariables::new()),
            Declares::APrincipal => Some(TriggerVariables::new().actor(
                ActorBlock {
                    role: ActorRole::Principal,
                    platform: PlatformId::Twitch,
                    login: LoginSlot::Declared,
                },
                |_| ActorIdentity {
                    id: "1".to_owned(),
                    display_name: "Someone".to_owned(),
                    login: Some("someone".to_owned()),
                },
            )),
        }
    }
}

pub(crate) fn overlay_definition(kind_id: &str, config: OverlayConfig) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new("sub-alert"),
        display_name: "Sub alert".to_owned(),
        kind_id: kind_id.to_owned(),
        enabled: true,
        position: 0,
        config,
        config_schema_version: 2,
        generator_version: 0,
        source_overrides: Vec::new(),
        credential: OverlayCredential::new("2f8b1d0c9a7e6f5b4c3d2e1f0a9b8c7d"),
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
    }
}

pub(crate) fn overlay_named(id: &str, kind_id: &str) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(id),
        display_name: format!("{id} overlay"),
        ..overlay_definition(kind_id, OverlayConfig::new())
    }
}

pub(crate) fn trigger_registry(stubs: Vec<StubTrigger>) -> TriggerRegistry {
    let mut registry = TriggerRegistry::new();
    for stub in stubs {
        registry
            .register(Box::new(stub))
            .expect("every stub carries its own id");
    }
    registry
}

pub(crate) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread runtime")
}

pub(crate) fn pump(rt: &tokio::runtime::Runtime) {
    rt.block_on(async {
        for _ in 0..RUNTIME_PUMPS {
            tokio::task::yield_now().await;
        }
    });
}

pub(crate) async fn stopped_server_handle(backend: &Arc<TestBackend>) -> ServerHandle {
    let bus = EventBus::new(Arc::new(StubEventLog));
    let engine = Arc::new(spawn_action_engine(
        Arc::clone(&bus),
        stub_catalog(),
        Arc::new(StubActions),
        Arc::new(StubHistory),
        Arc::new(SubActionRegistry::new()),
        Arc::new(ActionCancelRegistry::new()),
    ));
    stopped_server(ServerConfig::new(
        Arc::clone(backend) as Arc<dyn SettingsRepo>,
        Arc::clone(backend) as Arc<dyn CredentialsRepo>,
        bus,
        Arc::new(StubActions),
        Arc::clone(backend) as Arc<dyn GlobalsRepo>,
        Arc::clone(backend) as Arc<dyn UserGlobalsRepo>,
        Arc::new(StubOverlays),
        engine,
    ))
    .await
    .expect("a stopped server handle")
}

pub(crate) fn stopped_handle(
    rt: &tokio::runtime::Runtime,
    backend: &Arc<TestBackend>,
) -> ServerHandle {
    rt.block_on(stopped_server_handle(backend))
}

#[derive(Default)]
pub(crate) struct RecordingSink {
    plays: AtomicUsize,
    stoppables: AtomicUsize,
    controlled: AtomicUsize,
}

impl RecordingSink {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(crate) fn plays(&self) -> usize {
        self.plays.load(Ordering::SeqCst)
    }

    pub(crate) fn stoppables(&self) -> usize {
        self.stoppables.load(Ordering::SeqCst)
    }

    pub(crate) fn controlled(&self) -> usize {
        self.controlled.load(Ordering::SeqCst)
    }

    pub(crate) fn calls(&self) -> usize {
        self.plays() + self.stoppables() + self.controlled()
    }
}

#[async_trait::async_trait]
impl AudioSink for RecordingSink {
    async fn play(&self, _: PcmBuffer) -> Result<(), AudioError> {
        self.plays.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn play_stoppable(&self, _: PcmBuffer) -> Result<PlaybackHandle, AudioError> {
        self.stoppables.fetch_add(1, Ordering::SeqCst);
        Ok(PlaybackHandle::default())
    }

    async fn play_controlled(&self, _: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        self.controlled.fetch_add(1, Ordering::SeqCst);
        Ok(ControlledPlayback::resolved(Ok(())))
    }
}

pub(crate) fn tone() -> PcmBuffer {
    PcmBuffer::new(vec![0_i16; 16], 48_000, 1)
}

pub(crate) struct Sandboxed<T> {
    inner: T,
    _media_root: tempfile::TempDir,
}

impl<T> Sandboxed<T> {
    pub(crate) fn map<U>(self, wrap: impl FnOnce(T) -> U) -> Sandboxed<U> {
        Sandboxed {
            inner: wrap(self.inner),
            _media_root: self._media_root,
        }
    }
}

impl<T> std::ops::Deref for Sandboxed<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

pub(crate) async fn sandboxed_backend(
    url: &str,
    key: [u8; 32],
) -> Sandboxed<forge_storage_sqlite::SqliteBackend> {
    let media_root = tempfile::tempdir().expect("a temporary media root");
    let backend = forge_storage_sqlite::SqliteBackend::open_for_test(
        url,
        key,
        media_root.path().to_owned(),
        None,
    )
    .await
    .expect("a backend");
    Sandboxed {
        inner: backend,
        _media_root: media_root,
    }
}

pub(crate) fn held_queue_scheduler(
    rt: &tokio::runtime::Runtime,
    queue: forge_types::Queue,
) -> forge_runtime::QueueSchedulerHandle {
    rt.block_on(async {
        let bus = EventBus::new(Arc::new(StubEventLog));
        let engine = spawn_action_engine(
            Arc::clone(&bus),
            stub_catalog(),
            Arc::new(StubActions),
            Arc::new(StubHistory),
            Arc::new(SubActionRegistry::new()),
            Arc::new(ActionCancelRegistry::new()),
        );
        let id = queue.id;
        let scheduler = forge_runtime::QueueScheduler::spawn(engine, bus, vec![queue]);
        scheduler
            .set_mode(id, forge_runtime::QueueMode::HOLDING)
            .await
            .expect("the queue was registered at spawn");
        scheduler
    })
}

pub(crate) fn hold_one_dispatch(
    rt: &tokio::runtime::Runtime,
    scheduler: &forge_runtime::QueueSchedulerHandle,
    queue_id: forge_types::QueueId,
) {
    rt.block_on(scheduler.dispatch(forge_runtime::SchedulerRequest {
        queue_id,
        action_id: ActionId::new(),
        trigger_event_id: EventId::new(),
        trigger_kind: None,
        initial_args: forge_types::ArgStack::new(),
        bypass_pause: false,
    }))
    .expect("the scheduler is running");
    pump(rt);
}

pub(crate) const REWARDS: &str = "rewards";

pub(crate) struct FakeCollections {
    listing:
        Mutex<forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>>,
    lists: AtomicUsize,
    signal: forge_platform_core::CollectionRevisionSignal,
}

impl FakeCollections {
    pub(crate) fn listing(
        listing: forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            listing: Mutex::new(listing),
            lists: AtomicUsize::new(0),
            signal: forge_platform_core::CollectionRevisionSignal::new(),
        })
    }

    pub(crate) fn relist(
        &self,
        listing: forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
    ) {
        *self.listing.lock().unwrap() = listing;
    }

    pub(crate) fn bump(&self) {
        self.signal.bump();
    }

    pub(crate) fn list_calls(&self) -> usize {
        self.lists.load(Ordering::SeqCst)
    }

    pub(crate) fn metadata() -> forge_platform_core::CollectionMetadata {
        forge_platform_core::CollectionMetadata {
            id: forge_platform_core::CollectionId::new(REWARDS),
            label: "Channel point rewards".to_owned(),
            icon: forge_platform_core::SectionIcon::new("diamond"),
            capacity: None,
            fields: Vec::new(),
            toggles: Vec::new(),
        }
    }

    pub(crate) fn installed_as(
        self: &Arc<Self>,
        builtin: &str,
    ) -> crate::integrations::BuiltinRegistry {
        let registry = crate::integrations::BuiltinRegistry::default();
        registry.install(crate::integrations::BuiltinObject {
            collections: Some(Arc::clone(self) as Arc<dyn forge_platform_core::BuiltinCollections>),
            ..crate::unavailable_builtin::unavailable_builtin(&forge_types::IntegrationId::new(
                builtin,
            ))
        });
        registry
    }
}

pub(crate) fn reward(
    id: &str,
    title: &str,
    access: forge_platform_core::CollectionItemAccess,
) -> forge_platform_core::CollectionItem {
    forge_platform_core::CollectionItem {
        id: forge_platform_core::CollectionItemId::new(id),
        title: title.to_owned(),
        values: std::collections::BTreeMap::new(),
        toggles: std::collections::BTreeMap::new(),
        access,
    }
}

pub(crate) fn owned_reward(id: &str, title: &str) -> forge_platform_core::CollectionItem {
    reward(
        id,
        title,
        forge_platform_core::CollectionItemAccess::Manageable,
    )
}

pub(crate) fn foreign_reward(id: &str, title: &str) -> forge_platform_core::CollectionItem {
    reward(
        id,
        title,
        forge_platform_core::CollectionItemAccess::ReadOnly {
            reason: "created elsewhere".to_owned(),
        },
    )
}

#[async_trait::async_trait]
impl forge_platform_core::BuiltinCollections for FakeCollections {
    fn collections(&self) -> Vec<forge_platform_core::CollectionMetadata> {
        vec![Self::metadata()]
    }

    fn revisions(&self) -> forge_platform_core::CollectionRevisions {
        self.signal.subscribe()
    }

    async fn list(
        &self,
        _: &forge_platform_core::CollectionId,
    ) -> forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>> {
        self.lists.fetch_add(1, Ordering::SeqCst);
        self.listing.lock().unwrap().clone()
    }

    async fn create(
        &self,
        _: &forge_platform_core::CollectionId,
        _: &std::collections::BTreeMap<String, forge_platform_core::QuickActionFieldValue>,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn update(
        &self,
        _: &forge_platform_core::CollectionId,
        _: &forge_platform_core::CollectionItemId,
        _: &std::collections::BTreeMap<String, forge_platform_core::QuickActionFieldValue>,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn delete(
        &self,
        _: &forge_platform_core::CollectionId,
        _: &forge_platform_core::CollectionItemId,
    ) -> forge_platform_core::CollectionOutcome<()> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }

    async fn set_toggle(
        &self,
        _: &forge_platform_core::CollectionId,
        _: &forge_platform_core::CollectionItemId,
        _: &str,
        _: bool,
    ) -> forge_platform_core::CollectionOutcome<forge_platform_core::CollectionItem> {
        Err(forge_platform_core::CollectionFailure::Transport)
    }
}

pub(crate) const NESTING_KIND: &str = "stub.container";
pub(crate) const NESTED_CHAIN_KEY: &str = "body";

struct StubRunner {
    id: &'static str,
    nests: bool,
}

#[async_trait::async_trait]
impl forge_registry::SubActionRunner for StubRunner {
    fn id(&self) -> &str {
        self.id
    }

    fn category(&self) -> forge_registry::SubActionCategory {
        forge_registry::SubActionCategory::Chat
    }

    fn label(&self) -> &str {
        self.id
    }

    fn summary(&self) -> &str {
        self.id
    }

    fn search_text(&self) -> &str {
        self.id
    }

    fn icon_name(&self) -> &str {
        "bolt"
    }

    fn default_config(&self) -> forge_types::SubActionConfig {
        forge_types::SubActionConfig::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        if self.nests {
            vec![FormField::SubChain {
                key: NESTED_CHAIN_KEY,
                label: "Body",
            }]
        } else {
            Vec::new()
        }
    }

    fn validate_config(
        &self,
        _: &forge_types::SubActionConfig,
    ) -> Result<(), forge_registry::RegistryError> {
        Ok(())
    }

    async fn execute(
        &self,
        _: &forge_types::SubActionConfig,
        _: &forge_registry::RunContext<'_>,
    ) -> (
        forge_types::SubActionTelemetry,
        Option<forge_types::ArgStack>,
    ) {
        unreachable!("stub steps are inspected, never run")
    }
}

pub(crate) fn owned_sub_actions(owned: &[(&'static str, &'static str)]) -> SubActionRegistry {
    let mut registry = SubActionRegistry::new();
    registry
        .register(Box::new(StubRunner {
            id: NESTING_KIND,
            nests: true,
        }))
        .expect("the container stub registers");
    for (kind, owner) in owned {
        registry
            .owned_by(forge_types::IntegrationId::from_static(owner))
            .register(Box::new(StubRunner {
                id: kind,
                nests: false,
            }))
            .expect("every owned stub carries its own id");
    }
    registry
}

pub(crate) fn owned_triggers(owned: &[(&'static str, &'static str)]) -> TriggerRegistry {
    let mut registry = TriggerRegistry::new();
    for (kind, owner) in owned {
        registry
            .owned_by(forge_types::IntegrationId::from_static(owner))
            .register(Box::new(StubTrigger::new(
                kind,
                kind,
                TriggerCategory::Chat,
                Declares::Nothing,
            )))
            .expect("every owned stub carries its own id");
    }
    registry
}

pub(crate) fn step(kind_id: &str, enabled: bool) -> forge_types::SubActionStep {
    forge_types::SubActionStep {
        kind_id: kind_id.to_owned(),
        config: forge_types::SubActionConfig::new(),
        enabled,
        continue_on_error: false,
        condition: None,
        label: None,
    }
}

pub(crate) fn container(
    body: Vec<forge_types::SubActionStep>,
    enabled: bool,
) -> forge_types::SubActionStep {
    let encoded = body
        .into_iter()
        .map(|nested| {
            Variant::Object(forge_types::SubActionConfig::from([
                ("kind_id".to_owned(), Variant::String(nested.kind_id)),
                ("config".to_owned(), Variant::Object(nested.config)),
                ("enabled".to_owned(), Variant::Bool(nested.enabled)),
            ]))
        })
        .collect();
    forge_types::SubActionStep {
        config: forge_types::SubActionConfig::from([(
            NESTED_CHAIN_KEY.to_owned(),
            Variant::Array(encoded),
        )]),
        ..step(NESTING_KIND, enabled)
    }
}

pub(crate) fn action_running(name: &str, steps: Vec<forge_types::SubActionStep>) -> Action {
    Action {
        id: ActionId::new(),
        name: name.to_owned(),
        group: None,
        queue_id: forge_types::QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: forge_types::ExecutionMode::Sequential,
        description: None,
        sub_actions: steps,
    }
}

pub(crate) fn trigger_of(kind_id: &str) -> forge_types::TriggerInstance {
    forge_types::TriggerInstance {
        id: forge_types::TriggerInstanceId::new(),
        kind_id: kind_id.to_owned(),
        name: kind_id.to_owned(),
        overrides: TriggerConfig::new(),
        enabled: true,
        user_defined: true,
        platform_scope: forge_types::PlatformScope::default(),
        cooldown_secs: 0,
        cooldown_global: true,
        permission_rung: forge_types::PermissionRung::Everyone,
    }
}

pub(crate) const STORAGE_KEY: [u8; 32] = [0x11; 32];

pub(crate) struct IdleFactory(pub(crate) forge_types::IntegrationId);

#[async_trait::async_trait]
impl crate::integration_supervisor::IntegrationFactory for IdleFactory {
    fn id(&self) -> forge_types::IntegrationId {
        self.0.clone()
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        Ok(true)
    }

    async fn start(&self) -> Result<crate::integration_supervisor::RunningIntegration, String> {
        Ok(crate::integration_supervisor::RunningIntegration::idle())
    }
}

pub(crate) fn idle_supervisor(
    rt: &tokio::runtime::Runtime,
    settings: Arc<dyn SettingsRepo>,
    ids: &[forge_types::IntegrationId],
) -> crate::integration_supervisor::IntegrationSupervisor {
    let _entered = rt.enter();
    crate::integration_supervisor::IntegrationSupervisor::launch(
        ids.iter()
            .map(|id| {
                Arc::new(IdleFactory(id.clone()))
                    as Arc<dyn crate::integration_supervisor::IntegrationFactory>
            })
            .collect(),
        settings,
        forge_runtime::IntegrationGate::new(),
        crate::integrations::BuiltinRegistry::default(),
        forge_runtime::spawn_live_viewer_aggregator(),
    )
}

pub(crate) async fn sandboxed_provider() -> Sandboxed<Arc<dyn DataProvider>> {
    sandboxed_backend("sqlite::memory:", STORAGE_KEY)
        .await
        .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>)
}

async fn default_queue(backend: &Arc<dyn DataProvider>) -> forge_types::QueueId {
    backend
        .queue_repo()
        .get_by_name("Default")
        .await
        .unwrap()
        .expect("migrations seed the default queue")
        .id
}

pub(crate) async fn seed_action(
    backend: &Arc<dyn DataProvider>,
    name: &str,
    steps: Vec<forge_types::SubActionStep>,
) -> ActionId {
    let action = Action {
        queue_id: default_queue(backend).await,
        ..action_running(name, steps)
    };
    backend.action_repo().save(&action).await.unwrap();
    action.id
}

pub(crate) async fn seed_trigger(
    backend: &Arc<dyn DataProvider>,
    kind_id: &str,
) -> forge_types::TriggerInstanceId {
    let instance = trigger_of(kind_id);
    backend
        .trigger_instance_repo()
        .save(&instance)
        .await
        .unwrap();
    instance.id
}

pub(crate) async fn link(
    backend: &Arc<dyn DataProvider>,
    action: ActionId,
    trigger: forge_types::TriggerInstanceId,
) {
    backend
        .trigger_instance_repo()
        .link_action(action, trigger, 0)
        .await
        .unwrap();
}

pub(crate) fn lifecycle_switch(
    cx: &mut gpui::TestAppContext,
    rt: &tokio::runtime::Runtime,
    initial: crate::integration_supervisor::LifecycleStates,
) -> (
    gpui::Entity<crate::integration_lifecycle::IntegrationLifecycle>,
    crate::integration_switch::IntegrationSwitch,
) {
    let (settings, _writes) = test_backend();
    let supervisor = idle_supervisor(rt, settings as Arc<dyn SettingsRepo>, &[]);
    cx.update(|cx| {
        let lifecycle = gpui::AppContext::new(cx, |_| {
            crate::integration_lifecycle::IntegrationLifecycle::new(initial)
        });
        let switch =
            crate::integration_switch::IntegrationSwitch::new(lifecycle.clone(), supervisor);
        (lifecycle, switch)
    })
}

pub(crate) fn switch_lifecycle(
    cx: &mut gpui::TestAppContext,
    lifecycle: &gpui::Entity<crate::integration_lifecycle::IntegrationLifecycle>,
    states: crate::integration_supervisor::LifecycleStates,
) {
    lifecycle.update(cx, |lifecycle, cx| {
        lifecycle.replace(states);
        cx.notify();
    });
    cx.run_until_parked();
}

pub(crate) struct NavLog {
    screens: Vec<crate::screen::Screen>,
    _sub: gpui::Subscription,
}

impl NavLog {
    pub(crate) fn screens(&self) -> Vec<crate::screen::Screen> {
        self.screens.clone()
    }
}

pub(crate) fn nav_log<V: gpui::EventEmitter<crate::sidebar::NavRequested>>(
    view: &gpui::Entity<V>,
    cx: &mut gpui::App,
) -> gpui::Entity<NavLog> {
    gpui::AppContext::new(cx, |cx| NavLog {
        screens: Vec::new(),
        _sub: cx.subscribe(
            view,
            |log: &mut NavLog, _, event: &crate::sidebar::NavRequested, _| {
                log.screens.push(event.0.clone());
            },
        ),
    })
}

pub(crate) fn error_toasts(cx: &gpui::App) -> usize {
    cx.global::<crate::toasts::Toasts>()
        .items()
        .iter()
        .filter(|toast| toast.kind == forge_components::ToastKind::Error)
        .count()
}

pub(crate) fn install_presentation(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(crate::presentation::Presentation::new(
            forge_components::ThemeId::ForgeDefault,
            forge_components::Density::Cozy,
        ))
    });
}

pub(crate) struct ClickArea {
    pub(crate) x: std::ops::Range<f32>,
    pub(crate) y: std::ops::Range<f32>,
    pub(crate) step_x: f32,
    pub(crate) step_y: f32,
}

pub(crate) fn click_grid(vcx: &mut gpui::VisualTestContext, area: ClickArea) {
    let mut y = area.y.start + area.step_y / 2.0;
    while y < area.y.end {
        let mut x = area.x.start + area.step_x / 2.0;
        while x < area.x.end {
            vcx.simulate_click(
                gpui::point(gpui::px(x), gpui::px(y)),
                gpui::Modifiers::none(),
            );
            x += area.step_x;
        }
        y += area.step_y;
    }
}

const CRUMB_WINDOW_W: f32 = 1000.0;
const CRUMB_WINDOW_H: f32 = 700.0;
const CRUMB_BAND_W: f32 = 400.0;
const CRUMB_BAND_H: f32 = 64.0;
const CRUMB_SCAN_STEP: f32 = 4.0;

pub(crate) fn hub_crumb_targets<V>(
    cx: &mut gpui::TestAppContext,
    build: impl FnOnce(&mut gpui::Window, &mut gpui::Context<V>) -> V,
) -> Vec<crate::screen::Screen>
where
    V: gpui::Render + gpui::EventEmitter<crate::sidebar::NavRequested>,
{
    install_presentation(cx);
    let (view, vcx) = cx.add_window_view(build);
    let log = vcx.update(|_, cx| nav_log(&view, cx));
    vcx.simulate_resize(gpui::size(
        gpui::px(CRUMB_WINDOW_W),
        gpui::px(CRUMB_WINDOW_H),
    ));
    vcx.run_until_parked();
    click_grid(
        vcx,
        ClickArea {
            x: 0.0..CRUMB_BAND_W,
            y: 0.0..CRUMB_BAND_H,
            step_x: CRUMB_SCAN_STEP,
            step_y: CRUMB_SCAN_STEP,
        },
    );
    log.read_with(vcx, |log, _| log.screens())
        .into_iter()
        .filter(|screen| matches!(screen, crate::screen::Screen::Integrations(_)))
        .collect()
}

pub(crate) fn quiet_bus(rt: &tokio::runtime::Runtime) -> Arc<EventBus> {
    rt.block_on(async { EventBus::new(Arc::new(StubEventLog)) })
}
