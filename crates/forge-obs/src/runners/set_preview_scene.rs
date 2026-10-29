use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::ObsSink;
use crate::runners::fields::SCENE;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::SCENE_SET_PREVIEW;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SCENE]);

pub struct SetPreviewSceneRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetPreviewSceneRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetPreviewSceneRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Preview Scene"
    }

    fn summary(&self) -> &str {
        "Sets the current preview scene in OBS Studio Mode."
    }

    fn search_text(&self) -> &str {
        "obs scene preview studio mode set switch"
    }

    fn icon_name(&self) -> &str {
        "monitor"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, KIND_ID);
        let scene = match SCENE.read(config, ctx.arg_stack) {
            Ok(scene) => scene,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = SubActionOutcome::from_result(&self.sink.set_preview_scene(&scene).await);
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{MockSink, make_ctx};
    use forge_types::Variant;
    use std::collections::BTreeMap;

    #[tokio::test]
    async fn execute_reports_success_with_correct_kind() {
        let runner = SetPreviewSceneRunner::new(Arc::new(MockSink));
        let stack = ArgStack::new();
        let config = BTreeMap::from([("scene".to_owned(), Variant::String("Intro".to_owned()))]);
        let (tel, extra) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.scene.set_preview");
        assert!(extra.is_none());
    }
}
