use std::sync::Arc;

use async_trait::async_trait;
use forge_events::EventSource;
use forge_registry::{
    ConfigProblem, ConfigSpec, RunContext, StepTimer, SubActionCategory, SubActionRunner,
    TypedField,
};
use forge_storage::GlobalsRepo;
use forge_types::{ArgStack, SubActionConfig, SubActionOutcome, SubActionTelemetry};

use super::core_globals_shared::{AMOUNT, GLOBAL_BY_NAME, global_name};
use crate::core_messages;
use crate::global_mutation::GlobalMutation;

pub const GLOBAL_INCREMENT_KIND_ID: &str = "core.globals.increment";

const CONFIG: ConfigSpec = ConfigSpec::new(&[&GLOBAL_BY_NAME, &AMOUNT]);

fn read_config(config: &SubActionConfig, args: &ArgStack) -> Result<(String, i64), ConfigProblem> {
    Ok((
        global_name(&GLOBAL_BY_NAME, config, args)?,
        AMOUNT.read(config, args)?,
    ))
}

pub(crate) struct CoreGlobalsIncrementRunner {
    globals: Arc<dyn GlobalsRepo>,
}

impl CoreGlobalsIncrementRunner {
    pub(crate) fn new(globals: Arc<dyn GlobalsRepo>) -> Self {
        Self { globals }
    }
}

#[async_trait]
impl SubActionRunner for CoreGlobalsIncrementRunner {
    fn id(&self) -> &str {
        GLOBAL_INCREMENT_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Globals
    }

    fn label(&self) -> &str {
        "Increment Global"
    }

    fn summary(&self) -> &str {
        "Increment a numeric global variable by an amount"
    }

    fn search_text(&self) -> &str {
        "increment global variable counter add"
    }

    fn icon_name(&self) -> &str {
        "plus"
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
        let (name, amount) = match read_config(config, ctx.arg_stack) {
            Ok(values) => values,
            Err(problem) => return (timer.invalid_config(&problem), None),
        };

        let mutation = GlobalMutation {
            globals: self.globals.as_ref(),
            publisher: ctx.publisher,
            source: EventSource::Core,
            caused_by: ctx.parent_event_id,
        };
        let outcome = match mutation.increment(&name, amount).await {
            Ok(_increment) => SubActionOutcome::Success,
            Err(e) => SubActionOutcome::Failed(core_messages::storage_failed(e).into()),
        };

        (timer.finish(outcome), None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MapGlobals;
    use forge_events::test_support::NullPublisher;

    use forge_registry::RunContext;
    use forge_types::EventId;
    use forge_types::Variant;

    fn cfg(name: &str, amount: i64) -> SubActionConfig {
        let mut c = SubActionConfig::new();
        c.insert("name".to_owned(), Variant::String(name.to_owned()));
        c.insert("amount".to_owned(), Variant::Int(amount));
        c
    }

    #[tokio::test]
    async fn increment_missing_key_reports_failed() {
        let globals = Arc::new(MapGlobals::default());
        let stack = ArgStack::new();
        let ctx = RunContext::leaf(&stack, 0, EventId::new(), &NullPublisher);
        let runner = CoreGlobalsIncrementRunner::new(globals);
        let outcome = runner.execute(&cfg("ghost", 1), &ctx).await.0.outcome;
        assert!(matches!(outcome, SubActionOutcome::Failed(_)));
    }
}
