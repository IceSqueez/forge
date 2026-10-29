use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    FormField, RegistryError, RunContext, SubActionCategory, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::ObsSink;
use crate::vocabulary::fields::query;
use crate::vocabulary::{sub_action_ids, variables};

const KIND_ID: &str = sub_action_ids::SOURCE_GET_LIST;

pub struct QueryInputListRunner {
    sink: Arc<dyn ObsSink>,
}

impl QueryInputListRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for QueryInputListRunner {
    fn id(&self) -> &str {
        KIND_ID
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

    fn default_config(&self) -> SubActionConfig {
        BTreeMap::new()
    }

    fn config_fields(&self) -> Vec<FormField> {
        vec![]
    }

    fn validate_config(&self, _config: &SubActionConfig) -> Result<(), RegistryError> {
        Ok(())
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
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        match self.sink.get_input_list().await {
            Ok(variant) => {
                let mut stack = ctx.arg_stack.clone();
                if let Variant::Object(ref map) = variant
                    && let Some(names) = map.get(query::ALL_NAMES)
                {
                    stack = stack.set(variables::source_query::ALL_NAMES.to_owned(), names.clone());
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
