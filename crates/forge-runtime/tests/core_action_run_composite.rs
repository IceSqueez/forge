#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use common::{code_of, failure_message, verbatim_arg};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use forge_events::{Event, EventPublisher};
use forge_registry::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, RegistryError, RunContext,
    SubActionRunner,
};
use forge_runtime::sub_action_runners::CoreActionRunRunner;
use forge_storage::{ActionRepo, ActionTelemetry, ExecutionStatus, StorageError};
use forge_types::{
    Action, ActionId, ArgStack, EventId, ExecutionMode, QueueId, SubActionConfig, SubActionOutcome,
    SubActionStep, Variant,
};
use time::OffsetDateTime;

struct NullPublisher;
impl EventPublisher for NullPublisher {
    fn publish(&self, _event: Event) {}
}

struct MockActionRepo {
    map: Mutex<HashMap<ActionId, Action>>,
    fail_get: bool,
}

impl MockActionRepo {
    fn new() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            fail_get: false,
        }
    }

    fn failing_get() -> Self {
        Self {
            fail_get: true,
            ..Self::new()
        }
    }

    fn seed(&self, action: Action) {
        self.map.lock().unwrap().insert(action.id, action);
    }
}

#[async_trait]
impl ActionRepo for MockActionRepo {
    async fn list(&self) -> Result<Vec<Action>, StorageError> {
        Ok(self.map.lock().unwrap().values().cloned().collect())
    }
    async fn get(&self, id: ActionId) -> Result<Option<Action>, StorageError> {
        if self.fail_get {
            return Err(StorageError::Connection {
                reason: "forced get failure".to_owned(),
            });
        }
        Ok(self.map.lock().unwrap().get(&id).cloned())
    }
    async fn save(&self, _action: &Action) -> Result<(), StorageError> {
        Ok(())
    }
    async fn delete(&self, _id: ActionId) -> Result<bool, StorageError> {
        Ok(false)
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

enum ExecResponse {
    Signal(ChainSignal),
    DepthExceeded,
}

struct Recorded {
    step_kinds: Vec<String>,
    args: BTreeMap<String, Variant>,
}

struct MockChainExecutor {
    response: ExecResponse,
    calls: AtomicUsize,
    recorded: Mutex<Option<Recorded>>,
}

impl MockChainExecutor {
    fn with_signal(signal: ChainSignal) -> Self {
        Self {
            response: ExecResponse::Signal(signal),
            calls: AtomicUsize::new(0),
            recorded: Mutex::new(None),
        }
    }

    fn completing() -> Self {
        Self::with_signal(ChainSignal::Completed)
    }

    fn depth_exceeded() -> Self {
        Self {
            response: ExecResponse::DepthExceeded,
            calls: AtomicUsize::new(0),
            recorded: Mutex::new(None),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn recorded_args(&self) -> BTreeMap<String, Variant> {
        self.recorded
            .lock()
            .unwrap()
            .as_ref()
            .expect("run_child_chain was never called")
            .args
            .clone()
    }

    fn recorded_step_kinds(&self) -> Vec<String> {
        self.recorded
            .lock()
            .unwrap()
            .as_ref()
            .expect("run_child_chain was never called")
            .step_kinds
            .clone()
    }
}

#[async_trait]
impl ChainExecutor for MockChainExecutor {
    async fn run_child_chain(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        _parent_event_id: EventId,
    ) -> Result<ChildChainOutcome, RegistryError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.recorded.lock().unwrap() = Some(Recorded {
            step_kinds: steps.iter().map(|s| s.kind_id.clone()).collect(),
            args: arg_stack.snapshot(),
        });
        match &self.response {
            ExecResponse::DepthExceeded => Err(RegistryError::DepthExceeded(8)),
            ExecResponse::Signal(signal) => Ok(ChildChainOutcome {
                signal: signal.clone(),
                arg_stack: arg_stack.clone(),
                telemetry: Vec::new(),
            }),
        }
    }

    fn cancel_signal(&self) -> CancelSignal {
        CancelSignal::new()
    }
}

fn target_with_steps(kinds: &[&str]) -> Action {
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
        sub_actions: kinds
            .iter()
            .map(|k| SubActionStep {
                kind_id: (*k).to_owned(),
                config: SubActionConfig::new(),
                enabled: true,
                continue_on_error: false,
                condition: None,
                label: None,
            })
            .collect(),
    }
}

fn cfg(action_id: &str) -> SubActionConfig {
    let mut c = SubActionConfig::new();
    c.insert(
        "action_id".to_owned(),
        Variant::String(action_id.to_owned()),
    );
    c
}

fn cfg_inherit(action_id: &str, inherit: bool) -> SubActionConfig {
    let mut c = cfg(action_id);
    c.insert("inherit_args".to_owned(), Variant::Bool(inherit));
    c
}

fn stack_with(pairs: &[(&str, &str)]) -> ArgStack {
    let mut s = ArgStack::new();
    for (k, v) in pairs {
        s = s.set((*k).to_owned(), Variant::String((*v).to_owned()));
    }
    s
}

async fn run(
    runner: &CoreActionRunRunner,
    config: &SubActionConfig,
    parent_stack: &ArgStack,
    executor: &dyn ChainExecutor,
) -> SubActionOutcome {
    let ctx = RunContext {
        arg_stack: parent_stack,
        index: 0,
        parent_event_id: EventId::new(),
        publisher: &NullPublisher,
        executor,
        cancel: CancelSignal::new(),
        control: forge_registry::ControlCell::new(),
        telemetry: forge_registry::TelemetrySink::new(),
    };
    runner.execute(config, &ctx).await.0.outcome
}

#[tokio::test]
async fn runs_the_target_actions_sub_actions_as_the_child_chain() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let target = target_with_steps(&["core.log.write", "twitch.chat.send_message"]);
    let id = target.id;
    repo.seed(target);

    let runner = CoreActionRunRunner::new(repo);
    let executor = MockChainExecutor::completing();
    let outcome = run(&runner, &cfg(&id.to_string()), &ArgStack::new(), &executor).await;

    assert!(matches!(outcome, SubActionOutcome::Success));
    assert_eq!(executor.call_count(), 1);
    assert_eq!(
        executor.recorded_step_kinds(),
        vec!["core.log.write", "twitch.chat.send_message"],
    );
}

#[tokio::test]
async fn inherit_args_default_passes_the_parent_arg_stack_to_the_child() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let target = target_with_steps(&["core.log.write"]);
    let id = target.id;
    repo.seed(target);

