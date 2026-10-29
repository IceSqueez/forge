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

const KIND_ID: &str = sub_action_ids::SCENE_SWITCH_CURRENT;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SCENE]);

pub struct SwitchCurrentSceneRunner {
    sink: Arc<dyn ObsSink>,
}

impl SwitchCurrentSceneRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SwitchCurrentSceneRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Switch Scene"
    }

    fn summary(&self) -> &str {
        "Sets the current OBS program scene."
    }

    fn search_text(&self) -> &str {
        "obs switch scene current program set"
    }

    fn icon_name(&self) -> &str {
        "arrows-shuffle"
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

        let already_current = matches!(
            self.sink.get_current_scene().await,
            Ok(Some(current)) if current == scene
        );
        let outcome = if already_current {
            SubActionOutcome::Success
        } else {
            SubActionOutcome::from_result(&self.sink.set_scene(&scene).await)
        };
        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runners::test_support::{RecordingSink, make_ctx};
    use forge_types::Variant;
    use std::collections::BTreeMap;

    fn scene_config(scene: &str) -> SubActionConfig {
        BTreeMap::from([("scene".to_owned(), Variant::String(scene.to_owned()))])
    }

    #[tokio::test]
    async fn switching_to_the_scene_already_on_program_reports_success_without_setting_it() {
        let sink = RecordingSink::new();
        let runner = SwitchCurrentSceneRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&scene_config("Gameplay"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(sink.calls(), vec!["get_current_scene".to_owned()]);
    }

    #[tokio::test]
    async fn switching_to_a_scene_that_is_not_on_program_sets_it() {
        let sink = RecordingSink::new();
        let runner = SwitchCurrentSceneRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&scene_config("Starting Soon"), &make_ctx(&stack))
            .await;

        assert_eq!(telemetry.outcome, SubActionOutcome::Success);
        assert_eq!(
            sink.calls(),
            vec![
                "get_current_scene".to_owned(),
                "set_scene(Starting Soon)".to_owned()
            ],
        );
    }

    #[tokio::test]
    async fn the_no_op_guard_compares_the_interpolated_scene_name() {
        let sink = RecordingSink::new();
        let runner = SwitchCurrentSceneRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack =
            ArgStack::new().set("target".to_owned(), Variant::String("Gameplay".to_owned()));

        runner
            .execute(&scene_config("%target%"), &make_ctx(&stack))
            .await;

        assert_eq!(sink.calls(), vec!["get_current_scene".to_owned()]);
    }

    #[tokio::test]
    async fn an_unreadable_program_scene_still_attempts_the_switch() {
        let sink = RecordingSink::failing();
        let runner = SwitchCurrentSceneRunner::new(Arc::clone(&sink) as Arc<dyn ObsSink>);
        let stack = ArgStack::new();

        let (telemetry, _) = runner
            .execute(&scene_config("Gameplay"), &make_ctx(&stack))
            .await;

        assert!(matches!(telemetry.outcome, SubActionOutcome::Failed(_)));
        assert!(
            sink.calls().contains(&"set_scene(Gameplay)".to_owned()),
            "the switch was skipped after a failed read: {:?}",
            sink.calls(),
        );
    }
}
