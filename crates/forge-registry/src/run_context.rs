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
    /// Re-entrant entry point a composite runner uses to run a nested chain one
    /// level below the current step; leaf runners are handed a null executor.
    pub executor: &'a dyn ChainExecutor,
    /// Cancellation shared with every nested child chain of this execution;
    /// composite/looping runners poll it at boundaries. Leaf runners get a
    /// fresh, never-tripped signal.
    pub cancel: CancelSignal,
    /// Where `break`/`continue`/`stop` leaves raise their flow-control signal for
    /// the enclosing sequential chain to drain. Drained only by `drive_sequential`;
    /// a leaf built through `RunContext::leaf` writes into a cell nobody reads.
    pub control: ControlCell,
    /// Where a composite runner deposits the re-tagged telemetry of its nested
    /// steps for the enclosing chain to splice into its flat list. Drained after
    /// each step by the chain driver; a leaf writes into a cell nobody drains.
    pub telemetry: TelemetrySink,
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
        // The null executor must ignore the steps it is handed (a leaf runner is
        // never supposed to launch a child chain): it returns `Completed` with no
        // telemetry even when given a real step. A genuine executor would emit a
        // telemetry row here.
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
