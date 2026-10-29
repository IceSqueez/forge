use async_trait::async_trait;
use forge_registry::{
    BREAK_LOOP_KIND_ID, ConfigSpec, ControlSignal, RunContext, StepTimer, SubActionCategory,
    SubActionRunner,
};
use forge_types::{ArgStack, SubActionConfig, SubActionTelemetry};

pub struct CoreLogicBreakLoopRunner;

#[async_trait]
impl SubActionRunner for CoreLogicBreakLoopRunner {
    fn id(&self) -> &str {
        BREAK_LOOP_KIND_ID
    }

    fn category(&self) -> SubActionCategory {
        SubActionCategory::Logic
    }

    fn label(&self) -> &str {
        "Break Loop"
    }

    fn summary(&self) -> &str {
        "Exit the innermost enclosing loop"
    }

    fn search_text(&self) -> &str {
        "break loop exit stop iteration flow control"
    }

    fn icon_name(&self) -> &str {
        "arrow-out"
    }

    fn config_spec(&self) -> Option<ConfigSpec> {
        Some(ConfigSpec::new(&[]))
    }

    async fn execute(
        &self,
        _config: &SubActionConfig,
        ctx: &RunContext<'_>,
    ) -> (SubActionTelemetry, Option<ArgStack>) {
        let timer = StepTimer::start(ctx, self.id());
        ctx.control.set(ControlSignal::Break);
        (timer.success(), None)
    }
}
