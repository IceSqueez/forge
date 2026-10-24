#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod config_references;
mod memory_scripts;
pub(crate) mod pack_flow;
mod pack_transfer;
mod setup_transfer;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use forge_audio::{AudioError, AudioSink, ControlledPlayback, PcmBuffer, PlaybackHandle};
use forge_events::Event;
use forge_registry::{
    ActorBlock, ActorIdentity, ChatTriggerFamily, EventFilter, FormField, KindPlatformContract,
    LoginSlot, SubActionRegistry, TriggerCategory, TriggerKindDescriptor, TriggerRegistry,
    TriggerVariables,
};
use forge_runtime::{ActionCancelRegistry, Catalog, EventBus, spawn_action_engine};
use forge_server::{ServerConfig, ServerHandle, stopped_server};
use forge_storage::trigger_instance::MockTriggerInstanceRepo;
use forge_storage::{
    ActionRepo, ActionStats, ActionTelemetry, BanLedgerRepo, CatalogRevision, ChatHistoryRepo,
    CredentialId, CredentialsRepo, DataProvider, DonationRepo, EventLogRepo, ExecutionStatus,
    GlobalEntry, GlobalsRepo, HistoryRepo, LatestValueRepo, MediaBlob, MediaBlobId, MediaReferrer,
    MediaReferrerKind, MediaRepo, OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId,
    OverlayRepo, QueueRepo, ScheduledRunRepo, ScriptRecord, ScriptReferences, ScriptRepo,
    ScriptTelemetry, SettingsRepo, SoundboardClipsRepo, StorageError, TriggerInstanceRepo,
    TtsFiltersRepo, UserGlobalEntry, UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
};
use forge_types::{
    Action, ActionId, ActorRole, EventId, ExecutionContext, PlatformId, ScriptId, TriggerConfig,
    Variant,
};
use time::OffsetDateTime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub(crate) use memory_scripts::MemoryScripts;

const RUNTIME_PUMPS: usize = 8;

pub(crate) type SettingWrite = (String, String);

#[derive(Clone)]
pub(crate) enum ReadFault {
    Fails,
    Held(Arc<tokio::sync::Notify>),
}

pub(crate) const SETTINGS_READ_FAILED: &str = "settings store unreachable";
pub(crate) const SETTINGS_WRITE_FAILED: &str = "settings store is read-only";

pub(crate) struct TestBackend {
    values: Mutex<HashMap<String, String>>,
    writes: UnboundedSender<SettingWrite>,
    read_faults: Mutex<HashMap<String, ReadFault>>,
    write_faults: Mutex<std::collections::HashSet<String>>,
    overlays: Option<Arc<dyn OverlayRepo>>,
    scripts: Option<Arc<dyn ScriptRepo>>,
    media: Arc<dyn MediaRepo>,
    export_gate: Option<Arc<tokio::sync::Notify>>,
}

pub(crate) fn test_backend() -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(None, None, Arc::new(EmptyMediaLibrary))
}

pub(crate) fn test_backend_with_scripts(scripts: Arc<dyn ScriptRepo>) -> Arc<TestBackend> {
    backend_over(None, Some(scripts), Arc::new(EmptyMediaLibrary)).0
}

pub(crate) fn test_backend_with_overlays(
    overlays: Arc<dyn OverlayRepo>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(Some(overlays), None, Arc::new(EmptyMediaLibrary))
}

pub(crate) fn test_backend_with_media(
    media: Arc<dyn MediaRepo>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(None, None, media)
}

pub(crate) fn test_backend_with_gated_export() -> (Arc<TestBackend>, Arc<tokio::sync::Notify>) {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let gate = Arc::new(tokio::sync::Notify::new());
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.export_gate = Some(Arc::clone(&gate));
    (Arc::new(backend), gate)
}

