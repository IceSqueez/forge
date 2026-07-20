use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct LookupItemsRunner {
    sink: Arc<dyn VTubeSink>,
}

impl LookupItemsRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for LookupItemsRunner {
    fn id(&self) -> &str {
        "vtube.lookup.items"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Get Items"
    }

    fn summary(&self) -> &str {
        "Queries VTube Studio for the item instances currently in the scene."
    }

    fn search_text(&self) -> &str {
        "vtube get items scene instance ids file names lookup vts"
    }

    fn icon_name(&self) -> &str {
        "image"
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

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        match self.sink.get_items().await {
            Ok(variant) => {
                let mut stack = ArgStack::new();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get("instance_ids") {
                        stack = stack.set("vtube.items.instance_ids".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("file_names") {
                        stack = stack.set("vtube.items.file_names".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("count") {
                        stack = stack.set("vtube.items.count".to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: "vtube.lookup.items".to_owned(),
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
                    kind: "vtube.lookup.items".to_owned(),
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};

    #[tokio::test]
    async fn execute_extracts_item_fields_into_arg_stack() {
        let runner = LookupItemsRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;

        assert_eq!(tel.outcome, SubActionOutcome::Success);
        let out = extra.expect("success must surface an arg stack");
        assert_eq!(
            out.get("vtube.items.instance_ids"),
            Some(&Variant::Array(vec![Variant::String("inst-1".to_owned())]))
        );
        assert_eq!(
            out.get("vtube.items.file_names"),
            Some(&Variant::Array(vec![Variant::String(
                "crown.png".to_owned()
            )]))
        );
        assert_eq!(out.get("vtube.items.count"), Some(&Variant::Int(1)));
    }

    #[tokio::test]
    async fn execute_on_sink_error_fails_with_no_arg_stack() {
        let runner = LookupItemsRunner::new(Arc::new(MockSink::failing()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;

        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(extra.is_none());
    }
}
