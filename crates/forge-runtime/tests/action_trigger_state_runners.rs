#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::sub_action_runners::{
    CoreActionDisableRunner, CoreActionEnableRunner, CoreActionToggleRunner,
    CoreTriggerDisableRunner, CoreTriggerEnableRunner, CoreTriggerToggleRunner,
};
use forge_storage::{
    ActionRepo, ActionTelemetry, ExecutionStatus, StorageError, TriggerInstanceRepo,
};
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionMode, QueueId, SubActionConfig, SubActionOutcome,
    TriggerInstance, TriggerInstanceId, Variant,
};
use time::OffsetDateTime;

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

struct MockActionRepo {
    map: Mutex<HashMap<ActionId, Action>>,
    writes: AtomicUsize,
    fail_writes: bool,
}

impl MockActionRepo {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            writes: AtomicUsize::new(0),
            fail_writes: false,
        }
    }

    fn failing() -> Self {
        Self {
            fail_writes: true,
            ..Self::new()
        }
    }

    fn seed(&self, action: Action) {
        self.map.lock().unwrap().insert(action.id, action);
    }

    fn enabled_of(&self, id: ActionId) -> Option<bool> {
        self.map.lock().unwrap().get(&id).map(|a| a.enabled)
    }

    fn write_count(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ActionRepo for MockActionRepo {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(self.map.lock().unwrap().values().cloned().collect())
    }
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        Ok(self.map.lock().unwrap().get(&id).cloned())
    }
    async fn save(&self, action: &Action) -> Result<(), StorageError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if self.fail_writes {
            return Err(StorageError::Connection {
                reason: "forced write failure".to_owned(),
            });
        }
        self.map.lock().unwrap().insert(action.id, action.clone());
        Ok(())
    }
    async fn delete(&self, id: ActionId) -> Result<bool, StorageError> {
        Ok(self.map.lock().unwrap().remove(&id).is_some())
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
        _action_id: ActionId,
        _started_at: OffsetDateTime,
        _duration_ms: u64,
        _status: ExecutionStatus,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn prune_executions_before(&self, _cutoff: OffsetDateTime) -> Result<u64, StorageError> {
        Ok(0)
    }
}

struct MockTriggerInstanceRepo {
    map: Mutex<HashMap<TriggerInstanceId, TriggerInstance>>,
    writes: AtomicUsize,
    fail_writes: bool,
}

impl MockTriggerInstanceRepo {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            writes: AtomicUsize::new(0),
            fail_writes: false,
        }
    }

    fn failing() -> Self {
        Self {
            fail_writes: true,
            ..Self::new()
        }
    }

    fn seed(&self, instance: TriggerInstance) {
        self.map.lock().unwrap().insert(instance.id, instance);
    }

    fn enabled_of(&self, id: TriggerInstanceId) -> Option<bool> {
        self.map.lock().unwrap().get(&id).map(|i| i.enabled)
    }

    fn write_count(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl TriggerInstanceRepo for MockTriggerInstanceRepo {
    async fn list_all(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        Ok(self.map.lock().unwrap().values().cloned().collect())
    }
    async fn list_user_defined(&self) -> Result<Vec<TriggerInstance>, StorageError> {
        Ok(vec![])
    }
    async fn list_for_action(
        &self,
        _action_id: ActionId,
    ) -> Result<Vec<TriggerInstance>, StorageError> {
        Ok(vec![])
    }
    async fn actions_using(
        &self,
        _instance_id: TriggerInstanceId,
    ) -> Result<Vec<ActionId>, StorageError> {
        Ok(vec![])
    }
    async fn link_action(
        &self,
        _action_id: ActionId,
        _instance_id: TriggerInstanceId,
        _position: i64,
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn unlink_action(
        &self,
        _action_id: ActionId,
        _instance_id: TriggerInstanceId,
    ) -> Result<bool, StorageError> {
        Ok(false)
    }
    async fn get(&self, id: TriggerInstanceId) -> Result<Option<TriggerInstance>, StorageError> {
        Ok(self.map.lock().unwrap().get(&id).cloned())
    }
    async fn save(&self, instance: &TriggerInstance) -> Result<(), StorageError> {
        self.map
            .lock()
            .unwrap()
            .insert(instance.id, instance.clone());
        Ok(())
    }
    async fn delete(&self, id: TriggerInstanceId) -> Result<bool, StorageError> {
        Ok(self.map.lock().unwrap().remove(&id).is_some())
    }
    async fn upsert_default(
        &self,
        _kind_id: &str,
        _name: &str,
    ) -> Result<TriggerInstanceId, StorageError> {
        Err(StorageError::NotFound {
            key: "upsert_default unused".to_owned(),
        })
    }
    async fn set_enabled(&self, id: TriggerInstanceId, enabled: bool) -> Result<(), StorageError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if self.fail_writes {
            return Err(StorageError::Connection {
                reason: "forced write failure".to_owned(),
            });
        }
        if let Some(instance) = self.map.lock().unwrap().get_mut(&id) {
            instance.enabled = enabled;
        }
        Ok(())
    }
}

fn make_action(enabled: bool) -> Action {
    Action {
        id: ActionId::new(),
        name: "act".to_owned(),
        group: None,
        queue_id: QueueId::new(),
        enabled,
        concurrent: false,
        bypass_pause: false,
        execution_mode: ExecutionMode::Sequential,
        description: None,
        sub_actions: vec![],
    }
}

fn make_instance(enabled: bool) -> TriggerInstance {
    TriggerInstance {
        id: TriggerInstanceId::new(),
        kind_id: "test.kind".to_owned(),
        name: "trig".to_owned(),
        overrides: BTreeMap::new(),
        enabled,
        user_defined: true,
        platform_scope: Default::default(),
        cooldown_secs: 0,
        cooldown_global: true,
    }
}

fn action_cfg(id: &str) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert("action_id".to_owned(), Variant::String(id.to_owned()));
    c
}

