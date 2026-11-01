#![expect(clippy::unwrap_used, reason = "test fixtures")]
mod common;
use common::repos::InMemoryActionRepo;
use forge_events::test_support::NullPublisher;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use forge_registry::{RunContext, SubActionRunner};
use forge_runtime::{CoreActionStateRunner, CoreTriggerStateRunner};
use forge_storage::{ActionRepo, StorageError, TriggerInstanceRepo};
use forge_types::{
    Action, ActionId, ArgStack, EventId, PermissionRung, QueueId, SubActionConfig,
    SubActionOutcome, TriggerInstance, TriggerInstanceId, Variant,
};

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
    ) -> Result<(), StorageError> {
        Ok(())
    }
    async fn link_action_at(
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
    async fn set_enabled(
        &self,
        id: TriggerInstanceId,
        enabled: bool,
    ) -> Result<bool, StorageError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if self.fail_writes {
            return Err(StorageError::Connection {
                reason: "forced write failure".to_owned(),
            });
        }
        Ok(self
            .map
            .lock()
            .unwrap()
            .get_mut(&id)
            .map(|instance| instance.enabled = enabled)
            .is_some())
    }

    async fn count_of_kinds(
        &self,
        kind_ids: &[String],
    ) -> Result<usize, forge_storage::StorageError> {
        Ok(forge_storage::TriggerInstanceRepo::list_all(self)
            .await?
            .iter()
            .filter(|instance| kind_ids.contains(&instance.kind_id))
            .count())
    }

    async fn instances_by_action(
        &self,
    ) -> Result<
        std::collections::HashMap<forge_types::ActionId, Vec<forge_types::TriggerInstance>>,
        forge_storage::StorageError,
    > {
        Ok(Default::default())
    }

    async fn actions_by_instance(
        &self,
    ) -> Result<
        std::collections::HashMap<forge_types::TriggerInstanceId, Vec<forge_types::ActionId>>,
        forge_storage::StorageError,
    > {
        Ok(Default::default())
    }

    async fn apply_changes(
        &self,
        _changes: &forge_storage::TriggerChanges,
    ) -> Result<(), forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn commit_overlay_wiring(
        &self,
        _wiring: &forge_storage::TriggerWiring,
        _queues: std::sync::Arc<dyn forge_storage::QueueRepo>,
        _actions: std::sync::Arc<dyn forge_storage::ActionRepo>,
    ) -> Result<(), forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn set_all_enabled(
        &self,
        _ids: &[forge_types::TriggerInstanceId],
        _enabled: bool,
    ) -> Result<(), forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn toggle_enabled(
        &self,
        id: forge_types::TriggerInstanceId,
    ) -> Result<Option<bool>, forge_storage::StorageError> {
        let Some(instance) = self.get(id).await? else {
            return Ok(None);
        };
        self.set_enabled(id, !instance.enabled).await?;
        Ok(Some(!instance.enabled))
    }

    async fn archive(
        &self,
        _id: forge_types::TriggerInstanceId,
    ) -> Result<bool, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn restore(
        &self,
        _id: forge_types::TriggerInstanceId,
    ) -> Result<bool, forge_storage::StorageError> {
        Err(forge_storage::StorageError::NotReady)
    }

    async fn list_archived(
        &self,
    ) -> Result<Vec<forge_types::TriggerInstance>, forge_storage::StorageError> {
        Ok(Vec::new())
    }
}

