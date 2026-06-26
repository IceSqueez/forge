use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{Event, EventPublisher, EventSource};
use forge_registry::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, RegistryError, RunContext,
    SubActionRegistry, effective_config,
};
use forge_types::{ArgStack, EventId, SubActionOutcome, SubActionStep, SubActionTelemetry};
use serde_json::json;
use tracing::warn;

use crate::Config;
use crate::action_engine::skipped_telemetry;

pub struct ChainEngine {
    registry: Arc<SubActionRegistry>,
    publisher: Arc<dyn EventPublisher>,
    config: Config,
}

pub struct ChainRun {
    pub signal: ChainSignal,
    pub arg_stack: ArgStack,
    pub telemetry: Vec<SubActionTelemetry>,
}

impl ChainEngine {
    pub fn new(
        registry: Arc<SubActionRegistry>,
        publisher: Arc<dyn EventPublisher>,
        config: Config,
    ) -> Self {
        Self {
            registry,
            publisher,
            config,
        }
    }

    /// Scope for the action's top-level chain (depth 0); its steps run at depth 1.
    pub fn root_scope(self: &Arc<Self>, cancel: CancelSignal) -> ChainScope {
        ChainScope {
            engine: Arc::clone(self),
            depth: 0,
            cancel,
        }
    }

    /// Drives an action's top-level chain sequentially: builds the depth-0 scope
    /// the steps re-enter through and delegates to the executor-driven loop.
    pub async fn run_sequential(
        self: &Arc<Self>,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
        cancel: &CancelSignal,
    ) -> ChainRun {
        let scope = self.root_scope(cancel.clone());
        self.drive_sequential(steps, arg_stack, parent_event_id, &scope)
            .await
    }

    /// Drives an action's top-level chain concurrently from the depth-0 scope.
    pub async fn run_concurrent(
        self: &Arc<Self>,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
        cancel: &CancelSignal,
    ) -> ChainRun {
        let scope = self.root_scope(cancel.clone());
        self.drive_concurrent(steps, arg_stack, parent_event_id, &scope)
            .await
    }

    /// Runs `steps` in order, embedding `executor` in each step's `RunContext`
    /// so a composite step re-enters one nesting level down. Cancellation is read
    /// from the executor's shared signal and polled at every step boundary.
    async fn drive_sequential(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
        executor: &dyn ChainExecutor,
    ) -> ChainRun {
        let cancel = executor.cancel_signal();
        let mut current = arg_stack.clone();
        let mut telemetry = Vec::new();

        for (index, step) in steps.iter().enumerate() {
            if !step.enabled {
                continue;
            }
            if cancel.is_cancelled() {
                return ChainRun {
                    signal: ChainSignal::Aborted,
                    arg_stack: current,
                    telemetry,
                };
            }

            let run_event = Event::caused_by(
                EventSource::Core,
                "subaction.run",
                json!({ "step_index": index, "kind": step.kind_id }),
                parent_event_id,
            );
            let run_event_id = run_event.id;
            self.publisher.publish(run_event);

            let run_ctx = RunContext {
                arg_stack: &current,
                index,
                parent_event_id: run_event_id,
                publisher: self.publisher.as_ref(),
                executor,
                cancel: cancel.clone(),
            };

            let (tel, updated) = match self.registry.get(&step.kind_id) {
                Some(runner) => {
                    let resolved = effective_config(&runner.default_config(), &step.config);
                    runner.execute(&resolved, &run_ctx).await
                }
                None => {
                    warn!(
                        "unknown sub-action kind_id: {} — skipping step",
                        step.kind_id
                    );
                    (skipped_telemetry(index, &step.kind_id), None)
                }
            };

            if let Some(new_stack) = updated {
                current = new_stack;
            }

            let failure = match &tel.outcome {
                SubActionOutcome::Failed(m) => Some(m.clone()),
                _ => None,
            };
            telemetry.push(tel);

            if let Some(msg) = failure {
                return ChainRun {
                    signal: ChainSignal::Error(msg),
                    arg_stack: current,
                    telemetry,
                };
            }
        }

        ChainRun {
            signal: ChainSignal::Completed,
            arg_stack: current,
            telemetry,
        }
    }