fn trigger_cfg(id: &str) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "trigger_instance_id".to_owned(),
        Variant::String(id.to_owned()),
    );
    c
}

fn as_action_repo(repo: &Arc<MockActionRepo>) -> Arc<dyn ActionRepo> {
    repo.clone()
}

fn as_trigger_repo(repo: &Arc<MockTriggerInstanceRepo>) -> Arc<dyn TriggerInstanceRepo> {
    repo.clone()
}

async fn run(runner: &dyn SubActionRunner, config: &SubActionConfig) -> SubActionOutcome {
    let stack = ArgStack::new();
    let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
    runner.execute(config, &ctx).await.0.outcome
}

#[tokio::test]
async fn action_enable_and_disable_force_the_persisted_flag() {
    type Build = fn(Arc<MockActionRepo>) -> Box<dyn SubActionRunner>;
    let rows: [(Build, bool, bool); 2] = [
        (|r| Box::new(CoreActionEnableRunner::new(r)), false, true),
        (|r| Box::new(CoreActionDisableRunner::new(r)), true, false),
    ];
    for (build, start, expected) in rows {
        let repo = Arc::new(MockActionRepo::new());
        let action = make_action(start);
        let id = action.id;
        repo.seed(action);

        let runner = build(Arc::clone(&repo));
        let outcome = run(&*runner, &action_cfg(&id.to_string())).await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            repo.enabled_of(id),
            Some(expected),
            "start={start} should be forced to {expected}"
        );
    }
}

#[tokio::test]
async fn action_toggle_flips_the_persisted_flag_in_both_directions() {
    for (start, flipped) in [(true, false), (false, true)] {
        let repo = Arc::new(MockActionRepo::new());
        let action = make_action(start);
        let id = action.id;
        repo.seed(action);

        let runner = CoreActionToggleRunner::new(as_action_repo(&repo));
        let outcome = run(&runner, &action_cfg(&id.to_string())).await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            repo.enabled_of(id),
            Some(flipped),
            "start={start} should flip to {flipped}"
        );
    }
}

#[tokio::test]
async fn trigger_enable_and_disable_force_the_persisted_flag() {
    type Build = fn(Arc<MockTriggerInstanceRepo>) -> Box<dyn SubActionRunner>;
    let rows: [(Build, bool, bool); 2] = [
        (|r| Box::new(CoreTriggerEnableRunner::new(r)), false, true),
        (|r| Box::new(CoreTriggerDisableRunner::new(r)), true, false),
    ];
    for (build, start, expected) in rows {
        let repo = Arc::new(MockTriggerInstanceRepo::new());
        let instance = make_instance(start);
        let id = instance.id;
        repo.seed(instance);

        let runner = build(Arc::clone(&repo));
        let outcome = run(&*runner, &trigger_cfg(&id.to_string())).await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            repo.enabled_of(id),
            Some(expected),
            "start={start} should be forced to {expected}"
        );
    }
}

