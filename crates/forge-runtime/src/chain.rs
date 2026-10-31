use std::sync::Arc;

use async_trait::async_trait;
use forge_events::{
    Event, EventPublisher, EventSource, RunOutcome, SubActionDone, SubActionRun, TypedEvent,
};
use forge_registry::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, ControlCell, ControlSignal,
    Interpolation, RegistryError, RunContext, RunningAction, StepTimer, SubActionRegistry,
    SubActionRunner, TelemetrySink, effective_config, validate_declared_config,
};
use forge_types::{
    ArgStack, EventId, IntegrationAvailability, SubActionOutcome, SubActionStep,
    SubActionTelemetry, UserText, Variant, variant_preview,
};
use std::collections::BTreeMap;
use tracing::warn;

use crate::Config;
use crate::action_engine::{
    condition_failed_telemetry, condition_skipped_telemetry, disabled_telemetry, skipped_telemetry,
};
use crate::bus::{EventBus, LineageHold};
use crate::condition::ConditionGate;
use crate::core_messages;
use crate::integration_gate::{IntegrationGate, run_gated_step, step_owner};

pub(crate) fn capture_args_in(
    registry: &SubActionRegistry,
    step: &SubActionStep,
    scope: &ArgStack,
) -> BTreeMap<String, String> {
    let Some(runner) = registry.get(&step.kind_id) else {
        return BTreeMap::new();
    };
    let hidden: Vec<&str> = runner
        .config_fields()
        .iter()
        .filter_map(|field| match field {
            forge_registry::FormField::SubChain { key, .. }
            | forge_registry::FormField::Code { key, .. } => Some(*key),
            _ => None,
        })
        .collect();
    let resolved = effective_config(&runner.default_config(), &step.config);
    let mut captured = BTreeMap::new();
    for (key, value) in &resolved {
        if hidden.contains(&key.as_str()) {
            continue;
        }
        let preview = match value {
            Variant::String(text) => {
                let interpolated = scope.interpolate(text);
                if interpolated.is_empty() {
                    continue;
                }
                variant_preview(&Variant::String(interpolated))
            }
            other => variant_preview(other),
        };
        captured.insert(key.clone(), preview);
    }
    captured
}

pub(crate) fn capture_produced(before: &ArgStack, after: &ArgStack) -> BTreeMap<String, String> {
    let mut produced = BTreeMap::new();
    for (name, value) in after.snapshot() {
        if before.get(&name) != Some(&value) {
            produced.insert(name, variant_preview(&value));
        }
    }
    produced
}

pub(crate) fn publish_subaction_done(
    publisher: &dyn EventPublisher,
    caused_by: EventId,
    telemetry: &SubActionTelemetry,
) {
    let message = match &telemetry.outcome {
        SubActionOutcome::Success => None,
        SubActionOutcome::Skipped(m) => Some(m.clone()),
        SubActionOutcome::Failed(_) | SubActionOutcome::IntegrationDisabled(_) => {
            telemetry.outcome.failure_reason()
        }
    };
    let integration = match &telemetry.outcome {
        SubActionOutcome::IntegrationDisabled(integration) => Some(integration.as_str().to_owned()),
        _ => None,
    };
    SubActionDone {
        step_index: telemetry.index,
        kind: telemetry.kind.clone(),
        outcome: RunOutcome::from(&telemetry.outcome),
        duration_ms: telemetry.duration_ms,
        message: message.as_ref().map(UserText::code_or_text),
        message_text: message,
        integration,
    }
    .publish(publisher, EventSource::Core, Some(caused_by));
}

pub(crate) async fn execute_step(
    registry: &SubActionRegistry,
    integrations: &IntegrationGate,
    runner: &dyn SubActionRunner,
    step: &SubActionStep,
    ctx: &RunContext<'_>,
) -> (SubActionTelemetry, Option<ArgStack>) {
    let resolved = effective_config(&runner.default_config(), &step.config);
    if let Err(problem) =
        validate_declared_config(runner, &resolved, Interpolation::Against(ctx.arg_stack))
    {
        return (
            StepTimer::start(ctx, &step.kind_id).invalid_config(&problem),
            None,
        );
    }
    let owner = step_owner(registry, runner, &step.kind_id, &resolved, ctx.arg_stack);
    run_gated_step(
        integrations,
        owner.as_ref(),
        ctx,
        &step.kind_id,
        runner.execute(&resolved, ctx),
    )
    .await
}

pub struct ChainEngine {
    registry: Arc<SubActionRegistry>,
    publisher: Arc<dyn EventPublisher>,
    lineage: Option<Arc<EventBus>>,
    gate: Arc<ConditionGate>,
    integrations: IntegrationGate,
    config: Config,
}