fn backend_over(
    overlays: Option<Arc<dyn OverlayRepo>>,
    scripts: Option<Arc<dyn ScriptRepo>>,
    media: Arc<dyn MediaRepo>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    let (writes, rx) = unbounded_channel();
    (
        Arc::new(TestBackend {
            values: Mutex::new(HashMap::new()),
            writes,
            read_faults: Mutex::default(),
            write_faults: Mutex::default(),
            overlays,
            scripts,
            media,
            export_gate: None,
        }),
        rx,
    )
}

pub(crate) struct GatedSchedule {
    pub(crate) handle: forge_transfer::ScheduledBackupHandle,
    pub(crate) export_gate: Arc<tokio::sync::Notify>,
    _data_dir: tempfile::TempDir,
}

pub(crate) fn gated_running_schedule() -> GatedSchedule {
    let key = forge_transfer::create_schedule_key(&forge_transfer::Passphrase::new(
        "a gated schedule passphrase".to_owned(),
    ))
    .unwrap();
    let schedule = enabled_schedule_over_test_backend(
        Some(key.encode().unwrap()),
        forge_transfer::TransferExclusivity::new(),
    );
    schedule.handle.run_now();
    schedule
}

pub(crate) fn gated_running_schedule_sharing(
    exclusivity: forge_transfer::TransferExclusivity,
) -> GatedSchedule {
    let key = forge_transfer::create_schedule_key(&forge_transfer::Passphrase::new(
        "a gated schedule passphrase".to_owned(),
    ))
    .unwrap();
    let schedule = enabled_schedule_over_test_backend(Some(key.encode().unwrap()), exclusivity);
    schedule.handle.run_now();
    schedule
}

pub(crate) fn keyless_enabled_schedule() -> GatedSchedule {
    enabled_schedule_over_test_backend(None, forge_transfer::TransferExclusivity::new())
}

fn enabled_schedule_over_test_backend(
    encoded_key: Option<String>,
    exclusivity: forge_transfer::TransferExclusivity,
) -> GatedSchedule {
    let export_gate = Arc::new(tokio::sync::Notify::new());
    let (writes, _) = unbounded_channel();
    let last_success = OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let mut values = HashMap::from([
        (
            forge_storage::reserved_keys::BACKUP_SCHEDULE_ENABLED.to_owned(),
            "true".to_owned(),
        ),
        (
            forge_storage::reserved_keys::BACKUP_SCHEDULE_LAST_SUCCESS_AT.to_owned(),
            last_success,
        ),
    ]);
    if let Some(encoded) = encoded_key {
        values.insert(
            forge_storage::reserved_keys::BACKUP_SCHEDULE_KEY.to_owned(),
            encoded,
        );
    }
    let backend = Arc::new(TestBackend {
        values: Mutex::new(values),
        writes,
        read_faults: Mutex::default(),
        write_faults: Mutex::default(),
        overlays: None,
        scripts: None,
        media: Arc::new(EmptyMediaLibrary),
        export_gate: Some(Arc::clone(&export_gate)),
    });
    let data_dir = tempfile::tempdir().unwrap();
    let parts = forge_transfer::ScheduledBackupParts {
        provider: backend,
        data_dir: data_dir.path().to_path_buf(),
        exclusivity,
        clock: Arc::new(forge_transfer::SystemBackupClock),
    };
    let handle = forge_transfer::spawn_scheduled_backups(parts);
    GatedSchedule {
        handle,
        export_gate,
        _data_dir: data_dir,
    }
}

pub(crate) fn writes_to(writes: &mut UnboundedReceiver<SettingWrite>, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    while let Ok((written, value)) = writes.try_recv() {
        if written == key {
            values.push(value);
        }
    }
    values
}

impl TestBackend {
    pub(crate) fn seed(&self, key: &str, value: &str) {
        self.values
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
    }

    pub(crate) fn fault_reads(&self, key: &str, fault: ReadFault) {
        self.read_faults
            .lock()
            .unwrap()
            .insert(key.to_owned(), fault);
    }

