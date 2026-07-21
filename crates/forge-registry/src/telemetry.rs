use std::collections::BTreeMap;

use forge_types::{SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

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

    pub fn failed(self, message: impl Into<String>) -> SubActionTelemetry {
        self.finish(SubActionOutcome::Failed(message.into()))
    }

    pub fn skipped(self, message: impl Into<String>) -> SubActionTelemetry {
        self.finish(SubActionOutcome::Skipped(message.into()))
    }
}
