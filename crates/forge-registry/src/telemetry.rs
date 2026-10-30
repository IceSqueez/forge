use std::collections::BTreeMap;

use forge_types::{MessageArg, SubActionOutcome, SubActionTelemetry, UserText};
use time::OffsetDateTime;

use crate::config_problem::ConfigProblem;
use crate::run_context::RunContext;

pub struct StepTimer {
    started_at: OffsetDateTime,
    index: usize,
    kind: String,
}

impl StepTimer {
    pub fn start(ctx: &RunContext<'_>, kind: impl Into<String>) -> Self {
        Self {
            started_at: OffsetDateTime::now_utc(),
            index: ctx.index,
            kind: kind.into(),
        }
    }

    pub fn started_at(&self) -> OffsetDateTime {
        self.started_at
    }

    pub fn finish(self, outcome: SubActionOutcome) -> SubActionTelemetry {
        let duration_ms = (OffsetDateTime::now_utc() - self.started_at)
            .whole_milliseconds()
            .max(0) as u64;
        SubActionTelemetry {
            index: self.index,
            kind: self.kind,
            started_at: self.started_at,
            duration_ms,
            outcome,
            args_in: BTreeMap::new(),
            produced: BTreeMap::new(),
        }
    }

    pub fn success(self) -> SubActionTelemetry {
        self.finish(SubActionOutcome::Success)
    }

    pub fn failed(self, message: UserText) -> SubActionTelemetry {
        self.finish(SubActionOutcome::Failed(message))
    }

    pub fn skipped(self, message: UserText) -> SubActionTelemetry {
        self.finish(SubActionOutcome::Skipped(message))
    }

    pub fn invalid_config(self, problem: &ConfigProblem) -> SubActionTelemetry {
        let message = problem.message(|keys| MessageArg::StepFields {
            kind: self.kind.clone(),
            keys: keys.to_vec(),
        });
        self.failed(message.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_events::{Event, EventPublisher};
    use forge_types::{ArgStack, EventId};

    struct NullPublisher;
    impl EventPublisher for NullPublisher {
        fn publish(&self, _event: Event) {}
    }

    static NULL_PUBLISHER: NullPublisher = NullPublisher;

    fn ctx(stack: &ArgStack, index: usize) -> RunContext<'_> {
        RunContext::leaf(stack, index, EventId::new(), &NULL_PUBLISHER)
    }

    #[test]
    fn finish_copies_kind_and_index_from_context_and_leaves_arg_maps_empty() {
        let stack = ArgStack::new();
        let tel = StepTimer::start(&ctx(&stack, 3), "core.demo").finish(SubActionOutcome::Success);
        assert_eq!(tel.kind, "core.demo");
        assert_eq!(tel.index, 3);
        assert!(tel.args_in.is_empty());
        assert!(tel.produced.is_empty());
    }

    #[test]
    fn named_helpers_map_to_their_outcome_variant_and_carry_the_message() {
        let stack = ArgStack::new();
        assert_eq!(
            StepTimer::start(&ctx(&stack, 0), "k").success().outcome,
            SubActionOutcome::Success
        );
        assert_eq!(
            StepTimer::start(&ctx(&stack, 0), "k")
                .failed(forge_types::UserText::unlocalized("boom"))
                .outcome,
            SubActionOutcome::Failed(forge_types::UserText::unlocalized("boom"))
        );
        assert_eq!(
            StepTimer::start(&ctx(&stack, 0), "k")
                .skipped(forge_types::UserText::unlocalized("off"))
                .outcome,
            SubActionOutcome::Skipped(forge_types::UserText::unlocalized("off"))
        );
    }

    #[test]
    fn started_at_accessor_matches_emitted_row_and_is_a_real_instant() {
        let stack = ArgStack::new();
        let before = OffsetDateTime::now_utc();
        let timer = StepTimer::start(&ctx(&stack, 0), "k");
        let captured = timer.started_at();
        let tel = timer.success();
        let after = OffsetDateTime::now_utc();
        assert_eq!(tel.started_at, captured);
        assert!(captured >= before && captured <= after);
    }
}
