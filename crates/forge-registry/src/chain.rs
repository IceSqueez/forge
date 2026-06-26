use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use forge_types::{ArgStack, EventId, SubActionStep, SubActionTelemetry};

use crate::error::RegistryError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainSignal {
    Completed,
    /// Halts the whole action; absorbed only at the action-root executor.
    Stop,
    /// Unwinds to the nearest enclosing loop.
    Break,
    /// Skips to the next iteration of the nearest enclosing loop.
    Continue,
    Error(String),
    /// External cancellation observed at a step or iteration boundary.
    Aborted,
}

pub struct ChildChainOutcome {
    pub signal: ChainSignal,
    pub arg_stack: ArgStack,
    pub telemetry: Vec<SubActionTelemetry>,
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
