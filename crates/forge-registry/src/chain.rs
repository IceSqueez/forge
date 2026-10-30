use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use forge_types::{
    ActionId, ArgStack, EventId, IntegrationAvailability, SubActionStep, SubActionTelemetry,
    UserText,
};
use tokio::sync::watch;

use crate::error::RegistryError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainSignal {
    Completed,
    Stop(StopMark),
    Break,
    Continue,
    Error(UserText),
    Aborted,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StopMark {
    pub failed: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlSignal {
    Break,
    Continue,
    Stop(StopMark),
}

#[derive(Clone, Default)]
pub struct ControlCell(Arc<Mutex<Option<ControlSignal>>>);

impl ControlCell {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, signal: ControlSignal) {
        *self.guard() = Some(signal);
    }

    pub fn take(&self) -> Option<ControlSignal> {
        self.guard().take()
    }

    fn guard(&self) -> MutexGuard<'_, Option<ControlSignal>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

pub struct ChildChainOutcome {
    pub signal: ChainSignal,
    pub arg_stack: ArgStack,
    pub telemetry: Vec<SubActionTelemetry>,
}

#[derive(Clone, Default)]
pub struct TelemetrySink(Arc<Mutex<Vec<SubActionTelemetry>>>);

impl TelemetrySink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn extend(&self, rows: impl IntoIterator<Item = SubActionTelemetry>) {
        self.guard().extend(rows);
    }

    pub fn drain(&self) -> Vec<SubActionTelemetry> {
        std::mem::take(&mut self.guard())
    }

    fn guard(&self) -> MutexGuard<'_, Vec<SubActionTelemetry>> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Clone)]
pub struct CancelSignal(Arc<watch::Sender<bool>>);

impl Default for CancelSignal {
    fn default() -> Self {
        Self(Arc::new(watch::Sender::new(false)))
    }
}

impl CancelSignal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }

    pub async fn cancelled(&self) {
        let mut cancelled = self.0.subscribe();
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the sender lives in self, so the wait only ends once cancelled"
        )]
        let _ = cancelled.wait_for(|cancelled| *cancelled).await;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunningAction {
    pub action_id: ActionId,
    pub start_event_id: EventId,
    pub trigger_event_id: EventId,
}

#[async_trait]
pub trait ChainExecutor: Send + Sync {
    async fn run_child_chain(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
    ) -> Result<ChildChainOutcome, RegistryError>;

    fn cancel_signal(&self) -> CancelSignal;

    fn integration_availability(&self) -> Option<Arc<dyn IntegrationAvailability>> {
        None
    }

    fn running_action(&self) -> Option<RunningAction> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const HANG_GUARD: Duration = Duration::from_secs(30);

    #[tokio::test]
    async fn cancelled_stays_pending_until_a_cancel_lands() {
        let signal = CancelSignal::new();

        tokio::select! {
            biased;
            () = signal.cancelled() => panic!("nothing cancelled this signal"),
            () = std::future::ready(()) => {}
        }
    }

    #[tokio::test]
    async fn cancelled_resolves_at_once_for_an_already_cancelled_signal() {
        let signal = CancelSignal::new();
        signal.cancel();

        tokio::select! {
            biased;
            () = signal.cancelled() => {}
            () = std::future::ready(()) => panic!("a cancelled signal must not wait"),
        }
    }

    #[tokio::test]
    async fn cancelling_a_clone_wakes_a_parked_waiter_on_the_original() {
        let signal = CancelSignal::new();
        let clone = signal.clone();
        let waiter = tokio::spawn(async move { signal.cancelled().await });
        tokio::task::yield_now().await;

        clone.cancel();

        tokio::time::timeout(HANG_GUARD, waiter)
            .await
            .expect("the waiter was never woken")
            .unwrap();
    }
}
