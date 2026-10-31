use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry, Variant};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{sub_action_ids, variables};

pub(crate) struct QueryStreamStatusRunner {
    sink: Arc<dyn ObsSink>,
}

impl QueryStreamStatusRunner {
    pub(crate) fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QueryStreamStatusRunner {
    fn id(&self) -> &str {
        sub_action_ids::STREAMING_GET_STATUS
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Get Streaming Status"
    }

    fn summary(&self) -> &str {
        "Queries OBS for the current stream output state and elapsed duration."
    }

    fn search_text(&self) -> &str {
        "obs stream streaming get status active duration live"
    }

    fn icon_name(&self) -> &str {
        "wifi"
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![
                variables::streaming::IS_ACTIVE.to_owned(),
                variables::streaming::DURATION_MS.to_owned(),
            ],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        match self.sink.get_stream_status().await {
            Ok(status) => {
                let stack = ctx
                    .arg_stack
                    .clone()
                    .set(
                        variables::streaming::IS_ACTIVE.to_owned(),
                        Variant::Bool(status.active),
                    )
                    .set(
                        variables::streaming::DURATION_MS.to_owned(),
                        Variant::Int(status.duration_ms),
                    );
                (timer.success(), Some(stack))
            }
            Err(e) => (timer.finish(step_failure::failed(&e)), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_populates_stream_active_and_duration_from_sink() {
        let runner = QueryStreamStatusRunner::new(Arc::new(MockSink));
        let empty = ArgStack::new();
        let (telemetry, stack) = runner.execute(&BTreeMap::new(), &make_ctx(&empty)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let stack = stack.unwrap();
        assert_eq!(
            stack.get("obs.stream.is_active"),
            Some(&Variant::Bool(true))
        );
        assert_eq!(
            stack.get("obs.stream.duration_ms"),
            Some(&Variant::Int(45_000)),
        );
    }
}