#[tokio::test]
async fn trigger_toggle_flips_the_persisted_flag_in_both_directions() {
    for (start, flipped) in [(true, false), (false, true)] {
        let repo = Arc::new(MockTriggerInstanceRepo::new());
        let instance = make_instance(start);
        let id = instance.id;
        repo.seed(instance);

        let runner = CoreTriggerToggleRunner::new(as_trigger_repo(&repo));
        let outcome = run(&runner, &trigger_cfg(&id.to_string())).await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert_eq!(
            repo.enabled_of(id),
            Some(flipped),
            "start={start} should flip to {flipped}"
        );
    }
}

#[tokio::test]
async fn action_runners_fail_and_persist_nothing_for_unknown_id() {
    type Build = fn(Arc<MockActionRepo>) -> Box<dyn SubActionRunner>;
    let builds: [Build; 3] = [
        |r| Box::new(CoreActionEnableRunner::new(r)),
        |r| Box::new(CoreActionDisableRunner::new(r)),
        |r| Box::new(CoreActionToggleRunner::new(r)),
    ];
    for build in builds {
        let repo = Arc::new(MockActionRepo::new());
        let missing = ActionId::new();

        let runner = build(Arc::clone(&repo));
        let outcome = run(&*runner, &action_cfg(&missing.to_string())).await;

        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert_eq!(
            repo.write_count(),
            0,
            "an unknown action must short-circuit before persisting"
        );
    }
}

#[tokio::test]
async fn trigger_toggle_fails_and_persists_nothing_for_unknown_id() {
    let repo = Arc::new(MockTriggerInstanceRepo::new());
    let missing = TriggerInstanceId::new();

    let runner = CoreTriggerToggleRunner::new(as_trigger_repo(&repo));
    let outcome = run(&runner, &trigger_cfg(&missing.to_string())).await;

    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    assert_eq!(
        repo.write_count(),
        0,
        "toggle must not write when the instance is unknown"
    );
}

#[tokio::test]
async fn trigger_enable_and_disable_on_unknown_id_are_noop_success() {
    type Build = fn(Arc<MockTriggerInstanceRepo>) -> Box<dyn SubActionRunner>;
    let builds: [Build; 2] = [
        |r| Box::new(CoreTriggerEnableRunner::new(r)),
        |r| Box::new(CoreTriggerDisableRunner::new(r)),
    ];
    for build in builds {
        let repo = Arc::new(MockTriggerInstanceRepo::new());
        let missing = TriggerInstanceId::new();

        let runner = build(Arc::clone(&repo));
        let outcome = run(&*runner, &trigger_cfg(&missing.to_string())).await;

        assert!(matches!(outcome, SubActionOutcome::Success));
        assert!(
            repo.enabled_of(missing).is_none(),
            "the absent instance must not be materialized"
        );
    }
}

#[tokio::test]
async fn action_enable_fails_and_persists_nothing_for_unparseable_id() {
    let repo = Arc::new(MockActionRepo::new());

    let runner = CoreActionEnableRunner::new(as_action_repo(&repo));
    let outcome = run(&runner, &action_cfg("not-a-ulid")).await;

    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("invalid action_id")),
        "got {outcome:?}"
    );
    assert_eq!(repo.write_count(), 0);
}

#[tokio::test]
async fn trigger_toggle_fails_and_persists_nothing_for_unparseable_id() {
    let repo = Arc::new(MockTriggerInstanceRepo::new());

    let runner = CoreTriggerToggleRunner::new(as_trigger_repo(&repo));
    let outcome = run(&runner, &trigger_cfg("not-a-ulid")).await;

    assert!(
        matches!(&outcome, SubActionOutcome::Failed(m) if m.contains("invalid trigger_instance_id")),
        "got {outcome:?}"
    );
    assert_eq!(repo.write_count(), 0);
}

#[tokio::test]
async fn action_enable_reports_failed_when_save_errors() {
    let repo = Arc::new(MockActionRepo::failing());
    let action = make_action(false);
    let id = action.id;
    repo.seed(action);

    let runner = CoreActionEnableRunner::new(as_action_repo(&repo));
    let outcome = run(&runner, &action_cfg(&id.to_string())).await;

    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
}

#[tokio::test]
async fn trigger_enable_reports_failed_when_set_enabled_errors() {
    let repo = Arc::new(MockTriggerInstanceRepo::failing());
    let instance = make_instance(false);
    let id = instance.id;
    repo.seed(instance);

    let runner = CoreTriggerEnableRunner::new(as_trigger_repo(&repo));
    let outcome = run(&runner, &trigger_cfg(&id.to_string())).await;

    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
}
