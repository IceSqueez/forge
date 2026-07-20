use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{FormField, RegistryError, RunContext, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry, Variant};
use time::OffsetDateTime;

use crate::sink::VTubeSink;

pub struct LookupHotkeysRunner {
    sink: Arc<dyn VTubeSink>,
}

impl LookupHotkeysRunner {
    pub fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for LookupHotkeysRunner {
    fn id(&self) -> &str {
        "vtube.lookup.hotkeys"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::VTube
    }

    fn label(&self) -> &str {
        "Get Hotkeys"
    }

    fn summary(&self) -> &str {
        "Queries VTube Studio for the hotkeys in the current model."
    }

    fn search_text(&self) -> &str {
        "vtube get hotkeys current model names ids lookup vts"
    }

    fn icon_name(&self) -> &str {
        "keyboard"
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

        match self.sink.get_hotkeys().await {
            Ok(variant) => {
                let mut stack = ArgStack::new();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get("names") {
                        stack = stack.set("vtube.hotkeys.names".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("ids") {
                        stack = stack.set("vtube.hotkeys.ids".to_owned(), v.clone());
                    }
                    if let Some(v) = map.get("count") {
                        stack = stack.set("vtube.hotkeys.count".to_owned(), v.clone());
                    }
                }
                (
                    SubActionTelemetry {
                        args_in: ::std::collections::BTreeMap::new(),
                        produced: ::std::collections::BTreeMap::new(),
                        kind: "vtube.lookup.hotkeys".to_owned(),
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
                    kind: "vtube.lookup.hotkeys".to_owned(),
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
    async fn execute_extracts_hotkey_fields_into_arg_stack() {
        let runner = LookupHotkeysRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;

        assert_eq!(tel.outcome, SubActionOutcome::Success);
        let out = extra.expect("success must surface an arg stack");
        assert_eq!(
            out.get("vtube.hotkeys.names"),
            Some(&Variant::Array(vec![
                Variant::String("Wave".to_owned()),
                Variant::String("Blush".to_owned()),
            ]))
        );
        assert_eq!(
            out.get("vtube.hotkeys.ids"),
            Some(&Variant::Array(vec![
                Variant::String("hk-1".to_owned()),
                Variant::String("hk-2".to_owned()),
            ]))
        );
        assert_eq!(out.get("vtube.hotkeys.count"), Some(&Variant::Int(2)));
    }

    #[tokio::test]
    async fn execute_on_sink_error_fails_with_no_arg_stack() {
        let runner = LookupHotkeysRunner::new(Arc::new(MockSink::failing()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;

        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(extra.is_none());
    }
}