    let runner = CoreActionRunRunner::new(repo);
    let executor = MockChainExecutor::completing();
    let parent = stack_with(&[("user", "alice")]);
    run(&runner, &cfg(&id.to_string()), &parent, &executor).await;

    assert_eq!(
        executor.recorded_args().get("user"),
        Some(&Variant::String("alice".to_owned())),
        "inherited child stack must carry the parent arg",
    );
}

#[tokio::test]
async fn inherit_args_false_starts_the_child_with_a_fresh_stack() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let target = target_with_steps(&["core.log.write"]);
    let id = target.id;
    repo.seed(target);

    let runner = CoreActionRunRunner::new(repo);
    let executor = MockChainExecutor::completing();
    let parent = stack_with(&[("user", "alice")]);
    run(
        &runner,
        &cfg_inherit(&id.to_string(), false),
        &parent,
        &executor,
    )
    .await;

    assert!(
        executor.recorded_args().is_empty(),
        "a non-inheriting child must not see any parent arg, got {:?}",
        executor.recorded_args(),
    );
}

enum Expect {
    Success,
    FailedExact(&'static str),
}

#[tokio::test]
async fn child_signal_maps_to_the_sub_action_outcome() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let target = target_with_steps(&["core.log.write"]);
    let id = target.id;
    repo.seed(target);
    let runner = CoreActionRunRunner::new(repo);

    let cases = [
        (ChainSignal::Completed, Expect::Success),
        (ChainSignal::Stop(Default::default()), Expect::Success),
        (ChainSignal::Break, Expect::Success),
        (ChainSignal::Continue, Expect::Success),
        (
            ChainSignal::Error(forge_types::UserText::unlocalized("boom")),
            Expect::FailedExact("boom"),
        ),
        (
            ChainSignal::Aborted,
            Expect::FailedExact("core.action_run_cancelled"),
        ),
    ];

    for (signal, expect) in cases {
        let executor = MockChainExecutor::with_signal(signal.clone());
        let outcome = run(&runner, &cfg(&id.to_string()), &ArgStack::new(), &executor).await;
        match expect {
            Expect::Success => assert!(
                matches!(outcome, SubActionOutcome::Success),
                "{signal:?} should map to Success, got {outcome:?}",
            ),
            Expect::FailedExact(msg) => assert!(
                matches!(&outcome, SubActionOutcome::Failed(m) if m.code_or_text() == msg),
                "{signal:?} should map to Failed({msg:?}), got {outcome:?}",
            ),
        }
    }
}

#[tokio::test]
async fn depth_exceeded_error_maps_to_failed_with_nesting_message() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let target = target_with_steps(&["core.log.write"]);
    let id = target.id;
    repo.seed(target);

    let runner = CoreActionRunRunner::new(repo);
    let executor = MockChainExecutor::depth_exceeded();
    let outcome = run(&runner, &cfg(&id.to_string()), &ArgStack::new(), &executor).await;

    assert_eq!(
        code_of(failure_message(&outcome)),
        "core.action_nesting_too_deep"
    );
}

#[tokio::test]
async fn unknown_action_id_fails_without_running_a_child_chain() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let missing = ActionId::new();

    let runner = CoreActionRunRunner::new(repo);
    let executor = MockChainExecutor::completing();
    let outcome = run(
        &runner,
        &cfg(&missing.to_string()),
        &ArgStack::new(),
        &executor,
    )
    .await;

    let refusal = failure_message(&outcome);
    assert_eq!(code_of(refusal), "core.unknown_action");
    assert_eq!(
        verbatim_arg(refusal, "action"),
        Some(missing.to_string().as_str())
    );
    assert_eq!(
        executor.call_count(),
        0,
        "an unknown target must short-circuit before the child chain",
    );
}

