#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures fail loudly by design"
)]

mod booted;
mod logs;
mod pumps;
mod schedules;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use forge_audio::{AudioError, AudioSink, ControlledPlayback, PlaybackHandle};
use forge_events::Event;
use forge_platform_core::PlatformEndpoints;
use forge_platform_core::PlatformError;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::DonationIngest;
use forge_runtime::{ActionCancelRegistry, Catalog, EventBus, spawn_action_engine};
use forge_server::{ServerConfig, ServerHandle, stopped_server};
use forge_speech_types::PcmBuffer;
use forge_storage::MockTriggerInstanceRepo;
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
    Action, ActionId, EventId, ExecutionContext, OverlayCredential, ScriptId, Variant,
};
use time::OffsetDateTime;

use crate::integration_factories::{wire_discord, wire_midi, wire_obs, wire_vtube};
use crate::integrations::{creds_of, publisher, register_platform_triggers};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

pub use booted::{
    ALL_INTEGRATIONS, BootedHost, RecordingSinkFactory, boot_host, fake_identities, guarded,
    on_app_runtime, open_store, runs_here_in_isolation, seed_integrations_and_server_off,
};
pub use logs::{console_line, install_tracing_interest_floor, signal_fields};
pub use pumps::{alive_tasks, pump, pump_until, runtime};
pub use schedules::{
    GatedSchedule, gated_running_schedule, gated_running_schedule_sharing, keyless_enabled_schedule,
};

const PUMP_HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(90);

pub type SettingWrite = (String, String);

#[derive(Clone)]
pub enum ReadFault {
    Fails,
    Held(Arc<tokio::sync::Notify>),
}

pub const SETTINGS_READ_FAILED: &str = "settings store unreachable";
pub const SETTINGS_WRITE_FAILED: &str = "settings store is read-only";

pub struct TestBackend {
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

pub fn test_backend() -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(None, None, Arc::new(EmptyMediaLibrary))
}

pub fn test_backend_with_scripts(scripts: Arc<dyn ScriptRepo>) -> Arc<TestBackend> {
    backend_over(None, Some(scripts), Arc::new(EmptyMediaLibrary)).0
}

pub fn test_backend_with_scripts_and_actions(
    scripts: Arc<dyn ScriptRepo>,
    actions: Arc<dyn ActionRepo>,
) -> Arc<TestBackend> {
    let backend = test_backend_with_scripts(scripts);
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.actions = Some(actions);
    Arc::new(backend)
}

pub fn test_backend_with_actions_and_triggers(
    actions: Arc<dyn ActionRepo>,
    triggers: Arc<dyn TriggerInstanceRepo>,
) -> Arc<TestBackend> {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.actions = Some(actions);
    backend.triggers = Some(triggers);
    Arc::new(backend)
}

pub fn test_backend_with_overlays(
    overlays: Arc<dyn OverlayRepo>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(Some(overlays), None, Arc::new(EmptyMediaLibrary))
}

pub fn test_backend_with_media(
    media: Arc<dyn MediaRepo>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    backend_over(None, None, media)
}

pub fn test_backend_observing_shutdown(
    on_shutdown: impl Fn() + Send + Sync + 'static,
) -> Arc<TestBackend> {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.on_shutdown = Some(Box::new(on_shutdown));
    Arc::new(backend)
}

pub fn test_backend_with_gated_export() -> (Arc<TestBackend>, Arc<tokio::sync::Notify>) {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let gate = Arc::new(tokio::sync::Notify::new());
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.export_gate = Some(Arc::clone(&gate));
    (Arc::new(backend), gate)
}

pub fn test_backend_with_late_export() -> (Arc<TestBackend>, Arc<tokio::sync::Notify>) {
    let (backend, _) = backend_over(None, None, Arc::new(EmptyMediaLibrary));
    let gate = Arc::new(tokio::sync::Notify::new());
    let mut backend = Arc::into_inner(backend).unwrap();
    backend.export_gate = Some(Arc::clone(&gate));
    backend.export_lands_late = true;
    (Arc::new(backend), gate)
}

