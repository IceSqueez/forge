use std::sync::Arc;

use async_trait::async_trait;
use forge_registry::runner::SubActionConfig;
use forge_registry::{
    ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner, TypedField,
};
use forge_types::{ArgStack, SubActionOutcome, SubActionTelemetry};

use crate::ObsSink;
use crate::runners::fields::ACTIVE;
use crate::vocabulary::sub_action_ids;

const KIND_ID: &str = sub_action_ids::REPLAY_SET_ACTIVE;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&ACTIVE]);

pub struct ReplaySetActiveRunner {
    sink: Arc<dyn ObsSink>,
}

impl ReplaySetActiveRunner {
    pub fn new(sink: Arc<dyn ObsSink>) -> Self {
        Self { sink }
    }
}

#[async_trait]
impl SubActionRunner for ReplaySetActiveRunner {
    fn id(&self) -> &str {
        KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Obs
    }

    fn label(&self) -> &str {
        "Set Replay Buffer Active"
    }

    fn summary(&self) -> &str {
        "Starts or stops the OBS replay buffer to an explicit state."
    }

    fn search_text(&self) -> &str {
        "obs replay buffer start stop set on off active"
    }

    fn icon_name(&self) -> &str {
        "player-record"
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
        let on = match ACTIVE.read(config, ctx.arg_stack) {
            Ok(on) => on,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };
        let result = if on {
            self.sink.start_replay_buffer().await
        } else {
            self.sink.stop_replay_buffer().await
        };
        (timer.finish(SubActionOutcome::from_result(&result)), None)
    }
}
