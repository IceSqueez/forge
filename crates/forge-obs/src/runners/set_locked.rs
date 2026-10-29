use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    ToggleField, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::ObsSink;
use crate::runners::fields::{SCENE, SOURCE};
use crate::vocabulary::{config_keys, sub_action_ids};

const KIND_ID: &str = sub_action_ids::SOURCE_SET_LOCKED;

const LOCKED: ToggleField = ToggleField {
    key: config_keys::LOCKED,
    label: "Locked",
    default: true,
};

const CONFIG: ConfigSpec = ConfigSpec::new(&[&SCENE, &SOURCE, &LOCKED]);

fn read_config(
    config: &SubActionConfig,
    args: &ArgStack,
) -> Result<(String, String, bool), ConfigProblem> {
    Ok((
        SCENE.read(config, args)?,
        SOURCE.read(config, args)?,
        LOCKED.read(config, args)?,
    ))
}

pub struct SetLockedRunner {
    sink: Arc<dyn ObsSink>,
}

impl SetLockedRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for SetLockedRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Source Locked"
    }

    fn summary(&self) -> &str {
        "Locks or unlocks a source within an OBS scene."
    }

    fn search_text(&self) -> &str {
        "obs source lock locked unlock scene item"
    }

    fn icon_name(&self) -> &str {
        "lock"
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
        let (scene, source, locked) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let outcome = SubActionOutcome::from_result(
            &self.sink.set_source_locked(&scene, &source, locked).await,
        );
        (timer.finish(outcome), None)
    }
}
