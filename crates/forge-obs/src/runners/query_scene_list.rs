use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    RunContext, StepCatalogEntry, StepTimer, SubActionCategory, SubActionIo, SubActionRunner,
};
use forge_types::{ArgStack, SubActionTelemetry, Variant};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{sub_action_ids, variables};

pub(crate) struct QuerySceneListRunner {
    sink: Arc<dyn ObsSink>,
}

impl QuerySceneListRunner {
    pub(crate) fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

impl StepCatalogEntry for QuerySceneListRunner {
    fn category(&self) -> SubActionCategory {
        SubActionCategory::Integration
    }
    fn label(&self) -> &str {
        "Get Scene List"
    }
    fn summary(&self) -> &str {
        "Queries OBS for all scenes and the current program scene."
    }
    fn search_text(&self) -> &str {
        "obs scenes get list all current program"
    }
    fn icon_name(&self) -> &str {
        "layout-grid"
    }
}

#[async_trait]
impl SubActionRunner for QuerySceneListRunner {
    fn id(&self) -> &str {
        sub_action_ids::SCENE_GET_LIST
    }

    fn scope_io(&self) -> SubActionIo {
        SubActionIo {
            fixed_outputs: vec![
                variables::scene_query::ALL_NAMES.to_owned(),
                variables::scene_query::CURRENT.to_owned(),
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

        match self.sink.get_scene_list().await {
            Ok(scenes) => {
                let all_names =
                    Variant::Array(scenes.names.into_iter().map(Variant::String).collect());
                let stack = ctx
                    .arg_stack
                    .clone()
                    .set(variables::scene_query::ALL_NAMES.to_owned(), all_names)
                    .set(
                        variables::scene_query::CURRENT.to_owned(),
                        Variant::String(scenes.current),
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
    async fn execute_populates_scene_names_and_current_from_sink() {
        let runner = QuerySceneListRunner::new(Arc::new(MockSink));
        let empty = ArgStack::new();
        let (telemetry, stack) = runner.execute(&BTreeMap::new(), &make_ctx(&empty)).await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Success));
        let stack = stack.unwrap();
        assert_eq!(
            stack.get("obs.scenes.names"),
            Some(&Variant::Array(vec![
                Variant::String("Intro".to_owned()),
                Variant::String("Gameplay".to_owned()),
            ])),
        );
        assert_eq!(
            stack.get("obs.scene"),
            Some(&Variant::String("Gameplay".to_owned())),
        );
    }
}
