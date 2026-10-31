use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_types::{ArgStack, SubActionTelemetry, Variant};

use crate::runners::step_failure;
use crate::sink::{VTubeSink, result_keys};
use crate::vocabulary::{sub_action_ids, variables};

pub(crate) struct LookupCurrentModelRunner {
    sink: Arc<dyn VTubeSink>,
}

impl LookupCurrentModelRunner {
    pub(crate) fn new(sink: Arc<dyn VTubeSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for LookupCurrentModelRunner {
    fn id(&self) -> &str {
        sub_action_ids::LOOKUP_CURRENT_MODEL
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Integration
    }

    fn label(&self) -> &str {
        "Get Current Model"
    }

    fn summary(&self) -> &str {
        "Queries VTube Studio for the currently loaded model."
    }

    fn search_text(&self) -> &str {
        "vtube get current model name id loaded lookup vts"
    }

    fn icon_name(&self) -> &str {
        "user"
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());

        match self.sink.get_current_model().await {
            Ok(variant) => {
                let mut stack = ctx.arg_stack.clone();
                if let Variant::Object(ref map) = variant {
                    if let Some(v) = map.get(result_keys::NAME) {
                        stack = stack.set(variables::model::NAME.to_owned(), v.clone());
                    }
                    if let Some(v) = map.get(result_keys::ID) {
                        stack = stack.set(variables::model::ID.to_owned(), v.clone());
                    }
                    if let Some(v) = map.get(result_keys::LOADED) {
                        stack = stack.set(variables::model::LOADED.to_owned(), v.clone());
                    }
                }
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
    async fn execute_extracts_model_fields_into_arg_stack() {
        let runner = LookupCurrentModelRunner::new(Arc::new(MockSink::new()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;

        assert_eq!(tel.outcome, SubActionOutcome::Success);
        let out = extra.expect("success must surface an arg stack");
        assert_eq!(
            out.get("vtube.model.name"),
            Some(&Variant::String("MyAvatar".to_owned()))
        );
        assert_eq!(
            out.get("vtube.model.id"),
            Some(&Variant::String("model-abc".to_owned()))
        );
        assert_eq!(out.get("vtube.model.loaded"), Some(&Variant::Bool(true)));
    }

    #[tokio::test]
    async fn execute_on_sink_error_fails_with_no_arg_stack() {
        let runner = LookupCurrentModelRunner::new(Arc::new(MockSink::failing()));
        let stack = ArgStack::new();
        let ctx = make_ctx(&stack);
        let (tel, extra) = runner.execute(&BTreeMap::new(), &ctx).await;

        assert!(matches!(tel.outcome, SubActionOutcome::Failed(_)));
        assert!(extra.is_none());
    }
}
