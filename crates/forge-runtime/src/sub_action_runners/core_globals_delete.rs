use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventSource;
use forge_registry::{ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::core_globals_shared::{GLOBAL_BY_NAME, global_name};
use crate::core_messages;
use crate::global_mutation::GlobalMutation;

const CONFIG: ConfigSpec = ConfigSpec::new(&[&GLOBAL_BY_NAME]);

pub(crate) struct CoreGlobalsDeleteRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsDeleteRunner {
    pub(crate) fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsDeleteRunner {
    fn id(&self) -> &str {
        "core.globals.delete"
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Delete Global"
    }

    fn summary(&self) -> &str {
        "Remove a global variable"
    }

    fn search_text(&self) -> &str {
        "delete global variable remove clear"
    }

    fn icon_name(&self) -> &str {
        "database-minus"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(CONFIG)
    }

    async fn execute(
        &self,
        config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        let name = match global_name(&GLOBAL_BY_NAME, config, ctx.arg_stack) {
            Ok(name) => name,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let mutation = GlobalMutation {
            globals: self.globals.as_ref(),
            publisher: ctx.publisher,
            source: EventSource::Core,
            caused_by: ctx.parent_event_id,
        };
        let outcome = match mutation.delete(&name).await {
            Ok(_deletion) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
        };

        (timer.finish(outcome), None)
    }
}