fn make_action(enabled: bool) -> Action {
    Action {
        enabled,
        ..common::sequential_action(ActionId::new(), "act", QueueId::new(), vec![])
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
        permission_rung: PermissionRung::Everyone,
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

fn as_action_repo(repo: &Arc<InMemoryActionRepo>) -> Arc<dyn ActionRepo> {
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
    type Build = fn(Arc<InMemoryActionRepo>) -> Box<dyn SubActionRunner>;
    let rows: [(Build, bool, bool); 2] = [
        (|r| Box::new(CoreActionStateRunner::enable(r)), false, true),
        (|r| Box::new(CoreActionStateRunner::disable(r)), true, false),
    ];
    for (build, start, expected) in rows {
        let repo = Arc::new(InMemoryActionRepo::new());
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
        let repo = Arc::new(InMemoryActionRepo::new());
        let action = make_action(start);
        let id = action.id;
        repo.seed(action);

        let runner = CoreActionStateRunner::toggle(as_action_repo(&repo));
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
        (|r| Box::new(CoreTriggerStateRunner::enable(r)), false, true),
        (
            |r| Box::new(CoreTriggerStateRunner::disable(r)),
            true,
            false,
        ),
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

        let runner = CoreTriggerStateRunner::toggle(as_trigger_repo(&repo));
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
    type Build = fn(Arc<InMemoryActionRepo>) -> Box<dyn SubActionRunner>;
    let builds: [Build; 3] = [
        |r| Box::new(CoreActionStateRunner::enable(r)),
        |r| Box::new(CoreActionStateRunner::disable(r)),
        |r| Box::new(CoreActionStateRunner::toggle(r)),
    ];
    for build in builds {
        let repo = Arc::new(InMemoryActionRepo::new());
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

    let runner = CoreTriggerStateRunner::toggle(as_trigger_repo(&repo));
    let outcome = run(&runner, &trigger_cfg(&missing.to_string())).await;

    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    assert_eq!(
        repo.write_count(),
        0,
        "toggle must not write when the instance is unknown"
    );
}

#[tokio::test]
async fn trigger_enable_and_disable_on_unknown_id_fail_without_materializing_it() {
    type Build = fn(Arc<MockTriggerInstanceRepo>) -> Box<dyn SubActionRunner>;
    let builds: [Build; 2] = [
        |r| Box::new(CoreTriggerStateRunner::enable(r)),
        |r| Box::new(CoreTriggerStateRunner::disable(r)),
    ];
    for build in builds {
        let repo = Arc::new(MockTriggerInstanceRepo::new());
        let missing = TriggerInstanceId::new();

        let runner = build(Arc::clone(&repo));
        let outcome = run(&*runner, &trigger_cfg(&missing.to_string())).await;

        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
        assert!(
            repo.enabled_of(missing).is_none(),
            "the absent instance must not be materialized"
        );
    }
}

type ParseFailureCase = (
    Box<dyn SubActionRunner>,
    SubActionConfig,
    Box<dyn Fn() -> usize>,
);

fn action_case(
    build: fn(Arc<dyn ActionRepo>) -> Box<dyn SubActionRunner>,
    id: &str,
) -> ParseFailureCase {
    let repo = Arc::new(InMemoryActionRepo::new());
    let runner = build(as_action_repo(&repo));
    (runner, action_cfg(id), Box::new(move || repo.write_count()))
}

fn trigger_case(
    build: fn(Arc<dyn TriggerInstanceRepo>) -> Box<dyn SubActionRunner>,
    id: &str,
) -> ParseFailureCase {
    let repo = Arc::new(MockTriggerInstanceRepo::new());
    let runner = build(as_trigger_repo(&repo));
    (
        runner,
        trigger_cfg(id),
        Box::new(move || repo.write_count()),
    )
}

#[tokio::test]
async fn unparseable_id_fails_with_shape_only_text_and_persists_nothing() {
    const SENTINEL: &str = "not-a-ulid-a-viewer-typed-this";

    let cases: [(&str, &str, ParseFailureCase); 6] = [
        (
            "core.action.enable",
            "action_id",
            action_case(|r| Box::new(CoreActionStateRunner::enable(r)), SENTINEL),
        ),
        (
            "core.action.disable",
            "action_id",
            action_case(|r| Box::new(CoreActionStateRunner::disable(r)), SENTINEL),
        ),
        (
            "core.action.toggle",
            "action_id",
            action_case(|r| Box::new(CoreActionStateRunner::toggle(r)), SENTINEL),
        ),
        (
            "core.trigger.enable",
            "trigger_instance_id",
            trigger_case(|r| Box::new(CoreTriggerStateRunner::enable(r)), SENTINEL),
        ),
        (
            "core.trigger.disable",
            "trigger_instance_id",
            trigger_case(|r| Box::new(CoreTriggerStateRunner::disable(r)), SENTINEL),
        ),
        (
            "core.trigger.toggle",
            "trigger_instance_id",
            trigger_case(|r| Box::new(CoreTriggerStateRunner::toggle(r)), SENTINEL),
        ),
    ];

    for (kind, expected_field, (runner, config, writes)) in cases {
        let outcome = run(&*runner, &config).await;

        let SubActionOutcome::Failed(msg) = outcome else {
            panic!("{kind} accepted an unparseable id: {outcome:?}");
        };
        assert_eq!(
            msg,
            common::plain_field_problem(kind, expected_field, "config.unknown_entry"),
            "{kind} must name the field without echoing the value"
        );
        assert_eq!(writes(), 0, "{kind} persisted after a parse failure");
    }
}

#[tokio::test]
async fn action_enable_reports_failed_when_save_errors() {
    let repo = Arc::new(InMemoryActionRepo::failing());
    let action = make_action(false);
    let id = action.id;
    repo.seed(action);

    let runner = CoreActionStateRunner::enable(as_action_repo(&repo));
    let outcome = run(&runner, &action_cfg(&id.to_string())).await;

    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
}

#[tokio::test]
async fn trigger_enable_reports_failed_when_set_enabled_errors() {
    let repo = Arc::new(MockTriggerInstanceRepo::failing());
    let instance = make_instance(false);
    let id = instance.id;
    repo.seed(instance);

    let runner = CoreTriggerStateRunner::enable(as_trigger_repo(&repo));
    let outcome = run(&runner, &trigger_cfg(&id.to_string())).await;

    assert!(matches!(outcome, SubActionOutcome::Failed(_)));
}