enum ConditionVerdict {
    Run,
    Skip,
    Fail(UserText),
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
        gate: Arc<ConditionGate>,
        config: Config,
    ) -> Self {
        Self {
            registry,
            publisher,
            lineage: None,
            gate,
            integrations: IntegrationGate::new(),
            config,
        }
    }

    pub(crate) fn on_bus(
        registry: Arc<SubActionRegistry>,
        bus: Arc<EventBus>,
        gate: Arc<ConditionGate>,
        config: Config,
    ) -> Self {
        Self {
            lineage: Some(Arc::clone(&bus)),
            ..Self::new(registry, bus, gate, config)
        }
    }

    fn publish_step_run(&self, run_event: Event) -> Option<LineageHold<'_>> {
        match self.lineage.as_deref() {
            Some(bus) => Some(bus.publish_held(run_event)),
            None => {
                self.publisher.publish(run_event);
                None
            }
        }
    }

    pub fn integration_gate(&self) -> &IntegrationGate {
        &self.integrations
    }

    async fn check_condition(&self, step: &SubActionStep, scope: &ArgStack) -> ConditionVerdict {
        let Some(cond) = step.condition.as_deref() else {
            return ConditionVerdict::Run;
        };
        if cond.trim().is_empty() {
            return ConditionVerdict::Run;
        }
        match self.gate.evaluate_with_args(cond, scope).await {
            Ok(true) => ConditionVerdict::Run,
            Ok(false) => ConditionVerdict::Skip,
            Err(e) => ConditionVerdict::Fail(core_messages::condition_failed(e).into()),
        }
    }

    pub fn root_scope(self: &Arc<Self>, cancel: CancelSignal) -> ChainScope {
        ChainScope {
            engine: Arc::clone(self),
            depth: 0,
            cancel,
            running: None,
        }
    }

    pub(crate) async fn run_action(
        self: &Arc<Self>,
        running: RunningAction,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        cancel: &CancelSignal,
    ) -> ChainRun {
        let scope = ChainScope {
            running: Some(running),
            ..self.root_scope(cancel.clone())
        };
        self.drive_sequential(steps, arg_stack, running.start_event_id, &scope)
            .await
    }

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

    async fn drive_sequential(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
        executor: &dyn ChainExecutor,
    ) -> ChainRun {
        let cancel = executor.cancel_signal();
        let control = ControlCell::new();
        let nested_sink = TelemetrySink::new();
        let mut current = arg_stack.clone();
        let mut telemetry = Vec::new();

        for (index, step) in steps.iter().enumerate() {
            if !step.enabled {
                let mut tel = disabled_telemetry(index, &step.kind_id);
                tel.args_in = capture_args_in(&self.registry, step, &current);
                publish_subaction_done(self.publisher.as_ref(), parent_event_id, &tel);
                telemetry.push(tel);
                continue;
            }
            if cancel.is_cancelled() {
                return ChainRun {
                    signal: ChainSignal::Aborted,
                    arg_stack: current,
                    telemetry,
                };
            }

            match self.check_condition(step, &current).await {
                ConditionVerdict::Run => {}
                ConditionVerdict::Skip => {
                    let mut tel = condition_skipped_telemetry(index, &step.kind_id);
                    tel.args_in = capture_args_in(&self.registry, step, &current);
                    publish_subaction_done(self.publisher.as_ref(), parent_event_id, &tel);
                    telemetry.push(tel);
                    continue;
                }
                ConditionVerdict::Fail(msg) => {
                    let mut tel = condition_failed_telemetry(index, &step.kind_id, msg.clone());
                    tel.args_in = capture_args_in(&self.registry, step, &current);
                    publish_subaction_done(self.publisher.as_ref(), parent_event_id, &tel);
                    telemetry.push(tel);
                    if !step.continue_on_error {
                        return ChainRun {
                            signal: ChainSignal::Error(msg),
                            arg_stack: current,
                            telemetry,
                        };
                    }
                    continue;
                }
            }

            let args_in = capture_args_in(&self.registry, step, &current);

            let run_event = SubActionRun {
                step_index: index,
                kind: step.kind_id.clone(),
            }
            .into_event(EventSource::Core, Some(parent_event_id));
            let run_event_id = run_event.id;
            let _step_lineage = self.publish_step_run(run_event);

            let run_ctx = RunContext {
                arg_stack: &current,
                index,
                parent_event_id: run_event_id,
                publisher: self.publisher.as_ref(),
                executor,
                cancel: cancel.clone(),
                control: control.clone(),
                telemetry: nested_sink.clone(),
            };

            let (mut tel, updated) = match self.registry.get(&step.kind_id) {
                Some(runner) => {
                    execute_step(&self.registry, &self.integrations, runner, step, &run_ctx).await
                }
                None => {
                    warn!(
                        "unknown sub-action kind_id: {} - skipping step",
                        step.kind_id
                    );
                    (skipped_telemetry(index, &step.kind_id), None)
                }
            };

            let nested = nested_sink.drain();

            let produced = match &updated {
                Some(new_stack) => capture_produced(&current, new_stack),
                None => BTreeMap::new(),
            };
            if let Some(new_stack) = updated {
                current = new_stack;
            }

            tel.args_in = args_in;
            tel.produced = produced;
            publish_subaction_done(self.publisher.as_ref(), run_event_id, &tel);

            let failure = tel.outcome.failure_reason();
            telemetry.push(tel);
            telemetry.extend(nested);

            if let Some(msg) = failure
                && !step.continue_on_error
            {
                return ChainRun {
                    signal: ChainSignal::Error(msg),
                    arg_stack: current,
                    telemetry,
                };
            }

            if let Some(control_signal) = control.take() {
                return ChainRun {
                    signal: match control_signal {
                        ControlSignal::Break => ChainSignal::Break,
                        ControlSignal::Continue => ChainSignal::Continue,
                        ControlSignal::Stop(mark) => ChainSignal::Stop(mark),
                    },
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
}

pub struct ChainScope {
    engine: Arc<ChainEngine>,
    depth: u32,
    cancel: CancelSignal,
    running: Option<RunningAction>,
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
            running: self.running,
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

    fn integration_availability(&self) -> Option<Arc<dyn IntegrationAvailability>> {
        Some(Arc::new(self.engine.integrations.clone()))
    }

    fn running_action(&self) -> Option<RunningAction> {
        self.running
    }
}

#[cfg(test)]
mod tests {
    use forge_events::test_support::NullPublisher;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use forge_registry::{
        ConfigSpec, IntegerEditor, IntegerField, SubActionCategory, SubActionConfig,
        SubActionRunner, ValueBounds,
    };
    use forge_types::{IntegrationId, UserText, Variant, integration_disabled_message};
    use time::OffsetDateTime;

    use super::*;

    struct ScriptedRunner {
        id: String,
        outcome: SubActionOutcome,
        set_binding: Option<(String, Variant)>,
        observed: Arc<Mutex<Vec<BTreeMap<String, Variant>>>>,
        cancel_on_run: Option<CancelSignal>,
        runs: Arc<AtomicUsize>,
        spec: Option<ConfigSpec>,
    }

    fn scripted(id: &str, outcome: SubActionOutcome) -> ScriptedRunner {
        ScriptedRunner {
            id: id.to_owned(),
            outcome,
            set_binding: None,
            observed: Arc::new(Mutex::new(Vec::new())),
            cancel_on_run: None,
            runs: Arc::new(AtomicUsize::new(0)),
            spec: None,
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
        fn config_spec(&self) -> Option<ConfigSpec> {
            self.spec
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
                args_in: BTreeMap::new(),
                produced: BTreeMap::new(),
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
        let publisher: Arc<dyn EventPublisher> = Arc::new(NullPublisher);
        let config = Config {
            max_nesting_depth,
            ..Default::default()
        };
        let gate = Arc::new(ConditionGate::new(&config));
        Arc::new(ChainEngine::new(reg, publisher, gate, config))
    }

    fn step(kind: &str) -> SubActionStep {
        SubActionStep {
            kind_id: kind.to_owned(),
            config: BTreeMap::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }
    }

    fn flagged_step(kind: &str) -> SubActionStep {
        SubActionStep {
            continue_on_error: true,
            ..step(kind)
        }
    }

    fn disabled_step(kind: &str) -> SubActionStep {
        SubActionStep {
            enabled: false,
            ..step(kind)
        }
    }

    fn step_if(kind: &str, condition: &str) -> SubActionStep {
        SubActionStep {
            condition: Some(condition.to_owned()),
            ..step(kind)
        }
    }

    struct CapturingPublisher {
        events: Arc<Mutex<Vec<Event>>>,
    }

    impl EventPublisher for CapturingPublisher {
        fn publish(&self, event: Event) {
            self.events.lock().unwrap().push(event);
        }
    }

    fn capturing_engine(
        reg: Arc<SubActionRegistry>,
        max_nesting_depth: u32,
    ) -> (Arc<ChainEngine>, Arc<Mutex<Vec<Event>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let publisher: Arc<dyn EventPublisher> = Arc::new(CapturingPublisher {
            events: Arc::clone(&events),
        });
        let config = Config {
            max_nesting_depth,
            ..Default::default()
        };
        let gate = Arc::new(ConditionGate::new(&config));
        let eng = Arc::new(ChainEngine::new(reg, publisher, gate, config));
        (eng, events)
    }

    fn done_events(events: &Arc<Mutex<Vec<Event>>>) -> Vec<(usize, String, Option<EventId>)> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "subaction.done")
            .map(|e| {
                let idx = e.payload["step_index"].as_u64().unwrap() as usize;
                let outcome = e.payload["outcome"].as_str().unwrap().to_owned();
                (idx, outcome, e.caused_by)
            })
            .collect()
    }

    fn run_ids(events: &Arc<Mutex<Vec<Event>>>) -> BTreeMap<usize, EventId> {
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "subaction.run")
            .map(|e| (e.payload["step_index"].as_u64().unwrap() as usize, e.id))
            .collect()
    }

    #[tokio::test]
    async fn child_chain_entry_admitted_only_within_the_depth_bound() {
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

    #[test]
    fn cancel_signal_propagates_through_clones() {
        let signal = CancelSignal::new();
        let child = signal.clone();
        assert!(!child.is_cancelled());
        signal.cancel();
        assert!(child.is_cancelled());
    }

    #[tokio::test]
    async fn sequential_step_failure_halts_chain_with_error_signal() {
        let downstream_runs = Arc::new(AtomicUsize::new(0));
        let first = scripted(
            "seq.fail",
            SubActionOutcome::Failed(forge_types::VerbatimText::new("boom").into()),
        );
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

        assert_eq!(
            run.signal,
            ChainSignal::Error(forge_types::VerbatimText::new("boom").into())
        );
        assert_eq!(run.telemetry.len(), 1);
        assert!(
            matches!(&run.telemetry[0].outcome, SubActionOutcome::Failed(m) if m.code_or_text() == "boom"),
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
    async fn disabled_step_is_recorded_as_skipped_at_its_real_index_but_not_executed() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut mid = scripted("seq.disabled", SubActionOutcome::Success);
        mid.runs = Arc::clone(&runs);
        let eng = engine(
            registry(vec![
                Box::new(scripted("seq.before", SubActionOutcome::Success)),
                Box::new(mid),
                Box::new(scripted("seq.after", SubActionOutcome::Success)),
            ]),
            8,
        );

        let run = eng
            .run_sequential(
                &[
                    step("seq.before"),
                    disabled_step("seq.disabled"),
                    step("seq.after"),
                ],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Completed);
        assert_eq!(run.telemetry.len(), 3);
        assert_eq!(run.telemetry[0].kind, "seq.before");
        assert_eq!(run.telemetry[1].index, 1);
        assert_eq!(run.telemetry[1].kind, "seq.disabled");
        assert!(
            matches!(&run.telemetry[1].outcome, SubActionOutcome::Skipped(m) if m.code_or_text() == "core.step_disabled"),
        );
        assert_eq!(run.telemetry[2].kind, "seq.after");
        assert_eq!(runs.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn sequential_flagged_step_failure_lets_later_steps_run_and_chain_completes() {
        let downstream_runs = Arc::new(AtomicUsize::new(0));
        let first = scripted(
            "seq.flagfail",
            SubActionOutcome::Failed(forge_types::VerbatimText::new("boom").into()),
        );
        let mut second = scripted("seq.after", SubActionOutcome::Success);
        second.runs = Arc::clone(&downstream_runs);

        let eng = engine(registry(vec![Box::new(first), Box::new(second)]), 8);
        let run = eng
            .run_sequential(
                &[flagged_step("seq.flagfail"), step("seq.after")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Completed);
        assert_eq!(run.telemetry.len(), 2);
        assert!(
            matches!(&run.telemetry[0].outcome, SubActionOutcome::Failed(m) if m.code_or_text() == "boom"),
            "flagged failure is still recorded as a Failed row",
        );
        assert_eq!(run.telemetry[1].outcome, SubActionOutcome::Success);
        assert_eq!(
            downstream_runs.load(Ordering::Relaxed),
            1,
            "step after a flagged failure must still execute",
        );
    }

    const LIMIT: IntegerField = IntegerField {
        key: "limit",
        label: "Limit",
        editor: IntegerEditor::Number,
        default: 1,
        bounds: ValueBounds {
            min: Some(1),
            max: Some(5),
        },
    };

    fn invalid_limit_failure() -> UserText {
        crate::test_support::failure::step_problem(
            "cfg.limited",
            &forge_registry::ConfigProblem::field(
                "limit",
                forge_registry::ConfigReason::OutOfRange(ValueBounds::between(1, 5)),
            ),
        )
    }

    fn limited(id: &str) -> ScriptedRunner {
        ScriptedRunner {
            spec: Some(ConfigSpec::new(&[&LIMIT])),
            ..scripted(id, SubActionOutcome::Success)
        }
    }

    fn with_limit(step: SubActionStep, limit: Variant) -> SubActionStep {
        SubActionStep {
            config: BTreeMap::from([("limit".to_owned(), limit)]),
            ..step
        }
    }

    #[tokio::test]
    async fn an_invalid_step_config_fails_the_step_without_running_it_and_halts_the_chain() {
        let limited_runner = limited("cfg.limited");
        let limited_runs = Arc::clone(&limited_runner.runs);
        let after = scripted("cfg.after", SubActionOutcome::Success);
        let after_runs = Arc::clone(&after.runs);
        let eng = engine(registry(vec![Box::new(limited_runner), Box::new(after)]), 8);

        let run = eng
            .run_sequential(
                &[
                    with_limit(step("cfg.limited"), Variant::Int(9)),
                    step("cfg.after"),
                ],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(
            (
                run.signal,
                run.telemetry
                    .iter()
                    .map(|row| row.outcome.clone())
                    .collect::<Vec<_>>(),
                limited_runs.load(Ordering::Relaxed),
                after_runs.load(Ordering::Relaxed),
            ),
            (
                ChainSignal::Error(invalid_limit_failure()),
                vec![SubActionOutcome::Failed(invalid_limit_failure())],
                0,
                0,
            )
        );
    }

    #[tokio::test]
    async fn continue_on_error_carries_the_chain_past_an_invalid_step_config() {
        let limited_runner = limited("cfg.limited");
        let limited_runs = Arc::clone(&limited_runner.runs);
        let after = scripted("cfg.after", SubActionOutcome::Success);
        let after_runs = Arc::clone(&after.runs);
        let eng = engine(registry(vec![Box::new(limited_runner), Box::new(after)]), 8);

        let run = eng
            .run_sequential(
                &[
                    with_limit(flagged_step("cfg.limited"), Variant::Int(0)),
                    step("cfg.after"),
                ],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(
            (
                run.signal,
                run.telemetry
                    .iter()
                    .map(|row| row.outcome.clone())
                    .collect::<Vec<_>>(),
                limited_runs.load(Ordering::Relaxed),
                after_runs.load(Ordering::Relaxed),
            ),
            (
                ChainSignal::Completed,
                vec![
                    SubActionOutcome::Failed(invalid_limit_failure()),
                    SubActionOutcome::Success,
                ],
                0,
                1,
            )
        );
    }

    #[tokio::test]
    async fn a_templated_step_config_is_checked_against_the_run_arguments() {
        for (size, outcome, runs) in [
            (3, SubActionOutcome::Success, 1),
            (6, SubActionOutcome::Failed(invalid_limit_failure()), 0),
        ] {
            let limited_runner = limited("cfg.limited");
            let limited_runs = Arc::clone(&limited_runner.runs);
            let eng = engine(registry(vec![Box::new(limited_runner)]), 8);

            let run = eng
                .run_sequential(
                    &[with_limit(
                        flagged_step("cfg.limited"),
                        Variant::String("%size%".to_owned()),
                    )],
                    &ArgStack::new().set("size".to_owned(), Variant::Int(size)),
                    EventId::new(),
                    &CancelSignal::new(),
                )
                .await;

            assert_eq!(
                (
                    run.telemetry[0].outcome.clone(),
                    limited_runs.load(Ordering::Relaxed)
                ),
                (outcome, runs),
                "size {size}"
            );
        }
    }

    #[tokio::test]
    async fn sequential_unflagged_failure_after_a_flagged_one_still_halts() {
        let third_runs = Arc::new(AtomicUsize::new(0));
        let mut third = scripted("seq.tail", SubActionOutcome::Success);
        third.runs = Arc::clone(&third_runs);
        let eng = engine(
            registry(vec![
                Box::new(scripted(
                    "seq.flagfail",
                    SubActionOutcome::Failed(forge_types::VerbatimText::new("tolerated").into()),
                )),
                Box::new(scripted(
                    "seq.hardfail",
                    SubActionOutcome::Failed(forge_types::VerbatimText::new("halt").into()),
                )),
                Box::new(third),
            ]),
            8,
        );
        let run = eng
            .run_sequential(
                &[
                    flagged_step("seq.flagfail"),
                    step("seq.hardfail"),
                    step("seq.tail"),
                ],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(
            run.signal,
            ChainSignal::Error(forge_types::VerbatimText::new("halt").into())
        );
        assert_eq!(run.telemetry.len(), 2);
        assert_eq!(third_runs.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn subaction_done_outcome_string_reflects_each_step_outcome() {
        let eng_reg = registry(vec![
            Box::new(scripted("d.ok", SubActionOutcome::Success)),
            Box::new(scripted(
                "d.fail",
                SubActionOutcome::Failed(forge_types::VerbatimText::new("nope").into()),
            )),
            Box::new(scripted("d.off", SubActionOutcome::Success)),
        ]);
        let (eng, events) = capturing_engine(eng_reg, 8);
        eng.run_sequential(
            &[step("d.ok"), flagged_step("d.fail"), disabled_step("d.off")],
            &ArgStack::new(),
            EventId::new(),
            &CancelSignal::new(),
        )
        .await;

        let mut done = done_events(&events);
        done.sort_by_key(|(idx, _, _)| *idx);
        let outcomes: Vec<(usize, &str)> = done.iter().map(|(i, o, _)| (*i, o.as_str())).collect();
        assert_eq!(
            outcomes,
            vec![(0, "success"), (1, "failed"), (2, "skipped")],
        );

        let failed_msg = events
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.kind == "subaction.done" && e.payload["outcome"] == "failed")
            .and_then(|e| e.payload["message"].as_str().map(str::to_owned));
        assert_eq!(failed_msg.as_deref(), Some("nope"));
    }

    #[tokio::test]
    async fn subaction_done_links_run_id_for_executed_and_parent_for_disabled() {
        let (eng, events) = capturing_engine(
            registry(vec![
                Box::new(scripted("l.ok", SubActionOutcome::Success)),
                Box::new(scripted("l.off", SubActionOutcome::Success)),
            ]),
            8,
        );
        let parent = EventId::new();
        eng.run_sequential(
            &[step("l.ok"), disabled_step("l.off")],
            &ArgStack::new(),
            parent,
            &CancelSignal::new(),
        )
        .await;

        let runs = run_ids(&events);
        assert!(runs.contains_key(&0));
        assert!(
            !runs.contains_key(&1),
            "disabled step must not emit subaction.run"
        );

        for (idx, _outcome, caused_by) in done_events(&events) {
            match idx {
                0 => assert_eq!(
                    caused_by,
                    Some(runs[&0]),
                    "executed step's done is caused by its own run event",
                ),
                1 => assert_eq!(
                    caused_by,
                    Some(parent),
                    "disabled step's done is caused by the parent event",
                ),
                other => panic!("unexpected step index {other}"),
            }
        }
    }

    #[tokio::test]
    async fn condition_verdict_controls_whether_the_step_runs() {
        for (condition, should_run) in [
            (Some("1 == 1"), true),  // true -> run
            (Some("1 == 2"), false), // false -> skip
            (Some(""), true),        // empty -> run
            (Some("   "), true),     // whitespace-only -> run
            (None, true),            // absent -> run
        ] {
            let runs = Arc::new(AtomicUsize::new(0));
            let mut r = scripted("g.step", SubActionOutcome::Success);
            r.runs = Arc::clone(&runs);
            let eng = engine(registry(vec![Box::new(r)]), 8);

            let the_step = match condition {
                Some(c) => step_if("g.step", c),
                None => step("g.step"),
            };
            let run = eng
                .run_sequential(
                    &[the_step],
                    &ArgStack::new(),
                    EventId::new(),
                    &CancelSignal::new(),
                )
                .await;

            assert_eq!(run.signal, ChainSignal::Completed, "cond {condition:?}");
            assert_eq!(run.telemetry.len(), 1);
            assert_eq!(
                runs.load(Ordering::Relaxed),
                should_run as usize,
                "cond {condition:?}"
            );
            if should_run {
                assert_eq!(run.telemetry[0].outcome, SubActionOutcome::Success);
            } else {
                assert!(
                    matches!(&run.telemetry[0].outcome, SubActionOutcome::Skipped(m) if m.code_or_text() == "core.condition_not_met"),
                    "false condition skips with its own code, got {:?}",
                    run.telemetry[0].outcome,
                );
            }
        }
    }

    #[tokio::test]
    async fn false_condition_skips_at_real_index_and_later_steps_still_run() {
        let gated_runs = Arc::new(AtomicUsize::new(0));
        let after_runs = Arc::new(AtomicUsize::new(0));
        let mut gated = scripted("c.gated", SubActionOutcome::Success);
        gated.runs = Arc::clone(&gated_runs);
        let mut after = scripted("c.after", SubActionOutcome::Success);
        after.runs = Arc::clone(&after_runs);
        let eng = engine(
            registry(vec![
                Box::new(scripted("c.before", SubActionOutcome::Success)),
                Box::new(gated),
                Box::new(after),
            ]),
            8,
        );
        let run = eng
            .run_sequential(
                &[
                    step("c.before"),
                    step_if("c.gated", "1 == 2"),
                    step("c.after"),
                ],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Completed);
        assert_eq!(run.telemetry.len(), 3);
        assert_eq!(run.telemetry[1].index, 1);
        assert_eq!(run.telemetry[1].kind, "c.gated");
        assert!(
            matches!(&run.telemetry[1].outcome, SubActionOutcome::Skipped(m) if m.code_or_text() == "core.condition_not_met"),
        );
        assert_eq!(gated_runs.load(Ordering::Relaxed), 0);
        assert_eq!(
            after_runs.load(Ordering::Relaxed),
            1,
            "step after a skipped one must still run",
        );
    }

    #[tokio::test]
    async fn condition_eval_error_halts_sequential_chain_unless_step_is_flagged() {
        for (flagged, expect_after_runs, expect_completed) in
            [(false, 0usize, false), (true, 1usize, true)]
        {
            let gated_runs = Arc::new(AtomicUsize::new(0));
            let after_runs = Arc::new(AtomicUsize::new(0));
            let mut gated = scripted("e.gated", SubActionOutcome::Success);
            gated.runs = Arc::clone(&gated_runs);
            let mut after = scripted("e.after", SubActionOutcome::Success);
            after.runs = Arc::clone(&after_runs);
            let eng = engine(registry(vec![Box::new(gated), Box::new(after)]), 8);
            let gated_step = SubActionStep {
                condition: Some("1 + 1".to_owned()),
                continue_on_error: flagged,
                ..step("e.gated")
            };
            let run = eng
                .run_sequential(
                    &[gated_step, step("e.after")],
                    &ArgStack::new(),
                    EventId::new(),
                    &CancelSignal::new(),
                )
                .await;

            assert!(
                matches!(&run.telemetry[0].outcome, SubActionOutcome::Failed(m) if m.code_or_text() == "core.condition_failed"),
                "flagged={flagged}: gated row must be a condition-eval failure, got {:?}",
                run.telemetry[0].outcome,
            );
            assert_eq!(
                gated_runs.load(Ordering::Relaxed),
                0,
                "flagged={flagged}: a condition error must not run the subaction",
            );
            assert_eq!(
                after_runs.load(Ordering::Relaxed),
                expect_after_runs,
                "flagged={flagged}",
            );
            if expect_completed {
                assert_eq!(run.signal, ChainSignal::Completed, "flagged={flagged}");
            } else {
                assert!(
                    matches!(&run.signal, ChainSignal::Error(m) if m.code_or_text() == "core.condition_failed"),
                    "flagged={flagged}: {:?}",
                    run.signal,
                );
            }
        }
    }

    #[tokio::test]
    async fn condition_is_interpolated_against_scope_before_evaluation() {
        for (produced, should_run) in [(7i64, true), (3i64, false)] {
            let gated_runs = Arc::new(AtomicUsize::new(0));
            let mut producer = scripted("s.producer", SubActionOutcome::Success);
            producer.set_binding = Some(("n".to_owned(), Variant::Int(produced)));
            let mut gated = scripted("s.gated", SubActionOutcome::Success);
            gated.runs = Arc::clone(&gated_runs);
            let eng = engine(registry(vec![Box::new(producer), Box::new(gated)]), 8);
            let run = eng
                .run_sequential(
                    &[step("s.producer"), step_if("s.gated", "%n% == 7")],
                    &ArgStack::new(),
                    EventId::new(),
                    &CancelSignal::new(),
                )
                .await;

            assert_eq!(run.signal, ChainSignal::Completed, "produced={produced}");
            assert_eq!(
                gated_runs.load(Ordering::Relaxed),
                should_run as usize,
                "produced={produced}",
            );
        }
    }

    fn twitch_owned_registry(
        twitch_runs: &Arc<AtomicUsize>,
        core_runs: &Arc<AtomicUsize>,
    ) -> Arc<SubActionRegistry> {
        let mut owned = scripted("twitch.step", SubActionOutcome::Success);
        owned.runs = Arc::clone(twitch_runs);
        let mut core = scripted("core.after", SubActionOutcome::Success);
        core.runs = Arc::clone(core_runs);
        let mut reg = SubActionRegistry::new();
        reg.owned_by(IntegrationId::from_static("twitch"))
            .register(Box::new(owned))
            .unwrap();
        reg.register(Box::new(core)).unwrap();
        Arc::new(reg)
    }

    #[tokio::test]
    async fn a_step_of_a_disabled_integration_fails_the_sequential_chain_naming_the_integration() {
        let twitch_runs = Arc::new(AtomicUsize::new(0));
        let core_runs = Arc::new(AtomicUsize::new(0));
        let (eng, events) = capturing_engine(twitch_owned_registry(&twitch_runs, &core_runs), 8);
        eng.integration_gate()
            .disable(IntegrationId::from_static("twitch"));

        let run = eng
            .run_sequential(
                &[step("twitch.step"), step("core.after")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(
            run.signal,
            ChainSignal::Error(UserText::from(integration_disabled_message(
                &IntegrationId::from_static("twitch")
            )))
        );
        assert_eq!(twitch_runs.load(Ordering::Relaxed), 0);
        assert_eq!(core_runs.load(Ordering::Relaxed), 0, "the chain must stop");
        let done = events
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.kind == "subaction.done")
            .cloned()
            .expect("the disabled step must still report subaction.done");
        assert_eq!(done.payload["outcome"], "failed");
        assert_eq!(done.payload["integration"], "twitch");
    }

    #[tokio::test]
    async fn a_coded_step_failure_publishes_its_code_as_the_message_and_the_coded_text_beside_it() {
        let twitch = IntegrationId::from_static("twitch");
        let (eng, events) = capturing_engine(
            twitch_owned_registry(
                &Arc::new(AtomicUsize::new(0)),
                &Arc::new(AtomicUsize::new(0)),
            ),
            8,
        );
        eng.integration_gate().disable(twitch.clone());

        eng.run_sequential(
            &[step("twitch.step")],
            &ArgStack::new(),
            EventId::new(),
            &CancelSignal::new(),
        )
        .await;

        let done = events
            .lock()
            .unwrap()
            .iter()
            .find(|e| e.kind == "subaction.done")
            .cloned()
            .expect("the disabled step reports subaction.done");
        let text: UserText =
            serde_json::from_value(done.payload["message_text"].clone()).expect("coded text");
        assert_eq!(
            (done.payload["message"].as_str(), text),
            (
                Some("common.integration_disabled"),
                UserText::from(integration_disabled_message(&twitch))
            )
        );
    }

    #[tokio::test]
    async fn continue_on_error_carries_the_chain_past_a_disabled_integration_step() {
        let twitch_runs = Arc::new(AtomicUsize::new(0));
        let core_runs = Arc::new(AtomicUsize::new(0));
        let eng = engine(twitch_owned_registry(&twitch_runs, &core_runs), 8);
        eng.integration_gate()
            .disable(IntegrationId::from_static("twitch"));

        let run = eng
            .run_sequential(
                &[flagged_step("twitch.step"), step("core.after")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Completed);
        assert_eq!(
            run.telemetry[0].outcome,
            SubActionOutcome::IntegrationDisabled(IntegrationId::from_static("twitch"))
        );
        assert_eq!(core_runs.load(Ordering::Relaxed), 1);
    }

    fn chat_send_registry() -> Arc<SubActionRegistry> {
        let mut reg = SubActionRegistry::new();
        reg.register(Box::new(
            crate::sub_action_runners::TwitchChatSendMessageRunner,
        ))
        .unwrap();
        Arc::new(reg)
    }

    fn chat_send_to(target: &str) -> SubActionStep {
        SubActionStep {
            config: BTreeMap::from([
                ("message".to_owned(), Variant::String("hello".to_owned())),
                ("target".to_owned(), Variant::String(target.to_owned())),
            ]),
            ..step("twitch.chat.send_message")
        }
    }

    fn send_requests(events: &Arc<Mutex<Vec<Event>>>) -> usize {
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.kind == "chat.send.request")
            .count()
    }

    #[tokio::test]
    async fn a_chat_send_aimed_by_argument_at_a_disabled_platform_fails_unsent() {
        let (eng, events) = capturing_engine(chat_send_registry(), 8);
        eng.integration_gate()
            .disable(IntegrationId::from_static("kick"));
        let args = ArgStack::new().set("platform".to_owned(), Variant::String("kick".to_owned()));

        let run = eng
            .run_sequential(
                &[chat_send_to("%platform%")],
                &args,
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(
            run.telemetry[0].outcome,
            SubActionOutcome::IntegrationDisabled(IntegrationId::from_static("kick"))
        );
        assert_eq!(send_requests(&events), 0);
    }

    #[tokio::test]
    async fn a_chat_send_to_an_enabled_platform_is_published_while_another_is_disabled() {
        let (eng, events) = capturing_engine(chat_send_registry(), 8);
        eng.integration_gate()
            .disable(IntegrationId::from_static("twitch"));

        let run = eng
            .run_sequential(
                &[chat_send_to("kick")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(run.signal, ChainSignal::Completed);
        assert_eq!(send_requests(&events), 1);
    }

    #[tokio::test]
    async fn a_blank_target_chat_send_fails_the_run_unsent_when_every_chat_platform_is_disabled() {
        let (eng, events) = capturing_engine(chat_send_registry(), 8);
        for platform in ["twitch", "youtube", "kick"] {
            eng.integration_gate()
                .disable(IntegrationId::from_static(platform));
        }

        let run = eng
            .run_sequential(
                &[chat_send_to("")],
                &ArgStack::new(),
                EventId::new(),
                &CancelSignal::new(),
            )
            .await;

        assert_eq!(
            run.telemetry[0].outcome,
            SubActionOutcome::Failed(
                forge_types::MessageRef::new(forge_types::NO_CHAT_PLATFORM_ENABLED).into()
            )
        );
        assert_eq!(send_requests(&events), 0);
    }
}
