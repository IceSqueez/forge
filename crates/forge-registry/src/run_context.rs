use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_types::{ArgStack, EventId, SubActionStep};

use crate::chain::{CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome};
use crate::error::RegistryError;

pub struct RunContext<'a> {
    pub arg_stack: &'a ArgStack,
    pub index: usize,
    pub parent_event_id: EventId,
    pub publisher: &'a dyn EventPublisher,
    /// Re-entrant entry point a composite runner uses to run a nested chain one
    /// level below the current step; leaf runners are handed a null executor.
    pub executor: &'a dyn ChainExecutor,
    /// Cancellation shared with every nested child chain of this execution;
    /// composite/looping runners poll it at boundaries. Leaf runners get a
    /// fresh, never-tripped signal.
    pub cancel: CancelSignal,
}

impl<'a> RunContext<'a> {
    /// Context for a runner that neither launches a child chain nor observes
    /// cancellation. The re-entrant slots are filled with a null executor (runs
    /// nothing) and a fresh cancel signal, so it is never correct to call
    /// `executor`/`cancel` from a runner constructed this way.
    pub fn leaf(
        arg_stack: &'a ArgStack,
        index: usize,
        parent_event_id: EventId,
        publisher: &'a dyn EventPublisher,
    ) -> Self {
        Self {
            arg_stack,
            index,
            parent_event_id,
            publisher,
            executor: &NOOP_EXECUTOR,
            cancel: CancelSignal::new(),
        }
    }
}

struct NoopChainExecutor;

#[async_trait]
impl ChainExecutor for NoopChainExecutor {
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
        CancelSignal::new()
    }
}

static NOOP_EXECUTOR: NoopChainExecutor = NoopChainExecutor;
