#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use forge_events::Event;
use forge_registry::SubActionRegistry;
use forge_runtime::{ActionCancelRegistry, EventBus, spawn_action_engine};
use forge_server::{ServerConfig, ServerHandle, stopped_server};
use forge_storage::{
    ActionRepo, ActionStats, ActionTelemetry, ChatHistoryRepo, CredentialId, CredentialsRepo,
    DataProvider, EventLogRepo, ExecutionStatus, GlobalEntry, GlobalsRepo, HistoryRepo, MediaRepo,
    OverlayConfig, OverlayCredential, OverlayDefinition, OverlayId, OverlayRepo, QueueRepo,
    ScriptRecord, ScriptRepo, ScriptTelemetry, SettingsRepo, SoundboardClipsRepo, StorageError,
    TriggerInstanceRepo, TtsFiltersRepo, UserGlobalEntry, UserGlobalsRepo, ViewerRepo,
    VoiceAliasRepo,
};
use forge_types::{Action, ActionId, EventId, ExecutionContext, ScriptId, Variant};
use time::OffsetDateTime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

const RUNTIME_PUMPS: usize = 8;

pub(crate) type SettingWrite = (String, String);

pub(crate) struct TestBackend {
    values: Mutex<HashMap<String, String>>,
    writes: UnboundedSender<SettingWrite>,
}

pub(crate) fn test_backend() -> (Arc<TestBackend>, UnboundedReceiver<SettingWrite>) {
    let (writes, rx) = unbounded_channel();
    (
        Arc::new(TestBackend {
            values: Mutex::new(HashMap::new()),
            writes,
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
        unreachable!("the settings pane reaches no sub-repo")
    }

    fn media_repo(&self) -> Arc<dyn MediaRepo> {
        unreachable!("the settings pane reaches no sub-repo")
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

/// Why: gpui's test scheduler rejects any wake reaching it from a foreign thread, so
/// the view's tokio work has to stay on the test thread and be pumped explicitly.
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