    /// Concurrent sibling of `drive_sequential`: every enabled step runs on its
    /// own future with the same embedded `executor`; the first failure in step
    /// order becomes the chain's `Error` signal.
    async fn drive_concurrent(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
        executor: &dyn ChainExecutor,
    ) -> ChainRun {
        use futures_util::future::join_all;

        let cancel = executor.cancel_signal();
        if cancel.is_cancelled() {
            return ChainRun {
                signal: ChainSignal::Aborted,
                arg_stack: arg_stack.clone(),
                telemetry: Vec::new(),
            };
        }

        let futures: Vec<_> = steps
            .iter()
            .enumerate()
            .filter(|(_, step)| step.enabled)
            .map(|(index, step)| {
                let run_event = Event::caused_by(
                    EventSource::Core,
                    "subaction.run",
                    json!({ "step_index": index, "kind": step.kind_id }),
                    parent_event_id,
                );
                let run_event_id = run_event.id;
                self.publisher.publish(run_event);

                let run_ctx = RunContext {
                    arg_stack,
                    index,
                    parent_event_id: run_event_id,
                    publisher: self.publisher.as_ref(),
                    executor,
                    cancel: cancel.clone(),
                };

                async move {
                    match self.registry.get(&step.kind_id) {
                        Some(runner) => {
                            let resolved = effective_config(&runner.default_config(), &step.config);
                            runner.execute(&resolved, &run_ctx).await
                        }
                        None => {
                            warn!(
                                "unknown sub-action kind_id: {} — skipping step",
                                step.kind_id
                            );
                            (skipped_telemetry(index, &step.kind_id), None)
                        }
                    }
                }
            })
            .collect();

        let results = join_all(futures).await;

        let mut telemetry = Vec::new();
        let mut first_failure: Option<String> = None;
        for (tel, _) in results {
            if first_failure.is_none()
                && let SubActionOutcome::Failed(msg) = &tel.outcome
            {
                first_failure = Some(msg.clone());
            }
            telemetry.push(tel);
        }

        let signal = match first_failure {
            Some(msg) => ChainSignal::Error(msg),
            None => ChainSignal::Completed,
        };

        ChainRun {
            signal,
            arg_stack: arg_stack.clone(),
            telemetry,
        }
    }
}

pub struct ChainScope {
    engine: Arc<ChainEngine>,
    depth: u32,
    cancel: CancelSignal,
}

#[async_trait]
impl ChainExecutor for ChainScope {
    async fn run_child_chain(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
    ) -> Result<ChildChainOutcome, RegistryError> {
        let child_depth = self.depth + 1;
        if child_depth > self.engine.config.max_nesting_depth {
            return Err(RegistryError::DepthExceeded(child_depth));
        }

        let child_scope = ChainScope {
            engine: Arc::clone(&self.engine),
            depth: child_depth,
            cancel: self.cancel.clone(),
        };
        let run = self
            .engine
            .drive_sequential(steps, arg_stack, parent_event_id, &child_scope)
            .await;

        Ok(ChildChainOutcome {
            signal: run.signal,
            arg_stack: run.arg_stack,
            telemetry: run.telemetry,
        })
    }

