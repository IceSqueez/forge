use async_trait::async_trait;
use forge_events::EventPublisher;
use forge_types::{ArgStack, EventId, SubActionStep};

use crate::chain::{
    CancelSignal, ChainExecutor, ChainSignal, ChildChainOutcome, ControlCell, TelemetrySink,
};
use crate::error::RegistryError;

pub struct RunContext<'a> {
    pub arg_stack: &'a ArgStack,
    pub index: usize,
    pub parent_event_id: EventId,
    pub publisher: &'a dyn EventPublisher,
    /// Leaf runners are handed a null executor that runs no nested chain.
    pub executor: &'a dyn ChainExecutor,
    /// Leaf runners get a fresh, never-tripped signal.
    pub cancel: CancelSignal,
    /// Drained only by `drive_sequential`; a leaf-built cell is never drained.
    pub control: ControlCell,
    /// Drained by the chain driver after each step; a leaf-built cell is never drained.
    pub telemetry: TelemetrySink,
}

impl<'a> RunContext<'a> {
    /// Fills the re-entrant slots with a null executor and a fresh cancel signal; never call `executor`/`cancel` expecting real chain behavior from a leaf.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use forge_events::Event;
    use forge_types::{ArgStack, SubActionConfig, Variant};

    use super::*;

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    fn one_step() -> Vec<SubActionStep> {
        vec![SubActionStep {
            kind_id: "core.log.write".to_owned(),
            config: SubActionConfig::new(),
            enabled: true,
            continue_on_error: false,
            condition: None,
            label: None,
        }]
    }

    #[tokio::test]
    async fn leaf_executor_runs_nothing_and_reports_completed() {
        let stack = ArgStack::new().set("user".to_owned(), Variant::String("alice".to_owned()));
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);

        let outcome = ctx
            .executor
            .run_child_chain(&one_step(), &stack, EventId::new())
            .await
            .expect("null executor never exceeds the depth bound");

        assert_eq!(outcome.signal, ChainSignal::Completed);
        assert!(
            outcome.telemetry.is_empty(),
            "the null executor must not run any step",
        );
    }

    #[test]
    fn leaf_cancel_signal_starts_untripped() {
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        assert!(!ctx.cancel.is_cancelled());
    }
}