pub fn test_backend_with_stalling_export() -> Arc<TestBackend> {
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

pub fn writes_to(writes: &mut UnboundedReceiver<SettingWrite>, key: &str) -> Vec<String> {
    let mut values = Vec::new();
    while let Ok((written, value)) = writes.try_recv() {
        if written == key {
            values.push(value);
        }
    }
    values
}

impl TestBackend {
    pub fn with_gated_export(
        settings: MemorySettings,
        export_gate: Arc<tokio::sync::Notify>,
    ) -> Arc<Self> {
        let (writes, _) = unbounded_channel();
        Arc::new(Self {
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
            export_gate: Some(export_gate),
            export_stalls_midway: false,
            export_lands_late: false,
            on_shutdown: None,
        })
    }

    pub fn seed<C: SettingCodec>(&self, setting: &Setting<C>, value: &C::Value) {
        self.settings.seed(setting, value).unwrap();
    }

    pub fn reads_of<C: SettingCodec>(&self, setting: &Setting<C>) -> usize {
        self.reads
            .lock()
            .unwrap()
            .get(setting.name())
            .copied()
            .unwrap_or_default()
    }

    pub fn seed_raw<C: SettingCodec>(&self, setting: &Setting<C>, raw: &str) {
        self.settings.seed_raw(setting, raw);
    }

    pub fn seed_raw_family<C: SettingCodec>(
        &self,
        family: &SettingFamily<C>,
        member: &str,
        raw: &str,
    ) {
        self.settings.seed_raw_family(family, member, raw).unwrap();
    }

    pub fn all_rows(&self) -> Vec<StoredSettingRow> {
        self.settings.rows()
    }

    pub fn row(&self, key: &str) -> Option<StoredSettingRow> {
        self.settings.row(key)
    }

    pub fn fault_reads(&self, key: &str, fault: ReadFault) {
        self.read_faults
            .lock()
            .unwrap()
            .insert(key.to_owned(), fault);
    }

    pub fn heal_reads(&self, key: &str) {
        self.read_faults.lock().unwrap().remove(key);
    }

    pub fn fail_writes(&self, key: &str) {
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

pub struct EmptyMediaLibrary;

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

pub struct StubEventLog;

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

pub struct StubOverlays;

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

pub struct StubActions;

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

pub fn stub_catalog() -> Arc<Catalog> {
    Catalog::new(
        Arc::new(StubActions),
        Arc::new(MockTriggerInstanceRepo::new()),
        CatalogRevision::new(),
    )
}

pub struct StubHistory;

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

pub fn overlay_definition(kind_id: &str, config: OverlayConfig) -> OverlayDefinition {
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

pub fn overlay_named(id: &str, kind_id: &str) -> OverlayDefinition {
    OverlayDefinition {
        id: OverlayId::new(id),
        display_name: format!("{id} overlay"),
        ..overlay_definition(kind_id, OverlayConfig::new())
    }
}

pub async fn within_hang_guard<F: std::future::Future>(awaited: F) -> F::Output {
    tokio::time::timeout(PUMP_HANG_GUARD, awaited)
        .await
        .expect("the awaited state never arrived")
}

pub async fn yield_until(mut reached: impl FnMut() -> bool) {
    within_hang_guard(async {
        while !reached() {
            tokio::task::yield_now().await;
        }
    })
    .await;
}

pub async fn stopped_server_handle(backend: &Arc<TestBackend>) -> ServerHandle {
    stopped_server(server_config(backend))
        .await
        .expect("a stopped server handle")
}

pub async fn stopped_server_hosting(
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

pub fn stopped_handle(rt: &tokio::runtime::Runtime, backend: &Arc<TestBackend>) -> ServerHandle {
    rt.block_on(stopped_server_handle(backend))
}

#[derive(Default)]
pub struct RecordingSink {
    plays: AtomicUsize,
    stoppables: AtomicUsize,
    controlled: AtomicUsize,
}

impl RecordingSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn plays(&self) -> usize {
        self.plays.load(Ordering::SeqCst)
    }

    pub fn stoppables(&self) -> usize {
        self.stoppables.load(Ordering::SeqCst)
    }

    pub fn controlled(&self) -> usize {
        self.controlled.load(Ordering::SeqCst)
    }

    pub fn calls(&self) -> usize {
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

pub fn tone() -> PcmBuffer {
    PcmBuffer::new(vec![0_i16; 16], 48_000, 1)
}

pub struct SilentPublisher;

impl forge_events::EventPublisher for SilentPublisher {
    fn publish(&self, _: Event) {}
}

pub fn neutral_pipeline_config() -> forge_tts_pipeline::PipelineConfig {
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

pub fn default_twitch_helix_gateway(
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

pub fn default_twitch_oauth_client() -> forge_platform_twitch::TwitchOAuthClient {
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
    ) -> Result<serde_json::Value, forge_platform_twitch::TwitchError> {
        Err(forge_platform_twitch::TwitchError::Transport(
            "offline".to_owned(),
        ))
    }
}

pub fn default_twitch_bot(
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

pub struct SilentSoundboard;

#[async_trait::async_trait]
impl forge_soundboard::SoundboardControl for SilentSoundboard {
    async fn play(
        &self,
        _clip_id: forge_types::ClipId,
        _output_device_override: Option<forge_types::OutputDevice>,
    ) -> Result<(), forge_soundboard::SoundboardError> {
        Ok(())
    }

    async fn stop(
        &self,
        _clip_id: forge_types::ClipId,
    ) -> Result<(), forge_soundboard::SoundboardError> {
        Ok(())
    }

    async fn stop_all(&self) -> Result<(), forge_soundboard::SoundboardError> {
        Ok(())
    }

    async fn set_master_volume(&self, _gain: f32) -> Result<(), forge_soundboard::SoundboardError> {
        Ok(())
    }
}

struct SilentSpeaker;

#[async_trait::async_trait]
impl forge_runtime::SpeakDispatcher for SilentSpeaker {
    async fn speak(
        &self,
        _text: String,
        _voice: Option<String>,
        _origin: forge_runtime::SpeechOrigin,
    ) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }
}

struct NoLatestValues;

impl forge_types::LatestValueReader for NoLatestValues {
    fn latest(
        &self,
        _slot: &str,
        _scope: forge_types::LatestScope<'_>,
    ) -> Option<forge_types::LatestValue> {
        None
    }
}

pub async fn every_registry_entry() -> (Arc<TriggerRegistry>, Arc<SubActionRegistry>) {
    let sandbox =
        forge_storage_sqlite::test_support::sandboxed_backend_at("sqlite::memory:", [0x11; 32])
            .await
            .map(|backend| Arc::new(backend) as Arc<dyn DataProvider>);
    let backend = Arc::clone(&*sandbox);
    let bus = EventBus::new(Arc::new(forge_runtime::NullEventLogRepo));
    let latest_values = forge_runtime::LatestValues::load(
        backend.latest_value_repo(),
        publisher(&bus),
        forge_types::Shared::new(String::new()),
    )
    .await;
    let hub =
        forge_server::DeliveryHub::new(Arc::clone(&bus) as Arc<dyn forge_events::EventObserver>);
    let mut wiring = forge_runtime::RuntimeWiring::new(forge_runtime::RuntimeWiringParts {
        tasks: forge_net::TaskOwner::new(),
        bus: Arc::clone(&bus),
        provider: Arc::clone(&backend),
        latest_values,
        script_speech: None,
        overlay_speech: None,
        overlay_kinds: Arc::new(forge_overlay::OverlayKindRegistry::new()),
        overlay_frames: Arc::new(hub) as Arc<dyn forge_runtime::OverlayFrameSink>,
        clock: Arc::new(forge_types::SystemWallClock),
    });
    wiring.register_core_sub_actions().unwrap();
    wiring.register_scheduled_run_sub_actions().unwrap();
    forge_runtime::register_core_triggers(wiring.triggers_mut()).unwrap();
    let (sub_actions, triggers) = wiring.registries_mut();
    register_platform_triggers(triggers);
    wire_obs(&mut *sub_actions, &backend, &bus, Default::default());
    wire_vtube(&mut *sub_actions, &backend, &bus, Default::default());
    wire_discord(
        &mut *sub_actions,
        &backend,
        &bus,
        &PlatformEndpoints::default(),
    );
    wire_midi(&mut *sub_actions, &backend, &bus, Vec::new());
    register_twitch_runners(&mut *sub_actions, &backend, &bus);
    register_youtube_runners(&mut *sub_actions, &bus);
    register_kick_runners(&mut *sub_actions, &bus);
    forge_soundboard::register_soundboard_sub_actions(
        &mut *sub_actions,
        Arc::new(SilentSoundboard),
    )
    .unwrap();
    forge_runtime::register_speech_sub_actions(
        &mut *sub_actions,
        Arc::new(SilentSpeaker),
        forge_runtime::TwitchEmoteLexicon::default(),
    )
    .unwrap();
    forge_runtime::register_donation_sub_actions(
        &mut *sub_actions,
        Arc::new(DonationIngest::new(
            backend.donation_repo(),
            publisher(&bus),
            forge_types::Shared::new(String::new()),
        )),
    )
    .unwrap();
    forge_runtime::register_latest_sub_actions(&mut *sub_actions, Arc::new(NoLatestValues))
        .unwrap();
    let frozen = wiring.freeze().await;
    (frozen.trigger_registry, frozen.sub_action_registry)
}

fn register_twitch_runners(
    sub_actions: &mut SubActionRegistry,
    backend: &Arc<dyn DataProvider>,
    bus: &Arc<EventBus>,
) {
    let creds = creds_of(backend);
    let manager = Arc::new(forge_platform_twitch::TwitchCredentialsManager::new(
        default_twitch_oauth_client(),
        Arc::clone(&creds),
    ));
    let transport: Arc<dyn forge_platform_twitch::HelixTransport> =
        Arc::new(forge_platform_twitch::HelixHttpTransport::new(
            default_twitch_helix_gateway(publisher(bus)),
            "test-client".to_owned(),
            manager as Arc<dyn forge_platform_twitch::HelixTokenSource>,
        ));
    forge_platform_twitch::register_twitch_sub_actions(
        sub_actions,
        transport,
        creds,
        forge_platform_twitch::TwitchLifecycle::new(),
        None,
    )
    .unwrap();
}

fn youtube_token_source() -> Arc<
    dyn Fn() -> futures_util::future::BoxFuture<'static, Result<String, PlatformError>>
        + Send
        + Sync,
> {
    Arc::new(|| Box::pin(async { Ok("token".to_owned()) }))
}

fn register_youtube_runners(sub_actions: &mut SubActionRegistry, bus: &Arc<EventBus>) {
    use forge_platform_youtube as yt;
    let api = yt::YoutubeDataApi::new(
        &forge_platform_core::PlatformEndpoints::default(),
        yt::SharedQuota::default(),
        publisher(bus),
    )
    .unwrap();
    let chat = yt::LiveChatIdHandle::new();
    let broadcast = yt::ActiveBroadcastIdHandle::new();
    yt::register_youtube_sub_actions(
        sub_actions,
        Arc::new(yt::YoutubeSendChat::new(
            api.clone(),
            youtube_token_source(),
            chat.clone(),
        )),
        Arc::new(yt::YoutubeModeration::new(
            api.clone(),
            youtube_token_source(),
            Arc::new(|| {
                Box::pin(async {
                    Ok(yt::YoutubeBroadcaster {
                        channel_id: "UCtest".to_owned(),
                        channel_title: "Test".to_owned(),
                    })
                })
            }),
            chat,
            Arc::new(forge_storage::MockBanLedgerRepo::new()),
        )),
        Arc::new(yt::YoutubeStreamMetadata::new(
            api.clone(),
            youtube_token_source(),
            broadcast.clone(),
        )),
        Arc::new(yt::YoutubeStreamStats::new(
            api.clone(),
            youtube_token_source(),
            broadcast.clone(),
        )),
        Arc::new(yt::YoutubeAdBreak::new(
            api.clone(),
            youtube_token_source(),
            broadcast.clone(),
        )),
        Arc::new(yt::YoutubeThumbnail::new(
            api.clone(),
            youtube_token_source(),
            broadcast,
        )),
        Arc::new(yt::YoutubeChannelLookup::new(api, youtube_token_source())),
    )
    .unwrap();
}

fn register_kick_runners(sub_actions: &mut SubActionRegistry, bus: &Arc<EventBus>) {
    use forge_platform_kick as kick;
    let api = kick::kick_api_gateway(
        &forge_platform_core::PlatformEndpoints::default(),
        Arc::new(forge_net::test_support::RecordingRateLimiter::granting()),
        publisher(bus),
    )
    .unwrap();
    kick::register_kick_sub_actions(
        sub_actions,
        kick::KickSubActionDeps {
            client: Arc::new(kick::KickSendChat::new(api.clone())),
            token_source: Arc::new(|| Box::pin(async { Ok("token".to_owned()) })),
            broadcaster_id_source: Arc::new(|| Box::pin(async { Ok(1) })),
            moderation: Arc::new(kick::KickModeration::new(
                api.clone(),
                Arc::new(forge_storage::MockBanLedgerRepo::new()),
            )),
            channel: Arc::new(kick::KickChannel::new(api.clone())),
            rewards: Arc::new(kick::KickRewards::new(api.clone())),
            categories: Arc::new(kick::KickCategories::new(api)),
        },
    )
    .unwrap();
}

pub fn kick_manager_over(
    repo: Arc<dyn CredentialsRepo>,
) -> Arc<forge_platform_kick::KickCredentialsManager> {
    fn limiter() -> Arc<dyn forge_net::RateLimiter> {
        Arc::new(forge_net::TokenBucketRateLimiter::new(
            forge_platform_twitch::TWITCH_OAUTH_BUDGET_CAPACITY,
            forge_platform_twitch::TWITCH_OAUTH_BUDGET_WINDOW,
        ))
    }
    let endpoints = PlatformEndpoints::default();
    Arc::new(forge_platform_kick::KickCredentialsManager::new(
        repo,
        forge_platform_kick::KickOAuthClient::new(
            &endpoints,
            forge_platform_kick::kick_oauth_gateway(&endpoints, limiter()).unwrap(),
            "client".to_owned(),
            forge_types::Secret::new("secret"),
        ),
        forge_platform_kick::kick_api_gateway(&endpoints, limiter(), Arc::new(SilentPublisher))
            .unwrap(),
    ))
}
