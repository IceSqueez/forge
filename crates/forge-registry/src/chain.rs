use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use forge_types::{ArgStack, EventId, SubActionStep, SubActionTelemetry};

use crate::error::RegistryError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainSignal {
    Completed,
    Stop(StopMark),
    Break,
    Continue,
    Error(String),
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

#[derive(Clone, Default)]
pub struct CancelSignal(Arc<AtomicBool>);

impl CancelSignal {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
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
}