    pub(crate) fn heal_reads(&self, key: &str) {
        self.read_faults.lock().unwrap().remove(key);
    }

    pub(crate) fn fail_writes(&self, key: &str) {
        self.write_faults.lock().unwrap().insert(key.to_owned());
    }
}

#[async_trait::async_trait]
impl SettingsRepo for TestBackend {
    async fn get_string(&self, key: &str) -> Result<Option<String>, StorageError> {
        let fault = self.read_faults.lock().unwrap().get(key).cloned();
        match fault {
            Some(ReadFault::Fails) => {
                return Err(StorageError::Connection {
                    reason: SETTINGS_READ_FAILED.to_owned(),
                });
            }
            Some(ReadFault::Held(gate)) => gate.notified().await,
            None => {}
        }
        Ok(self.values.lock().unwrap().get(key).cloned())
    }

    async fn set_string(&self, key: &str, value: &str) -> Result<(), StorageError> {
        if self.write_faults.lock().unwrap().contains(key) {
            return Err(StorageError::Connection {
                reason: SETTINGS_WRITE_FAILED.to_owned(),
            });
        }
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

impl TestBackend {
    fn scripts(&self) -> &Arc<dyn ScriptRepo> {
        self.scripts
            .as_ref()
            .unwrap_or_else(|| unreachable!("scripts are out of scope for the settings pane"))
    }
}

#[async_trait::async_trait]
impl ScriptRepo for TestBackend {
    async fn get(&self, id: ScriptId) -> Result<Option<ScriptRecord>, StorageError> {
        self.scripts().get(id).await
    }

    async fn get_by_name(&self, name: &str) -> Result<Option<ScriptRecord>, StorageError> {
        self.scripts().get_by_name(name).await
    }

    async fn save(&self, record: ScriptRecord) -> Result<(), StorageError> {
        self.scripts().save(record).await
    }

    async fn delete(&self, id: ScriptId) -> Result<bool, StorageError> {
        self.scripts().delete(id).await
    }

    async fn list(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        self.scripts().list().await
    }

    async fn list_enabled(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        self.scripts().list_enabled().await
    }

    async fn record_execution(
        &self,
        id: ScriptId,
        at: OffsetDateTime,
        duration_ms: u64,
        status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        self.scripts()
            .record_execution(id, at, duration_ms, status)
            .await
    }

    async fn telemetry(&self, id: ScriptId) -> Result<ScriptTelemetry, StorageError> {
        self.scripts().telemetry(id).await
    }

    async fn prune_executions_before(&self, cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        self.scripts().prune_executions_before(cutoff).await
    }

    async fn rename_cascading(
        &self,
        id: ScriptId,
        new_name: &str,
    ) -> Result<ScriptReferences, StorageError> {
        self.scripts().rename_cascading(id, new_name).await
    }

    async fn referrers(&self, name: &str) -> Result<ScriptReferences, StorageError> {
        self.scripts().referrers(name).await
    }

    async fn archive_script(&self, id: ScriptId) -> Result<bool, StorageError> {
        self.scripts().archive_script(id).await
    }

    async fn restore_script(&self, id: ScriptId) -> Result<bool, StorageError> {
        self.scripts().restore_script(id).await
    }

    async fn list_archived_scripts(&self) -> Result<Vec<ScriptRecord>, StorageError> {
        self.scripts().list_archived_scripts().await
    }
}

#[async_trait::async_trait]
impl DataProvider for TestBackend {
    fn action_repo(&self) -> Arc<dyn ActionRepo> {
        if self.scripts.is_some() {
            return Arc::new(StubActions);
        }
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
        Arc::clone(&self.media)
    }

    fn donation_repo(&self) -> Arc<dyn DonationRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn latest_value_repo(&self) -> Arc<dyn LatestValueRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn ban_ledger_repo(&self) -> Arc<dyn BanLedgerRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn scheduled_run_repo(&self) -> Arc<dyn ScheduledRunRepo> {
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn scheduled_run_revision(&self) -> CatalogRevision {
        unreachable!("the settings pane reaches no catalog")
    }

    fn catalog_revision(&self) -> CatalogRevision {
        unreachable!("the settings pane reaches no catalog")
    }

    async fn export(&self, _: &std::path::Path) -> Result<(), StorageError> {
        let Some(gate) = &self.export_gate else {
            unreachable!("the settings pane never exports")
        };
        gate.notified().await;
        Err(StorageError::Connection {
            reason: "the gated export was released without a snapshot".to_owned(),
        })
    }

    async fn schema_version(&self) -> Result<u32, StorageError> {
        if self.export_gate.is_none() {
            unreachable!("the settings pane never reads the schema version")
        }
        Ok(0)
    }

    async fn shutdown(&self) {}
}

pub(crate) struct EmptyMediaLibrary;

#[async_trait::async_trait]
impl MediaRepo for EmptyMediaLibrary {
    async fn store(&self, _: &str, _: Vec<u8>) -> Result<MediaBlob, StorageError> {
        unreachable!("the empty media library never stores a blob")
    }

    async fn import_file(&self, _: &std::path::Path) -> Result<MediaBlob, StorageError> {
        unreachable!("the empty media library never imports a file")
    }

    async fn get(&self, _: &MediaBlobId) -> Result<Option<MediaBlob>, StorageError> {
        Ok(None)
    }

    async fn list(&self) -> Result<Vec<MediaBlob>, StorageError> {
        Ok(Vec::new())
    }

    async fn total_bytes(&self) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn resolve(&self, id: &MediaBlobId) -> Result<std::path::PathBuf, StorageError> {
        Err(StorageError::NotFound {
            key: id.to_string(),
        })
    }

    async fn read(&self, id: &MediaBlobId) -> Result<Vec<u8>, StorageError> {
        Err(StorageError::NotFound {
            key: id.to_string(),
        })
    }

    async fn delete(&self, _: &MediaBlobId) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn retain(&self, _: &MediaReferrer, _: &MediaBlobId) -> Result<(), StorageError> {
        unreachable!("the empty media library holds no blob to retain")
    }

    async fn release(&self, _: &MediaReferrer) -> Result<bool, StorageError> {
        Ok(false)
    }

    async fn release_all(&self, _: MediaReferrerKind, _: &str) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn referrers(&self, _: &MediaBlobId) -> Result<Vec<MediaReferrer>, StorageError> {
        Ok(Vec::new())
    }

    async fn blob_of(&self, _: &MediaReferrer) -> Result<Option<MediaBlobId>, StorageError> {
        Ok(None)
    }
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

    async fn reissue_all_credentials(&self) -> Result<Vec<OverlayId>, StorageError> {
        Ok(Vec::new())
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
    defaults: TriggerConfig,
    chat_family: Option<ChatTriggerFamily>,
    contract: KindPlatformContract,
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
            defaults: TriggerConfig::new(),
            chat_family: None,
            contract: KindPlatformContract::PlatformSpecific(PlatformId::Twitch),
        }
    }

    pub(crate) fn with_contract(mut self, contract: KindPlatformContract) -> Self {
        self.contract = contract;
        self
    }

    pub(crate) fn with_fields(mut self, fields: Vec<FormField>) -> Self {
        self.fields = fields;
        self
    }

    pub(crate) fn with_defaults(mut self, defaults: TriggerConfig) -> Self {
        self.defaults = defaults;
        self
    }

    pub(crate) fn in_chat_family(mut self, family: ChatTriggerFamily) -> Self {
        self.chat_family = Some(family);
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
        self.contract
    }

    fn default_config(&self) -> TriggerConfig {
        self.defaults.clone()
    }

    fn config_fields(&self) -> Vec<FormField> {
        self.fields.clone()
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        self.chat_family
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

pub(crate) fn default_twitch_helix_gateway(
    bus: Arc<dyn forge_events::EventPublisher>,
) -> forge_net::Gateway {
    forge_platform_twitch::helix_gateway(
        &forge_platform_core::PlatformEndpoints::default(),
        Arc::new(forge_net::TokenBucketRateLimiter::new(
            forge_platform_twitch::HELIX_BUDGET_CAPACITY,
            forge_platform_twitch::HELIX_BUDGET_WINDOW,
        )),
        bus,
    )
    .unwrap()
}

pub(crate) fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current-thread runtime")
}

pub(crate) fn worker_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("multi-thread runtime")
}

pub(crate) fn stored_shortcut_overrides(
    raw: Option<&str>,
) -> Result<crate::shortcut_overrides::ShortcutOverrides, StorageError> {
    let (backend, _writes) = test_backend();
    if let Some(raw) = raw {
        backend.seed(forge_storage::reserved_keys::KEYBOARD_SHORTCUTS, raw);
    }
    runtime().block_on(crate::shortcut_overrides::load_overrides(backend))
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
        manual_test: false,
        initial_args: forge_types::ArgStack::new(),
        admission: forge_runtime::QueueAdmission::default(),
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
impl forge_runtime::IntegrationFactory<crate::integrations::BuiltinObject> for IdleFactory {
    fn id(&self) -> forge_types::IntegrationId {
        self.0.clone()
    }

    fn teardown_grace(&self) -> std::time::Duration {
        forge_runtime::DEFAULT_TEARDOWN_GRACE
    }

    async fn is_configured(&self) -> Result<bool, StorageError> {
        Ok(true)
    }

    async fn start(
        &self,
    ) -> Result<forge_runtime::RunningIntegration<crate::integrations::BuiltinObject>, String> {
        Ok(forge_runtime::RunningIntegration::idle())
    }
}

pub(crate) fn idle_supervisor(
    rt: &tokio::runtime::Runtime,
    settings: Arc<dyn SettingsRepo>,
    ids: &[forge_types::IntegrationId],
) -> forge_runtime::IntegrationSupervisor<crate::integrations::BuiltinObject> {
    let _entered = rt.enter();
    forge_runtime::IntegrationSupervisor::launch(
        ids.iter()
            .map(|id| {
                Arc::new(IdleFactory(id.clone()))
                    as Arc<
                        dyn forge_runtime::IntegrationFactory<crate::integrations::BuiltinObject>,
                    >
            })
            .collect(),
        settings,
        forge_runtime::IntegrationGate::new(),
        Arc::new(crate::integrations::BuiltinRegistry::default()),
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
        .get(forge_types::DEFAULT_QUEUE_ID)
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
    initial: forge_runtime::LifecycleStates,
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
    states: forge_runtime::LifecycleStates,
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
    cx.global::<forge_components::Toasts>()
        .items()
        .iter()
        .filter(|toast| toast.kind == forge_components::ToastKind::Error)
        .count()
}

pub(crate) fn install_presentation(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(forge_components::Presentation::new(
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

pub(crate) type LiveAliases = Arc<std::sync::RwLock<forge_voice::VoiceAliasResolver>>;

struct SilentPublisher;

impl forge_events::EventPublisher for SilentPublisher {
    fn publish(&self, _: Event) {}
}

pub(crate) fn spawn_speak_queue(
    aliases: Vec<forge_voice::VoiceAlias>,
) -> (
    forge_speak_queue::SpeakQueueHandle,
    forge_speak_queue::SpeakEventStream,
    LiveAliases,
) {
    let resolver = Arc::new(std::sync::RwLock::new(
        forge_voice::VoiceAliasResolver::new(
            aliases,
            forge_voice::AssignmentStrategy::DeterministicByName,
            forge_voice::IgnoreProfile::default(),
            forge_voice::SynthesisDefaults::default(),
        ),
    ));
    let deps = forge_speak_queue::QueueDeps {
        registry: Arc::new(std::sync::RwLock::new(forge_tts_core::TtsRegistry::new())),
        resolver: Arc::clone(&resolver),
        pipeline: forge_speak_queue::PipelineConfigHandle::new(
            forge_tts_pipeline::PipelineConfig::default(),
        ),
        audio_sink: Arc::new(forge_audio::NullSink),
        event_bus: Arc::new(SilentPublisher),
        disabled_engines: std::collections::HashSet::new(),
        engine_gains: HashMap::new(),
    };
    let (handle, stream) =
        forge_speak_queue::spawn(forge_speak_queue::QueueConfig::default(), deps);
    (handle, stream, resolver)
}

pub(crate) fn live_alias(
    resolver: &LiveAliases,
    viewer_id: &str,
) -> Option<forge_voice::VoiceAlias> {
    resolver
        .read()
        .expect("resolver lock")
        .aliases
        .iter()
        .find(|alias| alias.viewer_id == viewer_id)
        .cloned()
}

pub(crate) type AliasShape = (
    String,
    String,
    String,
    String,
    String,
    Option<f32>,
    Option<f32>,
    forge_voice::AliasState,
);

pub(crate) fn alias_shape(alias: &forge_voice::VoiceAlias) -> AliasShape {
    (
        alias.id.0.clone(),
        alias.viewer_id.clone(),
        alias.viewer_name.clone(),
        alias.engine_id.0.clone(),
        alias.voice_id.0.clone(),
        alias.pitch_semitones,
        alias.rate_multiplier,
        alias.state.clone(),
    )
}

struct TracingInterestFloor;

impl tracing::Subscriber for TracingInterestFloor {
    fn register_callsite(
        &self,
        _: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::sometimes()
    }

    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        false
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, _: &tracing::Event<'_>) {}

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

pub(crate) fn install_tracing_interest_floor() {
    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INSTALLED.get_or_init(|| {
        let _ = tracing::subscriber::set_global_default(TracingInterestFloor);
        tracing::callsite::rebuild_interest_cache();
    });
}

struct CurrentSchemaDatabase;

impl forge_storage::DatabaseFileSchema for CurrentSchemaDatabase {
    fn applied_schema_version(&self, _: &std::path::Path) -> Result<Option<u32>, StorageError> {
        Ok(None)
    }

    fn inspect(
        &self,
        _: &std::path::Path,
    ) -> Result<Option<forge_storage::StoredDatabase>, StorageError> {
        Ok(Some(forge_storage::StoredDatabase {
            schema: forge_storage::StoredSchema::Current {
                version: forge_storage::EXPECTED_SCHEMA_VERSION,
            },
            holds_credentials: false,
        }))
    }
}

fn legacy_backup_bundle(dir: &std::path::Path) -> std::path::PathBuf {
    use std::io::Write;

    let database = b"restored database";
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(database.len() as u64);
    header.set_mode(0o600);
    builder
        .append_data(
            &mut header,
            format!(
                "{}/{}",
                forge_transfer::ARCHIVE_ROOT,
                forge_storage::DATABASE_FILE_NAME
            ),
            &database[..],
        )
        .unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&builder.into_inner().unwrap()).unwrap();
    let bundle = dir.join("legacy.tar.gz");
    std::fs::write(&bundle, gz.finish().unwrap()).unwrap();
    bundle
}

pub(crate) fn staged_restore_in(
    rt: &tokio::runtime::Runtime,
    data_dir: &std::path::Path,
) -> forge_transfer::StagedRestore {
    let bundles = tempfile::TempDir::new().unwrap();
    rt.block_on(forge_transfer::stage_full_restore(
        forge_transfer::StageRequest {
            bundle: legacy_backup_bundle(bundles.path()),
            data_dir: data_dir.to_path_buf(),
            passphrase: None,
            database_schema: Arc::new(CurrentSchemaDatabase),
        },
    ))
    .unwrap()
}
