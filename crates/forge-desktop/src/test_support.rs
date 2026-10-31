mod boot_smoke;
mod config_references;
mod memory_scripts;
pub(crate) mod pack_flow;
mod pack_transfer;
mod setup_transfer;

use forge_types::OverlayCredential;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use forge_audio::{AudioError, AudioSink, ControlledPlayback, PlaybackHandle};
use forge_events::Event;
use forge_overlay::SampleContext;
use forge_registry::ActorRole;
use forge_registry::{
    ActorBlock, ActorIdentity, ChatTriggerFamily, EventFilter, FormField, KindPlatformContract,
    LoginSlot, SubActionRegistry, TriggerCategory, TriggerKindDescriptor, TriggerRegistry,
    TriggerVariables,
};
use forge_runtime::{
    ActionCancelRegistry, Catalog, EventBus, OverlayFrameSink, OverlayReceivers,
    OverlaySampleSource, spawn_action_engine,
};
use forge_server::{ServerConfig, ServerHandle, stopped_server};
use forge_speech_types::PcmBuffer;
use forge_storage::MockTriggerInstanceRepo;
use forge_storage::settings::core_settings::{
    BACKUP_SCHEDULE_ENABLED, BACKUP_SCHEDULE_KEY, BACKUP_SCHEDULE_LAST_SUCCESS_AT,
};
use forge_storage::test_support::{MemorySettings, StoredSettingRow};
use forge_storage::{
    ActionRepo, ActionStats, ActionTelemetry, BanLedgerRepo, CatalogRevision, ChatHistoryRepo,
    CredentialId, CredentialsRepo, DataProvider, DonationRepo, EventLogRepo, ExecutionStatus,
    GlobalEntry, GlobalsRepo, HistoryRepo, LatestValueRepo, MediaBlob, MediaBlobId, MediaReferrer,
    MediaReferrerKind, MediaRepo, OverlayConfig, OverlayDefinition, OverlayId, OverlayRepo,
    QueueRepo, RowClasses, RowGate, ScheduledRunRepo, ScriptRecord, ScriptReferences, ScriptRepo,
    ScriptTelemetry, Setting, SettingCodec, SettingFamily, SettingRow, SettingsRepo,
    SoundboardClipsRepo, StorageError, TriggerInstanceRepo, TtsFiltersRepo, UserGlobalEntry,
    UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
};
use forge_types::{
    Action, ActionId, EventId, ExecutionContext, PlatformId, ScriptId, TriggerConfig, Variant,
};
use time::OffsetDateTime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub(crate) use memory_scripts::MemoryScripts;

const RUNTIME_PUMPS: usize = 8;
const PUMP_HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(30);
const WORKER_DRAIN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);
const WORKER_DRAIN_POLL: std::time::Duration = std::time::Duration::from_millis(1);

pub(crate) type SettingWrite = (String, String);

#[derive(Clone)]
pub(crate) enum ReadFault {
    Fails,
    Held(Arc<tokio::sync::Notify>),
}

pub(crate) const SETTINGS_READ_FAILED: &str = "settings store unreachable";
pub(crate) const SETTINGS_WRITE_FAILED: &str = "settings store is read-only";

pub(crate) struct TestBackend {
    settings: MemorySettings,
    writes: UnboundedSender<SettingWrite>,
    reads: Mutex<HashMap<String, usize>>,
    read_faults: Mutex<HashMap<String, ReadFault>>,
    write_faults: Mutex<std::collections::HashSet<String>>,
    overlays: Option<Arc<dyn OverlayRepo>>,
    scripts: Option<Arc<dyn ScriptRepo>>,
    actions: Option<Arc<dyn ActionRepo>>,
    triggers: Option<Arc<dyn TriggerInstanceRepo>>,
    media: Arc<dyn MediaRepo>,
    export_gate: Option<Arc<tokio::sync::Notify>>,
    export_stalls_midway: bool,
    export_lands_late: bool,
    on_shutdown: Option<Box<dyn Fn() + Send + Sync>>,
}

pub(crate) fn test_backend() -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(None, None, Arc::new(EmptyMediaLibrary))
}

pub(crate) fn test_backend_with_scripts(scripts: Arc<dyn ScriptRepo>) -> Arc<TestBackend> {
    backend_over(None, Some(scripts), Arc::new(EmptyMediaLibrary)).0
}