#[tokio::test]
async fn a_disabled_target_fails_naming_it_without_running_a_child_chain() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let mut target = target_with_steps(&["core.log.write"]);
    target.enabled = false;
    let id = target.id;
    repo.seed(target);
    let runner = CoreActionRunRunner::new(repo);
    let executor = MockChainExecutor::completing();

    let outcome = run(&runner, &cfg(&id.to_string()), &ArgStack::new(), &executor).await;

    let refusal = failure_message(&outcome);
    assert_eq!(code_of(refusal), "core.action_run_disabled");
    assert_eq!(verbatim_arg(refusal, "action"), Some("target"));
    assert_eq!(executor.call_count(), 0);
}

#[tokio::test]
async fn unparseable_action_id_fails_without_running_a_child_chain() {
    const SENTINEL: &str = "not-a-ulid-a-viewer-typed-this";
    let repo = std::sync::Arc::new(MockActionRepo::new());

    let runner = CoreActionRunRunner::new(repo);
    let executor = MockChainExecutor::completing();
    let outcome = run(&runner, &cfg(SENTINEL), &ArgStack::new(), &executor).await;

    let SubActionOutcome::Failed(msg) = outcome else {
        panic!("an unparseable action_id must fail, got {outcome:?}");
    };
    assert_eq!(
        msg,
        common::plain_field_problem("core.action.run", "action_id", "config.unknown_entry"),
        "the failure must name the field without echoing the value"
    );
    assert_eq!(executor.call_count(), 0);
}

#[tokio::test]
async fn repo_lookup_error_fails_without_running_a_child_chain() {
    let repo = std::sync::Arc::new(MockActionRepo::failing_get());
    let id = ActionId::new();

    let runner = CoreActionRunRunner::new(repo);
    let executor = MockChainExecutor::completing();
    let outcome = run(&runner, &cfg(&id.to_string()), &ArgStack::new(), &executor).await;

    let refusal = failure_message(&outcome);
    assert_eq!(code_of(refusal), "core.storage_failed");
    assert!(refusal.detail().is_some());
    assert_eq!(executor.call_count(), 0);
}

#[test]
fn validate_config_accepts_a_non_empty_action_id() {
    let runner = CoreActionRunRunner::new(std::sync::Arc::new(MockActionRepo::new()));
    assert!(
        runner
            .validate_config(&cfg("01ARZ3NDEKTSV4RRFFQ69G5FAV"))
            .is_ok()
    );
}

#[test]
fn validate_config_rejects_empty_or_missing_action_id() {
    let runner = CoreActionRunRunner::new(std::sync::Arc::new(MockActionRepo::new()));
    assert!(runner.validate_config(&cfg("")).is_err());
    assert!(runner.validate_config(&SubActionConfig::new()).is_err());
}

#[tokio::test]
async fn random_pick_target_runs_exactly_one_of_its_steps() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let mut target = target_with_steps(&["step.a", "step.b", "step.c"]);
    target.execution_mode = ExecutionMode::RandomPick;
    let id = target.id;
    repo.seed(target);
    let runner = CoreActionRunRunner::new(repo);

    for _ in 0..20 {
        let executor = MockChainExecutor::completing();
        let outcome = run(&runner, &cfg(&id.to_string()), &ArgStack::new(), &executor).await;
        assert!(matches!(outcome, SubActionOutcome::Success));
        let kinds = executor.recorded_step_kinds();
        assert_eq!(
            kinds.len(),
            1,
            "a random pick must run one step, ran {kinds:?}"
        );
        assert!(["step.a", "step.b", "step.c"].contains(&kinds[0].as_str()));
    }
}

#[tokio::test]
async fn random_pick_target_without_steps_succeeds_as_a_no_op() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let mut target = target_with_steps(&[]);
    target.execution_mode = ExecutionMode::RandomPick;
    let id = target.id;
    repo.seed(target);
    let runner = CoreActionRunRunner::new(repo);

    let executor = MockChainExecutor::completing();
    let outcome = run(&runner, &cfg(&id.to_string()), &ArgStack::new(), &executor).await;
    assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
}

#[tokio::test]
async fn concurrent_target_runs_all_its_steps_as_one_child_chain_in_order() {
    let repo = std::sync::Arc::new(MockActionRepo::new());
    let mut target = target_with_steps(&["step.a", "step.b", "step.c"]);
    target.concurrent = true;
    let id = target.id;
    repo.seed(target);
    let runner = CoreActionRunRunner::new(repo);

    let executor = MockChainExecutor::completing();
    let outcome = run(&runner, &cfg(&id.to_string()), &ArgStack::new(), &executor).await;

    assert!(matches!(outcome, SubActionOutcome::Success), "{outcome:?}");
    assert_eq!(
        (executor.call_count(), executor.recorded_step_kinds()),
        (
            1,
            vec![
                "step.a".to_owned(),
                "step.b".to_owned(),
                "step.c".to_owned()
            ]
        ),
        "a concurrent target must still run its steps as one sequential chain"
    );
}
