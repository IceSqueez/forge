#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionRegistry, SubActionRunner,
};
use forge_runtime::{
    ActionCancelRegistry, EventBus, ExecutionRequest, NullEventLogRepo, spawn_action_engine,
};
use forge_storage::{
    ActionRepo, ActionStats, ActionTelemetry, ExecutionStatus, HistoryRepo, StorageError,
};
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionContext, ExecutionMode, ExecutionOutcome,
    QueueId, SubActionConfig, SubActionOutcome, SubActionStep, SubActionTelemetry,
};
use time::OffsetDateTime;
use tokio::sync::Notify;

struct SpyActionRepo {
    actions: Mutex<HashMap<ActionId, Action>>,
    records: Mutex<Vec<(ActionId, u64, ExecutionStatus)>>,
    fail_record: bool,
}

impl SpyActionRepo {
    fn new() -> Self {
        Self {
            actions: Mutex::new(HashMap::new()),
            records: Mutex::new(Vec::new()),
            fail_record: false,
        }
    }

    fn failing_record() -> Self {
        Self {
            fail_record: true,
            ..Self::new()
        }
    }

    fn seed(&self, action: Action) {
        self.actions.lock().unwrap().insert(action.id, action);
    }

    fn records(&self) -> Vec<(ActionId, u64, ExecutionStatus)> {
        self.records.lock().unwrap().clone()
    }
}

#[async_trait]
impl ActionRepo for SpyActionRepo {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(self.actions.lock().unwrap().values().cloned().collect())
    }
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(self.actions.lock().unwrap().get(&id).cloned())
    }
    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Ok(())
    }
    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Ok(false)
    }
    async fn list_by_group<'a>(
        &'a self,
        _group: Option<&'a str>,
    ) -> Result<Vec<Action>, StorageError> {
        Ok(vec![])
    }
    async fn telemetry(&self, _id: ActionId) -> Result<ActionTelemetry, StorageError> {
        Ok(ActionTelemetry::default())
    }
    async fn record_execution(
        &self,
        action_id: ActionId,
        _started_at: OffsetDateTime,
        duration_ms: u64,
        status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        self.records
            .lock()
            .unwrap()
            .push((action_id, duration_ms, status));
        if self.fail_record {
            return Err(StorageError::Connection {
                reason: "forced record_execution failure".to_owned(),
            });
        }
        Ok(())
    }
    async fn prune_executions_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

struct SpyHistoryRepo {
    saved: Mutex<Vec<(ActionId, ExecutionOutcome)>>,
}

impl SpyHistoryRepo {
    fn new() -> Self {
        Self {
            saved: Mutex::new(Vec::new()),
        }
    }

    fn saved(&self) -> Vec<(ActionId, ExecutionOutcome)> {
        self.saved.lock().unwrap().clone()
    }
}

#[async_trait]
impl HistoryRepo for SpyHistoryRepo {
    async fn save(&self, ctx: &ExecutionContext) -> Result<(), StorageError> {
        self.saved
            .lock()
            .unwrap()
            .push((ctx.action_id, ctx.outcome.clone()));
        Ok(())
    }
    async fn recent_for_action(
        &self,
        _action_id: ActionId,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(vec![])
    }
    async fn recent_for_builtin(
        &self,
        _builtin_id: &str,
        _limit: u32,
    ) -> Result<Vec<ExecutionContext>, StorageError> {
        Ok(vec![])
    }
    async fn stats_summary(
        &self,
        _since: OffsetDateTime,
    ) -> Result<HashMap<ActionId, ActionStats>, StorageError> {
        Ok(HashMap::new())
    }
    async fn prune_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

struct FailRunner;

#[async_trait]
impl SubActionRunner for FailRunner {
    fn id(&self) -> &str {
        "test.fail"
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        ""
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "test.fail".to_owned(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 42,
                outcome: SubActionOutcome::Failed("boom".to_owned()),
            },
            None,
        )
    }
}

struct GateRunner {
    running: Arc<Notify>,
}

#[async_trait]
impl SubActionRunner for GateRunner {
    fn id(&self) -> &str {
        "test.gate"
    }
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Util
    }
    fn label(&self) -> &str {
        ""
    }
    fn summary(&self) -> &str {
        ""
    }
    fn search_text(&self) -> &str {
        ""
    }
    fn icon_name(&self) -> &str {
        ""
    }
    fn default_config(&self) -> SubActionConfig {
        SubActionConfig::new()
    }
    fn config_fields(&self) -> Vec<FormField> {
        Vec::new()
    }
    fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
    }
    async fn execute(
        &self,
        _: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        self.running.notify_one();
        for _ in 0..400 {
            if ctx.cancel.is_cancelled() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                index: ctx.index,
                kind: "test.gate".to_owned(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 0,
                outcome: SubActionOutcome::Success,
            },
            None,
        )
    }
}

