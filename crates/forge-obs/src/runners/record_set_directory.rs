use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, PickedTextField, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TextPicker, TypedField,
};
use forge_types::{ArgStack, SubActionTelemetry};

use super::step_failure;
use crate::ObsSink;
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::RECORDING_SET_DIRECTORY;

const PATH: PickedTextField = PickedTextField {
    key: config_keys::PATH,
    label: "Directory",
    picker: TextPicker::File,
    default: "",
    required: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&PATH]);

pub struct RecordSetDirectoryRunner {
    sink: Arc<dyn ObsSink>,
}

impl RecordSetDirectoryRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for RecordSetDirectoryRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Record Directory"
    }

    fn summary(&self) -> &str {
        "Sets the directory OBS writes recording files to."
    }

    fn search_text(&self) -> &str {
        "obs record directory folder path output file location"
    }

    fn icon_name(&self) -> &str {
        "folder"
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
        let path = match PATH.read(config, ctx.arg_stack) {
            Ok(path) => path,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = step_failure::outcome(&self.sink.set_record_directory(&path).await);
        (timer.finish(outcome), None)
    }
}
