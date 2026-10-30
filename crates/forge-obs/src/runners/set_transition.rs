use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::SCENE_SET_TRANSITION;

const TRANSITION: TextField = TextField {
    key: config_keys::TRANSITION,
    label: "Transition Name",
    editor: TextEditor::Line {
        placeholder: "e.g. Fade",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&TRANSITION]);

pub struct SetTransitionRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetTransitionRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetTransitionRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Scene Transition"
    }

    fn summary(&self) -> &str {
        "Sets the active scene transition by name."
    }

    fn search_text(&self) -> &str {
        "obs transition fade cut swipe set scene change"
    }

    fn icon_name(&self) -> &str {
        "shuffle"
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
        let transition = match TRANSITION.read(config, ctx.arg_stack) {
            Ok(transition) => transition,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome =
            step_failure::outcome(&self.sink.set_current_scene_transition(&transition).await);
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
        let runner = SetTransitionRunner::new(Arc::new(MockSink));
        let stack = ArgStack::new();
        let config = BTreeMap::from([("transition".to_owned(), Variant::String("Cut".to_owned()))]);
        let (tel, extra) = runner.execute(&config, &make_ctx(&stack)).await;
        assert_eq!(tel.outcome, SubActionOutcome::Success);
        assert_eq!(tel.kind, "obs.scene.set_transition");
        assert!(extra.is_none());
    }
}
