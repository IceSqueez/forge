#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test fixtures fail loudly by design"
)]

mod booted;
mod logs;
pub mod pack_fixture;
mod pumps;
mod schedules;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use forge_audio::{AudioError, AudioSink, ControlledPlayback, PlaybackControl};
use forge_events::Event;
use forge_platform_core::PlatformEndpoints;
use forge_platform_core::PlatformError;
use forge_registry::{SubActionRegistry, TriggerRegistry};
use forge_runtime::DonationIngest;
use forge_runtime::{Catalog, EventBus};
use forge_server::test_support::ServerDeps;
use forge_server::{ServerHandle, stopped_server};
use forge_speech_types::PcmBuffer;
use forge_storage::MockTriggerInstanceRepo;
use forge_storage::test_support::{MemorySettings, StoredSettingRow};
use forge_storage::{
    ActionRepo, ActionStats, ActionTelemetry, BanLedgerRepo, CatalogRevision, ChatHistoryRepo,
    CredentialId, CredentialsRepo, DataProvider, DonationRepo, EventLogRepo, ExecutionStatus,
    FollowKnowledgeRepo, GlobalEntry, GlobalsRepo, HistoryRepo, LatestValueRepo, MediaBlob,
    MediaBlobId, MediaReferrer, MediaReferrerKind, MediaRepo, OverlayConfig, OverlayDefinition,
    OverlayId, OverlayRepo, QueueRepo, RowClasses, RowGate, ScheduledRunRepo, ScriptRecord,
    ScriptReferences, ScriptRepo, ScriptTelemetry, Setting, SettingCodec, SettingFamily,
    SettingRow, SettingsRepo, SoundboardClipsRepo, StorageError, TriggerInstanceRepo,
    TtsFiltersRepo, UserGlobalEntry, UserGlobalKey, UserGlobalsRepo, ViewerRepo, VoiceAliasRepo,
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
pub use pumps::{alive_tasks, pump, pump_until, run_until_tasks_park, runtime};
pub use schedules::{
    GatedSchedule, gated_running_schedule, gated_running_schedule_sharing, keyless_enabled_schedule,
};

const SNAPSHOT_SCHEMA_VERSION: u32 = 0;
const PUMP_HANG_GUARD: std::time::Duration = std::time::Duration::from_secs(90);

pub type SettingWrite = (String, String);

#[derive(Clone)]
pub enum ReadFault {
    Fails,
    Held(Arc<tokio::sync::Notify>),
}

pub const SETTINGS_READ_FAILED: &str = "settings store unreachable";
pub const SETTINGS_WRITE_FAILED: &str = "settings store is read-only";

pub struct TestRepos {
    settings: MemorySettings,
    writes: UnboundedSender<SettingWrite>,
    reads: Mutex<HashMap<String, usize>>,
    read_faults: Mutex<HashMap<String, ReadFault>>,
    write_faults: Mutex<std::collections::HashSet<String>>,
    scripts: Option<Arc<dyn ScriptRepo>>,
}

impl TestRepos {
    fn over(
        settings: MemorySettings,
        writes: UnboundedSender<SettingWrite>,
        scripts: Option<Arc<dyn ScriptRepo>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            settings,
            writes,
            reads: Mutex::default(),
            read_faults: Mutex::default(),
            write_faults: Mutex::default(),
            scripts,
        })
    }
}

pub struct TestBackend {
    repos: Arc<TestRepos>,
    overlays: Option<Arc<dyn OverlayRepo>>,
    actions: Option<Arc<dyn ActionRepo>>,
    triggers: Option<Arc<dyn TriggerInstanceRepo>>,
    media: Arc<dyn MediaRepo>,
    export_gate: Option<Arc<tokio::sync::Notify>>,
    schema_version: Option<u32>,
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
    test_backend_with_scripts(scripts).reshaped(|backend| backend.actions = Some(actions))
}

pub fn test_backend_with_actions_and_triggers(
    actions: Arc<dyn ActionRepo>,
    triggers: Arc<dyn TriggerInstanceRepo>,
) -> Arc<TestBackend> {
    bare_backend().reshaped(|backend| {
        backend.actions = Some(actions);
        backend.triggers = Some(triggers);
    })
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
    bare_backend().reshaped(|backend| backend.on_shutdown = Some(Box::new(on_shutdown)))
}

