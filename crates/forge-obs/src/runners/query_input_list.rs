use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, StepTimer, SubActionCategory, SubActionIo, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry, Variant};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{sub_action_ids, variables};

pub(crate) struct QueryInputListRunner {
    sink: Arc<dyn ObsSink>,
}

impl QueryInputListRunner {
    pub(crate) fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QueryInputListRunner {
    fn id(&self) -> &str {
        sub_action_ids::SOURCE_GET_LIST
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Get Source/Input List"
    }

    fn summary(&self) -> &str {
        "Queries OBS for all inputs (sources) by name."
    }

    fn search_text(&self) -> &str {
        "obs sources inputs get list all names"
    }

    fn icon_name(&self) -> &str {
        "layers"
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![variables::source_query::ALL_NAMES.to_owned()],
            ..SubActionIo::default()
        }
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        match self.sink.get_input_list().await {
            Ok(names) => {
                let all_names = Variant::Array(names.into_iter().map(Variant::String).collect());
                let stack = ctx
                    .arg_stack
                    .clone()
                    .set(variables::source_query::ALL_NAMES.to_owned(), all_names);
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
    async fn execute_populates_input_names_from_sink() {
        let runner = QueryInputListRunner::new(Arc::new(MockSink));
        let empty = ArgStack::new();
        let (telemetry, stack) = runner.execute(&BTreeMap::new(), &make_ctx(&empty)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let stack = stack.unwrap();
        assert_eq!(
            stack.get("obs.sources.all_names"),
            Some(&Variant::Array(vec![
                Variant::String("Mic".to_owned()),
                Variant::String("Desktop Audio".to_owned()),
            ])),
        );
    }
}
