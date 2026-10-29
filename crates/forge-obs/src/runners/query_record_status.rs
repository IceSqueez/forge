use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{RunContext, SubActionCategory, SubActionIo, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;
use crate::vocabulary::fields::query;
use crate::vocabulary::{sub_action_ids, variables};

const KIND_ID: &str = sub_action_ids::RECORDING_GET_STATUS;

pub struct QueryRecordStatusRunner {
    sink: Arc<dyn ObsSink>,
}

impl QueryRecordStatusRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QueryRecordStatusRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Get Recording Status"
    }

    fn summary(&self) -> &str {
        "Queries OBS for the current recording state, pause state, and elapsed duration."
    }

    fn search_text(&self) -> &str {
        "obs record recording get status active paused duration"
    }

    fn icon_name(&self) -> &str {
        "circle"
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![
                variables::recording::IS_ACTIVE.to_owned(),
                variables::recording::IS_PAUSED.to_owned(),
                variables::recording::DURATION_MS.to_owned(),
            ],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        match self.sink.get_record_status().await {
            Ok(variant) => {
                let mut stack = ctx.arg_stack.clone();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get(query::IS_ACTIVE) {
                        stack = stack.set(variables::recording::IS_ACTIVE.to_owned(), v.clone());
                    }
                    if let Some(v) = map.get(query::IS_PAUSED) {
                        stack = stack.set(variables::recording::IS_PAUSED.to_owned(), v.clone());
                    }
                    if let Some(v) = map.get(query::DURATION_MS) {
                        stack = stack.set(variables::recording::DURATION_MS.to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: KIND_ID.to_owned(),
                        started_at,
                        duration_ms: start.elapsed().as_millis() as u64,
                        outcome: SubActionOutcome::Success,
                        index: ctx.index,
                    },
                    Some(stack),
                )
            }
            Err(e) => (
                SubActionTelemetry {
                    args_in: ::std::collections::BTreeMap::new(),
                    produced: ::std::collections::BTreeMap::new(),
                    kind: KIND_ID.to_owned(),
                    started_at,
                    duration_ms: start.elapsed().as_millis() as u64,
                    outcome: SubActionOutcome::Failed(e.to_string()),
                    index: ctx.index,
                },
                None,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_preserves_false_pause_flag_and_omits_absent_output_path() {
        let runner = QueryRecordStatusRunner::new(Arc::new(MockSink));
        let empty = ArgStack::new();
        let (telemetry, stack) = runner.execute(&BTreeMap::new(), &make_ctx(&empty)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let stack = stack.unwrap();
        assert_eq!(
            stack.get("obs.record.is_active"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(
            stack.get("obs.record.is_paused"),
            Some(&Variant::Bool(false))
        );
        assert_eq!(
            stack.get("obs.record.duration_ms"),
            Some(&Variant::Int(12_000)),
        );
        assert_eq!(stack.get("obs.record.output_path"), None);
    }
}