pub(crate) fn test_backend_with_scripts_and_actions(
    scripts: Arc<dyn ScriptRepo>,
    actions: Arc<dyn ActionRepo>,
) -> Arc<TestBackend> {
    let backend = test_backend_with_scripts(scripts);
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.actions = Some(actions);
    Arc::new(backend)
}

pub(crate) fn test_backend_with_actions_and_triggers(
    actions: Arc<dyn ActionRepo>,
    triggers: Arc<dyn TriggerInstanceRepo>,
) -> Arc<TestBackend> {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.actions = Some(actions);
    backend.triggers = Some(triggers);
    Arc::new(backend)
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

pub(crate) fn test_backend_observing_shutdown(
    on_shutdown: impl Fn() + Send + Sync + 'static,
) -> Arc<TestBackend> {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.on_shutdown = Some(Box::new(on_shutdown));
    Arc::new(backend)
}

pub(crate) fn test_backend_with_gated_export() -> (Arc<TestBackend>, Arc<tokio::sync::Notify>) {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let gate = Arc::new(tokio::sync::Notify::new());
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.export_gate = Some(Arc::clone(&gate));
    (Arc::new(backend), gate)
}

pub(crate) fn test_backend_with_late_export() -> (Arc<TestBackend>, Arc<tokio::sync::Notify>) {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let gate = Arc::new(tokio::sync::Notify::new());
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.export_gate = Some(Arc::clone(&gate));
    backend.export_lands_late = true;
    (Arc::new(backend), gate)
}

pub(crate) fn test_backend_with_stalling_export() -> Arc<TestBackend> {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.export_stalls_midway = true;
    Arc::new(backend)
}

fn backend_over(
    overlays: Option<Arc<dyn OverlayRepo>>,
    scripts: Option<Arc<dyn ScriptRepo>>,
    media: Arc<dyn MediaRepo>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    let (writes, rx) = unbounded_channel();
    (
        Arc::new(TestBackend {
            settings: MemorySettings::new(),
            writes,
            reads: Mutex::default(),
            read_faults: Mutex::default(),
            write_faults: Mutex::default(),
            overlays,
            scripts,
            actions: None,
            triggers: None,
            media,
            export_gate: None,
            export_stalls_midway: false,
            export_lands_late: false,
            on_shutdown: None,
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
    let settings = MemorySettings::new();
    settings.seed(&BACKUP_SCHEDULE_ENABLED, &true).unwrap();
    settings
        .seed(&BACKUP_SCHEDULE_LAST_SUCCESS_AT, &Some(last_success))
        .unwrap();
    if let Some(encoded) = encoded_key {
        settings.seed(&BACKUP_SCHEDULE_KEY, &Some(encoded)).unwrap();
    }
    let backend = Arc::new(TestBackend {
        settings,
        writes,
        reads: Mutex::default(),
        read_faults: Mutex::default(),
        write_faults: Mutex::default(),
        overlays: None,
        scripts: None,
        actions: None,
        triggers: None,
        media: Arc::new(EmptyMediaLibrary),
        export_gate: Some(Arc::clone(&export_gate)),
        export_stalls_midway: false,
        export_lands_late: false,
        on_shutdown: None,
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
    pub(crate) fn seed<C: SettingCodec>(&self, setting: &Setting<C>, value: &C::Value) {
        self.settings.seed(setting, value).unwrap();
    }

    pub(crate) fn reads_of<C: SettingCodec>(&self, setting: &Setting<C>) -> usize {
        self.reads
            .lock()
            .unwrap()
            .get(setting.name())
            .copied()
            .unwrap_or_default()
    }

    pub(crate) fn seed_raw<C: SettingCodec>(&self, setting: &Setting<C>, raw: &str) {
        self.settings.seed_raw(setting, raw);
    }

    pub(crate) fn seed_raw_family<C: SettingCodec>(
        &self,
        family: &SettingFamily<C>,
        member: &str,
        raw: &str,
    ) {
        self.settings.seed_raw_family(family, member, raw).unwrap();
    }

    pub(crate) fn all_rows(&self) -> Vec<StoredSettingRow> {
        self.settings.rows()
    }

    pub(crate) fn row(&self, key: &str) -> Option<StoredSettingRow> {
        self.settings.row(key)
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

    fn guard_write(&self, key: &str) -> Result<(), StorageError> {
        if self.write_faults.lock().unwrap().contains(key) {
            return Err(StorageError::Connection {
                reason: SETTINGS_WRITE_FAILED.to_owned(),
            });
        }
        Ok(())
    }

    fn record_write(&self, key: &str, value: &str) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the receiver may already be gone"
        )]
        let _ = self.writes.send((key.to_owned(), value.to_owned()));
    }
}

#[async_trait::async_trait]
impl SettingsRepo for TestBackend {
    async fn get_raw(&self, gate: RowGate, key: &str) -> Result<Option<String>, StorageError> {
        *self
            .reads
            .lock()
            .unwrap()
            .entry(key.to_owned())
            .or_default() += 1;
        let fault = self.read_faults.lock().unwrap().get(key).cloned();
        match fault {
            Some(ReadFault::Fails) => {
                return Err(StorageError::Connection {
                    reason: SETTINGS_READ_FAILED.to_owned(),
                });
            }
            Some(ReadFault::Held(held)) => held.notified().await,
            None => {}
        }
        self.settings.get_raw(gate, key).await
    }

    async fn delete_raw(&self, gate: RowGate, key: &str) -> Result<bool, StorageError> {
        self.settings.delete_raw(gate, key).await
    }

    async fn load_rows(&self, gate: RowGate) -> Result<Vec<SettingRow>, StorageError> {
        self.settings.load_rows(gate).await
    }

    async fn put_row(
        &self,
        gate: RowGate,
        key: &str,
        value: &str,
        classes: RowClasses,
    ) -> Result<(), StorageError> {
        self.guard_write(key)?;
        self.settings.put_row(gate, key, value, classes).await?;
        self.record_write(key, value);
        Ok(())
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
        if let Some(actions) = &self.actions {
            return Arc::clone(actions);
        }
        if self.scripts.is_some() {
            return Arc::new(StubActions);
        }
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn trigger_instance_repo(&self) -> Arc<dyn TriggerInstanceRepo> {
        if let Some(triggers) = &self.triggers {
            return Arc::clone(triggers);
        }
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

    async fn export(&self, snapshot: &std::path::Path) -> Result<(), StorageError> {
        if self.export_stalls_midway {
            std::fs::write(snapshot, b"partial").unwrap();
            std::future::pending::<()>().await;
        }
        let Some(gate) = &self.export_gate else {
            unreachable!("the settings pane never exports")
        };
        gate.notified().await;
        if self.export_lands_late {
            std::fs::write(snapshot, b"finished too late").unwrap();
            return Ok(());
        }
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

    async fn shutdown(&self) {
        if let Some(on_shutdown) = &self.on_shutdown {
            on_shutdown();
        }
    }
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

    async fn restore(&self, _: &OverlayDefinition) -> Result<(), StorageError> {
        unreachable!("the settings pane never restores an overlay")
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
    filter_source: Option<forge_events::EventSource>,
    filter_prefix: Option<String>,
    refused: Option<(&'static str, &'static str)>,
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
            filter_source: None,
            filter_prefix: None,
            refused: None,
        }
    }

    pub(crate) fn refusing(mut self, key: &'static str, value: &'static str) -> Self {
        self.refused = Some((key, value));
        self
    }

    pub(crate) fn listening_to(
        mut self,
        source: Option<forge_events::EventSource>,
        prefix: &str,
    ) -> Self {
        self.filter_source = source;
        self.filter_prefix = Some(prefix.to_owned());
        self
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

    fn check_config(&self, config: &TriggerConfig) -> Result<(), forge_registry::ConfigProblem> {
        match self.refused {
            Some((key, value))
                if config.get(key).and_then(forge_types::Variant::as_str) == Some(value) =>
            {
                Err(forge_registry::ConfigProblem::field(
                    key,
                    forge_registry::ConfigReason::Required,
                ))
            }
            _ => Ok(()),
        }
    }

    fn chat_trigger_family(&self) -> Option<ChatTriggerFamily> {
        self.chat_family
    }

    fn event_filter(&self) -> EventFilter {
        EventFilter {
            source: self.filter_source,
            kind_prefix: self.filter_prefix.clone(),
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

pub(crate) fn default_twitch_oauth_client() -> forge_platform_twitch::TwitchOAuthClient {
    forge_platform_twitch::TwitchOAuthClient::new(
        forge_platform_twitch::twitch_oauth_gateway(
            &forge_platform_core::PlatformEndpoints::default(),
            Arc::new(forge_net::TokenBucketRateLimiter::new(
                forge_platform_twitch::TWITCH_OAUTH_BUDGET_CAPACITY,
                forge_platform_twitch::TWITCH_OAUTH_BUDGET_WINDOW,
            )),
        )
        .unwrap(),
        "client".to_owned(),
    )
}

struct OfflineHelixTransport;

#[async_trait::async_trait]
impl forge_platform_twitch::HelixTransport for OfflineHelixTransport {
    async fn execute(
        &self,
        _: forge_platform_twitch::HelixRequest,
    ) -> Result<serde_json::Value, forge_platform_twitch::HelixError> {
        Err(forge_platform_twitch::HelixError::Transport(
            "offline".to_owned(),
        ))
    }
}

pub(crate) fn default_twitch_bot(
    creds: Arc<dyn CredentialsRepo>,
    bus: Arc<dyn forge_events::EventPublisher>,
) -> Arc<forge_platform_twitch::TwitchBotAccount> {
    forge_platform_twitch::TwitchBotAccount::new(
        creds,
        default_twitch_oauth_client(),
        Arc::clone(&bus),
        default_twitch_helix_gateway(bus),
        Arc::new(OfflineHelixTransport),
    )
}

pub(crate) fn shown_in_english<F>(failure: F) -> String
where
    F: Into<crate::repo_failure::RepoFailure> + std::fmt::Display,
{
    crate::i18n::install_language(forge_storage::Language::En);
    let diagnostic = failure.to_string();
    let shown = failure.into().text();
    assert!(
        !shown.contains(&diagnostic),
        "the diagnostic reached the user: {shown}"
    );
    shown
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

pub(crate) fn alive_tasks(rt: &tokio::runtime::Runtime) -> usize {
    rt.metrics().num_alive_tasks()
}

pub(crate) fn wait_until_worker_drains_to(rt: &tokio::runtime::Runtime, background_tasks: usize) {
    let deadline = std::time::Instant::now() + WORKER_DRAIN_DEADLINE;
    while alive_tasks(rt) > background_tasks {
        assert!(
            std::time::Instant::now() < deadline,
            "the worker runtime still runs {} tasks, {background_tasks} expected",
            alive_tasks(rt)
        );
        std::thread::sleep(WORKER_DRAIN_POLL);
    }
}

pub(crate) fn stored_shortcut_overrides(
    raw: Option<&str>,
) -> Result<crate::shortcut_overrides::ShortcutOverrides, StorageError> {
    let (backend, _writes) = test_backend();
    if let Some(raw) = raw {
        backend.seed_raw(&crate::shortcut_overrides::KEYBOARD_SHORTCUTS, raw);
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

pub(crate) const FORMAT_ISOLATES: [char; 2] = ['\u{2068}', '\u{2069}'];

pub(crate) fn pump_until(rt: &tokio::runtime::Runtime, mut reached: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + PUMP_HANG_GUARD;
    while !reached() {
        assert!(
            std::time::Instant::now() < deadline,
            "the awaited state never arrived"
        );
        pump(rt);
    }
}

pub(crate) fn settle_until(
    cx: &mut gpui::TestAppContext,
    rt: &tokio::runtime::Runtime,
    mut reached: impl FnMut(&mut gpui::TestAppContext) -> bool,
) {
    let deadline = std::time::Instant::now() + PUMP_HANG_GUARD;
    loop {
        cx.run_until_parked();
        if reached(cx) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the awaited state never arrived"
        );
        pump(rt);
    }
}

pub(crate) fn settle_until_tasks_drain_to(
    cx: &mut gpui::TestAppContext,
    rt: &tokio::runtime::Runtime,
    background_tasks: usize,
) {
    settle_until(cx, rt, |_| alive_tasks(rt) <= background_tasks);
    cx.run_until_parked();
}

pub(crate) fn serial_blocking_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .expect("current-thread runtime")
}

pub(crate) fn settle_until_blocking_work_finishes(
    cx: &mut gpui::TestAppContext,
    rt: &tokio::runtime::Runtime,
) {
    cx.run_until_parked();
    pump(rt);
    rt.block_on(within_hang_guard(async {
        tokio::task::spawn_blocking(|| ()).await.expect("fence");
    }));
    pump(rt);
    cx.run_until_parked();
}

pub(crate) async fn within_hang_guard<F: std::future::Future>(awaited: F) -> F::Output {
    tokio::time::timeout(PUMP_HANG_GUARD, awaited)
        .await
        .expect("the awaited state never arrived")
}

pub(crate) async fn yield_until(mut reached: impl FnMut() -> bool) {
    within_hang_guard(async {
        while !reached() {
            tokio::task::yield_now().await;
        }
    })
    .await;
}

pub(crate) struct StalledDownloader;

#[async_trait::async_trait]
impl crate::update_flow::ArtifactDownloader for StalledDownloader {
    async fn download(
        &self,
        _artifact: &forge_update::Artifact,
        _staging_dir: &std::path::Path,
    ) -> Result<forge_update::StagedArtifact, forge_update::UpdateError> {
        std::future::pending().await
    }
}

pub(crate) struct NeutralOverlaySamples;

#[async_trait::async_trait]
impl OverlaySampleSource for NeutralOverlaySamples {
    async fn sample(&self, _: &OverlayId) -> SampleContext {
        SampleContext::neutral()
    }
}

pub(crate) struct NoOverlayPages;

#[async_trait::async_trait]
impl OverlayFrameSink for NoOverlayPages {
    async fn deliver_content(
        &self,
        _: &OverlayId,
        _: serde_json::Value,
        _: Option<u64>,
    ) -> OverlayReceivers {
        OverlayReceivers::default()
    }

    async fn deliver_reload(&self, _: &OverlayId) {}

    async fn revoke(&self, _: &OverlayId) {}
}

pub(crate) async fn stopped_server_handle(backend: &Arc<TestBackend>) -> ServerHandle {
    stopped_server(server_config(backend))
        .await
        .expect("a stopped server handle")
}

pub(crate) async fn stopped_server_hosting(
    backend: &Arc<TestBackend>,
    overlay_root: std::path::PathBuf,
) -> ServerHandle {
    let mut config = server_config(backend);
    config.overlay_root = overlay_root;
    stopped_server(config)
        .await
        .expect("a stopped server handle")
}

fn server_config(backend: &Arc<TestBackend>) -> ServerConfig {
    let bus = EventBus::new(Arc::new(StubEventLog));
    let engine = Arc::new(spawn_action_engine(
        Arc::clone(&bus),
        stub_catalog(),
        Arc::new(StubActions),
        Arc::new(StubHistory),
        Arc::new(SubActionRegistry::new()),
        Arc::new(ActionCancelRegistry::new()),
    ));
    ServerConfig::new(
        Arc::clone(backend) as Arc<dyn SettingsRepo>,
        Arc::clone(backend) as Arc<dyn CredentialsRepo>,
        bus,
        Arc::new(StubActions),
        Arc::clone(backend) as Arc<dyn GlobalsRepo>,
        Arc::clone(backend) as Arc<dyn UserGlobalsRepo>,
        Arc::new(StubOverlays),
        engine,
    )
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

pub(crate) fn sandboxed_over(provider: Arc<dyn DataProvider>) -> Sandboxed<Arc<dyn DataProvider>> {
    Sandboxed {
        inner: provider,
        _media_root: tempfile::tempdir().expect("a temporary media root"),
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
    held_listings: Mutex<std::collections::VecDeque<HeldListing>>,
    released_listings: AtomicUsize,
    lists: AtomicUsize,
    signal: forge_platform_core::CollectionRevisionSignal,
}

type HeldListing = (
    tokio::sync::oneshot::Receiver<()>,
    forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
);

impl FakeCollections {
    pub(crate) fn listing(
        listing: forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            listing: Mutex::new(listing),
            held_listings: Mutex::default(),
            released_listings: AtomicUsize::new(0),
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

    pub(crate) fn hold_next_listing(
        &self,
        listing: forge_platform_core::CollectionOutcome<Vec<forge_platform_core::CollectionItem>>,
    ) -> tokio::sync::oneshot::Sender<()> {
        let (release, released) = tokio::sync::oneshot::channel();
        self.held_listings
            .lock()
            .unwrap()
            .push_back((released, listing));
        release
    }

    pub(crate) fn bump(&self) {
        self.signal.bump();
    }

    pub(crate) fn list_calls(&self) -> usize {
        self.lists.load(Ordering::SeqCst)
    }

    pub(crate) fn released_listings(&self) -> usize {
        self.released_listings.load(Ordering::SeqCst)
    }

    pub(crate) fn metadata() -> forge_platform_core::CollectionMetadata {
        forge_platform_core::CollectionMetadata {
            id: forge_platform_core::CollectionId::new(REWARDS),
            label: forge_types::VerbatimText::new("Channel point rewards").into(),
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
            reason: forge_types::VerbatimText::new("created elsewhere").into(),
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
        let held = self.held_listings.lock().unwrap().pop_front();
        if let Some((released, listing)) = held {
            released.await.unwrap();
            self.released_listings.fetch_add(1, Ordering::SeqCst);
            return listing;
        }
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
    ) -> Result<
        forge_runtime::RunningIntegration<crate::integrations::BuiltinObject>,
        forge_runtime::IntegrationStartError,
    > {
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
        .link_action(action, trigger)
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
    screens: Vec<crate::navigation::Screen>,
    _sub: gpui::Subscription,
}

impl NavLog {
    pub(crate) fn screens(&self) -> Vec<crate::navigation::Screen> {
        self.screens.clone()
    }
}

pub(crate) fn nav_log<V: gpui::EventEmitter<crate::navigation::NavRequested>>(
    view: &gpui::Entity<V>,
    cx: &mut gpui::App,
) -> gpui::Entity<NavLog> {
    gpui::AppContext::new(cx, |cx| NavLog {
        screens: Vec::new(),
        _sub: cx.subscribe(
            view,
            |log: &mut NavLog, _, event: &crate::navigation::NavRequested, _| {
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

struct BlankWindow;

impl gpui::Render for BlankWindow {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::Empty
    }
}

pub(crate) fn press_undo(cx: &mut gpui::TestAppContext) {
    use gpui::AppContext as _;

    let action = cx.update(|cx| {
        let toasts = cx.global_mut::<forge_components::Toasts>();
        let id = toasts
            .items()
            .iter()
            .find(|toast| toast.kind == forge_components::ToastKind::Undo)
            .map(|toast| toast.id)
            .expect("an undo toast");
        toasts
            .take(id)
            .and_then(|toast| toast.action)
            .expect("an undo action")
    });
    let window: gpui::AnyWindowHandle = cx.add_window(|_, _| BlankWindow).into();
    cx.update(|cx| {
        cx.update_window(window, |_, window, cx| (action.on_action)(window, cx))
            .unwrap();
    });
}

pub(crate) fn blank_window(cx: &mut gpui::TestAppContext) {
    cx.add_window(|_, _| BlankWindow);
}

pub(crate) fn install_presentation(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(forge_components::Presentation::new(
            forge_components::ThemeId::ForgeDefault,
            forge_components::Density::Cozy,
        ))
    });
}

const OUTSIDE_TOOLBAR_HEIGHT: f32 = 40.0;
const FOCUS_HOPS_AFTER_A_CLICK: usize = 3;

pub(crate) struct ScreenUnderRoot<V> {
    root: gpui::FocusHandle,
    screen: gpui::Entity<V>,
    _forward: gpui::Subscription,
}

impl<V: gpui::Render> gpui::Render for ScreenUnderRoot<V> {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::{InteractiveElement, ParentElement, Styled, div, px};
        div()
            .track_focus(&self.root)
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_none().w_full().h(px(OUTSIDE_TOOLBAR_HEIGHT)))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(self.screen.clone()),
            )
    }
}

pub(crate) fn open_under_root<V: gpui::Render>(
    cx: &mut gpui::TestAppContext,
    screen: impl FnOnce(&mut gpui::App) -> gpui::Entity<V>,
) -> (gpui::Entity<V>, &mut gpui::VisualTestContext) {
    cx.update(crate::actions::register_key_bindings);
    let (root, vcx) = cx.add_window_view(|window, cx| {
        let root = cx.focus_handle();
        let forward = crate::navigable_list::forward_root_focus_to_list(&root, window, cx);
        ScreenUnderRoot {
            root,
            screen: screen(cx),
            _forward: forward,
        }
    });
    vcx.update(|window, _| window.activate_window());
    settle_focus(vcx);
    let screen = root.read_with(vcx, |root, _| root.screen.clone());
    (screen, vcx)
}

pub(crate) fn settle_focus(vcx: &mut gpui::VisualTestContext) {
    for _ in 0..FOCUS_HOPS_AFTER_A_CLICK {
        vcx.run_until_parked();
        vcx.update(|_, _| {});
    }
    vcx.run_until_parked();
}

pub(crate) fn click_outside_the_list(vcx: &mut gpui::VisualTestContext) {
    vcx.simulate_click(
        gpui::point(gpui::px(1.0), gpui::px(OUTSIDE_TOOLBAR_HEIGHT / 2.0)),
        gpui::Modifiers::none(),
    );
    settle_focus(vcx);
}

pub(crate) fn click_until(
    vcx: &mut gpui::VisualTestContext,
    area: &ClickArea,
    mut done: impl FnMut(&mut gpui::VisualTestContext) -> bool,
) -> bool {
    let mut y = area.y.start + area.step_y / 2.0;
    while y < area.y.end {
        let mut x = area.x.start + area.step_x / 2.0;
        while x < area.x.end {
            vcx.simulate_click(
                gpui::point(gpui::px(x), gpui::px(y)),
                gpui::Modifiers::none(),
            );
            settle_focus(vcx);
            if done(vcx) {
                return true;
            }
            x += area.step_x;
        }
        y += area.step_y;
    }
    false
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

pub(crate) fn click_selector(vcx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("nothing painted the selector {selector}"));
    vcx.simulate_click(bounds.center(), gpui::Modifiers::none());
}

pub(crate) fn hub_crumb_targets<V>(
    cx: &mut gpui::TestAppContext,
    category: Option<forge_platform_core::IntegrationCategory>,
    build: impl FnOnce(&mut gpui::Window, &mut gpui::Context<V>) -> V,
) -> Vec<crate::navigation::Screen>
where
    V: gpui::Render + gpui::EventEmitter<crate::navigation::NavRequested>,
{
    install_presentation(cx);
    let (view, vcx) = cx.add_window_view(build);
    let log = vcx.update(|_, cx| nav_log(&view, cx));
    vcx.simulate_resize(gpui::size(
        gpui::px(CRUMB_WINDOW_W),
        gpui::px(CRUMB_WINDOW_H),
    ));
    vcx.run_until_parked();
    click_selector(vcx, crate::hub_crumb::hub_crumb_id(category).leak());
    log.read_with(vcx, |log, _| log.screens())
}

pub(crate) fn quiet_bus(rt: &tokio::runtime::Runtime) -> Arc<EventBus> {
    rt.block_on(async { EventBus::new(Arc::new(StubEventLog)) })
}

pub(crate) type LiveAliases = Arc<std::sync::RwLock<forge_speak_queue::VoiceAliasResolver>>;

pub(crate) struct SilentPublisher;

impl forge_events::EventPublisher for SilentPublisher {
    fn publish(&self, _: Event) {}
}

pub(crate) fn spawn_speak_queue(
    aliases: Vec<forge_speech_types::VoiceAlias>,
) -> (
    forge_speak_queue::SpeakQueueHandle,
    forge_speak_queue::SpeakEventStream,
    LiveAliases,
) {
    let resolver = Arc::new(std::sync::RwLock::new(
        forge_speak_queue::VoiceAliasResolver::new(
            aliases,
            forge_speech_types::AssignmentStrategy::DeterministicByName,
            forge_speech_types::IgnoreProfile::default(),
            forge_speech_types::SynthesisDefaults::default(),
        ),
    ));
    let deps = forge_speak_queue::QueueDeps {
        registry: Arc::new(std::sync::RwLock::new(forge_tts_core::TtsRegistry::new())),
        resolver: Arc::clone(&resolver),
        pipeline: forge_speak_queue::PipelineConfigHandle::new(neutral_pipeline_config()),
        audio_sink: Arc::new(forge_audio::NullSink),
        event_bus: Arc::new(SilentPublisher),
        disabled_engines: std::collections::HashSet::new(),
        engine_gains: HashMap::new(),
    };
    let (handle, stream) =
        forge_speak_queue::spawn(forge_speak_queue::QueueConfig::default(), deps);
    (handle, stream, resolver)
}

struct ListedVoices {
    id: forge_speech_types::EngineId,
    voices: Vec<forge_speech_types::TtsVoice>,
}

#[async_trait::async_trait]
impl forge_tts_core::TtsEngine for ListedVoices {
    fn engine_id(&self) -> &forge_speech_types::EngineId {
        &self.id
    }

    fn capabilities(&self) -> &forge_tts_core::EngineCapabilities {
        static CAPABILITIES: forge_tts_core::EngineCapabilities =
            forge_tts_core::EngineCapabilities {
                ssml: false,
                neural_voices: false,
                streaming: false,
                custom_lexicons: false,
            };
        &CAPABILITIES
    }

    async fn list_voices(
        &self,
    ) -> Result<Vec<forge_speech_types::TtsVoice>, forge_tts_core::TtsError> {
        Ok(self.voices.clone())
    }

    async fn synthesize(
        &self,
        _request: forge_tts_core::SynthesisRequest,
    ) -> Result<PcmBuffer, forge_tts_core::TtsError> {
        Ok(PcmBuffer::new(vec![0i16; 8], 22_050, 1))
    }
}

struct ListedVoicesFactory(
    forge_speech_types::EngineId,
    Vec<forge_speech_types::TtsVoice>,
);

impl forge_tts_core::TtsEngineFactory for ListedVoicesFactory {
    fn class(&self) -> forge_tts_core::EngineClass {
        forge_tts_core::EngineClass::Local
    }

    fn create(&self) -> Result<Box<dyn forge_tts_core::TtsEngine>, forge_tts_core::TtsError> {
        Ok(Box::new(ListedVoices {
            id: self.0.clone(),
            voices: self.1.clone(),
        }))
    }
}

pub(crate) fn spawn_speak_queue_listing(
    rt: &tokio::runtime::Runtime,
    engine: &forge_speech_types::EngineId,
    voices: Vec<forge_speech_types::TtsVoice>,
) -> forge_speak_queue::SpeakQueueHandle {
    let expected = voices.len();
    let mut registry = forge_tts_core::TtsRegistry::new();
    registry.register(
        engine.clone(),
        Arc::new(ListedVoicesFactory(engine.clone(), voices)),
    );
    let resolver = Arc::new(std::sync::RwLock::new(
        forge_speak_queue::VoiceAliasResolver::new(
            Vec::new(),
            forge_speech_types::AssignmentStrategy::DeterministicByName,
            forge_speech_types::IgnoreProfile::default(),
            forge_speech_types::SynthesisDefaults::default(),
        ),
    ));
    let deps = forge_speak_queue::QueueDeps {
        registry: Arc::new(std::sync::RwLock::new(registry)),
        resolver,
        pipeline: forge_speak_queue::PipelineConfigHandle::new(neutral_pipeline_config()),
        audio_sink: Arc::new(forge_audio::NullSink),
        event_bus: Arc::new(SilentPublisher),
        disabled_engines: std::collections::HashSet::new(),
        engine_gains: HashMap::new(),
    };
    let (handle, _events) = rt.block_on(async {
        forge_speak_queue::spawn(forge_speak_queue::QueueConfig::default(), deps)
    });
    pump_until(rt, || handle.available_voices().len() == expected);
    handle
}

pub(crate) fn live_alias(
    resolver: &LiveAliases,
    viewer_id: &str,
) -> Option<forge_speech_types::VoiceAlias> {
    resolver
        .read()
        .expect("resolver lock")
        .aliases()
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
    forge_speech_types::AliasState,
);

pub(crate) fn alias_shape(alias: &forge_speech_types::VoiceAlias) -> AliasShape {
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

pub(crate) fn neutral_pipeline_config() -> forge_tts_pipeline::PipelineConfig {
    forge_tts_pipeline::PipelineConfig {
        emote_tokens: forge_tts_pipeline::EmoteTokenSet::default(),
        skip_rules: forge_tts_pipeline::SkipRulesConfig::default(),
        replacement_rules: Vec::new(),
        word_blocklist: Vec::new(),
        blocklist_mode: forge_tts_pipeline::BlocklistMode::Censor,
        output: forge_tts_pipeline::OutputConfig {
            read_display_name_first: false,
            emote_treatment: forge_tts_pipeline::EmoteTreatment::Keep,
            strip_emoji: false,
            sanitize_punctuation: false,
            max_duration_secs: None,
            language_aware_voice: false,
        },
        strip_reward_emotes: false,
    }
}

pub(crate) fn install_tracing_interest_floor() {
    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INSTALLED.get_or_init(|| {
        drop(tracing::subscriber::set_global_default(
            TracingInterestFloor,
        ));
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

pub(crate) fn start_failure(detail: &str) -> forge_runtime::IntegrationStartError {
    forge_runtime::IntegrationStartError::new(
        forge_runtime::StartFailureCategory::Internal,
        forge_types::MessageRef::upstream_failure(forge_types::UpstreamDetail::from_redacted(
            detail,
        )),
    )
}
