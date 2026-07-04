use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use forge_types::{ArgStack, EventId, SubActionStep, SubActionTelemetry};

use crate::error::RegistryError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainSignal {
    Completed,
    /// Halts the whole action; re-propagated through every enclosing loop and
    /// absorbed only at the action-root, which records the run as failed iff the
    /// carried mark says so.
    Stop(StopMark),
    /// Unwinds to the nearest enclosing loop.
    Break,
    /// Skips to the next iteration of the nearest enclosing loop.
    Continue,
    Error(String),
    /// External cancellation observed at a step or iteration boundary.
    Aborted,
}

/// How the action-root records a run that a `stop` step halted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StopMark {
    pub failed: bool,
    pub reason: Option<String>,
}

/// In-band flow-control a leaf step raises for its immediately enclosing
/// sequential chain to act on once, the turn after the step returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlSignal {
    Break,
    Continue,
    Stop(StopMark),
}

/// One-shot mailbox a `break`/`continue`/`stop` leaf writes and its enclosing
/// `drive_sequential` drains right after the step returns. A fresh cell is minted
/// per sequential-chain invocation, so a raised signal never leaks past the chain
/// that must act on it; a leaf built through `RunContext::leaf` writes into a cell
/// nobody drains.
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

/// Side channel a composite runner writes the re-tagged telemetry of its nested
/// steps into, for the enclosing sequential/concurrent chain to drain and splice
/// into its own flat list right after the runner returns. A fresh cell is minted
/// per chain invocation and drained per step, so nested rows never leak past the
/// step that produced them; a runner built through `RunContext::leaf` writes into
/// a cell nobody drains.
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

/// Pollable cancellation flag shared across a single execution and its nested
/// child chains. Deliberately lives off the serde `ExecutionContext` (it is not
/// serializable) and is observed cooperatively between awaits.
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
    /// Runs `steps` as a child chain one nesting level below the caller, sharing
    /// the caller's cancellation signal. A step failure surfaces as
    /// `ChainSignal::Error` inside the returned outcome; `Err` is reserved for
    /// exceeding the nesting-depth bound.
    async fn run_child_chain(
        &self,
        steps: &[SubActionStep],
        arg_stack: &ArgStack,
        parent_event_id: EventId,
    ) -> Result<ChildChainOutcome, RegistryError>;

    fn cancel_signal(&self) -> CancelSignal;
}