    fn cancel_signal(&self) -> CancelSignal {
        self.cancel.clone()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_registry::{FormField, SubActionCategory, SubActionConfig, SubActionRunner};
    use forge_types::Variant;
    use time::OffsetDateTime;

    use super::*;

    struct NoopPublisher;

    impl EventPublisher for NoopPublisher {
        fn publish(&self, _event: Event) {}
    }

    /// Runner whose outcome, arg-stack mutation, and side effects are scripted
    /// per-test. `runs` and `observed` are shared `Arc`s so a test can inspect
    /// them after the runner has been moved into the registry.
    struct ScriptedRunner {
        id: String,
        outcome: SubActionOutcome,
        set_binding: Option<(String, Variant)>,
        observed: Arc<Mutex<Vec<BTreeMap<String, Variant>>>>,
        cancel_on_run: Option<CancelSignal>,
        runs: Arc<AtomicUsize>,
    }

    fn scripted(id: &str, outcome: SubActionOutcome) -> ScriptedRunner {
        ScriptedRunner {
            id: id.to_owned(),
            outcome,
            set_binding: None,
            observed: Arc::new(Mutex::new(Vec::new())),
            cancel_on_run: None,
            runs: Arc::new(AtomicUsize::new(0)),
        }
    }

    #[async_trait]
    impl SubActionRunner for ScriptedRunner {
        fn id(&self) -> &str {
            &self.id
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
            BTreeMap::new()
        }
        fn config_fields(&self) -> Vec<FormField> {
            Vec::new()
        }
        fn validate_config(&self, _: &SubActionConfig) -> Result<(), RegistryError> {
            Ok(())
        }
        async fn execute(
            &self,
            _config: &SubActionConfig,
            ctx: &RunContext<'_>,
        ) -> (SubActionTelemetry, Option<ArgStack>) {
            self.runs.fetch_add(1, Ordering::Relaxed);
            self.observed.lock().unwrap().push(ctx.arg_stack.snapshot());
            if let Some(c) = &self.cancel_on_run {
                c.cancel();
            }
            let updated = self
                .set_binding
                .as_ref()
                .map(|(k, v)| ctx.arg_stack.clone().set(k.clone(), v.clone()));
            let tel = SubActionTelemetry {
                index: ctx.index,
                kind: self.id.clone(),
                started_at: OffsetDateTime::now_utc(),
                duration_ms: 0,
                outcome: self.outcome.clone(),
            };
            (tel, updated)
        }
    }

    fn registry(runners: Vec<Box<dyn SubActionRunner>>) -> Arc<SubActionRegistry> {
        let mut reg = SubActionRegistry::new();
        for r in runners {
            reg.register(r).unwrap();
        }
        Arc::new(reg)
    }

    fn engine(reg: Arc<SubActionRegistry>, max_nesting_depth: u32) -> Arc<ChainEngine> {
        let publisher: Arc<dyn EventPublisher> = Arc::new(NoopPublisher);
        Arc::new(ChainEngine::new(
            reg,
            publisher,
            Config {
                max_nesting_depth,
                ..Default::default()
            },
        ))
    }

    fn step(kind: &str) -> SubActionStep {
        SubActionStep {
            kind_id: kind.to_owned(),
            config: BTreeMap::new(),
            enabled: true,
            label: None,
        }
    }

    // ---- Depth bound (invariant 1) -------------------------------------------

    #[tokio::test]
    async fn child_chain_entry_admitted_only_within_the_depth_bound() {
        // root_scope sits at depth 0; its first child chain enters depth 1. The
        // bound is the deepest child level allowed, so 0 rejects the first child
        // chain and any value >= 1 admits it. Deeper nesting is unreachable from
        // the public surface (no depth>0 scope constructor yet), so the reachable
        // boundary is exactly {0 rejects, 1 admits}.
        for (max_depth, admit) in [(0u32, false), (1, true), (4, true)] {
            let reg = registry(vec![Box::new(scripted("d.ok", SubActionOutcome::Success))]);
            let eng = engine(reg, max_depth);
            let scope = eng.root_scope(CancelSignal::new());
            let result = scope
                .run_child_chain(&[step("d.ok")], &ArgStack::new(), EventId::new())
                .await;
            if admit {
                let outcome =
                    result.unwrap_or_else(|e| panic!("max_depth={max_depth} rejected: {e:?}"));
                assert_eq!(outcome.signal, ChainSignal::Completed);
            } else {
                match result {
                    Err(RegistryError::DepthExceeded(d)) => assert_eq!(d, 1),
                    Err(e) => panic!("max_depth={max_depth} wrong error: {e:?}"),
                    Ok(_) => panic!("max_depth={max_depth} should have rejected child chain"),
                }
            }
        }
    }

    // ---- Cancellation (invariant 2) ------------------------------------------

    #[tokio::test]
    async fn pre_cancelled_signal_aborts_sequential_before_any_step_runs() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut r = scripted("seq.ok", SubActionOutcome::Success);
        r.runs = Arc::clone(&runs);
        let eng = engine(registry(vec![Box::new(r)]), 8);

        let cancel = CancelSignal::new();
        cancel.cancel();
        let run = eng
            .run_sequential(&[step("seq.ok")], &ArgStack::new(), EventId::new(), &cancel)
            .await;

        assert_eq!(run.signal, ChainSignal::Aborted);
        assert!(run.telemetry.is_empty());
        assert_eq!(runs.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn cancellation_mid_chain_aborts_with_partial_telemetry() {
        let cancel = CancelSignal::new();
        let downstream_runs = Arc::new(AtomicUsize::new(0));

        let mut first = scripted("seq.first", SubActionOutcome::Success);
        first.cancel_on_run = Some(cancel.clone());
        let mut second = scripted("seq.second", SubActionOutcome::Success);
        second.runs = Arc::clone(&downstream_runs);

        let eng = engine(registry(vec![Box::new(first), Box::new(second)]), 8);
        let run = eng
            .run_sequential(
                &[step("seq.first"), step("seq.second")],
                &ArgStack::new(),
                EventId::new(),
                &cancel,
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Aborted);
        assert_eq!(run.telemetry.len(), 1);
        assert_eq!(run.telemetry[0].kind, "seq.first");
        assert_eq!(downstream_runs.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn pre_cancelled_signal_aborts_concurrent_run() {
        let eng = engine(
            registry(vec![Box::new(scripted("c.x", SubActionOutcome::Success))]),
            8,
        );
        let cancel = CancelSignal::new();
        cancel.cancel();
        let run = eng
            .run_concurrent(&[step("c.x")], &ArgStack::new(), EventId::new(), &cancel)
            .await;

        assert_eq!(run.signal, ChainSignal::Aborted);
        assert!(run.telemetry.is_empty());
    }

    #[test]
    fn cancel_signal_propagates_through_clones() {
        // Why: child chains share the parent's cancel via clone; cancelling one
        // handle must be observable on every clone or nested chains can't stop.
        let signal = CancelSignal::new();
        let child = signal.clone();
        assert!(!child.is_cancelled());
        signal.cancel();
        assert!(child.is_cancelled());
    }

    // ---- Sequential outcome (invariant 3) ------------------------------------

    #[tokio::test]
    async fn sequential_step_failure_halts_chain_with_error_signal() {
        let downstream_runs = Arc::new(AtomicUsize::new(0));
        let first = scripted("seq.fail", SubActionOutcome::Failed("boom".to_owned()));
        let mut second = scripted("seq.after", SubActionOutcome::Success);
        second.runs = Arc::clone(&downstream_runs);

        let eng = engine(registry(vec![Box::new(first), Box::new(second)]), 8);
        let run = eng
            .run_sequential(
                &[step("seq.fail"), step("seq.after")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Error("boom".to_owned()));
        assert_eq!(run.telemetry.len(), 1);
        assert!(
            matches!(&run.telemetry[0].outcome, SubActionOutcome::Failed(m) if m == "boom"),
            "failed step must be recorded in telemetry"
        );
        assert_eq!(downstream_runs.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn unknown_kind_id_is_skipped_not_failed_and_chain_continues() {
        let eng = engine(
            registry(vec![Box::new(scripted(
                "seq.known",
                SubActionOutcome::Success,
            ))]),
            8,
        );
        let run = eng
            .run_sequential(
                &[step("seq.missing"), step("seq.known")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Completed);
        assert_eq!(run.telemetry.len(), 2);
        assert!(matches!(
            run.telemetry[0].outcome,
            SubActionOutcome::Skipped(_)
        ));
        assert_eq!(run.telemetry[1].outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn updated_arg_stack_threads_into_the_next_step() {
        let observed = Arc::new(Mutex::new(Vec::new()));
        let mut producer = scripted("seq.producer", SubActionOutcome::Success);
        producer.set_binding = Some(("token".to_owned(), Variant::Int(7)));
        let mut consumer = scripted("seq.consumer", SubActionOutcome::Success);
        consumer.observed = Arc::clone(&observed);

        let eng = engine(registry(vec![Box::new(producer), Box::new(consumer)]), 8);
        let run = eng
            .run_sequential(
                &[step("seq.producer"), step("seq.consumer")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        let seen = observed.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].get("token"), Some(&Variant::Int(7)));
        assert_eq!(run.arg_stack.get("token"), Some(&Variant::Int(7)));
    }

    #[tokio::test]
    async fn disabled_step_is_neither_executed_nor_recorded() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut r = scripted("seq.disabled", SubActionOutcome::Success);
        r.runs = Arc::clone(&runs);
        let eng = engine(registry(vec![Box::new(r)]), 8);

        let disabled = SubActionStep {
            kind_id: "seq.disabled".to_owned(),
            config: BTreeMap::new(),
            enabled: false,
            label: None,
        };
        let run = eng
            .run_sequential(
                &[disabled],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Completed);
        assert!(run.telemetry.is_empty());
        assert_eq!(runs.load(Ordering::Relaxed), 0);
    }

    // ---- Concurrent first-failure (invariant 4) ------------------------------

    #[tokio::test]
    async fn concurrent_run_reports_first_failure_in_step_order() {
        let eng = engine(
            registry(vec![
                Box::new(scripted("c.ok", SubActionOutcome::Success)),
                Box::new(scripted(
                    "c.fail.a",
                    SubActionOutcome::Failed("first".to_owned()),
                )),
                Box::new(scripted(
                    "c.fail.b",
                    SubActionOutcome::Failed("second".to_owned()),
                )),
            ]),
            8,
        );
        let run = eng
            .run_concurrent(
                &[step("c.ok"), step("c.fail.a"), step("c.fail.b")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Error("first".to_owned()));
        assert_eq!(run.telemetry.len(), 3);
    }

    #[tokio::test]
    async fn concurrent_run_all_success_completes() {
        let eng = engine(
            registry(vec![
                Box::new(scripted("c.a", SubActionOutcome::Success)),
                Box::new(scripted("c.b", SubActionOutcome::Success)),
            ]),
            8,
        );
        let run = eng
            .run_concurrent(
                &[step("c.a"), step("c.b")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Completed);
        assert_eq!(run.telemetry.len(), 2);
    }
}
