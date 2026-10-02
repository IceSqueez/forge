use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard};

use forge_registry::{RunContext, StepTimer};
use forge_types::{ArgStack, IntegrationId, SubActionOutcome, SubActionTelemetry};
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