pub fn test_backend_with_gated_export() -> (Arc<TestBackend>, Arc<tokio::sync::Notify>) {
    let gate = Arc::new(tokio::sync::Notify::new());
    let backend = bare_backend().reshaped(|backend| backend.gate_export(Arc::clone(&gate)));
    (backend, gate)
}

pub fn test_backend_with_late_export() -> (Arc<TestBackend>, Arc<tokio::sync::Notify>) {
    let gate = Arc::new(tokio::sync::Notify::new());
    let backend = bare_backend().reshaped(|backend| {
        backend.gate_export(Arc::clone(&gate));
        backend.export_lands_late = true;
    });
    (backend, gate)
}

pub fn test_backend_with_stalling_export() -> Arc<TestBackend> {
    bare_backend().reshaped(|backend| backend.export_stalls_midway = true)
}

fn bare_backend() -> Arc<TestBackend> {
    backend_over(None, None, Arc::new(EmptyMediaLibrary)).0
}

fn backend_over(
    overlays: Option<Arc<dyn OverlayRepo>>,
    scripts: Option<Arc<dyn ScriptRepo>>,
    media: Arc<dyn MediaRepo>,
) -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    let (writes, rx) = unbounded_channel();
    (
        Arc::new(TestBackend {
            repos: TestRepos::over(MemorySettings::new(), writes, scripts),
            overlays,
            actions: None,
            triggers: None,
            media,
            export_gate: None,
            schema_version: None,
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
        let mut backend = Self {
            repos: TestRepos::over(settings, writes, None),
            overlays: None,
            actions: None,
            triggers: None,
            media: Arc::new(EmptyMediaLibrary),
            export_gate: None,
            schema_version: None,
            export_stalls_midway: false,
            export_lands_late: false,
            on_shutdown: None,
        };
        backend.gate_export(export_gate);
        Arc::new(backend)
    }

    #[must_use]
    pub fn with_stub_actions(self: Arc<Self>) -> Arc<Self> {
        self.reshaped(|backend| backend.actions = Some(Arc::new(StubActions)))
    }

    fn reshaped(self: Arc<Self>, change: impl FnOnce(&mut Self)) -> Arc<Self> {
        let mut backend = Arc::into_inner(self).unwrap();
        change(&mut backend);
        Arc::new(backend)
    }

    fn gate_export(&mut self, gate: Arc<tokio::sync::Notify>) {
        self.export_gate = Some(gate);
        self.schema_version = Some(SNAPSHOT_SCHEMA_VERSION);
    }
}

impl TestBackend {
    pub fn settings_repo(&self) -> Arc<dyn SettingsRepo> {
        Arc::clone(&self.repos) as Arc<dyn SettingsRepo>
    }

    pub fn credentials_repo(&self) -> Arc<dyn CredentialsRepo> {
        Arc::clone(&self.repos) as Arc<dyn CredentialsRepo>
    }
}

impl std::ops::Deref for TestBackend {
    type Target = TestRepos;

    fn deref(&self) -> &TestRepos {
        &self.repos
    }
}

impl TestRepos {
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
impl SettingsRepo for TestRepos {
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
impl CredentialsRepo for TestRepos {
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
impl GlobalsRepo for TestRepos {
    async fn get(&self, _: &str) -> Result<Option<Variant>, StorageError> {
        unreachable!("this TestBackend was built without globals")
    }

    async fn set(&self, _: &str, _: Variant, _: bool) -> Result<(), StorageError> {
        unreachable!("this TestBackend was built without globals")
    }

    async fn delete(&self, _: &str) -> Result<bool, StorageError> {
        unreachable!("this TestBackend was built without globals")
    }

    async fn list(&self) -> Result<Vec<GlobalEntry>, StorageError> {
        unreachable!("this TestBackend was built without globals")
    }

    async fn storage_bytes(&self) -> Result<u64, StorageError> {
        unreachable!("this TestBackend was built without globals")
    }

    async fn last_save_at(&self) -> Result<Option<OffsetDateTime>, StorageError> {
        unreachable!("this TestBackend was built without globals")
    }

    async fn incr(&self, _: &str, _: i64) -> Result<Variant, StorageError> {
        unreachable!("this TestBackend was built without globals")
    }

    async fn peek(
        &self,
        name: &str,
    ) -> Result<Option<forge_types::Variant>, forge_storage::StorageError> {
        forge_storage::GlobalsRepo::get(self, name).await
    }

    async fn persisted(&self, name: &str) -> Result<Option<bool>, forge_storage::StorageError> {
        Ok(forge_storage::GlobalsRepo::list(self)
            .await?
            .into_iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.persisted))
    }

    async fn set_persisted(
        &self,
        _name: &str,
        _persisted: bool,
    ) -> Result<bool, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn rename(
        &self,
        _old_name: &str,
        _new_name: &str,
    ) -> Result<(), forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn rename_and_set(
        &self,
        _old_name: &str,
        _new_name: &str,
        _value: Variant,
        _persisted: bool,
    ) -> Result<(), forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn count(&self) -> Result<usize, forge_storage::StorageError> {
        Ok(forge_storage::GlobalsRepo::list(self).await?.len())
    }

    async fn archive(&self, _name: &str) -> Result<bool, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn restore(&self, _name: &str) -> Result<bool, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn list_archived(
        &self,
    ) -> Result<Vec<forge_storage::GlobalEntry>, forge_storage::StorageError> {
        Ok(Vec::new())
    }

    async fn array_append(
        &self,
        _name: &str,
        _item: forge_types::Variant,
        _max_len: Option<usize>,
    ) -> Result<usize, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn array_remove(
        &self,
        _name: &str,
        _item: &forge_types::Variant,
        _remove_all: bool,
    ) -> Result<usize, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn toggle(&self, _name: &str) -> Result<bool, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }
}

#[async_trait::async_trait]
impl UserGlobalsRepo for TestRepos {
    async fn get(&self, _: &UserGlobalKey) -> Result<Option<Variant>, StorageError> {
        unreachable!("this TestBackend was built without user globals")
    }

    async fn set(&self, _: &UserGlobalKey, _: Variant) -> Result<(), StorageError> {
        unreachable!("this TestBackend was built without user globals")
    }

    async fn delete(&self, _: &UserGlobalKey) -> Result<bool, StorageError> {
        unreachable!("this TestBackend was built without user globals")
    }

    async fn list_for_user(&self, _: &str, _: &str) -> Result<Vec<UserGlobalEntry>, StorageError> {
        unreachable!("this TestBackend was built without user globals")
    }

    async fn list_for_broadcaster(&self, _: &str) -> Result<Vec<UserGlobalEntry>, StorageError> {
        unreachable!("this TestBackend was built without user globals")
    }

    async fn incr(
        &self,
        key: &UserGlobalKey,
        amount: i64,
    ) -> Result<forge_types::Variant, forge_storage::StorageError> {
        let current = forge_storage::UserGlobalsRepo::get(self, key).await?;
        let next = forge_storage::incremented(&key.name, current, amount)?;
        forge_storage::UserGlobalsRepo::set(self, key, next.clone()).await?;
        Ok(next)
    }
}

impl TestRepos {
    fn scripts(&self) -> &Arc<dyn ScriptRepo> {
        self.scripts
            .as_ref()
            .unwrap_or_else(|| unreachable!("this TestBackend was built without scripts"))
    }
}

#[async_trait::async_trait]
impl ScriptRepo for TestRepos {
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

    async fn prune_executions_before(
        &self,
        cutoff: OffsetDateTime,
        chunk_rows: u32,
    ) -> Result<u64, StorageError> {
        self.scripts()
            .prune_executions_before(cutoff, chunk_rows)
            .await
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
    fn globals_repo(&self) -> Arc<dyn GlobalsRepo> {
        Arc::clone(&self.repos) as Arc<dyn GlobalsRepo>
    }

    fn user_globals_repo(&self) -> Arc<dyn UserGlobalsRepo> {
        Arc::clone(&self.repos) as Arc<dyn UserGlobalsRepo>
    }

    fn settings_repo(&self) -> Arc<dyn SettingsRepo> {
        TestBackend::settings_repo(self)
    }

    fn script_repo(&self) -> Arc<dyn ScriptRepo> {
        Arc::clone(&self.repos) as Arc<dyn ScriptRepo>
    }

    fn credentials_repo(&self) -> Arc<dyn CredentialsRepo> {
        TestBackend::credentials_repo(self)
    }

    fn action_repo(&self) -> Arc<dyn ActionRepo> {
        if let Some(actions) = &self.actions {
            return Arc::clone(actions);
        }
        unreachable!("this TestBackend was built without actions")
    }

    fn trigger_instance_repo(&self) -> Arc<dyn TriggerInstanceRepo> {
        if let Some(triggers) = &self.triggers {
            return Arc::clone(triggers);
        }
        unreachable!("this TestBackend was built without a trigger_instance repo")
    }

    fn queue_repo(&self) -> Arc<dyn QueueRepo> {
        unreachable!("this TestBackend was built without a queue repo")
    }

    fn history_repo(&self) -> Arc<dyn HistoryRepo> {
        unreachable!("this TestBackend was built without a history repo")
    }

    fn event_log_repo(&self) -> Arc<dyn EventLogRepo> {
        unreachable!("this TestBackend was built without a event_log repo")
    }

    fn soundboard_clips_repo(&self) -> Arc<dyn SoundboardClipsRepo> {
        unreachable!("this TestBackend was built without a soundboard_clips repo")
    }

    fn voice_alias_repo(&self) -> Arc<dyn VoiceAliasRepo> {
        unreachable!("this TestBackend was built without a voice_alias repo")
    }

    fn viewer_repo(&self) -> Arc<dyn ViewerRepo> {
        unreachable!("this TestBackend was built without a viewer repo")
    }

    fn tts_filters_repo(&self) -> Arc<dyn TtsFiltersRepo> {
        unreachable!("this TestBackend was built without a tts_filters repo")
    }

    fn chat_history_repo(&self) -> Arc<dyn ChatHistoryRepo> {
        unreachable!("this TestBackend was built without a chat_history repo")
    }

    fn overlay_repo(&self) -> Arc<dyn OverlayRepo> {
        self.overlays
            .clone()
            .unwrap_or_else(|| unreachable!("this TestBackend was built without a overlay repo"))
    }

    fn media_repo(&self) -> Arc<dyn MediaRepo> {
        Arc::clone(&self.media)
    }

    fn donation_repo(&self) -> Arc<dyn DonationRepo> {
        unreachable!("this TestBackend was built without a donation repo")
    }

    fn latest_value_repo(&self) -> Arc<dyn LatestValueRepo> {
        unreachable!("this TestBackend was built without a latest_value repo")
    }

    fn ban_ledger_repo(&self) -> Arc<dyn BanLedgerRepo> {
        unreachable!("this TestBackend was built without a ban_ledger repo")
    }

    fn follow_knowledge_repo(&self) -> Arc<dyn FollowKnowledgeRepo> {
        unreachable!("this TestBackend was built without a follow_knowledge repo")
    }

    fn scheduled_run_repo(&self) -> Arc<dyn ScheduledRunRepo> {
        unreachable!("this TestBackend was built without a scheduled_run repo")
    }

    fn scheduled_run_revision(&self) -> CatalogRevision {
        unreachable!("this TestBackend was built without a catalog")
    }

    fn catalog_revision(&self) -> CatalogRevision {
        unreachable!("this TestBackend was built without a catalog")
    }

    async fn export(&self, snapshot: &std::path::Path) -> Result<(), StorageError> {
        if self.export_stalls_midway {
            std::fs::write(snapshot, b"partial").unwrap();
            std::future::pending::<()>().await;
        }
        let Some(gate) = &self.export_gate else {
            unreachable!("this TestBackend was built without a gated export")
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
        let Some(version) = self.schema_version else {
            unreachable!("this TestBackend was built without a schema version")
        };
        Ok(version)
    }

    async fn shutdown(&self) {
        if let Some(on_shutdown) = &self.on_shutdown {
            on_shutdown();
        }
    }

    async fn commit_changes(
        &self,
        _changes: &[forge_storage::StoreChange],
    ) -> Result<(), forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
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

    async fn blobs_by_referrer_id(
        &self,
        _: forge_storage::MediaReferrerKind,
        _: &str,
    ) -> Result<HashMap<String, MediaBlobId>, StorageError> {
        Ok(HashMap::new())
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
        unreachable!("this TestBackend was not built to creates an overlay")
    }

    async fn reissue_all_credentials(&self) -> Result<Vec<OverlayId>, StorageError> {
        Ok(Vec::new())
    }

    async fn save(&self, _: &OverlayDefinition) -> Result<(), StorageError> {
        unreachable!("this TestBackend was not built to saves an overlay")
    }

    async fn restore(&self, _: &OverlayDefinition) -> Result<(), StorageError> {
        unreachable!("this TestBackend was not built to restores an overlay")
    }

    async fn set_enabled(&self, _: &OverlayId, _: bool) -> Result<bool, StorageError> {
        unreachable!("this TestBackend was not built to toggles an overlay")
    }

    async fn delete(&self, _: &OverlayId) -> Result<bool, StorageError> {
        unreachable!("this TestBackend was not built to deletes an overlay")
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
        unreachable!("this TestBackend was not built to writes overlay content")
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
        unreachable!("this TestBackend was not built to writes an action")
    }

    async fn delete(&self, _: ActionId) -> Result<bool, StorageError> {
        unreachable!("this TestBackend was not built to deletes an action")
    }

    async fn telemetry(&self, _: ActionId) -> Result<ActionTelemetry, StorageError> {
        unreachable!("this TestBackend was not built to reads action telemetry")
    }

    async fn record_execution(
        &self,
        _: ActionId,
        _: OffsetDateTime,
        _: u64,
        _: ExecutionStatus,
    ) -> Result<(), StorageError> {
        unreachable!("this TestBackend was not built to runs an action")
    }

    async fn prune_executions_before(
        &self,
        _: OffsetDateTime,
        _chunk_rows: u32,
    ) -> Result<u64, StorageError> {
        Ok(0)
    }

    async fn count(&self) -> Result<usize, forge_storage::StorageError> {
        Ok(forge_storage::ActionRepo::list(self).await?.len())
    }

    async fn ids_named(
        &self,
        name: &str,
    ) -> Result<Vec<forge_types::ActionId>, forge_storage::StorageError> {
        Ok(forge_storage::ActionRepo::list(self)
            .await?
            .into_iter()
            .filter(|action| action.name == name)
            .map(|action| action.id)
            .collect())
    }

    async fn record_executions(
        &self,
        executions: &[forge_storage::ActionExecution],
    ) -> Result<(), forge_storage::StorageError> {
        for execution in executions {
            self.record_execution(
                execution.action_id,
                execution.started_at,
                execution.duration_ms,
                execution.status,
            )
            .await?;
        }
        Ok(())
    }

    async fn set_enabled(
        &self,
        _id: forge_types::ActionId,
        _enabled: bool,
    ) -> Result<bool, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn toggle_enabled(
        &self,
        _id: forge_types::ActionId,
    ) -> Result<Option<bool>, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn duplicate(
        &self,
        _source_id: forge_types::ActionId,
        _new_id: forge_types::ActionId,
        _new_name: &str,
    ) -> Result<(), forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn archive_reporting_cancelled_runs(
        &self,
        _id: forge_types::ActionId,
    ) -> Result<Option<u64>, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn restore(
        &self,
        _id: forge_types::ActionId,
    ) -> Result<bool, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn list_archived(&self) -> Result<Vec<forge_types::Action>, forge_storage::StorageError> {
        Ok(Vec::new())
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
        unreachable!("this TestBackend was not built to records a run")
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

    async fn save_batch(
        &self,
        contexts: &[forge_types::ExecutionContext],
    ) -> Result<(), forge_storage::StorageError> {
        for ctx in contexts {
            forge_storage::HistoryRepo::save(self, ctx).await?;
        }
        Ok(())
    }

    async fn recent(
        &self,
        _limit: u32,
    ) -> Result<Vec<forge_types::ExecutionContext>, forge_storage::StorageError> {
        Ok(Vec::new())
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
    stopped_server_hosting(backend, forge_paths::overlays_dir()).await
}

pub async fn stopped_server_hosting(
    backend: &Arc<TestBackend>,
    overlay_root: std::path::PathBuf,
) -> ServerHandle {
    let config = ServerDeps::new(overlay_root)
        .with_settings(backend.settings_repo())
        .with_credentials(backend.credentials_repo())
        .with_actions(Arc::new(StubActions))
        .with_globals(backend.globals_repo())
        .with_user_globals(backend.user_globals_repo())
        .with_overlays(Arc::new(StubOverlays))
        .build();
    stopped_server(config)
        .await
        .expect("a stopped server handle")
}

pub fn stopped_handle(rt: &tokio::runtime::Runtime, backend: &Arc<TestBackend>) -> ServerHandle {
    rt.block_on(stopped_server_handle(backend))
}

#[derive(Default)]
pub struct RecordingSink {
    starts: AtomicUsize,
}

impl RecordingSink {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn calls(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl AudioSink for RecordingSink {
    async fn start(&self, _: PcmBuffer) -> Result<ControlledPlayback, AudioError> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(ControlledPlayback::new(
            PlaybackControl::new(),
            std::future::ready(Ok(())),
        ))
    }
}

pub fn tone() -> PcmBuffer {
    PcmBuffer::new(vec![0_i16; 16], 48_000, 1)
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
        _request: forge_runtime::SpeechRequest,
    ) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }

    async fn speak_queued_for_run(
        &self,
        _request: forge_runtime::SpeechRequest,
        _run: forge_registry::CancelSignal,
    ) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }

    async fn speak_and_wait(
        &self,
        _request: forge_runtime::SpeechRequest,
        _cancel: forge_registry::CancelSignal,
    ) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }

    async fn available_voices(&self) -> Vec<forge_runtime::VoiceDescriptor> {
        Vec::new()
    }
}

#[async_trait::async_trait]
impl forge_runtime::SpeakQueueControl for SilentSpeaker {
    async fn stop_current(&self) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }

    async fn pause(&self) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }

    async fn resume(&self) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }

    async fn skip_current(&self) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }

    async fn clear_keep_current(&self) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl forge_runtime::VoiceAliasWriter for SilentSpeaker {
    async fn alias_set(
        &self,
        _viewer_id: String,
        _viewer_name: String,
        _engine_id: String,
        _voice_id: String,
    ) -> Result<(), forge_types::MessageRef> {
        Ok(())
    }

    async fn alias_switch(
        &self,
        _viewer_id: String,
        _engine_id: String,
        _voice_id: String,
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
        paths: forge_paths::AppPaths::at(std::env::temp_dir().join("forge-tests")),
        config: forge_runtime::Config::default(),
        os: forge_runtime::OsPorts::unavailable(),
        date_names: forge_runtime::DateNamesHandle::default(),
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
        forge_runtime::SpeechPorts::from_shared(Arc::new(SilentSpeaker)),
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
        Arc::new(forge_platform_twitch::BroadcasterSlot::default()),
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
    let api = yt::YouTubeDataApi::new(
        &forge_platform_core::PlatformEndpoints::default(),
        yt::SharedQuota::default(),
        publisher(bus),
    )
    .unwrap();
    let chat = yt::LiveChatIdHandle::new();
    let broadcast = yt::ActiveBroadcastIdHandle::new();
    yt::register_youtube_sub_actions(
        sub_actions,
        Arc::new(yt::YouTubeSendChat::new(
            api.clone(),
            youtube_token_source(),
            chat.clone(),
        )),
        Arc::new(yt::YouTubeModeration::new(
            api.clone(),
            youtube_token_source(),
            Arc::new(|| {
                Box::pin(async {
                    Ok(yt::YouTubeBroadcaster {
                        channel_id: "UCtest".to_owned(),
                        channel_title: "Test".to_owned(),
                    })
                })
            }),
            chat,
            Arc::new(forge_storage::MockBanLedgerRepo::new()),
        )),
        Arc::new(yt::YouTubeStreamMetadata::new(
            api.clone(),
            youtube_token_source(),
            broadcast.clone(),
        )),
        Arc::new(yt::YouTubeStreamStats::new(
            api.clone(),
            youtube_token_source(),
            broadcast.clone(),
        )),
        Arc::new(yt::YouTubeAdBreak::new(
            api.clone(),
            youtube_token_source(),
            broadcast.clone(),
        )),
        Arc::new(yt::YouTubeThumbnail::new(
            api.clone(),
            youtube_token_source(),
            broadcast,
        )),
        Arc::new(yt::YouTubeChannelLookup::new(api, youtube_token_source())),
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
        forge_platform_kick::kick_api_gateway(
            &endpoints,
            limiter(),
            Arc::new(forge_events::test_support::NullPublisher),
        )
        .unwrap(),
    ))
}
