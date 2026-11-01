use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_types::{ArgStack, EventId, SubActionStep};

use crate::chain::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, ControlCell, TelemetrySink,
};
use crate::error::RegistryError;
use crate::run_context::RunContext;

impl<'a> RunContext<'a> {
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
            control: ControlCell::new(),
            telemetry: TelemetrySink::new(),
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
