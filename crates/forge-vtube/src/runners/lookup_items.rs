use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::{VTubeSink, result_keys};
use crate::vocabulary::{sub_action_ids, variables};

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
        sub_action_ids::LOOKUP_ITEMS
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

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        match self.sink.get_items().await {
            Ok(variant) => {
                let mut stack = ctx.arg_stack.clone();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get(result_keys::INSTANCE_IDS) {
                        stack = stack.set(variables::items::INSTANCE_IDS.to_owned(), v.clone());
                    }
                    if let Some(v) = map.get(result_keys::FILE_NAMES) {
                        stack = stack.set(variables::items::FILE_NAMES.to_owned(), v.clone());
                    }
                    if let Some(v) = map.get(result_keys::COUNT) {
                        stack = stack.set(variables::items::COUNT.to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: sub_action_ids::LOOKUP_ITEMS.to_owned(),
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
                    kind: sub_action_ids::LOOKUP_ITEMS.to_owned(),
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
