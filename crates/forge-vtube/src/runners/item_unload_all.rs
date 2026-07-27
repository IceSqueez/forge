use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct ItemUnloadAllRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemUnloadAllRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ItemUnloadAllRunner {
    fn id(&self) -> &str {
        "vtube.item.unload_all"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Remove All Items"
    }

    fn summary(&self) -> &str {
        "Removes every item currently loaded in the VTube Studio scene."
    }

    fn search_text(&self) -> &str {
        "vtube item remove clear unload all scene vts"
    }

    fn icon_name(&self) -> &str {
        "trash"
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

        let outcome = SubActionOutcome::from_result(&self.sink.unload_all_items().await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: "vtube.item.unload_all".to_owned(),
                started_at,
                duration_ms: start.elapsed().as_millis() as u64,
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};

    #[tokio::test]
    async fn execute_succeeds_with_no_config() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemUnloadAllRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "vtube.item.unload_all");
        assert!(extra.is_none());
        assert!(sink.was_called());
    }

    #[tokio::test]
    async fn execute_propagates_sink_error() {
        let runner = ItemUnloadAllRunner::new(Arc::new(MockSink::failing()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, _) = runner.execute(&BTreeMap::new(), &ctx).await;
        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
    }
}
