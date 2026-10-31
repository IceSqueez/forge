use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TextEditor, TextField,
    TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::COLLECTION_SWITCH;

const NAME: TextField = TextField {
    key: config_keys::NAME,
    label: "Collection Name",
    editor: TextEditor::Line {
        placeholder: "e.g. Main Collection",
    },
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&NAME]);

pub(crate) struct SceneCollectionSwitchRunner {
    sink: Arc<dyn ObsSink>,
}

impl SceneCollectionSwitchRunner {
    pub(crate) fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SceneCollectionSwitchRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Switch Scene Collection"
    }

    fn summary(&self) -> &str {
        "Switches OBS to a different scene collection by name."
    }

    fn search_text(&self) -> &str {
        "obs scene collection switch change load"
    }

    fn icon_name(&self) -> &str {
        "stack-2"
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
        let name = match NAME.read(config, ctx.arg_stack) {
            Ok(name) => name,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.sink.set_current_scene_collection(&name).await);
        (timer.finish(outcome), None)
    }
}
