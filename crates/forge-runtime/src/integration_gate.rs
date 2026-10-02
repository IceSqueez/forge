use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use forge_registry::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, RegistryError, RunContext,
    StepTimer, SubActionRegistry, SubActionRunner,
};
use forge_types::{
    ArgStack, EventId, IntegrationAvailability, IntegrationId, SubActionConfig, SubActionOutcome,
    SubActionStep, SubActionTelemetry,
};
use tokio::sync::Notify;

#[derive(Clone, Default)]
pub struct IntegrationGate {
    state: Arc<Mutex<GateState>>,
}

#[derive(Default)]
struct GateState {
    disabled: HashSet<IntegrationId>,
    next_step_id: u64,
    in_flight: HashMap<IntegrationId, HashMap<u64, Arc<Notify>>>,
}

impl IntegrationGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn disable(&self, integration: IntegrationId) -> usize {
        let mut state = self.lock();
        let cancelled = state.in_flight.remove(&integration).map_or(0, |steps| {
            for revoked in steps.values() {
                revoked.notify_one();
            }
            steps.len()
        });
        state.disabled.insert(integration);
        cancelled
    }

    pub fn enable(&self, integration: &IntegrationId) {
        self.lock().disabled.remove(integration);
    }

    pub fn is_disabled(&self, integration: &IntegrationId) -> bool {
        self.lock().disabled.contains(integration)
    }

    fn admit(&self, integration: &IntegrationId) -> Option<AdmittedStep> {
        let mut state = self.lock();
        if state.disabled.contains(integration) {
            return None;
        }
        let step_id = state.next_step_id;
        state.next_step_id = state.next_step_id.wrapping_add(1);
        let revoked = Arc::new(Notify::new());
        state
            .in_flight
            .entry(integration.clone())
            .or_default()
            .insert(step_id, Arc::clone(&revoked));
        Some(AdmittedStep {
            gate: self.clone(),
            integration: integration.clone(),
            step_id,
            revoked,
        })
    }

    fn release(&self, integration: &IntegrationId, step_id: u64) {
        let mut state = self.lock();
        if let Some(steps) = state.in_flight.get_mut(integration) {
            steps.remove(&step_id);
            if steps.is_empty() {
                state.in_flight.remove(integration);
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, GateState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl IntegrationAvailability for IntegrationGate {
    fn is_disabled(&self, integration: &IntegrationId) -> bool {
        IntegrationGate::is_disabled(self, integration)
    }
}

pub(crate) struct GatedLeafExecutor {
    gate: IntegrationGate,
    cancel: CancelSignal,
}

impl GatedLeafExecutor {
    pub(crate) fn new(gate: IntegrationGate, cancel: CancelSignal) -> Self {
        Self { gate, cancel }
    }
}

#[async_trait]
impl ChainExecutor for GatedLeafExecutor {
    async fn run_child_chain(
        &self,
        _steps: &[SubActionStep],
        arg_stack: &ArgStack,
        _parent_event_id: EventId,
    ) -> Result<ChildChainOutcome, RegistryError> {
        Ok(ChildChainOutcome {
            signal: ChainSignal::Completed,
            arg_stack: arg_stack.clone(),
            telemetry: Vec::new(),
        })
    }

    fn cancel_signal(&self) -> CancelSignal {
        self.cancel.clone()
    }

    fn integration_availability(&self) -> Option<Arc<dyn IntegrationAvailability>> {
        Some(Arc::new(self.gate.clone()))
    }
}

struct AdmittedStep {
    gate: IntegrationGate,
    integration: IntegrationId,
    step_id: u64,
    revoked: Arc<Notify>,
}

impl Drop for AdmittedStep {
    fn drop(&mut self) {
        self.gate.release(&self.integration, self.step_id);
    }
}

pub(crate) fn step_owner(
    registry: &SubActionRegistry,
    runner: &dyn SubActionRunner,
    kind_id: &str,
    config: &SubActionConfig,
    arg_stack: &ArgStack,
) -> Option<IntegrationId> {
    registry
        .owning_integration(kind_id)
        .cloned()
        .or_else(|| runner.targeted_integration(config, arg_stack))
}

pub(crate) async fn run_gated_step<F>(
    gate: &IntegrationGate,
    owner: Option<&IntegrationId>,
    ctx: &RunContext<'_>,
    kind_id: &str,
    run: F,
) -> (SubActionTelemetry, Option<ArgStack>)
where
    F: Future<Output = (SubActionTelemetry, Option<ArgStack>)>,
{
    let Some(owner) = owner else {
        return run.await;
    };
    let timer = StepTimer::start(ctx, kind_id);
    let Some(admitted) = gate.admit(owner) else {
        return (
            timer.finish(SubActionOutcome::IntegrationDisabled(owner.clone())),
            None,
        );
    };
    tokio::select! {
        biased;
        () = admitted.revoked.notified() => (
            timer.finish(SubActionOutcome::IntegrationDisabled(owner.clone())),
            None,
        ),
        result = run => result,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::pin::pin;
    use std::sync::atomic::{AtomicBool, Ordering};

    use forge_events::{Event, EventPublisher};
    use forge_types::EventId;
    use futures_util::FutureExt;
    use tokio::sync::oneshot;

    use super::*;

    struct NoopPublisher;

    impl EventPublisher for NoopPublisher {
        fn publish(&self, _: Event) {}
    }

    struct DropFlag(Arc<AtomicBool>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn twitch() -> IntegrationId {
        IntegrationId::from_static("twitch")
    }

    fn kick() -> IntegrationId {
        IntegrationId::from_static("kick")
    }

    fn telemetry(ctx: &RunContext<'_>, outcome: SubActionOutcome) -> SubActionTelemetry {
        StepTimer::start(ctx, "test.step").finish(outcome)
    }

    async fn run_once(
        gate: &IntegrationGate,
        owner: Option<&IntegrationId>,
        ran: &AtomicBool,
    ) -> SubActionOutcome {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);
        let (tel, _) = run_gated_step(gate, owner, &ctx, "test.step", async {
            ran.store(true, Ordering::SeqCst);
            (telemetry(&ctx, SubActionOutcome::Success), None)
        })
        .await;
        tel.outcome
    }

    #[tokio::test]
    async fn a_step_of_a_disabled_integration_never_runs_and_fails_as_integration_disabled() {
        let gate = IntegrationGate::new();
        gate.disable(twitch());
        let ran = AtomicBool::new(false);

        let outcome = run_once(&gate, Some(&twitch()), &ran).await;

        assert_eq!(outcome, SubActionOutcome::IntegrationDisabled(twitch()));
        assert!(!ran.load(Ordering::SeqCst), "the runner must not execute");
    }

    #[tokio::test]
    async fn a_step_of_an_enabled_integration_runs_and_keeps_its_own_outcome() {
        let gate = IntegrationGate::new();
        gate.disable(kick());
        let ran = AtomicBool::new(false);

        let outcome = run_once(&gate, Some(&twitch()), &ran).await;

        assert_eq!(outcome, SubActionOutcome::Success);
        assert!(ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn a_core_step_runs_regardless_of_disabled_integrations() {
        let gate = IntegrationGate::new();
        gate.disable(twitch());
        let ran = AtomicBool::new(false);

        let outcome = run_once(&gate, None, &ran).await;

        assert_eq!(outcome, SubActionOutcome::Success);
        assert!(ran.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn re_enabling_an_integration_admits_its_steps_again() {
        let gate = IntegrationGate::new();
        gate.disable(twitch());
        gate.enable(&twitch());
        let ran = AtomicBool::new(false);

        let outcome = run_once(&gate, Some(&twitch()), &ran).await;

        assert_eq!(outcome, SubActionOutcome::Success);
        assert!(!gate.is_disabled(&twitch()));
    }

    #[tokio::test]
    async fn disabling_mid_flight_cancels_the_running_step_and_drops_its_work() {
        let gate = IntegrationGate::new();
        let owner = twitch();
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);
        let dropped = Arc::new(AtomicBool::new(false));
        let guard = DropFlag(Arc::clone(&dropped));
        let (_never_tx, never_rx) = oneshot::channel::<()>();
        let mut step = pin!(run_gated_step(
            &gate,
            Some(&owner),
            &ctx,
            "test.step",
            async {
                let _guard = guard;
                let _ = never_rx.await;
                (telemetry(&ctx, SubActionOutcome::Success), None)
            },
        ));
        assert!(
            step.as_mut().now_or_never().is_none(),
            "step must be in flight"
        );

        let cancelled = gate.disable(twitch());
        let (tel, produced) = step.await;

        assert_eq!(cancelled, 1);
        assert_eq!(tel.outcome, SubActionOutcome::IntegrationDisabled(twitch()));
        assert!(produced.is_none());
        assert!(
            dropped.load(Ordering::SeqCst),
            "the runner future must be dropped"
        );
    }

    #[tokio::test]
    async fn disabling_another_integration_leaves_an_in_flight_step_running() {
        let gate = IntegrationGate::new();
        let owner = twitch();
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let mut step = pin!(run_gated_step(
            &gate,
            Some(&owner),
            &ctx,
            "test.step",
            async {
                let _ = release_rx.await;
                (telemetry(&ctx, SubActionOutcome::Success), None)
            },
        ));
        assert!(step.as_mut().now_or_never().is_none());

        let cancelled = gate.disable(kick());
        release_tx.send(()).unwrap();
        let (tel, _) = step.await;

        assert_eq!(cancelled, 0);
        assert_eq!(tel.outcome, SubActionOutcome::Success);
    }

    #[tokio::test]
    async fn finished_and_abandoned_steps_are_released_from_the_in_flight_set() {
        let gate = IntegrationGate::new();
        let owner = twitch();
        let ran = AtomicBool::new(false);
        run_once(&gate, Some(&twitch()), &ran).await;

        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);
        let (_never_tx, never_rx) = oneshot::channel::<()>();
        {
            let mut abandoned = pin!(run_gated_step(
                &gate,
                Some(&owner),
                &ctx,
                "test.step",
                async {
                    let _ = never_rx.await;
                    (telemetry(&ctx, SubActionOutcome::Success), None)
                },
            ));
            assert!(abandoned.as_mut().now_or_never().is_none());
        }

        assert_eq!(gate.disable(twitch()), 0);
    }

    #[tokio::test]
    async fn every_in_flight_step_of_the_integration_is_cancelled_at_once() {
        let gate = IntegrationGate::new();
        let owner = twitch();
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NoopPublisher);
        let (_tx_a, rx_a) = oneshot::channel::<()>();
        let (_tx_b, rx_b) = oneshot::channel::<()>();
        let mut first = pin!(run_gated_step(&gate, Some(&owner), &ctx, "a", async {
            let _ = rx_a.await;
            (telemetry(&ctx, SubActionOutcome::Success), None)
        }));
        let mut second = pin!(run_gated_step(&gate, Some(&owner), &ctx, "b", async {
            let _ = rx_b.await;
            (telemetry(&ctx, SubActionOutcome::Success), None)
        }));
        assert!(first.as_mut().now_or_never().is_none());
        assert!(second.as_mut().now_or_never().is_none());

        assert_eq!(gate.disable(twitch()), 2);
        let (a, b) = tokio::join!(first, second);

        assert_eq!(a.0.outcome, SubActionOutcome::IntegrationDisabled(twitch()));
        assert_eq!(b.0.outcome, SubActionOutcome::IntegrationDisabled(twitch()));
    }

    #[tokio::test]
    async fn the_quick_action_executor_reports_live_gate_state_to_scripts() {
        let gate = IntegrationGate::new();
        let executor = GatedLeafExecutor::new(gate.clone(), CancelSignal::new());
        let availability = executor
            .integration_availability()
            .expect("the quick-action executor must expose integration availability");

        gate.disable(twitch());
        let while_disabled = availability.is_disabled(&twitch());
        gate.enable(&twitch());
        let after_enable = availability.is_disabled(&twitch());

        assert!(
            while_disabled,
            "a disable after construction must be visible"
        );
        assert!(!after_enable, "a re-enable must be visible");
    }

    #[tokio::test]
    async fn the_quick_action_executor_completes_child_chains_without_running_them() {
        let executor = GatedLeafExecutor::new(IntegrationGate::new(), CancelSignal::new());
        let stack = ArgStack::new().set("user".to_owned(), forge_types::Variant::Int(7));
        let steps = [SubActionStep {
            kind_id: "test.step".to_owned(),
            config: SubActionConfig::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }];

        let outcome = executor
            .run_child_chain(&steps, &stack, EventId::new())
            .await
            .unwrap();

        assert_eq!(outcome.signal, ChainSignal::Completed);
        assert!(outcome.telemetry.is_empty(), "no child step may run");
        assert_eq!(outcome.arg_stack.snapshot(), stack.snapshot());
    }
}
