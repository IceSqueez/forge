use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry};
use time::OffsetDateTime;

use crate::runners::step_failure;
use crate::sink::VTubeSink;
use crate::vocabulary::sub_action_ids;

pub(crate) struct ItemUnloadAllRunner {
    sink: Arc<dyn VTubeSink>,
}

impl ItemUnloadAllRunner {
    pub(crate) fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ItemUnloadAllRunner {
    fn id(&self) -> &str {
        sub_action_ids::ITEM_UNLOAD_ALL
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

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let started_at = OffsetDateTime::now_utc();
        let start = Instant::now();

        let outcome = step_failure::outcome(&self.sink.unload_all_items().await);

        (
            SubActionTelemetry {
                args_in: ::std::collections::BTreeMap::new(),
                produced: ::std::collections::BTreeMap::new(),
                kind: sub_action_ids::ITEM_UNLOAD_ALL.to_owned(),
                started_at,
                duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX),
                outcome,
                index: ctx.index,
            },
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::SubActionOutcome;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_succeeds_with_no_config() {
        let sink = Arc::new(MockSink::new());
        let runner = ItemUnloadAllRunner::new(Arc::clone(&sink) as Arc<dyn VTubeSink>);
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
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