fn action_with(steps: Vec<&str>) -> Action {
    Action {
        id: ActionId::new(),
        name: "target".to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled: true,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: steps
            .into_iter()
            .map(|k| SubActionStep {
                kind_id: k.to_owned(),
                config: SubActionConfig::new(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            })
            .collect(),
    }
}

fn request(action_id: ActionId) -> ExecutionRequest {
    ExecutionRequest {
        action_id,
        trigger_event_id: EventId::new(),
        trigger_kind: None,
        initial_args: ArgStack::new(),
    }
}

async fn eventually<F: Fn() -> bool>(pred: F) -> bool {
    for _ in 0..80 {
        if pred() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    pred()
}

#[tokio::test]
async fn successful_execution_records_one_success_row() {
    let repo = Arc::new(SpyActionRepo::new());
    let action = action_with(vec![]); // empty chain → Completed → Success
    let id = action.id;
    repo.seed(action);

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        repo.clone(),
        Arc::new(SpyHistoryRepo::new()),
        Arc::new(SubActionRegistry::new()),
        Arc::new(ActionCancelRegistry::new()),
    );
    engine.dispatch(request(id)).await.unwrap();

    assert!(
        eventually(|| repo.records().len() == 1).await,
        "a completed run must record exactly one telemetry row",
    );
    let records = repo.records();
    assert_eq!(records[0].0, id);
    assert_eq!(records[0].2, ExecutionStatus::Success);
}

#[tokio::test]
async fn failed_execution_records_one_error_row() {
    let repo = Arc::new(SpyActionRepo::new());
    let action = action_with(vec!["test.fail"]);
    let id = action.id;
    repo.seed(action);

    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(FailRunner)).unwrap();

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        repo.clone(),
        Arc::new(SpyHistoryRepo::new()),
        Arc::new(reg),
        Arc::new(ActionCancelRegistry::new()),
    );
    engine.dispatch(request(id)).await.unwrap();

    assert!(
        eventually(|| repo.records().len() == 1).await,
        "a failed run must record exactly one telemetry row",
    );
    let records = repo.records();
    assert_eq!(
        records[0].2,
        ExecutionStatus::Error,
        "a failed outcome maps to ExecutionStatus::Error",
    );
    assert_eq!(
        records[0].1, 42,
        "the recorded duration is the run's real total"
    );
}

#[tokio::test]
async fn cancelled_execution_records_no_row_but_saves_history() {
    let repo = Arc::new(SpyActionRepo::new());
    let action = action_with(vec!["test.gate"]);
    let id = action.id;
    repo.seed(action);

    let running = Arc::new(Notify::new());
    let mut reg = SubActionRegistry::new();
    reg.register(Box::new(GateRunner {
        running: Arc::clone(&running),
    }))
    .unwrap();

    let history = Arc::new(SpyHistoryRepo::new());
    let cancel_registry = Arc::new(ActionCancelRegistry::new());
    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        repo.clone(),
        history.clone(),
        Arc::new(reg),
        Arc::clone(&cancel_registry),
    );
    engine.dispatch(request(id)).await.unwrap();

    tokio::time::timeout(Duration::from_secs(5), running.notified())
        .await
        .expect("gated action never reached its in-flight point");
    cancel_registry.cancel(id);

    assert!(
        eventually(|| history
            .saved()
            .iter()
            .any(|(a, o)| *a == id && matches!(o, ExecutionOutcome::Cancelled)))
        .await,
        "a cancelled run must still be saved to history",
    );
    assert!(
        repo.records().is_empty(),
        "a cancelled run must NOT record a telemetry row, got {:?}",
        repo.records(),
    );
}

#[tokio::test]
async fn telemetry_write_error_is_swallowed_and_engine_keeps_running() {
    let repo = Arc::new(SpyActionRepo::failing_record());
    let action = action_with(vec![]);
    let id = action.id;
    repo.seed(action);

    let bus = EventBus::new(Arc::new(NullEventLogRepo));
    let engine = spawn_action_engine(
        Arc::clone(&bus),
        repo.clone(),
        Arc::new(SpyHistoryRepo::new()),
        Arc::new(SubActionRegistry::new()),
        Arc::new(ActionCancelRegistry::new()),
    );

    engine.dispatch(request(id)).await.unwrap();
    engine.dispatch(request(id)).await.unwrap();

    assert!(
        eventually(|| repo.records().len() == 2).await,
        "engine must keep processing after a swallowed telemetry write error",
    );
}
